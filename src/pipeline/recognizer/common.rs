use anyhow::{Context, Result, anyhow};
use fast_image_resize as fir;
use ndarray::Array4;
use rayon::prelude::*;

use crate::types::Frame;

pub const INPUT_SIZE: u32 = 224;
pub const NUM_LANDMARKS: usize = 21;
pub const PALM_INPUT_SIZE: u32 = 192;

#[derive(Clone, Debug)]
pub struct HandposeOutput {
    pub raw_landmarks: Vec<[f32; 3]>,
    pub projected_landmarks: Vec<(f32, f32)>,
    pub confidence: f32,
    pub handedness: f32,
    pub palm_regions: Vec<crate::types::PalmRegion>,
}

#[derive(Clone, Debug)]
pub struct LetterboxInfo {
    pub scale: f32,
    pub pad_x: f32,
    pub pad_y: f32,
    pub orig_w: u32,
    pub orig_h: u32,
}

#[derive(Clone, Debug)]
pub struct CropTransform {
    pub center: (f32, f32),
    pub side: f32,
    pub angle: f32,
    pub output_size: u32,
    pub orig_w: u32,
    pub orig_h: u32,
}

#[allow(dead_code)]
pub fn prepare_frame(frame: &Frame) -> Result<(Array4<f32>, LetterboxInfo)> {
    prepare_frame_with_size(frame, INPUT_SIZE)
}

pub fn prepare_frame_with_size(
    frame: &Frame,
    target_size: u32,
) -> Result<(Array4<f32>, LetterboxInfo)> {
    let expected_len = (frame.width as usize)
        .saturating_mul(frame.height as usize)
        .saturating_mul(4);
    if frame.rgba.len() != expected_len {
        return Err(anyhow!(
            "frame buffer size mismatch: got {}, expected {}",
            frame.rgba.len(),
            expected_len
        ));
    }

    let scale = target_size as f32 / (frame.width.max(frame.height) as f32);
    let new_w = (frame.width as f32 * scale).round().max(1.0) as u32;
    let new_h = (frame.height as f32 * scale).round().max(1.0) as u32;

    let src_image = fir::images::Image::from_vec_u8(
        frame.width,
        frame.height,
        frame.rgba.clone(),
        fir::PixelType::U8x4,
    )?;
    let mut dst_image = fir::images::Image::new(new_w, new_h, fir::PixelType::U8x4);
    let mut resizer = fir::Resizer::new();
    let resize_options = fir::ResizeOptions::new()
        .resize_alg(fir::ResizeAlg::Interpolation(fir::FilterType::Bilinear));
    resizer
        .resize(&src_image, &mut dst_image, Some(&resize_options))
        .context("fast resize failed")?;
    let resized = dst_image.into_vec();

    let pad_x = ((target_size as i64 - new_w as i64) / 2).max(0) as usize;
    let pad_y = ((target_size as i64 - new_h as i64) / 2).max(0) as usize;
    let mut canvas = vec![0u8; (target_size as usize) * (target_size as usize) * 4];
    for px in canvas.chunks_mut(4) {
        px[3] = 255;
    }
    let dst_stride = target_size as usize * 4;
    let src_stride = new_w as usize * 4;
    for row in 0..(new_h as usize) {
        let dst_offset = (pad_y + row) * dst_stride + pad_x * 4;
        let src_offset = row * src_stride;
        let dst_slice = &mut canvas[dst_offset..dst_offset + src_stride];
        let src_slice = &resized[src_offset..src_offset + src_stride];
        dst_slice.copy_from_slice(src_slice);
    }

    let normalized: Vec<f32> = canvas
        .par_chunks_exact(4)
        .flat_map_iter(|px| {
            [
                px[0] as f32 / 255.0,
                px[1] as f32 / 255.0,
                px[2] as f32 / 255.0,
            ]
        })
        .collect();
    let input = Array4::<f32>::from_shape_vec(
        (1, target_size as usize, target_size as usize, 3),
        normalized,
    )
    .map_err(|err| anyhow!("failed to build input tensor: {err}"))?;

    let letterbox = LetterboxInfo {
        scale,
        pad_x: pad_x as f32,
        pad_y: pad_y as f32,
        orig_w: frame.width,
        orig_h: frame.height,
    };

    Ok((input, letterbox))
}

pub fn decode_landmarks(flat: &[f32]) -> Result<Vec<[f32; 3]>> {
    if flat.len() < NUM_LANDMARKS * 3 {
        return Err(anyhow!(
            "unexpected landmarks length: got {}, need {}",
            flat.len(),
            NUM_LANDMARKS * 3
        ));
    }

    let mut landmarks = Vec::with_capacity(NUM_LANDMARKS);
    for chunk in flat.chunks_exact(3).take(NUM_LANDMARKS) {
        landmarks.push([chunk[0], chunk[1], chunk[2]]);
    }
    Ok(landmarks)
}

