use std::{
    mem,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use crossbeam_channel::{Receiver, Sender, unbounded};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Context, Hsla, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, ParentElement, Render, RenderImage,
    SharedString, Styled, StyledImage, TitlebarOptions, Window, WindowControlArea,
    WindowDecorations, WindowOptions, div, img, px,
};
use gpui_component::{ActiveTheme, Root, StyledExt, button::Button, h_flex, v_flex};
use image::{Frame as ImageFrame, ImageBuffer, Rgba};

use crate::{
    model_download::{ModelDownloadEvent, ModelKind},
    pipeline::{
        CameraDevice, CameraStream, CompositedFrame, RecognizerBackend, start_frame_compositor,
        start_recognizer,
    },
    types::{Frame, GestureResult, RecognizedFrame},
};

mod camera_view;
mod download;
mod main_view;
mod render_util;
mod titlebar;

const CAMERA_MIN_SIZE: (f32, f32) = (240.0, 180.0);
const CAMERA_MAX_SIZE: (f32, f32) = (720.0, 540.0);
const DEFAULT_CAMERA_RATIO: f32 = 4.0 / 3.0;
const RIGHT_PANEL_MIN_WIDTH: f32 = 320.0;
const RIGHT_PANEL_MAX_WIDTH: f32 = 720.0;
const RIGHT_PANEL_INITIAL_WIDTH: f32 = 480.0;
const STARTUP_CARD_WIDTH: f32 = 420.0;

pub fn launch_ui(
    app: &mut App,
    camera_frame_rx: Receiver<Frame>,
    camera_frame_tx: Sender<Frame>,
    recognizer_backend: RecognizerBackend,
) -> gpui::Result<()> {
    let window_options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(gpui::Point {
                x: px(12.0),
                y: px(12.0),
            }),
        }),
        window_decorations: Some(WindowDecorations::Client),
        window_min_size: Some(gpui::Size {
            width: px(800.0),
            height: px(600.0),
        }),
        ..Default::default()
    };

    app.open_window(window_options, move |window, app| {
        let view = app.new(|_| AppView::new(camera_frame_rx, camera_frame_tx, recognizer_backend));
        app.new(|cx| {
            let root = Root::new(view, window, cx);
            #[cfg(target_os = "macos")]
            {
                cx.activate(true);
                window.activate_window();
            }
            root
        })
    })?;

    Ok(())
}

struct AppView {
    screen: Screen,
    composited_rx: Option<Receiver<CompositedFrame>>,
    camera_frame_rx: Option<Receiver<Frame>>,
    camera_frame_tx: Sender<Frame>,
    recognized_tx: Sender<RecognizedFrame>,
    recognizer_backend: RecognizerBackend,
    _frame_compositor_handle: thread::JoinHandle<()>,
    recognizer_handle: Option<thread::JoinHandle<()>>,
    camera_stream: Option<CameraStream>,
    available_cameras: Vec<CameraDevice>,
    selected_camera_idx: Option<usize>,
    camera_error: Option<String>,
    latest_frame: Option<Frame>,
    latest_result: Option<GestureResult>,
    latest_image: Option<Arc<RenderImage>>,
    latest_fps: Option<f32>,
    last_frame_ts: Option<Instant>,
    download_rx: Receiver<DownloadMessage>,
    _download_handle: thread::JoinHandle<()>,
    camera_picker_open: bool,
    right_panel_width: f32,
    panel_resize_state: Option<PanelResizeState>,
    is_refreshing_cameras: bool,
}

enum Screen {
    Camera(CameraState),
    Download(DownloadState),
    Main,
}

enum CameraState {
    Unavailable {
        message: String,
    },
    Selection {
        options: Vec<CameraDevice>,
        selected: usize,
        start_error: Option<String>,
    },
    Ready,
}

struct DownloadState {
    downloaded: u64,
    total: Option<u64>,
    message: String,
    error: Option<String>,
    finished: bool,
    handpose_ready: bool,
    palm_ready: bool,
    current_model: Option<ModelKind>,
    start_time: Instant,
}

impl DownloadState {
    fn new() -> Self {
        Self {
            downloaded: 0,
            total: None,
            message: "Preparing model download...".to_string(),
            error: None,
            finished: false,
            handpose_ready: false,
            palm_ready: false,
            current_model: None,
            start_time: Instant::now(),
        }
    }

