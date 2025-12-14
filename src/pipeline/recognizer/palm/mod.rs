mod anchors;

use std::{cmp::Ordering, f32::consts::PI, path::PathBuf};

use anchors::{ANCHORS, NUM_ANCHORS};
use anyhow::{Context, Result, anyhow};
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Tensor;

use crate::types::{Frame, PalmRegion};

use super::common::{LetterboxInfo, PALM_INPUT_SIZE, prepare_frame_with_size};

const PALM_LANDMARKS: usize = 7;

#[derive(Clone, Debug)]
pub struct PalmDetectorConfig {
    pub score_threshold: f32,
    pub nms_threshold: f32,
    pub top_k: usize,
}

impl Default for PalmDetectorConfig {
    fn default() -> Self {
        Self {
            score_threshold: 0.5,
            nms_threshold: 0.3,
            top_k: 32,
        }
    }
}

pub struct PalmDetector {
    session: Session,
    cfg: PalmDetectorConfig,
}

impl PalmDetector {
    pub fn new(model_path: &PathBuf, cfg: PalmDetectorConfig) -> Result<Self> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(2)?
            .commit_from_file(model_path)
            .with_context(|| {
                format!("failed to load palm detector from {}", model_path.display())
            })?;

        Ok(Self { session, cfg })
    }

    pub fn detect(&mut self, frame: &Frame) -> Result<Vec<PalmRegion>> {
        let (input, letterbox) = prepare_frame_with_size(frame, PALM_INPUT_SIZE)?;
        let tensor = Tensor::from_array(input)?;

        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .context("failed to run palm detector session")?;

        if outputs.len() < 2 {
            return Err(anyhow!(
                "palm detector returned {} outputs, expected at least 2",
                outputs.len()
            ));
        }

        let box_and_landmarks = outputs[0].try_extract_array::<f32>()?;
        let scores = outputs[1].try_extract_array::<f32>()?;

        let box_shape = box_and_landmarks.shape().to_vec();
        let score_shape = scores.shape().to_vec();

        let decoded = decode_palm_outputs(
            box_and_landmarks
                .as_slice()
                .ok_or_else(|| anyhow!("palm boxes not contiguous"))?,
            &box_shape,
            scores
                .as_slice()
                .ok_or_else(|| anyhow!("palm scores not contiguous"))?,
            &score_shape,
            &letterbox,
            &self.cfg,
        )?;

        Ok(decoded)
    }
}

