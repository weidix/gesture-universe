#[path = "../src/model_download.rs"]
mod model_download;

use model_download::{default_model_path, ensure_model_available};
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use image::{Rgba, RgbaImage, imageops::FilterType};
use tract_onnx::prelude::*;

const INPUT_SIZE: u32 = 224;
const NUM_LANDMARKS: usize = 21;
const CONNECTIONS: &[(usize, usize)] = &[
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 4),
    (0, 5),
    (5, 6),
    (6, 7),
    (7, 8),
    (0, 9),
    (9, 10),
    (10, 11),
    (11, 12),
    (0, 13),
    (13, 14),
    (14, 15),
    (15, 16),
    (0, 17),
    (17, 18),
    (18, 19),
    (19, 20),
    (5, 9),
    (9, 13),
    (13, 17),
];

struct LetterboxInfo {
    scale: f32,
    pad_x: f32,
    pad_y: f32,
    orig_w: u32,
    orig_h: u32,
}

fn main() -> Result<()> {
    env_logger::init();

    let mut args = std::env::args().skip(1);
    let input_image = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("demo/ok.png"));
    let output_image = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("demo/image_with_landmarks.png"));
    let model_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(default_model_path);

    let (input_tensor, mut canvas, letterbox) =
        prepare_image(&input_image).context("failed to read input image")?;
    ensure_model_available(&model_path)?;
    let model = load_model(&model_path)?;

    println!(
        "Running inference with model {} on {}",
        model_path.display(),
        input_image.display()
    );
    let outputs = model
        .run(tvec![input_tensor.into()])
        .context("inference failed")?;

    let (landmarks, confidence, handedness) = decode_outputs(&outputs)?;
    println!(
        "Model returned confidence {:.3} (handedness {:.3}, 1.0 = right hand)",
        confidence, handedness
    );

    let projected = project_landmarks(&landmarks, &letterbox);
    draw_skeleton(&mut canvas, &projected);
    canvas
        .save(&output_image)
        .with_context(|| format!("failed to save {}", output_image.display()))?;

    println!("Wrote {}", output_image.display());
    Ok(())
}

fn load_model(model_path: &PathBuf) -> TractResult<TypedRunnableModel<TypedModel>> {
    let mut model = tract_onnx::onnx().model_for_path(model_path)?;
    model.set_input_fact(
        0,
        InferenceFact::dt_shape(
            f32::datum_type(),
            tvec![
                1.to_dim(),
                (INPUT_SIZE as usize).to_dim(),
                (INPUT_SIZE as usize).to_dim(),
                3.to_dim()
            ],
        ),
    )?;

    model.into_optimized()?.into_runnable()
}