    fn update_from_event(&mut self, event: ModelDownloadEvent) {
        match event {
            ModelDownloadEvent::AlreadyPresent { model } => {
                self.message = format!(
                    "{} model already present, continuing...",
                    model_label(model)
                );
                self.set_ready(model);
                self.downloaded = 0;
                self.total = None;
            }
            ModelDownloadEvent::Started { model, total } => {
                self.current_model = Some(model);
                self.downloaded = 0;
                self.total = total;
                self.message = format!("Downloading {} model...", model_label(model));
            }
            ModelDownloadEvent::Progress {
                model,
                downloaded,
                total,
            } => {
                self.current_model = Some(model);
                self.downloaded = downloaded;
                self.total = total;
                self.message = format!("Downloading {} model...", model_label(model));
            }
            ModelDownloadEvent::Finished { model } => {
                self.set_ready(model);
                self.message = format!("{} model ready", model_label(model));
            }
        }
        self.finished = self.handpose_ready && self.palm_ready;
    }

    fn set_ready(&mut self, model: ModelKind) {
        match model {
            ModelKind::HandposeEstimator => self.handpose_ready = true,
            ModelKind::PalmDetector => self.palm_ready = true,
        }
    }
}

fn model_label(model: ModelKind) -> &'static str {
    match model {
        ModelKind::HandposeEstimator => "Handpose estimator",
        ModelKind::PalmDetector => "Palm detector",
    }
}

enum DownloadMessage {
    Event(ModelDownloadEvent),
    Error(String),
}

struct PanelResizeState {
    start_pointer_x: f32,
    start_width: f32,
}

impl AppView {
    fn new(
        camera_frame_rx: Receiver<Frame>,
        camera_frame_tx: Sender<Frame>,
        recognizer_backend: RecognizerBackend,
    ) -> Self {
        let (recognized_tx, recognized_rx) = crossbeam_channel::bounded(1);
        let (composited_rx, compositor_handle) = start_frame_compositor(recognized_rx);
        let (download_tx, download_rx) = unbounded();
        let download_handle =
            download::spawn_model_download(recognizer_backend.clone(), download_tx);
        let (_initial_camera_state, available_cameras) = Self::initial_camera_state();
        let selected_camera_idx = if available_cameras.is_empty() {
            None
        } else {
            Some(0)
        };

        Self {
            screen: Screen::Download(DownloadState::new()),
            composited_rx: Some(composited_rx),
            camera_frame_rx: Some(camera_frame_rx),
            camera_frame_tx,
            recognized_tx,
            recognizer_backend,
            _frame_compositor_handle: compositor_handle,
            recognizer_handle: None,
            camera_stream: None,
            available_cameras,
            selected_camera_idx,
            camera_error: None,
            latest_frame: None,
            latest_result: None,
            latest_image: None,
            latest_fps: None,
            last_frame_ts: None,
            download_rx,
            _download_handle: download_handle,
            camera_picker_open: false,
            right_panel_width: RIGHT_PANEL_INITIAL_WIDTH,
            panel_resize_state: None,
            is_refreshing_cameras: false,
        }
    }

    fn start_recognizer_if_needed(&mut self) {
        if self.recognizer_handle.is_some() {
            return;
        }

        let Some(frame_rx) = self.camera_frame_rx.take() else {
            log::warn!("missing frame receiver for recognizer");
            return;
        };

        let backend = self.recognizer_backend.clone();
        let handle = start_recognizer(backend, frame_rx, self.recognized_tx.clone());
        self.recognizer_handle = Some(handle);
    }

    fn update_fps(&mut self, ts: Instant) {
        if let Some(prev) = self.last_frame_ts.replace(ts) {
            if let Some(delta) = ts.checked_duration_since(prev) {
                if delta.as_secs_f32() > 0.0 {
                    let current = 1.0 / delta.as_secs_f32();
                    let smoothed = if let Some(prev_fps) = self.latest_fps {
                        prev_fps * 0.8 + current * 0.2
                    } else {
                        current
                    };
                    self.latest_fps = Some(smoothed.min(240.0));
                }
            }
        }
    }
}

impl Render for AppView {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> impl gpui::IntoElement {
        cx.defer_in(window, |_, _, cx| {
            cx.notify();
        });

        let mut screen = mem::replace(&mut self.screen, Screen::Main);
        let view = match screen {
            Screen::Camera(mut state) => {
                let view = self.render_camera_view(&mut state, window, cx);
                match state {
                    CameraState::Ready => {
                        self.start_recognizer_if_needed();
                        screen = Screen::Main;
                    }
                    _ => {
                        screen = Screen::Camera(state);
                    }
                }
                view
            }
            Screen::Download(mut state) => {
                self.poll_download_events(&mut state);
                let min_time_passed = state.start_time.elapsed() >= Duration::from_millis(1200);
                let should_switch = state.finished && state.error.is_none() && min_time_passed;
                let view = self.render_download_view(&state, cx);
                if should_switch {
                    let (initial_camera_state, _) = Self::initial_camera_state();
                    screen = Screen::Camera(initial_camera_state);
                } else {
                    screen = Screen::Download(state);
                }
                view
            }
            Screen::Main => {
                screen = Screen::Main;
                self.render_main(window, cx)
            }
        };
        self.screen = screen;
        view
    }
}
