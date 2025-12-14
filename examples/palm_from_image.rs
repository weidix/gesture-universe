#[allow(dead_code)]
#[path = "../src/pipeline/recognizer/common.rs"]
mod common;
#[allow(dead_code)]
#[path = "../src/model_download.rs"]
mod model_download;
#[allow(dead_code)]
#[path = "../src/pipeline/recognizer/palm/mod.rs"]
mod palm;
#[allow(dead_code)]
#[path = "../src/pipeline/skeleton.rs"]
mod skeleton;
#[allow(dead_code)]
#[path = "../src/types.rs"]
mod types;

use anyhow::{Context, Result, anyhow};
use image::RgbaImage;
use model_download::{default_palm_detector_model_path, ensure_palm_detector_model_ready};
use std::path::PathBuf;
use types::{Frame, PalmRegion};

use crate::palm::{PalmDetector, PalmDetectorConfig};

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
        .unwrap_or_else(|| PathBuf::from("demo/image_with_palmmarks.png"));

    let mut frame = load_frame(&input_image).context("failed to read input image")?;

    let palm_detector_model_path = default_palm_detector_model_path();
    ensure_palm_detector_model_ready(&palm_detector_model_path, |_evt| {})?;

    let mut palm_detector =
        PalmDetector::new(&palm_detector_model_path, PalmDetectorConfig::default())?;

    let palms = palm_detector.detect(&frame)?;
    if palms.is_empty() {
        println!("No palms detected in {}", input_image.display());
        return Ok(());
    }

    println!(
        "Detected {} palms in {}",
        palms.len(),
        input_image.display()
    );

    overlay(&mut frame, &palms);

    let output = RgbaImage::from_raw(frame.width, frame.height, frame.rgba)
        .ok_or_else(|| anyhow!("failed to build image buffer"))?;
    output
        .save(&output_image)
        .with_context(|| format!("failed to save {}", output_image.display()))?;
    println!("Wrote {}", output_image.display());

    Ok(())
}

fn load_frame(path: &PathBuf) -> Result<Frame> {
    let image = image::open(path)
        .with_context(|| format!("failed to open image {}", path.display()))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();
    Ok(Frame {
        rgba,
        width,
        height,
        timestamp: std::time::Instant::now(),
    })
}

fn overlay(frame: &mut Frame, palms: &[PalmRegion]) {
    skeleton::draw_palm_regions(&mut frame.rgba, frame.width, frame.height, palms);
}
