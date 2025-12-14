use std::{path::PathBuf, thread};

use anyhow::{Context, Result, anyhow};
use crossbeam_channel::{Receiver, Sender};
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Tensor;

use super::{
    HandposeEngine, RecognizerBackend,
    common::{self, HandposeOutput},
    palm::{PalmDetector, PalmDetectorConfig, crop_from_palm, pick_primary_region},
    run_worker_loop,
};
use crate::{
    model_download::{ensure_handpose_estimator_model_ready, ensure_palm_detector_model_ready},
    types::{Frame, RecognizedFrame},
};

pub fn start_worker(
    backend: RecognizerBackend,
    frame_rx: Receiver<Frame>,
    result_tx: Sender<RecognizedFrame>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let handpose_estimator_model_path = backend.handpose_estimator_model_path();
        let palm_detector_model_path = backend.palm_detector_model_path();

        if let Err(err) =
            ensure_handpose_estimator_model_ready(&handpose_estimator_model_path, |_evt| {})
        {
            log::error!(
                "failed to prepare handpose model at {}: {err:?}",
                handpose_estimator_model_path.display()
            );
            return;
        }

        if let Err(err) = ensure_palm_detector_model_ready(&palm_detector_model_path, |_evt| {}) {
            log::error!(
                "failed to prepare palm detector model at {}: {err:?}",
                palm_detector_model_path.display()
            );
            return;
        }

        let engine = match OrtEngine::new(&handpose_estimator_model_path, &palm_detector_model_path)
        {
            Ok(engine) => {
                log::info!(
                    "handpose ORT backend ready using {} and palm detector {}",
                    handpose_estimator_model_path.display(),
                    palm_detector_model_path.display()
                );
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
    handpose: Session,
    palm_detector: PalmDetector,
}

impl OrtEngine {
    fn new(model_path: &PathBuf, palm_detector_model_path: &PathBuf) -> Result<Self> {
        let handpose = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(2)?
            .commit_from_file(model_path)
            .with_context(|| format!("failed to load ORT session from {}", model_path.display()))?;

        let palm_detector =
            PalmDetector::new(palm_detector_model_path, PalmDetectorConfig::default())?;

        Ok(Self {
            handpose,
            palm_detector,
        })
    }
}

impl HandposeEngine for OrtEngine {
    fn infer(&mut self, frame: &Frame) -> Result<HandposeOutput> {
        let palm_regions = self.palm_detector.detect(frame).unwrap_or_else(|err| {
            log::warn!("palm detection failed: {err:?}");
            Vec::new()
        });

        if palm_regions.is_empty() {
            return Ok(HandposeOutput {
                raw_landmarks: Vec::new(),
                projected_landmarks: Vec::new(),
                confidence: 0.0,
                handedness: 0.0,
                palm_regions,
            });
        }

        let selected = pick_primary_region(&palm_regions)
            .unwrap_or_else(|| palm_regions.get(0).expect("palm detection list not empty"));
        let (center, side, angle) = crop_from_palm(selected);

        let (input, transform) =
            common::prepare_rotated_crop(frame, center, side, angle, common::INPUT_SIZE)?;
        let tensor = Tensor::from_array(input)?;
        let outputs = self
            .handpose
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

        let projected = common::project_landmarks_with_transform(&landmarks, &transform);

        Ok(HandposeOutput {
            raw_landmarks: landmarks,
            projected_landmarks: projected,
            confidence: (confidence * selected.score).clamp(0.0, 1.0),
            handedness,
            palm_regions,
        })
    }
}
