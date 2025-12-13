use anyhow::{Context, Result, anyhow};
use fast_image_resize as fir;
use ndarray::Array4;
use rayon::prelude::*;

use crate::types::Frame;

pub const INPUT_SIZE: u32 = 224;
pub const NUM_LANDMARKS: usize = 21;

#[derive(Clone, Debug)]
pub struct HandposeOutput {
    pub raw_landmarks: Vec<[f32; 3]>,
    pub projected_landmarks: Vec<(f32, f32)>,
    pub confidence: f32,
    pub handedness: f32,
}

#[derive(Clone, Debug)]
pub struct LetterboxInfo {
    pub scale: f32,
    pub pad_x: f32,
    pub pad_y: f32,
    pub orig_w: u32,
    pub orig_h: u32,
}

pub fn prepare_frame(frame: &Frame) -> Result<(Array4<f32>, LetterboxInfo)> {
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

    let scale = INPUT_SIZE as f32 / (frame.width.max(frame.height) as f32);
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

    let pad_x = ((INPUT_SIZE as i64 - new_w as i64) / 2).max(0) as usize;
    let pad_y = ((INPUT_SIZE as i64 - new_h as i64) / 2).max(0) as usize;
    let mut canvas = vec![0u8; (INPUT_SIZE as usize) * (INPUT_SIZE as usize) * 4];
    for px in canvas.chunks_mut(4) {
        px[3] = 255;
    }
    let dst_stride = INPUT_SIZE as usize * 4;
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
    let input =
        Array4::<f32>::from_shape_vec((1, INPUT_SIZE as usize, INPUT_SIZE as usize, 3), normalized)
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
