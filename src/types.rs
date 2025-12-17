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
    pub label: String,
    pub confidence: f32,
    #[allow(dead_code)]
    pub timestamp: Instant,
    pub landmarks: Option<Vec<(f32, f32)>>,
    pub detail: Option<GestureDetail>,
    pub palm_regions: Vec<PalmRegion>,
}

#[derive(Clone, Debug)]
pub struct PalmRegion {
    pub bbox: [f32; 4],
    pub landmarks: Vec<(f32, f32)>,
    pub score: f32,
}

#[derive(Clone, Debug)]
pub struct RecognizedFrame {
    pub frame: Frame,
    pub result: GestureResult,
}

impl GestureResult {
    #[allow(dead_code)]
    pub fn display_text(&self) -> String {
        if let Some(detail) = &self.detail {
            format!(
                "{}{} ({:.0}%)",
                detail.primary.emoji(),
                detail.primary.display_name(),
                self.confidence * 100.0
            )
        } else {
            format!("{} ({:.0}%)", self.label, self.confidence * 100.0)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handedness {
    Left,
    Right,
    Unknown,
}

impl Handedness {
    pub fn label(&self) -> &'static str {
        match self {
            Handedness::Left => "左手",
            Handedness::Right => "右手",
            Handedness::Unknown => "未知",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FingerState {
    Extended,
    HalfBent,
    Folded,
}

impl FingerState {
    pub fn label(&self) -> &'static str {
        match self {
            FingerState::Extended => "伸直",
            FingerState::HalfBent => "半弯",
            FingerState::Folded => "弯曲",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureKind {
    Call,
    Dislike,
    Fist,
    Four,
    Grabbing,
    Grip,
    HandHeart,
    HandHeart2,
    Holy,
    Like,
    LittleFinger,
    MiddleFinger,
    Mute,
    NoGesture,
    Ok,
    One,
    Palm,
    Peace,
    PeaceInverted,
    Point,
    Rock,
    Stop,
    StopInverted,
    TakePicture,
    Three,
    Three2,
    Three3,
    ThreeGun,
    ThumbIndex,
    ThumbIndex2,
    Timeout,
    TwoUp,
    TwoUpInverted,
    XSign,
    Unknown,
}

impl GestureKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            GestureKind::Call => "打电话",
            GestureKind::Dislike => "点踩",
            GestureKind::Fist => "握拳",
            GestureKind::Four => "四指",
            GestureKind::Grabbing => "抓取",
            GestureKind::Grip => "握持",
            GestureKind::HandHeart => "比心",
            GestureKind::HandHeart2 => "比心2",
            GestureKind::Holy => "祈祷",
            GestureKind::Like => "点赞",
            GestureKind::LittleFinger => "小指",
            GestureKind::MiddleFinger => "中指",
            GestureKind::Mute => "静音",
            GestureKind::NoGesture => "无手势",
            GestureKind::Ok => "OK",
            GestureKind::One => "一",
            GestureKind::Palm => "手掌",
            GestureKind::Peace => "和平/剪刀手",
            GestureKind::PeaceInverted => "倒V",
            GestureKind::Point => "指向",
            GestureKind::Rock => "摇滚",
            GestureKind::Stop => "停止",
            GestureKind::StopInverted => "倒停止",
            GestureKind::TakePicture => "拍照",
            GestureKind::Three => "三指",
            GestureKind::Three2 => "三指2",
            GestureKind::Three3 => "三指3",
            GestureKind::ThreeGun => "三指枪",
            GestureKind::ThumbIndex => "拇指食指",
            GestureKind::ThumbIndex2 => "拇指食指2",
            GestureKind::Timeout => "暂停",
            GestureKind::TwoUp => "两指向上",
            GestureKind::TwoUpInverted => "倒两指",
            GestureKind::XSign => "X标志",
            GestureKind::Unknown => "未知手势",
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            GestureKind::Call => "🤙 ",
            GestureKind::Dislike => "👎 ",
            GestureKind::Fist => "✊ ",
            GestureKind::Four => "🖖 ",
            GestureKind::Grabbing => "🤜 ",
            GestureKind::Grip => "✊ ",
            GestureKind::HandHeart => "🫰 ",
            GestureKind::HandHeart2 => "🫶 ",
            GestureKind::Holy => "🙏 ",
            GestureKind::Like => "👍 ",
            GestureKind::LittleFinger => "🤙 ",
            GestureKind::MiddleFinger => "🖕 ",
            GestureKind::Mute => "🤐 ",
            GestureKind::NoGesture => "⋯ ",
            GestureKind::Ok => "👌 ",
            GestureKind::One => "☝️ ",
            GestureKind::Palm => "🖐 ",
            GestureKind::Peace => "✌️ ",
            GestureKind::PeaceInverted => "🤞 ",
            GestureKind::Point => "👉 ",
            GestureKind::Rock => "🤘 ",
            GestureKind::Stop => "✋ ",
            GestureKind::StopInverted => "🤚 ",
            GestureKind::TakePicture => "📸 ",
            GestureKind::Three => "🤟 ",
            GestureKind::Three2 => "👌 ",
            GestureKind::Three3 => "🤏 ",
            GestureKind::ThreeGun => "👈 ",
            GestureKind::ThumbIndex => "🤏 ",
            GestureKind::ThumbIndex2 => "👌 ",
            GestureKind::Timeout => "⏸️ ",
            GestureKind::TwoUp => "✌️ ",
            GestureKind::TwoUpInverted => "🤞 ",
            GestureKind::XSign => "❌ ",
            GestureKind::Unknown => "⋯ ",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureMotion {
    Steady,
    Fanning,
    VerticalWave,
    Moving,
}

impl GestureMotion {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            GestureMotion::Steady => "保持",
            GestureMotion::Fanning => "左右扇动",
            GestureMotion::VerticalWave => "上下挥动",
            GestureMotion::Moving => "移动中",
        }
    }
}

#[derive(Clone, Debug)]
pub struct GestureDetail {
    pub primary: GestureKind,
    pub secondary: Option<GestureKind>,
    pub handedness: Handedness,
    pub finger_states: [FingerState; 5],
    pub motion: GestureMotion,
}
