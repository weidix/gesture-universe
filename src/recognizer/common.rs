use anyhow::{Result, anyhow};
use image::{RgbaImage, imageops::FilterType};
use ndarray::Array4;

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
    let Some(img) = RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone()) else {
        return Err(anyhow!("failed to build RGBA image from frame"));
    };

    let scale = INPUT_SIZE as f32 / (frame.width.max(frame.height) as f32);
    let new_w = (frame.width as f32 * scale).round().max(1.0) as u32;
    let new_h = (frame.height as f32 * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(&img, new_w, new_h, FilterType::CatmullRom);

    let pad_x = ((INPUT_SIZE as i64 - new_w as i64) / 2).max(0) as f32;
    let pad_y = ((INPUT_SIZE as i64 - new_h as i64) / 2).max(0) as f32;
    let mut canvas =
        RgbaImage::from_pixel(INPUT_SIZE, INPUT_SIZE, image::Rgba([0u8, 0u8, 0u8, 255u8]));
    for y in 0..new_h {
        for x in 0..new_w {
            let px = *resized.get_pixel(x, y);
            let lx = (x as f32 + pad_x).round() as u32;
            let ly = (y as f32 + pad_y).round() as u32;
            if lx < canvas.width() && ly < canvas.height() {
                canvas.put_pixel(lx, ly, px);
            }
        }
    }

    let mut input = Array4::<f32>::zeros((1, INPUT_SIZE as usize, INPUT_SIZE as usize, 3));
    for y in 0..INPUT_SIZE {
        for x in 0..INPUT_SIZE {
            let pixel = canvas.get_pixel(x, y).0;
            input[[0, y as usize, x as usize, 0]] = pixel[0] as f32 / 255.0;
            input[[0, y as usize, x as usize, 1]] = pixel[1] as f32 / 255.0;
            input[[0, y as usize, x as usize, 2]] = pixel[2] as f32 / 255.0;
        }
    }

    let letterbox = LetterboxInfo {
        scale,
        pad_x,
        pad_y,
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