fn prepare_image(path: &PathBuf) -> Result<(Tensor, RgbaImage, LetterboxInfo)> {
    let image = image::open(path)
        .with_context(|| format!("failed to open image {}", path.display()))?
        .to_rgba8();
    let (orig_w, orig_h) = image.dimensions();

    let scale = INPUT_SIZE as f32 / (orig_w.max(orig_h) as f32);
    let new_w = (orig_w as f32 * scale).round().max(1.0) as u32;
    let new_h = (orig_h as f32 * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(&image, new_w, new_h, FilterType::CatmullRom);

    let pad_x = ((INPUT_SIZE as i64 - new_w as i64) / 2).max(0) as f32;
    let pad_y = ((INPUT_SIZE as i64 - new_h as i64) / 2).max(0) as f32;
    let mut letterboxed = RgbaImage::from_pixel(INPUT_SIZE, INPUT_SIZE, Rgba([0, 0, 0, 255]));
    for y in 0..new_h {
        for x in 0..new_w {
            let px = *resized.get_pixel(x, y);
            let lx = (x as f32 + pad_x).round() as u32;
            let ly = (y as f32 + pad_y).round() as u32;
            if lx < letterboxed.width() && ly < letterboxed.height() {
                letterboxed.put_pixel(lx, ly, px);
            }
        }
    }

    let mut input =
        tract_ndarray::Array4::<f32>::zeros((1, INPUT_SIZE as usize, INPUT_SIZE as usize, 3));
    for y in 0..INPUT_SIZE {
        for x in 0..INPUT_SIZE {
            let pixel = letterboxed.get_pixel(x, y).0;
            input[[0, y as usize, x as usize, 0]] = pixel[0] as f32 / 255.0;
            input[[0, y as usize, x as usize, 1]] = pixel[1] as f32 / 255.0;
            input[[0, y as usize, x as usize, 2]] = pixel[2] as f32 / 255.0;
        }
    }

    let letterbox = LetterboxInfo {
        scale,
        pad_x,
        pad_y,
        orig_w,
        orig_h,
    };

    Ok((input.into_tensor(), image, letterbox))
}

fn decode_outputs(outputs: &[TValue]) -> Result<(Vec<[f32; 3]>, f32, f32)> {
    if outputs.is_empty() {
        return Err(anyhow!("model returned no outputs"));
    }

    let coords = outputs[0].to_array_view::<f32>()?;
    let coords = coords
        .to_shape((NUM_LANDMARKS, 3))
        .map_err(|_| anyhow!("unexpected landmarks shape"))?;
    let mut landmarks = Vec::with_capacity(NUM_LANDMARKS);
    for row in coords.outer_iter() {
        landmarks.push([row[0], row[1], row[2]]);
    }

    let confidence = outputs
        .get(1)
        .and_then(|t| t.to_array_view::<f32>().ok())
        .and_then(|v| v.iter().next().copied())
        .unwrap_or(0.0);
    let handedness = outputs
        .get(2)
        .and_then(|t| t.to_array_view::<f32>().ok())
        .and_then(|v| v.iter().next().copied())
        .unwrap_or(0.0);

    Ok((landmarks, confidence, handedness))
}

fn project_landmarks(landmarks: &[[f32; 3]], letterbox: &LetterboxInfo) -> Vec<(f32, f32)> {
    landmarks
        .iter()
        .map(|[x, y, _z]| {
            let px = (x - letterbox.pad_x) / letterbox.scale;
            let py = (y - letterbox.pad_y) / letterbox.scale;
            let clamped_x = px.clamp(0.0, (letterbox.orig_w.saturating_sub(1)) as f32);
            let clamped_y = py.clamp(0.0, (letterbox.orig_h.saturating_sub(1)) as f32);
            (clamped_x, clamped_y)
        })
        .collect()
}

fn draw_skeleton(image: &mut RgbaImage, points: &[(f32, f32)]) {
    let line_color = Rgba([255, 142, 82, 255]);
    for &(a, b) in CONNECTIONS {
        if let (Some(pa), Some(pb)) = (points.get(a), points.get(b)) {
            draw_line(image, pa, pb, line_color);
        }
    }

    let point_color = Rgba([56, 163, 255, 255]);
    for &(x, y) in points {
        draw_circle(image, (x as i32, y as i32), 3, point_color);
    }
}

fn draw_line(image: &mut RgbaImage, p0: &(f32, f32), p1: &(f32, f32), color: Rgba<u8>) {
    let (mut x0, mut y0) = (p0.0 as i32, p0.1 as i32);
    let (x1, y1) = (p1.0 as i32, p1.1 as i32);
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        put_pixel_safe(image, x0, y0, color);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_circle(image: &mut RgbaImage, center: (i32, i32), radius: i32, color: Rgba<u8>) {
    let (cx, cy) = center;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= radius * radius {
                put_pixel_safe(image, cx + dx, cy + dy, color);
            }
        }
    }
}

fn put_pixel_safe(image: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if x < 0 || y < 0 {
        return;
    }
    let (ux, uy) = (x as u32, y as u32);
    if ux < image.width() && uy < image.height() {
        image.put_pixel(ux, uy, color);
    }
}