fn decode_palm_outputs(
    box_landmark: &[f32],
    box_shape: &[usize],
    scores: &[f32],
    score_shape: &[usize],
    letterbox: &LetterboxInfo,
    cfg: &PalmDetectorConfig,
) -> Result<Vec<PalmRegion>> {
    if box_shape.len() < 3 {
        return Err(anyhow!(
            "unexpected palm box shape {:?}, need [batch, anchors, features]",
            box_shape
        ));
    }
    if score_shape.len() < 3 {
        return Err(anyhow!(
            "unexpected palm score shape {:?}, need [batch, anchors, 1]",
            score_shape
        ));
    }

    let anchor_dim = *box_shape
        .get(box_shape.len().saturating_sub(2))
        .ok_or_else(|| anyhow!("missing anchor dimension in palm box shape"))?;
    let feature_dim = *box_shape
        .last()
        .ok_or_else(|| anyhow!("missing feature dimension in palm box shape"))?;

    let score_anchor_dim = *score_shape
        .get(score_shape.len().saturating_sub(2))
        .ok_or_else(|| anyhow!("missing anchor dimension in palm score shape"))?;
    let score_feature_dim = *score_shape
        .last()
        .ok_or_else(|| anyhow!("missing feature dimension in palm score shape"))?;

    if feature_dim < 4 + PALM_LANDMARKS * 2 {
        return Err(anyhow!(
            "palm box feature dimension too small: {feature_dim}"
        ));
    }

    if anchor_dim != score_anchor_dim {
        return Err(anyhow!(
            "anchor dimension mismatch between boxes ({anchor_dim}) and scores ({score_anchor_dim})"
        ));
    }

    let anchors = NUM_ANCHORS.min(anchor_dim);
    let pad_bias_x = letterbox.pad_x / letterbox.scale;
    let pad_bias_y = letterbox.pad_y / letterbox.scale;
    let scale = letterbox.orig_w.max(letterbox.orig_h) as f32;
    let target_input = PALM_INPUT_SIZE as f32;

    let mut candidates = Vec::new();
    for anchor_idx in 0..anchors {
        let score_offset = anchor_idx
            .checked_mul(score_feature_dim)
            .ok_or_else(|| anyhow!("palm score offset overflow"))?;
        let raw_score = *scores
            .get(score_offset)
            .ok_or_else(|| anyhow!("missing score for palm anchor {anchor_idx}"))?;
        let score = sigmoid(raw_score);
        if score < cfg.score_threshold {
            continue;
        }

        let feature_offset = anchor_idx
            .checked_mul(feature_dim)
            .ok_or_else(|| anyhow!("palm feature offset overflow"))?;
        let anchor = ANCHORS
            .get(anchor_idx)
            .copied()
            .ok_or_else(|| anyhow!("missing anchor {anchor_idx}"))?;

        let cx_delta = *box_landmark
            .get(feature_offset)
            .ok_or_else(|| anyhow!("missing cx for anchor {anchor_idx}"))?
            / target_input;
        let cy_delta = *box_landmark
            .get(feature_offset + 1)
            .ok_or_else(|| anyhow!("missing cy for anchor {anchor_idx}"))?
            / target_input;
        let w_delta = *box_landmark
            .get(feature_offset + 2)
            .ok_or_else(|| anyhow!("missing w for anchor {anchor_idx}"))?
            / target_input;
        let h_delta = *box_landmark
            .get(feature_offset + 3)
            .ok_or_else(|| anyhow!("missing h for anchor {anchor_idx}"))?
            / target_input;

        let cx = cx_delta + anchor[0];
        let cy = cy_delta + anchor[1];
        let hw = w_delta / 2.0;
        let hh = h_delta / 2.0;

        let mut x1 = (cx - hw) * scale - pad_bias_x;
        let mut y1 = (cy - hh) * scale - pad_bias_y;
        let mut x2 = (cx + hw) * scale - pad_bias_x;
        let mut y2 = (cy + hh) * scale - pad_bias_y;

        if x2 <= x1 || y2 <= y1 {
            continue;
        }

        clamp_box(
            &mut x1,
            &mut y1,
            &mut x2,
            &mut y2,
            letterbox.orig_w,
            letterbox.orig_h,
        );

        let mut landmarks = Vec::with_capacity(PALM_LANDMARKS);
        for l in 0..PALM_LANDMARKS {
            let lx = *box_landmark
                .get(feature_offset + 4 + l * 2)
                .ok_or_else(|| anyhow!("missing palm landmark x for {anchor_idx}:{l}"))?
                / target_input;
            let ly = *box_landmark
                .get(feature_offset + 4 + l * 2 + 1)
                .ok_or_else(|| anyhow!("missing palm landmark y for {anchor_idx}:{l}"))?
                / target_input;
            let gx = (lx + anchor[0]) * scale - pad_bias_x;
            let gy = (ly + anchor[1]) * scale - pad_bias_y;
            landmarks.push((gx, gy));
        }

        candidates.push(PalmCandidate {
            bbox: [x1, y1, x2, y2],
            landmarks,
            score,
        });
    }

    let kept = nms(&candidates, cfg.nms_threshold, cfg.top_k);
    let mut detections = Vec::with_capacity(kept.len());
    for idx in kept {
        if let Some(c) = candidates.get(idx) {
            detections.push(PalmRegion {
                bbox: c.bbox,
                landmarks: c.landmarks.clone(),
                score: c.score,
            });
        }
    }

    Ok(detections)
}

pub fn pick_primary_region<'a>(regions: &'a [PalmRegion]) -> Option<&'a PalmRegion> {
    regions
        .iter()
        .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(Ordering::Equal))
}

