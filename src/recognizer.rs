use std::{thread, time::Instant};

#[cfg(feature = "handpose-tract")]
use std::path::PathBuf;

use crossbeam_channel::{Receiver, Sender};

#[cfg(feature = "handpose-tract")]
use {
    anyhow::{Context, anyhow},
    image::{RgbaImage, imageops::FilterType},
    tract_onnx::prelude::*,
};

#[cfg(feature = "handpose-tract")]
use crate::model_download::{default_model_path, ensure_model_available};
use crate::types::{Frame, GestureResult};

#[cfg(feature = "handpose-tract")]
const INPUT_SIZE: u32 = 224;

#[cfg(feature = "handpose-tract")]
const NUM_LANDMARKS: usize = 21;

#[derive(Clone, Debug)]
pub enum RecognizerBackend {
    Placeholder,
    #[cfg(feature = "handpose-tract")]
    HandposeTract {
        model_path: PathBuf,
    },
}

impl Default for RecognizerBackend {
    fn default() -> Self {
        #[cfg(feature = "handpose-tract")]
        {
            return RecognizerBackend::HandposeTract {
                model_path: default_model_path(),
            };
        }

        #[allow(unreachable_code)]
        RecognizerBackend::Placeholder
    }
}

pub fn start_recognizer(
    backend: RecognizerBackend,
    frame_rx: Receiver<Frame>,
    result_tx: Sender<GestureResult>,
) -> thread::JoinHandle<()> {
    match backend {
        RecognizerBackend::Placeholder => start_placeholder_worker(frame_rx, result_tx),
        #[cfg(feature = "handpose-tract")]
        RecognizerBackend::HandposeTract { model_path } => {
            start_tract_worker(model_path, frame_rx, result_tx)
        }
    }
}

fn start_placeholder_worker(
    frame_rx: Receiver<Frame>,
    result_tx: Sender<GestureResult>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while let Some(frame) = recv_latest_frame(&frame_rx) {
            let gesture = classify_brightness(&frame);
            let _ = result_tx.try_send(gesture);
        }
    })
}

#[cfg(feature = "handpose-tract")]
fn start_tract_worker(
    model_path: PathBuf,
    frame_rx: Receiver<Frame>,
    result_tx: Sender<GestureResult>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        if let Err(err) = ensure_model_available(&model_path) {
            log::error!(
                "failed to prepare handpose model at {}: {err:?}",
                model_path.display()
            );
            return;
        }

        let model = match HandposeModel::new(&model_path) {
            Ok(model) => {
                log::info!(
                    "handpose tract backend ready ({} nodes) using {}",
                    model.node_count,
                    model_path.display()
                );
                model
            }
            Err(err) => {
                log::error!("failed to load handpose model: {err:?}");
                return;
            }
        };

        while let Some(frame) = recv_latest_frame(&frame_rx) {
            match model.infer(&frame) {
                Ok(gesture) => {
                    let _ = result_tx.try_send(gesture);
                }
                Err(err) => {
                    log::warn!("handpose inference failed: {err:?}");
                }
            }
        }
    })
}

fn recv_latest_frame(frame_rx: &Receiver<Frame>) -> Option<Frame> {
    let mut frame = frame_rx.recv().ok()?;
    // Drop stale frames if the recognizer is still busy to avoid backlog.
    while let Ok(newer) = frame_rx.try_recv() {
        frame = newer;
    }
    Some(frame)
}

fn classify_brightness(frame: &Frame) -> GestureResult {
    let avg_brightness = frame
        .rgba
        .chunks(4)
        .map(|px| {
            let r = px.get(0).copied().unwrap_or(0) as f32;
            let g = px.get(1).copied().unwrap_or(0) as f32;
            let b = px.get(2).copied().unwrap_or(0) as f32;
            (r + g + b) / 3.0
        })
        .sum::<f32>()
        / ((frame.rgba.len() / 4).max(1) as f32);

    let label = if avg_brightness > 100.0 {
        "Hand/bright motion"
    } else {
        "Low motion"
    };

    let confidence = (avg_brightness / 255.0).clamp(0.0, 1.0);

    GestureResult {
        label: label.to_string(),
        confidence,
        timestamp: Instant::now(),
        landmarks: None,
    }
}

#[cfg(feature = "handpose-tract")]
struct HandposeModel {
    model: TypedRunnableModel<TypedModel>,
    node_count: usize,
}

#[cfg(feature = "handpose-tract")]
impl HandposeModel {
    fn new(model_path: &PathBuf) -> TractResult<Self> {
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

        let node_count = model.nodes().len();
        let model = model.into_optimized()?.into_runnable()?;

        Ok(Self { model, node_count })
    }

    fn infer(&self, frame: &Frame) -> TractResult<GestureResult> {
        let (input, letterbox) = prepare_frame(frame)?;
        let outputs = self
            .model
            .run(tvec![input.into()])
            .context("failed to run handpose model")?;
        let (landmarks, confidence, handedness) = decode_outputs(&outputs)?;
        let projected = project_landmarks(&landmarks, &letterbox);

        let has_detection = confidence >= 0.2;
        let label = if has_detection {
            let hand = if handedness >= 0.5 { "Right" } else { "Left" };
            format!("{hand} hand")
        } else {
            "No hand detected".to_string()
        };

        Ok(GestureResult {
            label,
            confidence: confidence.clamp(0.0, 1.0),
            timestamp: Instant::now(),
            landmarks: if has_detection { Some(projected) } else { None },
        })
    }
}

#[cfg(feature = "handpose-tract")]
#[derive(Clone, Debug)]
struct LetterboxInfo {
    scale: f32,
    pad_x: f32,
    pad_y: f32,
    orig_w: u32,
    orig_h: u32,
}

#[cfg(feature = "handpose-tract")]
fn prepare_frame(frame: &Frame) -> TractResult<(Tensor, LetterboxInfo)> {
    let Some(img) = RgbaImage::from_raw(frame.width, frame.height, frame.rgba.clone()) else {
        return Err(anyhow!("failed to build RGBA image from frame").into());
    };

    // Letterbox to 224x224 while keeping aspect ratio.
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

    let mut input =
        tract_ndarray::Array4::<f32>::zeros((1, INPUT_SIZE as usize, INPUT_SIZE as usize, 3));
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

    Ok((input.into_tensor(), letterbox))
}

#[cfg(feature = "handpose-tract")]
fn decode_outputs(outputs: &[TValue]) -> TractResult<(Vec<[f32; 3]>, f32, f32)> {
    if outputs.is_empty() {
        return Err(anyhow!("model returned no outputs").into());
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

#[cfg(feature = "handpose-tract")]
fn project_landmarks(landmarks: &[[f32; 3]], letterbox: &LetterboxInfo) -> Vec<(f32, f32)> {
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
