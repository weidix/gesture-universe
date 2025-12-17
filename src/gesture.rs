use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use crate::{
    model_download::{
        default_gesture_classifier_model_path, ensure_gesture_classifier_model_ready,
    },
    types::{FingerState, GestureDetail, GestureKind, GestureMotion, Handedness},
};
use ndarray::Array2;
use ort::session::Session;

const MIN_CONFIDENCE: f32 = 0.2;
const MOTION_WINDOW: Duration = Duration::from_millis(1_200);

pub struct GestureClassifier {
    motion_tracker: MotionTracker,
    model_session: Option<Session>,
    class_to_gesture: HashMap<usize, GestureKind>,
}

impl GestureClassifier {
    pub fn new() -> Self {
        let (model_session, class_to_gesture) = Self::load_model_and_classes();

        if model_session.is_none() {
            log::warn!(
                "Failed to load gesture classification model, will use Unknown for all gestures"
            );
        }

        Self {
            motion_tracker: MotionTracker::new(),
            model_session,
            class_to_gesture,
        }
    }

    fn load_model_and_classes() -> (Option<Session>, HashMap<usize, GestureKind>) {
        let model_path = default_gesture_classifier_model_path();

        // Ensure model is downloaded
        if let Err(e) = ensure_gesture_classifier_model_ready(&model_path, |_evt| {}) {
            log::error!("Failed to prepare gesture classifier model: {}", e);
            return (None, HashMap::new());
        }

        // Load ONNX model
        let session = match Session::builder() {
            Ok(builder) => match builder.commit_from_file(&model_path) {
                Ok(session) => {
                    log::info!(
                        "Loaded gesture classification model from {}",
                        model_path.display()
                    );
                    Some(session)
                }
                Err(e) => {
                    log::error!(
                        "Failed to load gesture model from {}: {}",
                        model_path.display(),
                        e
                    );
                    None
                }
            },
            Err(e) => {
                log::error!("Failed to create ONNX session builder: {}", e);
                None
            }
        };

        // Hardcoded class mapping based on HAGRID dataset classes order
        // Order: call, dislike, fist, four, grabbing, grip, hand_heart, hand_heart2, holy, like,
        //        little_finger, middle_finger, mute, no_gesture, ok, one, palm, peace, peace_inverted,
        //        point, rock, stop, stop_inverted, take_picture, three, three2, three3, three_gun,
        //        thumb_index, thumb_index2, timeout, two_up, two_up_inverted, xsign
        let class_to_gesture: HashMap<usize, GestureKind> = [
            (0, GestureKind::Call),
            (1, GestureKind::Dislike),
            (2, GestureKind::Fist),
            (3, GestureKind::Four),
            (4, GestureKind::Grabbing),
            (5, GestureKind::Grip),
            (6, GestureKind::HandHeart),
            (7, GestureKind::HandHeart2),
            (8, GestureKind::Holy),
            (9, GestureKind::Like),
            (10, GestureKind::LittleFinger),
            (11, GestureKind::MiddleFinger),
            (12, GestureKind::Mute),
            (13, GestureKind::NoGesture),
            (14, GestureKind::Ok),
            (15, GestureKind::One),
            (16, GestureKind::Palm),
            (17, GestureKind::Peace),
            (18, GestureKind::PeaceInverted),
            (19, GestureKind::Point),
            (20, GestureKind::Rock),
            (21, GestureKind::Stop),
            (22, GestureKind::StopInverted),
            (23, GestureKind::TakePicture),
            (24, GestureKind::Three),
            (25, GestureKind::Three2),
            (26, GestureKind::Three3),
            (27, GestureKind::ThreeGun),
            (28, GestureKind::ThumbIndex),
            (29, GestureKind::ThumbIndex2),
            (30, GestureKind::Timeout),
            (31, GestureKind::TwoUp),
            (32, GestureKind::TwoUpInverted),
            (33, GestureKind::XSign),
        ]
        .iter()
        .copied()
        .collect();

        (session, class_to_gesture)
    }

    pub fn classify(
        &mut self,
        raw_landmarks: &[[f32; 3]],
        projected_landmarks: &[(f32, f32)],
        confidence: f32,
        handedness_score: f32,
        timestamp: Instant,
    ) -> Option<GestureDetail> {
        if confidence < MIN_CONFIDENCE {
            return None;
        }
        if raw_landmarks.len() < 21 || projected_landmarks.len() < 21 {
            return None;
        }

        // Keep the existing normalization for finger state detection
        let (normalized, _hand_span) = normalize_landmarks(raw_landmarks);
        let wrist_px = projected_landmarks.get(0).copied().unwrap_or((0.0, 0.0));
        let span_px = projected_span(projected_landmarks);
        let finger_states = [
            classify_thumb(&normalized),
            classify_finger(&normalized, [5, 6, 7, 8]),
            classify_finger(&normalized, [9, 10, 11, 12]),
            classify_finger(&normalized, [13, 14, 15, 16]),
            classify_finger(&normalized, [17, 18, 19, 20]),
        ];

        let handedness = handedness_from_score(handedness_score);

        // Use ONNX model for primary gesture detection
        let primary = self.detect_gesture_with_model(raw_landmarks);

        let motion = self
            .motion_tracker
            .update(wrist_px, span_px, timestamp, primary);

        Some(GestureDetail {
            primary,
            secondary: None, // No longer using secondary detection
            handedness,
            finger_states,
            motion,
        })
    }

