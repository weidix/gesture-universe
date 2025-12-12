use std::{path::PathBuf, thread};

use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{Receiver, Sender};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;

use super::{
    HandposeEngine,
    common::{self, HandposeOutput},
    run_worker_loop,
};
use crate::{
    model_download::ensure_model_available,
    types::{Frame, GestureResult},
};

pub fn start_worker(
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

        let engine = match OrtEngine::new(&model_path) {
            Ok(engine) => {
                log::info!("handpose ORT backend ready using {}", model_path.display());
                engine
            }
            Err(err) => {
                log::error!("failed to load ORT handpose model: {err:?}");
                return;
            }
        };

        run_worker_loop(engine, frame_rx, result_tx);
    })
}

struct OrtEngine {
    session: Session,
}

impl OrtEngine {
    fn new(model_path: &PathBuf) -> Result<Self> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(2)?
            .commit_from_file(model_path)
            .with_context(|| format!("failed to load ORT session from {}", model_path.display()))?;

        Ok(Self { session })
    }
}

impl HandposeEngine for OrtEngine {
    fn infer(&mut self, frame: &Frame) -> Result<HandposeOutput> {
        let (input, letterbox) = common::prepare_frame(frame)?;
        let tensor = Tensor::from_array(input)?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .context("failed to run ORT session")?;

        if outputs.len() < 1 {
            return Err(anyhow!("model returned no outputs"));
        }

        let coords = outputs[0].try_extract_array::<f32>()?;
        let flattened: Vec<f32> = coords.iter().copied().collect();
        let landmarks = common::decode_landmarks(&flattened)?;

        let confidence = if outputs.len() > 1 {
            outputs[1]
                .try_extract_array::<f32>()
                .ok()
                .and_then(|arr| arr.iter().next().copied())
                .unwrap_or(0.0)
        } else {
            0.0
        };
        let handedness = if outputs.len() > 2 {
            outputs[2]
                .try_extract_array::<f32>()
                .ok()
                .and_then(|arr| arr.iter().next().copied())
                .unwrap_or(0.0)
        } else {
            0.0
        };

        let projected = common::project_landmarks(&landmarks, &letterbox);

        Ok(HandposeOutput {
            raw_landmarks: landmarks,
            projected_landmarks: projected,
            confidence: confidence.clamp(0.0, 1.0),
            handedness,
        })
    }
}