pub fn crop_from_palm(region: &PalmRegion) -> ((f32, f32), f32, f32) {
    let center = if region.landmarks.is_empty() {
        (
            (region.bbox[0] + region.bbox[2]) * 0.5,
            (region.bbox[1] + region.bbox[3]) * 0.5,
        )
    } else {
        let (sum_x, sum_y) = region
            .landmarks
            .iter()
            .fold((0.0_f32, 0.0_f32), |acc, p| (acc.0 + p.0, acc.1 + p.1));
        (
            sum_x / region.landmarks.len() as f32,
            sum_y / region.landmarks.len() as f32,
        )
    };

    let base_w = (region.bbox[2] - region.bbox[0]).abs();
    let base_h = (region.bbox[3] - region.bbox[1]).abs();
    let landmark_span = if region.landmarks.is_empty() {
        0.0
    } else {
        let (min_x, max_x, min_y, max_y) = region
            .landmarks
            .iter()
            .fold((f32::MAX, f32::MIN, f32::MAX, f32::MIN), |acc, (x, y)| {
                (acc.0.min(*x), acc.1.max(*x), acc.2.min(*y), acc.3.max(*y))
            });
        (max_x - min_x).max(max_y - min_y)
    };
    // Expand generously to avoid cropping fingers away.
    let side = base_w.max(base_h).max(landmark_span).max(80.0) * 2.4;

    let angle = estimate_orientation(region);

    (center, side, angle)
}

pub fn estimate_orientation(region: &PalmRegion) -> f32 {
    if region.landmarks.len() < 2 {
        return 0.0;
    }

    // Principal direction via simple 2x2 covariance eigvec
    let (cx, cy) = region
        .landmarks
        .iter()
        .fold((0.0_f32, 0.0_f32), |acc, (x, y)| (acc.0 + x, acc.1 + y));
    let mean = (
        cx / region.landmarks.len() as f32,
        cy / region.landmarks.len() as f32,
    );

    let mut cov_xx = 0.0;
    let mut cov_xy = 0.0;
    let mut cov_yy = 0.0;
    for (x, y) in &region.landmarks {
        let dx = x - mean.0;
        let dy = y - mean.1;
        cov_xx += dx * dx;
        cov_xy += dx * dy;
        cov_yy += dy * dy;
    }
    cov_xx /= region.landmarks.len() as f32;
    cov_xy /= region.landmarks.len() as f32;
    cov_yy /= region.landmarks.len() as f32;

    let trace = cov_xx + cov_yy;
    let det = cov_xx * cov_yy - cov_xy * cov_xy;
    let lambda1 = (trace * 0.5 + ((trace * 0.5).powi(2) - det).max(0.0).sqrt()).max(1e-6);
    let (vx, vy) = if cov_xy.abs() > 1e-6 {
        (lambda1 - cov_yy, cov_xy)
    } else if cov_xx >= cov_yy {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };

    let angle = vy.atan2(vx);
    // Rotate palm to face upwards (roughly) to help downstream model
    angle - PI * 0.5
}

#[derive(Clone, Debug)]
struct PalmCandidate {
    bbox: [f32; 4],
    landmarks: Vec<(f32, f32)>,
    score: f32,
}

fn nms(candidates: &[PalmCandidate], threshold: f32, top_k: usize) -> Vec<usize> {
    let mut order: Vec<usize> = candidates.iter().enumerate().map(|(i, _)| i).collect();
    order.sort_by(|a, b| {
        candidates[*b]
            .score
            .partial_cmp(&candidates[*a].score)
            .unwrap_or(Ordering::Equal)
    });

    let mut keep: Vec<usize> = Vec::new();
    'outer: for &idx in &order {
        for &k in &keep {
            if iou(&candidates[idx].bbox, &candidates[k].bbox) >= threshold {
                continue 'outer;
            }
        }
        keep.push(idx);
        if keep.len() >= top_k {
            break;
        }
    }
    keep
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = a[2].min(b[2]);
    let y2 = a[3].min(b[3]);

    let inter_w = (x2 - x1).max(0.0);
    let inter_h = (y2 - y1).max(0.0);
    let inter = inter_w * inter_h;
    if inter <= 0.0 {
        return 0.0;
    }

    let area_a = (a[2] - a[0]).max(0.0) * (a[3] - a[1]).max(0.0);
    let area_b = (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0);
    let union = area_a + area_b - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn clamp_box(x1: &mut f32, y1: &mut f32, x2: &mut f32, y2: &mut f32, w: u32, h: u32) {
    let max_w = (w.saturating_sub(1)) as f32;
    let max_h = (h.saturating_sub(1)) as f32;
    *x1 = x1.clamp(0.0, max_w);
    *y1 = y1.clamp(0.0, max_h);
    *x2 = x2.clamp(0.0, max_w);
    *y2 = y2.clamp(0.0, max_h);
}
