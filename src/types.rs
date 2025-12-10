use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Frame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    #[allow(dead_code)]
    pub timestamp: Instant,
}

#[derive(Clone, Debug)]
pub struct GestureResult {
    #[allow(dead_code)]
    pub label: String,
    pub confidence: f32,
    #[allow(dead_code)]
    pub timestamp: Instant,
    pub landmarks: Option<Vec<(f32, f32)>>,
}

impl GestureResult {
    #[allow(dead_code)]
    pub fn display_text(&self) -> String {
        format!("{} ({:.0}%)", self.label, self.confidence * 100.0)
    }
}
