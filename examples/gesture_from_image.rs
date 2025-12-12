#[path = "../src/gesture.rs"]
mod gesture;
#[path = "../src/model_download.rs"]
mod model_download;
#[path = "../src/recognizer/common.rs"]
mod recognizer_common;
#[allow(dead_code)]
#[path = "../src/types.rs"]
mod types;

use anyhow::{anyhow, Context, Result};
use gesture::GestureClassifier;
use std::path::PathBuf;
use types::Frame;

use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Tensor as OrtTensor,
};

type Model = Session;
type InputTensor = OrtTensor<f32>;

fn main() -> Result<()> {
    env_logger::init();

    let mut args = std::env::args().skip(1);
    let mut image_paths: Vec<PathBuf> = args.by_ref().map(PathBuf::from).collect();
    if image_paths.is_empty() {
        image_paths = demo_images()?;
    }

    if image_paths.is_empty() {
        anyhow::bail!("未找到可用的测试图片");
    }

    let model_path = model_download::default_model_path();
    model_download::ensure_model_available(&model_path)?;
    let mut model = HandposeModel::new(&model_path)?;
    let mut classifier = GestureClassifier::new();

    println!(
        "使用模型 {} 对 {} 张图片进行手势分类",
        model_path.display(),
        image_paths.len()
    );

    for path in image_paths {
        let frame = load_frame(&path)?;
        let output = model
            .infer_landmarks(&frame)
            .with_context(|| format!("无法推理 {}", path.display()))?;

        if output.confidence < 0.2 {
            println!(
                "{} -> 未检测到手 (置信度 {:.0}%)",
                path.display(),
                output.confidence * 100.0
            );
            continue;
        }

        let detail = classifier.classify(
            &output.raw_landmarks,
            &output.projected_landmarks,
            output.confidence,
            output.handedness,
            frame.timestamp,
        );

        if let Some(detail) = detail {
            let finger_summary = finger_states_to_text(&detail.finger_states);
            println!(
                "{} -> {} | {:.0}% | {} | 状态: {} | 手指: {}",
                path.display(),
                format!(
                    "{}{}",
                    detail.primary.emoji(),
                    detail.primary.display_name()
                ),
                output.confidence * 100.0,
                detail.handedness.label(),
                detail.motion.label(),
                finger_summary
            );
        } else {
            println!(
                "{} -> 检测到手，但无法稳定识别手势 ({:.0}%)",
                path.display(),
                output.confidence * 100.0
            );
        }
    }

    Ok(())
}

struct HandposeModel {
    model: Model,
}

impl HandposeModel {
    fn new(model_path: &PathBuf) -> Result<Self> {
        model_download::ensure_model_available(model_path)?;

        let model = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(2)?
            .commit_from_file(model_path)
            .with_context(|| format!("failed to load model from {}", model_path.display()))?;

        Ok(Self { model })
    }

    fn infer_landmarks(&mut self, frame: &Frame) -> Result<recognizer_common::HandposeOutput> {
        let (input, letterbox) = recognizer_common::prepare_frame(frame)?;
        let inference = run_model(&mut self.model, input)?;

        let projected = recognizer_common::project_landmarks(&inference.landmarks, &letterbox);

        Ok(recognizer_common::HandposeOutput {
            raw_landmarks: inference.landmarks,
            projected_landmarks: projected,
            confidence: inference.confidence.clamp(0.0, 1.0),
            handedness: inference.handedness,
        })
    }
}

struct InferenceResult {
    landmarks: Vec<[f32; 3]>,
    confidence: f32,
    handedness: f32,
}

fn run_model(model: &mut Model, input: ndarray::Array4<f32>) -> Result<InferenceResult> {
    let tensor: InputTensor = OrtTensor::from_array(input)?;
    let outputs = model
        .run(ort::inputs![tensor])
        .context("failed to run handpose model")?;
    decode_ort_outputs(&outputs)
}

fn decode_ort_outputs(outputs: &ort::session::SessionOutputs<'_>) -> Result<InferenceResult> {
    if outputs.len() == 0 {
        return Err(anyhow!("model returned no outputs"));
    }

    let coords = outputs[0].try_extract_array::<f32>()?;
    let flattened: Vec<f32> = coords.iter().copied().collect();
    let landmarks = recognizer_common::decode_landmarks(&flattened)?;

    let confidence = if outputs.len() > 1 {
        outputs[1]
            .try_extract_array::<f32>()
            .ok()
            .and_then(|v| v.iter().next().copied())
            .unwrap_or(0.0)
    } else {
        0.0
    };
    let handedness = if outputs.len() > 2 {
        outputs[2]
            .try_extract_array::<f32>()
            .ok()
            .and_then(|v| v.iter().next().copied())
            .unwrap_or(0.0)
    } else {
        0.0
    };

    Ok(InferenceResult {
        landmarks,
        confidence,
        handedness,
    })
}

fn load_frame(path: &PathBuf) -> Result<Frame> {
    let image = image::open(path)
        .with_context(|| format!("无法打开图片 {}", path.display()))?
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

fn demo_images() -> Result<Vec<PathBuf>> {
    let mut images = Vec::new();
    for entry in std::fs::read_dir("demo").context("读取 demo 目录失败")? {
        let entry = entry?;
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ["png", "jpg", "jpeg"]
                .iter()
                .any(|v| ext.eq_ignore_ascii_case(v))
            {
                images.push(path);
            }
        }
    }
    images.sort();
    Ok(images)
}

fn finger_states_to_text(states: &[types::FingerState; 5]) -> String {
    const NAMES: [&str; 5] = ["拇指", "食指", "中指", "无名指", "小指"];
    NAMES
        .iter()
        .zip(states.iter())
        .map(|(name, state)| format!("{name} {}", state.label()))
        .collect::<Vec<_>>()
        .join("，")
}
