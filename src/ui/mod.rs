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
    camera::{self, CameraDevice, CameraStream},
    model_download::{DownloadEvent, ensure_model_available_with_callback},
    recognizer::{self, RecognizerBackend},
    types::{Frame, GestureResult},
};

mod camera_view;
mod download;
mod main_view;
mod render_util;
mod titlebar;

const CONNECTIONS: &[(usize, usize)] = &[
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 4),
    (0, 5),
    (5, 6),
    (6, 7),
    (7, 8),
    (0, 9),
    (9, 10),
    (10, 11),
    (11, 12),
    (0, 13),
    (13, 14),
    (14, 15),
    (15, 16),
    (0, 17),
    (17, 18),
    (18, 19),
    (19, 20),
    (5, 9),
    (9, 13),
    (13, 17),
];

const SKELETON_LINE_THICKNESS: i32 = 3;

const CAMERA_MIN_SIZE: (f32, f32) = (240.0, 180.0);
const CAMERA_MAX_SIZE: (f32, f32) = (720.0, 540.0);
const DEFAULT_CAMERA_RATIO: f32 = 4.0 / 3.0;
const RIGHT_PANEL_MIN_WIDTH: f32 = 320.0;
const RIGHT_PANEL_MAX_WIDTH: f32 = 720.0;
const RIGHT_PANEL_INITIAL_WIDTH: f32 = 480.0;

pub fn launch_ui(
    app: &mut App,
    frame_rx: Receiver<Frame>,
    result_rx: Receiver<GestureResult>,
    frame_to_rec_rx: Receiver<Frame>,
    frame_tx: Sender<Frame>,
    frame_to_rec_tx: Sender<Frame>,
    result_tx: Sender<GestureResult>,
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
        let view = app.new(|_| {
            AppView::new(
                frame_rx,
                result_rx,
                frame_to_rec_rx,
                frame_tx,
                frame_to_rec_tx,
                result_tx,
                recognizer_backend,
            )
        });
        app.new(|cx| Root::new(view, window, cx))
    })?;

    Ok(())
}

struct AppView {
    screen: Screen,
    frame_rx: Option<Receiver<Frame>>,
    result_rx: Option<Receiver<GestureResult>>,
    frame_to_rec_rx: Option<Receiver<Frame>>,
    frame_tx: Sender<Frame>,
    frame_to_rec_tx: Sender<Frame>,
    result_tx: Option<Sender<GestureResult>>,
    recognizer_backend: RecognizerBackend,
    recognizer_handle: Option<thread::JoinHandle<()>>,
    camera_stream: Option<CameraStream>,
    available_cameras: Vec<CameraDevice>,
    selected_camera_idx: Option<usize>,
    camera_error: Option<String>,
    latest_frame: Option<Frame>,
    latest_result: Option<GestureResult>,
    latest_image: Option<Arc<RenderImage>>,
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
            start_time: Instant::now(),
        }
    }
}

enum DownloadMessage {
    Event(DownloadEvent),
    Error(String),
}

struct PanelResizeState {
    start_pointer_x: f32,
    start_width: f32,
}

impl AppView {
    fn new(
        frame_rx: Receiver<Frame>,
        result_rx: Receiver<GestureResult>,
        frame_to_rec_rx: Receiver<Frame>,
        frame_tx: Sender<Frame>,
        frame_to_rec_tx: Sender<Frame>,
        result_tx: Sender<GestureResult>,
        recognizer_backend: RecognizerBackend,
    ) -> Self {
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
            frame_rx: Some(frame_rx),
            result_rx: Some(result_rx),
            frame_to_rec_rx: Some(frame_to_rec_rx),
            frame_tx,
            frame_to_rec_tx,
            result_tx: Some(result_tx),
            recognizer_backend,
            recognizer_handle: None,
            camera_stream: None,
            available_cameras,
            selected_camera_idx,
            camera_error: None,
            latest_frame: None,
            latest_result: None,
            latest_image: None,
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

        let Some(frame_rx) = self.frame_to_rec_rx.take() else {
            log::warn!("missing frame receiver for recognizer");
            return;
        };
        let Some(result_tx) = self.result_tx.take() else {
            log::warn!("missing result sender for recognizer");
            return;
        };

        let backend = self.recognizer_backend.clone();
        let handle = recognizer::start_recognizer(backend, frame_rx, result_tx);
        self.recognizer_handle = Some(handle);
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