    /// Normalize landmarks for ONNX model input (matching training normalization)
    fn normalize_for_model(landmarks: &[[f32; 3]]) -> Option<Vec<f32>> {
        if landmarks.len() != 21 {
            return None;
        }

        // Extract only x, y coordinates (drop z)
        let mut pts: Vec<[f32; 2]> = landmarks.iter().map(|p| [p[0], p[1]]).collect();

        // Translate to wrist (point 0) as origin
        let wrist = pts[0];
        for pt in pts.iter_mut() {
            pt[0] -= wrist[0];
            pt[1] -= wrist[1];
        }

        // Calculate palm width (distance between points 5 and 17)
        let palm_width = {
            let dx = pts[5][0] - pts[17][0];
            let dy = pts[5][1] - pts[17][1];
            (dx * dx + dy * dy).sqrt()
        };

        // If palm width is too small, try using middle finger base (point 9) distance from wrist
        let scale = if palm_width > 1e-6 {
            palm_width
        } else {
            let dx = pts[9][0];
            let dy = pts[9][1];
            (dx * dx + dy * dy).sqrt()
        };

        if scale <= 1e-6 {
            return None;
        }

        // Scale by palm width
        for pt in pts.iter_mut() {
            pt[0] /= scale;
            pt[1] /= scale;
        }

        // Flatten to 42-dimensional vector [x0, y0, x1, y1, ..., x20, y20]
        let mut result = Vec::with_capacity(42);
        for pt in pts {
            result.push(pt[0]);
            result.push(pt[1]);
        }

        Some(result)
    }

    fn detect_gesture_with_model(&mut self, raw_landmarks: &[[f32; 3]]) -> GestureKind {
        let session = match &mut self.model_session {
            Some(s) => s,
            None => return GestureKind::Unknown,
        };

        // Normalize landmarks for model input
        let input_vec = match Self::normalize_for_model(raw_landmarks) {
            Some(v) => v,
            None => return GestureKind::Unknown,
        };

        // Create ndarray input (1, 42) shape
        let input_array = match Array2::from_shape_vec((1, 42), input_vec) {
            Ok(arr) => arr,
            Err(_) => return GestureKind::Unknown,
        };

        // Create tensor from array
        use ort::value::Tensor;
        let tensor = match Tensor::from_array(input_array) {
            Ok(t) => t,
            Err(_) => return GestureKind::Unknown,
        };

        // Run model inference
        let outputs = match session.run(ort::inputs![tensor]) {
            Ok(outputs) => outputs,
            Err(e) => {
                log::warn!("Model inference failed: {}", e);
                return GestureKind::Unknown;
            }
        };

        // Get the output logits (first output)
        let logits_array = match outputs[0].try_extract_array::<f32>() {
            Ok(arr) => arr,
            Err(e) => {
                log::warn!("Failed to extract logits: {}", e);
                return GestureKind::Unknown;
            }
        };

        // Find the class with highest logit value (argmax)
        let predicted_class = logits_array
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| idx)
            .unwrap_or(0);

        // Map class index to GestureKind
        self.class_to_gesture
            .get(&predicted_class)
            .copied()
            .unwrap_or(GestureKind::Unknown)
    }
}

fn handedness_from_score(score: f32) -> Handedness {
    if score >= 0.5 {
        Handedness::Right
    } else if score > 0.0 {
        Handedness::Left
    } else {
        Handedness::Unknown
    }
}

fn normalize_landmarks(points: &[[f32; 3]]) -> (Vec<[f32; 3]>, f32) {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for [x, y, _z] in points {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }

    let span = (max_x - min_x).max(max_y - min_y).max(1e-3);
    let normalized = points
        .iter()
        .map(|[x, y, z]| [(*x - min_x) / span, (*y - min_y) / span, *z / span])
        .collect();

    (normalized, span)
}

