use std::{mem, sync::Arc, thread};

use crossbeam_channel::{Receiver, Sender, unbounded};
use gpui::{
    AnyElement, App, AppContext, Context, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, ParentElement, Render, RenderImage,
    SharedString, Styled, StyledImage, Window, WindowOptions, WindowDecorations, TitlebarOptions,
    div, img, px, WindowControlArea,
};
use gpui::prelude::FluentBuilder;
use gpui_component::{
    ActiveTheme, Root, Selectable, StyledExt,
    button::{Button, ButtonVariants},
    h_flex,
    tag::Tag,
    v_flex,
};
use image::{Frame as ImageFrame, ImageBuffer, Rgba};

use crate::{
    camera::{self, CameraDevice, CameraStream},
    model_download::{DownloadEvent, default_model_path, ensure_model_available_with_callback},
    recognizer::{self, RecognizerBackend},
    types::{Frame, GestureResult},
};

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
            title: Some("Gesture Universe".into()),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        window_decorations: Some(WindowDecorations::Client),
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
}

impl DownloadState {
    fn new() -> Self {
        Self {
            downloaded: 0,
            total: None,
            message: "Preparing model download...".to_string(),
            error: None,
            finished: false,
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
        let download_handle = spawn_model_download(recognizer_backend.clone(), download_tx);
        let (initial_camera_state, available_cameras) = Self::initial_camera_state();
        let selected_camera_idx = if available_cameras.is_empty() {
            None
        } else {
            Some(0)
        };

        Self {
            screen: Screen::Camera(initial_camera_state),
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
        }
    }

    fn initial_camera_state() -> (CameraState, Vec<CameraDevice>) {
        match camera::available_cameras() {
            Ok(cameras) if cameras.is_empty() => (
                CameraState::Unavailable {
                    message: "没有可用摄像头".to_string(),
                },
                Vec::new(),
            ),
            Ok(cameras) => (
                CameraState::Selection {
                    options: cameras.clone(),
                    selected: 0,
                    start_error: None,
                },
                cameras,
            ),
            Err(err) => {
                log::error!("failed to enumerate cameras: {err:?}");
                (
                    CameraState::Unavailable {
                        message: format!("没有可用摄像头: {err:#}"),
                    },
                    Vec::new(),
                )
            }
        }
    }

    fn switch_camera(&mut self, idx: usize) {
        if idx >= self.available_cameras.len() {
            self.camera_error = Some("无法找到所选摄像头".to_string());
            return;
        }

        let device = self.available_cameras[idx].clone();
        match self.start_camera_for_device(&device) {
            Ok(()) => {
                self.selected_camera_idx = Some(idx);
                self.camera_error = None;
            }
            Err(err) => {
                self.camera_error = Some(format!("无法启动摄像头: {err}"));
            }
        }
    }

    fn poll_download_events(&mut self, state: &mut DownloadState) {
        while let Ok(msg) = self.download_rx.try_recv() {
            match msg {
                DownloadMessage::Event(DownloadEvent::AlreadyPresent) => {
                    state.message = "Model already present, launching app...".to_string();
                }
                DownloadMessage::Event(DownloadEvent::Started { total }) => {
                    state.total = total;
                    state.message = "Downloading handpose model...".to_string();
                }
                DownloadMessage::Event(DownloadEvent::Progress { downloaded, total }) => {
                    state.downloaded = downloaded;
                    state.total = total;
                    state.message = "Downloading handpose model...".to_string();
                }
                DownloadMessage::Event(DownloadEvent::Finished) => {
                    state.finished = true;
                    state.message = "Model ready, starting app...".to_string();
                }
                DownloadMessage::Error(err) => {
                    state.error = Some(err);
                    state.finished = false;
                    state.message = "Model download failed".to_string();
                }
            }
        }
    }

    fn render_camera_view(
        &mut self,
        state: &mut CameraState,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        match state {
            CameraState::Unavailable { message } => v_flex()
                .gap_2()
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.group_box)
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.accent)
                        .font_semibold()
                        .child("⚠ 没有可用摄像头"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("请检查摄像头连接或权限设置"),
                )
                .child(div().text_color(theme.foreground).child(message.clone()))
                .into_any_element(),
            CameraState::Selection {
                options,
                selected,
                start_error,
            } => {
                if options.len() == 1 && self.camera_stream.is_none() && start_error.is_none() {
                    match self.start_camera_for_device(&options[0]) {
                        Ok(()) => {
                            *state = CameraState::Ready;
                            return div()
                                .child(div().child("正在启动摄像头..."))
                                .into_any_element();
                        }
                        Err(err) => {
                            *start_error = Some(format!("无法启动摄像头: {err}"));
                        }
                    }
                }

                let list =
                    options
                        .iter()
                        .enumerate()
                        .fold(v_flex().gap_2(), |list, (idx, device)| {
                            let is_selected = *selected == idx;
                            list.child(
                                Button::new(SharedString::from(format!("camera-option-{idx}")))
                                    .label(device.label.clone())
                                    .selected(is_selected)
                                    .outline()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.select_camera(idx);
                                        cx.notify();
                                    })),
                            )
                        });

                let mut container = v_flex()
                    .gap_3()
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.group_box)
                    .child(
                        div()
                            .text_base()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child("选择摄像头"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("检测到多个摄像头，请选择要使用的设备"),
                    )
                    .child(list)
                    .child(
                        Button::new(SharedString::from("camera-start"))
                            .primary()
                            .label("使用所选摄像头")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.start_selected_camera();
                                cx.notify();
                            })),
                    );

                if let Some(err) = start_error {
                    container = container.child(
                        Tag::danger()
                            .rounded_full()
                            .child(format!("无法启动摄像头: {err}")),
                    );
                }

                container.into_any_element()
            }
            CameraState::Ready => v_flex()
                .gap_2()
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.group_box)
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.foreground)
                        .child("⟳ 正在启动摄像头..."),
                )
                .into_any_element(),
        }
    }

    fn select_camera(&mut self, selected: usize) {
        if let Screen::Camera(CameraState::Selection {
            options,
            selected: current,
            start_error,
        }) = &mut self.screen
        {
            if selected < options.len() {
                *current = selected;
                *start_error = None;
                self.selected_camera_idx = Some(selected);
                self.available_cameras = options.clone();
            }
        }
    }

    fn stop_camera_stream(&mut self) {
        if let Some(stream) = self.camera_stream.take() {
            stream.stop();
        }
    }

    fn start_camera_for_device(&mut self, device: &CameraDevice) -> Result<(), String> {
        self.stop_camera_stream();

        camera::start_camera_stream(
            device.index.clone(),
            self.frame_tx.clone(),
            self.frame_to_rec_tx.clone(),
        )
        .map(|stream| {
            self.camera_stream = Some(stream);
            self.latest_frame = None;
            self.latest_result = None;
            self.latest_image = None;
            self.camera_error = None;
        })
        .map_err(|err| format!("{err:#}"))
    }

    fn start_selected_camera(&mut self) {
        let selected_device = match &self.screen {
            Screen::Camera(CameraState::Selection {
                options, selected, ..
            }) => {
                self.available_cameras = options.clone();
                options
                    .get(*selected)
                    .cloned()
                    .map(|device| (*selected, device))
            }
            _ => None,
        };

        let Some((selected_idx, device)) = selected_device else {
            if let Screen::Camera(CameraState::Selection { start_error, .. }) = &mut self.screen {
                *start_error = Some("无法找到所选摄像头".to_string());
            }
            return;
        };

        match self.start_camera_for_device(&device) {
            Ok(()) => {
                self.selected_camera_idx = Some(selected_idx);
                self.camera_error = None;
                self.camera_picker_open = false;
                self.screen = Screen::Download(DownloadState::new());
            }
            Err(err) => {
                if let Screen::Camera(CameraState::Selection { start_error, .. }) = &mut self.screen
                {
                    *start_error = Some(format!("无法启动摄像头: {err}"));
                }
            }
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

    fn camera_aspect_ratio(&self) -> f32 {
        if let Some(frame) = &self.latest_frame {
            if frame.height > 0 {
                return frame.width as f32 / frame.height as f32;
            }
        }
        DEFAULT_CAMERA_RATIO
    }

    fn start_panel_resize(
        &mut self,
        event: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.panel_resize_state = Some(PanelResizeState {
            start_pointer_x: f32::from(event.position.x),
            start_width: self.right_panel_width,
        });
        cx.notify();
    }

    fn update_panel_resize(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(state) = &self.panel_resize_state {
            if !event.dragging() {
                self.panel_resize_state = None;
                cx.notify();
                return;
            }

            let delta_x = f32::from(event.position.x) - state.start_pointer_x;
            let target_width = state.start_width - delta_x;
            let new_width = target_width
                .clamp(RIGHT_PANEL_MIN_WIDTH, RIGHT_PANEL_MAX_WIDTH);
            if (new_width - self.right_panel_width).abs() > f32::EPSILON {
                self.right_panel_width = new_width;
                cx.notify();
            }
        }
    }

    fn finish_panel_resize(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.panel_resize_state.take().is_some() {
            cx.notify();
        }
    }

    fn render_download_view(
        &self,
        state: &DownloadState,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let bar = progress_bar_string(state.downloaded, state.total);
        let detail = match (state.total, state.finished) {
            (_, true) => "Done".to_string(),
            (Some(total), false) if total > 0 => {
                let percent = (state.downloaded as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
                format!("{percent:.1}%")
            }
            _ => format!("Downloaded {} KB", state.downloaded / 1024),
        };

        let (status_icon, status_text, status_color) = if state.finished && state.error.is_none() {
            ("✓", "模型就绪", theme.success)
        } else if state.error.is_some() {
            ("✗", "模型下载失败", theme.accent)
        } else {
            ("⟳", "模型下载中", theme.foreground)
        };

        let mut container = v_flex()
            .gap_3()
            .p_6()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.group_box)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_color(status_color)
                            .font_semibold()
                            .child(format!("{} {}", status_icon, status_text)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("准备手势识别模型"),
                    ),
            )
            .child(
                div()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.muted)
                    .font_family(theme.mono_font_family.clone())
                    .text_color(theme.foreground)
                    .child(bar),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(detail),
            )
            .child(
                div()
                    .text_color(theme.foreground)
                    .child(state.message.clone()),
            );

        if let Some(err) = &state.error {
            container = container.child(Tag::danger().rounded_full().child(format!("错误: {err}")));
        }

        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .bg(theme.background)
            .child(container)
            .into_any_element()
    }

    fn render_main(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> AnyElement {
        // Drain recognizer results first so overlay uses the latest confidence/landmarks.
        let result_rx = self.result_rx.take();
        if let Some(rx) = result_rx.as_ref() {
            while let Ok(result) = rx.try_recv() {
                self.latest_result = Some(result);
            }
        }
        self.result_rx = result_rx;

        // Drain frames without holding an immutable borrow on self while we update state.
        let frame_rx = self.frame_rx.take();
        if let Some(rx) = frame_rx.as_ref() {
            let mut frames = Vec::new();
            while let Ok(frame) = rx.try_recv() {
                frames.push(frame);
            }

            for frame in frames {
                let overlay = self.latest_result.as_ref().and_then(|r| {
                    if r.confidence >= 0.5 {
                        r.landmarks.as_ref().map(|v| v.as_slice())
                    } else {
                        None
                    }
                });

                if let Some(image) = frame_to_image(&frame, overlay) {
                    self.replace_latest_image(image, window, cx);
                }
                self.latest_frame = Some(frame);
            }
        }
        self.frame_rx = frame_rx;

        let theme = cx.theme();

        let camera_label = self
            .selected_camera_idx
            .and_then(|idx| self.available_cameras.get(idx))
            .map(|c| c.label.clone())
            .unwrap_or_else(|| {
                if self.available_cameras.is_empty() {
                    "未检测到摄像头".to_string()
                } else {
                    "未选择摄像头".to_string()
                }
            });

        let frame_status = self
            .latest_frame
            .as_ref()
            .map(|f| format!("摄像头: {camera_label} {}x{} (最新)", f.width, f.height))
            .unwrap_or_else(|| format!("摄像头: {camera_label}，等待画面..."));

        let confidence_text = self
            .latest_result
            .as_ref()
            .map(|r| format!("{:.0}%", r.confidence * 100.0))
            .unwrap_or_else(|| "--".to_string());

        let ratio = self.camera_aspect_ratio();
        let panel_width = self
            .right_panel_width
            .clamp(RIGHT_PANEL_MIN_WIDTH, RIGHT_PANEL_MAX_WIDTH);
        self.right_panel_width = panel_width;
        let camera_height = (panel_width / ratio)
            .clamp(CAMERA_MIN_SIZE.1, CAMERA_MAX_SIZE.1);

        let frame_view: AnyElement = if let Some(image) = &self.latest_image {
            img(image.clone())
                .size_full()
                .object_fit(ObjectFit::Contain)
                .rounded_t_lg()
                .into_any_element()
        } else {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .rounded_t_lg()
                .child("等待摄像头...")
                .into_any_element()
        };

        let camera_shell = div()
            .relative()
            .w(px(panel_width))
            .h(px(camera_height))
            .overflow_hidden()
            .rounded_t_lg()
            .bg(gpui::rgb(0x000000))
            .child(frame_view);

        let mut picker_panel: Option<AnyElement> = None;
        if self.camera_picker_open && !self.available_cameras.is_empty() {
            let mut list = v_flex()
                .gap_1()
                .p_3()
                .rounded_lg()
                .bg(gpui::rgba(0x1a2332ee));

            for (idx, device) in self.available_cameras.iter().enumerate() {
                let is_selected = self.selected_camera_idx == Some(idx);
                list = list.child(
                    Button::new(SharedString::from(format!("camera-picker-{idx}")))
                        .label(device.label.clone())
                        .selected(is_selected)
                        .outline()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.switch_camera(idx);
                            cx.notify();
                        })),
                );
            }

            if let Some(err) = &self.camera_error {
                list = list.child(Tag::danger().rounded_full().child(err.clone()));
            }

            picker_panel = Some(list.into_any_element());
        } else if let Some(err) = &self.camera_error {
            picker_panel = Some(
                Tag::danger()
                    .rounded_full()
                    .child(err.clone())
                    .into_any_element(),
            );
        }

        let mut info_row = h_flex()
            .justify_between()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(gpui::rgb(0xa0aab8))
                    .child(format!("置信度: {confidence_text}")),
            );

        if self.available_cameras.len() > 1 {
            let picker_label = if self.camera_picker_open {
                "关闭摄像头选择"
            } else {
                "选择摄像头"
            };
            info_row = info_row.child(
                Button::new(SharedString::from("camera-picker-toggle"))
                    .outline()
                    .label(picker_label)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.camera_picker_open = !this.camera_picker_open;
                        cx.notify();
                    })),
            );
        }

        let mut camera_card = v_flex()
            .w(px(panel_width))
            .rounded_lg()
            .overflow_hidden()
            .bg(gpui::rgb(0x0f1419))
            .child(camera_shell)
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .child(info_row)
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::rgb(0x8b95a5))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(frame_status.clone()),
                    ),
            );

        if let Some(picker) = picker_panel {
            camera_card = camera_card.child(picker);
        }

        let (camera_icon, camera_text, camera_color) = if self.latest_frame.is_some() {
            ("●", "摄像头就绪", theme.success)
        } else {
            ("○", "等待摄像头", theme.muted_foreground)
        };

        let (recognizer_icon, recognizer_text, recognizer_color) = if self.recognizer_handle.is_some() {
            ("●", "识别运行中", theme.success)
        } else {
            ("○", "正在初始化", theme.muted_foreground)
        };

        let placeholder_block = div()
            .w(px(panel_width))
            .h(px(160.0))
            .rounded_lg()
            .bg(gpui::rgb(0x0f1419))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_xs()
                    .text_color(gpui::rgb(0x4a5568))
                    .child("预留区域"),
            );

        let panel_handle = div()
            .absolute()
            .left(px(-6.0))
            .top(px(0.0))
            .bottom(px(0.0))
            .w(px(12.0))
            .cursor_ew_resize()
            .bg(gpui::rgba(0x00000000))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::start_panel_resize))
            .on_mouse_move(cx.listener(Self::update_panel_resize))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::finish_panel_resize))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::finish_panel_resize));

        let right_panel = div()
            .relative()
            .w(px(panel_width))
            .child(
                v_flex()
                    .gap_3()
                    .child(camera_card)
                    .child(placeholder_block),
            )
            .child(panel_handle);

        // 自定义标题栏
        let titlebar = h_flex()
            .window_control_area(WindowControlArea::Drag)
            .h(px(32.0))
            .w_full()
            .px_3()
            .items_center()
            .justify_between()
            .bg(gpui::rgb(0x0f1419))
            .border_b_1()
            .border_color(gpui::rgb(0x2d3640))
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(gpui::rgb(0xe0e0e0))
                            .child("Gesture Universe"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(recognizer_color)
                            .child(format!("{} {}", recognizer_icon, recognizer_text)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(camera_color)
                            .child(format!("{} {}", camera_icon, camera_text)),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        div()
                            .id("minimize-button")
                            .w(px(36.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .window_control_area(WindowControlArea::Min)
                            .hover(|style| style.bg(gpui::rgb(0x2d3640)))
                            .text_color(gpui::rgb(0xb0b0b0))
                            .child("━"),
                    )
                    .child(
                        div()
                            .id("maximize-button")
                            .w(px(36.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .window_control_area(WindowControlArea::Max)
                            .hover(|style| style.bg(gpui::rgb(0x2d3640)))
                            .text_color(gpui::rgb(0xb0b0b0))
                            .child("□"),
                    )
                    .child(
                        div()
                            .id("close-button")
                            .w(px(36.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .window_control_area(WindowControlArea::Close)
                            .hover(|style| style.bg(gpui::rgb(0xe81123)))
                            .text_color(gpui::rgb(0xb0b0b0))
                            .hover(|style| style.text_color(gpui::rgb(0xffffff)))
                            .child("✕"),
                    ),
            );

        v_flex()
            .size_full()
            .bg(gpui::rgb(0x1a2332))
            .when(self.panel_resize_state.is_some(), |this| this.cursor_ew_resize())
            .on_mouse_move(cx.listener(Self::update_panel_resize))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::finish_panel_resize))
            .child(titlebar)
            .child(
                h_flex()
                    .flex_1()
                    .gap_3()
                    .p_4()
                    .items_start()
                    .child(div().flex_1())
                    .child(right_panel),
            )
            .into_any_element()
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
                let view = self.render_camera_view(&mut state, cx);
                match state {
                    CameraState::Ready => {
                        screen = Screen::Download(DownloadState::new());
                    }
                    _ => {
                        screen = Screen::Camera(state);
                    }
                }
                view
            }
            Screen::Download(mut state) => {
                self.poll_download_events(&mut state);
                let should_switch = state.finished && state.error.is_none();
                let view = self.render_download_view(&state, cx);
                if should_switch {
                    self.start_recognizer_if_needed();
                    screen = Screen::Main;
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

impl AppView {
    fn replace_latest_image(
        &mut self,
        new_image: Arc<RenderImage>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(old_image) = self.latest_image.replace(new_image) {
            // Explicitly drop the previous GPU texture; otherwise the sprite atlas keeps
            // every frame and memory will climb rapidly while the camera is running.
            cx.drop_image(old_image, Some(window));
        }
    }
}

fn spawn_model_download(
    backend: RecognizerBackend,
    tx: Sender<DownloadMessage>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let model_path = match backend {
            RecognizerBackend::HandposeTract { model_path } => model_path,
            _ => default_model_path(),
        };

        let result = ensure_model_available_with_callback(&model_path, |event| {
            let _ = tx.send(DownloadMessage::Event(event));
        });

        if let Err(err) = result {
            log::error!("failed to download model: {err:?}");
            let _ = tx.send(DownloadMessage::Error(format!("{err:#}")));
        }
    })
}

fn progress_bar_string(downloaded: u64, total: Option<u64>) -> String {
    const BAR_LEN: usize = 30;
    match total {
        Some(total) if total > 0 => {
            let pct = (downloaded as f64 / total as f64).clamp(0.0, 1.0);
            let filled = ((pct * BAR_LEN as f64).round() as usize).min(BAR_LEN);
            let empty = BAR_LEN.saturating_sub(filled);
            format!(
                "[{}{}] {:>5.1}%",
                "=".repeat(filled),
                " ".repeat(empty),
                pct * 100.0
            )
        }
        _ => {
            let spinner_width = ((downloaded / 64) as usize % (BAR_LEN.max(1))) + 1;
            format!(
                "[{:-<width$}] unknown size",
                ">",
                width = spinner_width.min(BAR_LEN)
            )
        }
    }
}

fn frame_to_image(frame: &Frame, overlay: Option<&[(f32, f32)]>) -> Option<Arc<RenderImage>> {
    let mut rgba = frame.rgba.clone();
    if let Some(points) = overlay {
        draw_skeleton(&mut rgba, frame.width, frame.height, points);
    }

    // GPUI expects BGRA; convert in place to avoid the async asset pipeline and flicker.
    for px in rgba.chunks_exact_mut(4) {
        px.swap(0, 2);
    }

    let buffer = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(frame.width, frame.height, rgba)?;
    let frame = ImageFrame::new(buffer);

    Some(Arc::new(RenderImage::new(vec![frame])))
}

fn draw_skeleton(buffer: &mut [u8], width: u32, height: u32, points: &[(f32, f32)]) {
    if points.len() < 2 {
        return;
    }

    let line_color = [96u8, 165u8, 250u8, 0u8];
    for &(a, b) in CONNECTIONS {
        if let (Some(pa), Some(pb)) = (points.get(a), points.get(b)) {
            draw_line(
                buffer,
                width,
                height,
                pa,
                pb,
                line_color,
                SKELETON_LINE_THICKNESS,
            );
        }
    }

    let point_color = [59u8, 130u8, 246u8, 0u8];
    for &(x, y) in points {
        draw_circle(buffer, width, height, (x as i32, y as i32), 3, point_color);
    }
}

fn draw_line(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    p0: &(f32, f32),
    p1: &(f32, f32),
    color: [u8; 4],
    thickness: i32,
) {
    let (mut x0, mut y0) = (p0.0 as i32, p0.1 as i32);
    let (x1, y1) = (p1.0 as i32, p1.1 as i32);
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let radius = (thickness.max(1) - 1) / 2;

    loop {
        put_pixel_safe(buffer, width, height, x0, y0, color);
        if radius > 0 {
            for ox in -radius..=radius {
                for oy in -radius..=radius {
                    if ox == 0 && oy == 0 {
                        continue;
                    }
                    if ox.abs() + oy.abs() <= radius {
                        put_pixel_safe(buffer, width, height, x0 + ox, y0 + oy, color);
                    }
                }
            }
        }
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_circle(
    buffer: &mut [u8],
    width: u32,
    height: u32,
    center: (i32, i32),
    radius: i32,
    color: [u8; 4],
) {
    let (cx, cy) = center;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= radius * radius {
                put_pixel_safe(buffer, width, height, cx + dx, cy + dy, color);
            }
        }
    }
}

fn put_pixel_safe(buffer: &mut [u8], width: u32, height: u32, x: i32, y: i32, color: [u8; 4]) {
    if x < 0 || y < 0 {
        return;
    }
    let (ux, uy) = (x as u32, y as u32);
    if ux >= width || uy >= height {
        return;
    }
    let idx = ((uy * width + ux) as usize) * 4;
    if idx + 3 < buffer.len() {
        buffer[idx..idx + 4].copy_from_slice(&color);
    }
}
