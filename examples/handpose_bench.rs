#[allow(dead_code)]
#[path = "../src/model_download.rs"]
mod model_download;

use anyhow::{Context, Result};
use image::{RgbaImage, imageops::FilterType};
use model_download::{
    default_handpose_estimator_model_path, ensure_handpose_estimator_model_ready,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use ort::{
    session::{Session, builder::GraphOptimizationLevel},
    value::Tensor as OrtTensor,
};

type Model = Session;
type InputArray = ndarray::Array4<f32>;
type InputTensor = OrtTensor<f32>;

#[derive(Clone)]
struct InferenceResult {
    confidence: f32,
}

const INPUT_SIZE: u32 = 224;

fn main() -> Result<()> {
    env_logger::init();

    let mut args = std::env::args().skip(1);
    let input_image = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("demo/ok.png"));
    let model_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(default_handpose_estimator_model_path);
    let duration_secs = args.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(1);

    let input_tensor = prepare_tensor(&input_image).context("failed to read input image")?;
    ensure_handpose_estimator_model_ready(&model_path, |_evt| {})?;
    let mut model = load_model(&model_path)?;

    println!(
        "Benchmarking model {} on {} for {}s",
        model_path.display(),
        input_image.display(),
        duration_secs
    );

    // Warm-up once to trigger any lazy initialisation.
    let warmup = infer(&mut model, input_tensor.clone())?;
    let warmup_conf = warmup.confidence;
    println!("Warm-up done (conf {:.3})", warmup_conf);

    let duration = Duration::from_secs(duration_secs.max(1));
    let start = Instant::now();
    let mut iterations: u64 = 0;
    let mut last_conf = warmup_conf;
    while start.elapsed() < duration {
        let outputs = infer(&mut model, input_tensor.clone())?;
        last_conf = outputs.confidence;
        iterations += 1;
    }
    let elapsed = start.elapsed();
    let fps = iterations as f64 / elapsed.as_secs_f64();

    println!(
        "Ran {} inferences in {:.3}s -> {:.1} fps (last conf {:.3})",
        iterations,
        elapsed.as_secs_f64(),
        fps,
        last_conf
    );

    Ok(())
}

fn load_model(model_path: &PathBuf) -> Result<Model> {
    let session = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(2)?
        .commit_from_file(model_path)
        .with_context(|| format!("failed to load model from {}", model_path.display()))?;
    Ok(session)
}

fn prepare_tensor(path: &PathBuf) -> Result<InputTensor> {
    let image = image::open(path)
        .with_context(|| format!("failed to open image {}", path.display()))?
        .to_rgba8();
    let (orig_w, orig_h) = image.dimensions();

    let scale = INPUT_SIZE as f32 / (orig_w.max(orig_h) as f32);
    let new_w = (orig_w as f32 * scale).round().max(1.0) as u32;
    let new_h = (orig_h as f32 * scale).round().max(1.0) as u32;
    let resized = image::imageops::resize(&image, new_w, new_h, FilterType::CatmullRom);

    let pad_x = ((INPUT_SIZE as i64 - new_w as i64) / 2).max(0) as u32;
    let pad_y = ((INPUT_SIZE as i64 - new_h as i64) / 2).max(0) as u32;
    let mut letterboxed =
        RgbaImage::from_pixel(INPUT_SIZE, INPUT_SIZE, image::Rgba([0, 0, 0, 255]));
    for y in 0..new_h {
        for x in 0..new_w {
            let px = *resized.get_pixel(x, y);
            letterboxed.put_pixel(x + pad_x, y + pad_y, px);
        }
    }

    let mut input = InputArray::zeros((1, INPUT_SIZE as usize, INPUT_SIZE as usize, 3));
    for y in 0..INPUT_SIZE {
        for x in 0..INPUT_SIZE {
            let pixel = letterboxed.get_pixel(x, y).0;
            input[[0, y as usize, x as usize, 0]] = pixel[0] as f32 / 255.0;
            input[[0, y as usize, x as usize, 1]] = pixel[1] as f32 / 255.0;
            input[[0, y as usize, x as usize, 2]] = pixel[2] as f32 / 255.0;
        }
    }

    array_to_input(input)
}

fn array_to_input(arr: InputArray) -> Result<InputTensor> {
    OrtTensor::from_array(arr).context("failed to build ORT tensor from input image")
}

fn infer(model: &mut Model, input: InputTensor) -> Result<InferenceResult> {
    let outputs = model.run(ort::inputs![input])?;
    decode_ort_outputs(&outputs)
}

fn decode_ort_outputs(outputs: &ort::session::SessionOutputs<'_>) -> Result<InferenceResult> {
    let confidence = if outputs.len() > 1 {
        outputs[1]
            .try_extract_array::<f32>()
            .ok()
            .and_then(|v| v.iter().next().copied())
            .unwrap_or(0.0)
    } else {
        0.0
    };

    Ok(InferenceResult { confidence })
}