fn projected_span(points: &[(f32, f32)]) -> f32 {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for &(x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    (max_x - min_x).max(max_y - min_y).max(1.0)
}

fn classify_finger(points: &[[f32; 3]], idx: [usize; 4]) -> FingerState {
    let wrist = points[0];
    let mcp = points[idx[0]];
    let pip = points[idx[1]];
    let dip = points[idx[2]];
    let tip = points[idx[3]];

    let dist_tip = distance3(tip, wrist);
    let dist_pip = distance3(pip, wrist);
    let dist_mcp = distance3(mcp, wrist);

    let straightness = average_straightness(sub(pip, mcp), sub(dip, pip), sub(tip, dip));

    let extension = dist_tip - dist_pip;
    let reach = dist_tip - dist_mcp;

    // Relaxed thresholds to reduce half-bent false positives (especially for pinky)
    if extension > 0.15 && straightness > 0.40 && reach > 0.06 {
        FingerState::Extended
    } else if extension < 0.08 || straightness < 0.18 || reach < 0.05 {
        FingerState::Folded
    } else {
        FingerState::HalfBent
    }
}

fn classify_thumb(points: &[[f32; 3]]) -> FingerState {
    let wrist = points[0];
    let cmc = points[1]; // Carpometacarpal joint
    let mcp = points[2]; // Metacarpophalangeal joint (corrected from points[1])
    let ip = points[3]; // Interphalangeal joint (corrected from points[2])
    let tip = points[4]; // Thumb tip
    let index_mcp = points[5];
    let pinky_mcp = points[17];

    // Calculate distances from wrist to various thumb joints
    let dist_tip_wrist = distance3(tip, wrist);
    let dist_ip_wrist = distance3(ip, wrist);
    let dist_mcp_wrist = distance3(mcp, wrist);

    // Calculate distances to other fingers to detect folding
    let dist_tip_index = distance3(tip, index_mcp);
    let dist_tip_pinky = distance3(tip, pinky_mcp);

    // Calculate straightness of thumb segments
    let straightness = average_straightness(sub(mcp, cmc), sub(ip, mcp), sub(tip, ip));

    // Minimum distance to index or pinky (indicates how close thumb is to palm)
    let spread = dist_tip_index.min(dist_tip_pinky);

    // Extension metric: how far tip extends beyond IP joint
    let extension = dist_tip_wrist - dist_ip_wrist;

    // Reach metric: how far tip extends beyond MCP joint
    let reach = dist_tip_wrist - dist_mcp_wrist;

    // Folded: thumb is close to palm and not straight (relaxed thresholds)
    if spread < 0.25 && (straightness < 0.28 || reach < 0.15) {
        FingerState::Folded
    // Extended: thumb is far from wrist, straight, and extends well beyond joints
    } else if dist_tip_wrist > 0.30 && straightness > 0.28 && extension > 0.08 {
        FingerState::Extended
    } else {
        FingerState::HalfBent
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn distance3(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn average_straightness(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    let ab = dot(normalize(a), normalize(b));
    let bc = dot(normalize(b), normalize(c));
    ((ab + bc) / 2.0).clamp(-1.0, 1.0)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-5 {
        [0.0, 0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

#[derive(Clone)]
struct MotionSample {
    time: Instant,
    x: f32,
    y: f32,
    span: f32,
}

struct MotionTracker {
    history: VecDeque<MotionSample>,
}

impl MotionTracker {
    fn new() -> Self {
        Self {
            history: VecDeque::new(),
        }
    }

    fn update(
        &mut self,
        point: (f32, f32),
        span: f32,
        now: Instant,
        primary: GestureKind,
    ) -> GestureMotion {
        self.history.push_back(MotionSample {
            time: now,
            x: point.0,
            y: point.1,
            span: span.max(1.0),
        });

        while let Some(front) = self.history.front() {
            if now.duration_since(front.time) > MOTION_WINDOW {
                self.history.pop_front();
            } else {
                break;
            }
        }

        if self.history.len() < 3 {
            return GestureMotion::Steady;
        }

        let avg_span =
            self.history.iter().map(|s| s.span).sum::<f32>() / (self.history.len() as f32);
        let norm = avg_span.max(1.0);

        let (min_x, max_x, min_y, max_y) =
            self.history
                .iter()
                .fold((f32::MAX, f32::MIN, f32::MAX, f32::MIN), |acc, s| {
                    (
                        acc.0.min(s.x),
                        acc.1.max(s.x),
                        acc.2.min(s.y),
                        acc.3.max(s.y),
                    )
                });

        let span_x = (max_x - min_x) / norm;
        let span_y = (max_y - min_y) / norm;

        let samples: Vec<MotionSample> = self.history.iter().cloned().collect();

        let direction_changes_x = direction_changes(&samples, |s| s.x, norm * 0.08);
        let direction_changes_y = direction_changes(&samples, |s| s.y, norm * 0.08);

        let is_open_palm = matches!(
            primary,
            GestureKind::Palm | GestureKind::Four | GestureKind::Unknown
        );

        if span_x > 0.55 && direction_changes_x >= 2 && is_open_palm {
            GestureMotion::Fanning
        } else if span_y > 0.55 && direction_changes_y >= 2 {
            GestureMotion::VerticalWave
        } else if span_x > 0.25 || span_y > 0.25 {
            GestureMotion::Moving
        } else {
            GestureMotion::Steady
        }
    }
}

fn direction_changes<F>(samples: &[MotionSample], select: F, min_step: f32) -> usize
where
    F: Fn(&MotionSample) -> f32,
{
    let mut changes = 0;
    let mut last_sign = 0i8;

    for pair in samples.windows(2) {
        let delta = select(&pair[1]) - select(&pair[0]);
        if delta.abs() < min_step {
            continue;
        }
        let sign = if delta > 0.0 { 1 } else { -1 };
        if last_sign != 0 && sign != last_sign {
            changes += 1;
        }
        last_sign = sign;
    }

    changes
}