#[allow(dead_code)]
pub fn project_landmarks(landmarks: &[[f32; 3]], letterbox: &LetterboxInfo) -> Vec<(f32, f32)> {
    landmarks
        .iter()
        .map(|[x, y, _z]| {
            let px = (x - letterbox.pad_x) / letterbox.scale;
            let py = (y - letterbox.pad_y) / letterbox.scale;
            let cx = px.clamp(0.0, (letterbox.orig_w.saturating_sub(1)) as f32);
            let cy = py.clamp(0.0, (letterbox.orig_h.saturating_sub(1)) as f32);
            (cx, cy)
        })
        .collect()
}

pub fn prepare_rotated_crop(
    frame: &Frame,
    center: (f32, f32),
    side: f32,
    angle: f32,
    output_size: u32,
) -> Result<(Array4<f32>, CropTransform)> {
    let expected_len = (frame.width as usize)
        .saturating_mul(frame.height as usize)
        .saturating_mul(4);
    if frame.rgba.len() != expected_len {
        return Err(anyhow!(
            "frame buffer size mismatch: got {}, expected {}",
            frame.rgba.len(),
            expected_len
        ));
    }
    let mut data =
        Vec::with_capacity((output_size as usize).saturating_mul(output_size as usize * 3));
    let half = output_size as f32 / 2.0;
    let scale = side / output_size as f32;
    let cos = angle.cos();
    let sin = angle.sin();

    for y in 0..output_size {
        let dy = (y as f32 + 0.5 - half) * scale;
        for x in 0..output_size {
            let dx = (x as f32 + 0.5 - half) * scale;
            let src_x = center.0 + dx * cos - dy * sin;
            let src_y = center.1 + dx * sin + dy * cos;
            let rgb = sample_rgb(frame, src_x, src_y);
            data.extend_from_slice(&rgb);
        }
    }

    let array =
        Array4::<f32>::from_shape_vec((1, output_size as usize, output_size as usize, 3), data)
            .map_err(|err| anyhow!("failed to build rotated crop tensor: {err}"))?;

    let transform = CropTransform {
        center,
        side,
        angle,
        output_size,
        orig_w: frame.width,
        orig_h: frame.height,
    };

    Ok((array, transform))
}

pub fn project_landmarks_with_transform(
    landmarks: &[[f32; 3]],
    transform: &CropTransform,
) -> Vec<(f32, f32)> {
    landmarks
        .iter()
        .map(|[x, y, _z]| transform.project(*x, *y))
        .collect()
}

impl CropTransform {
    pub fn project(&self, x: f32, y: f32) -> (f32, f32) {
        let half = self.output_size as f32 / 2.0;
        let scale = self.side / self.output_size as f32;
        let dx = (x - half) * scale;
        let dy = (y - half) * scale;
        let cos = self.angle.cos();
        let sin = self.angle.sin();
        let ox = self.center.0 + dx * cos - dy * sin;
        let oy = self.center.1 + dx * sin + dy * cos;
        (
            ox.clamp(0.0, (self.orig_w.saturating_sub(1)) as f32),
            oy.clamp(0.0, (self.orig_h.saturating_sub(1)) as f32),
        )
    }
}

fn sample_rgb(frame: &Frame, x: f32, y: f32) -> [f32; 3] {
    if x.is_nan() || y.is_nan() {
        return [0.0, 0.0, 0.0];
    }
    let x0 = x.floor();
    let y0 = y.floor();
    let x1 = x0 + 1.0;
    let y1 = y0 + 1.0;

    let (w, h) = (frame.width as i32, frame.height as i32);
    let fetch = |cx: f32, cy: f32| -> [f32; 3] {
        let ix = cx as i32;
        let iy = cy as i32;
        if ix < 0 || iy < 0 || ix >= w || iy >= h {
            return [0.0, 0.0, 0.0];
        }
        let idx = ((iy as u32 * frame.width + ix as u32) as usize) * 4;
        if idx + 2 >= frame.rgba.len() {
            return [0.0, 0.0, 0.0];
        }
        [
            frame.rgba[idx] as f32 / 255.0,
            frame.rgba[idx + 1] as f32 / 255.0,
            frame.rgba[idx + 2] as f32 / 255.0,
        ]
    };

    let fx = x - x0;
    let fy = y - y0;
    let c00 = fetch(x0, y0);
    let c10 = fetch(x1, y0);
    let c01 = fetch(x0, y1);
    let c11 = fetch(x1, y1);

    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    [
        lerp(lerp(c00[0], c10[0], fx), lerp(c01[0], c11[0], fx), fy),
        lerp(lerp(c00[1], c10[1], fx), lerp(c01[1], c11[1], fx), fy),
        lerp(lerp(c00[2], c10[2], fx), lerp(c01[2], c11[2], fx), fy),
    ]
}
