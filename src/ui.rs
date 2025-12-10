use std::{mem, sync::Arc, thread};

use crossbeam_channel::{Receiver, Sender, unbounded};
use gpui::{
    AnyElement, App, AppContext, Context, InteractiveElement, IntoElement, ObjectFit,
    ParentElement, Render, RenderImage, SharedString, StatefulInteractiveElement, Styled,
    StyledImage, Window, WindowOptions, div, img, px, rgb, rgba,
};
use image::{Frame as ImageFrame, ImageBuffer, Rgba};

use crate::{
    camera::{self, CameraDevice},
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
    app.open_window(WindowOptions::default(), move |_window, app| {
        app.new(|_| {
            AppView::new(
                frame_rx,
                result_rx,
                frame_to_rec_rx,
                frame_tx,
                frame_to_rec_tx,
                result_tx,
                recognizer_backend,
            )
        })
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
    camera_handle: Option<thread::JoinHandle<()>>,
    latest_frame: Option<Frame>,
    latest_result: Option<GestureResult>,
    latest_image: Option<Arc<RenderImage>>,
    download_rx: Receiver<DownloadMessage>,
    _download_handle: thread::JoinHandle<()>,
    camera_expanded: bool,
}

enum Screen {
    Camera(CameraState),
    Download(DownloadState),
    Main,
}

enum CameraState {
    Unavailable { message: String },
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
            spawn_model_download(recognizer_backend.clone(), download_tx);

        Self {
            screen: Screen::Camera(Self::initial_camera_state()),
            frame_rx: Some(frame_rx),
            result_rx: Some(result_rx),
            frame_to_rec_rx: Some(frame_to_rec_rx),
            frame_tx,
            frame_to_rec_tx,
            result_tx: Some(result_tx),
            recognizer_backend,
            recognizer_handle: None,
            camera_handle: None,
            latest_frame: None,
            latest_result: None,
            latest_image: None,
            download_rx,
            _download_handle: download_handle,
            camera_expanded: false,
        }
    }

    fn initial_camera_state() -> CameraState {
        match camera::available_cameras() {
            Ok(cameras) if cameras.is_empty() => CameraState::Unavailable {
                message: "没有可用摄像头".to_string(),
            },
            Ok(cameras) => CameraState::Selection {
                options: cameras,
                selected: 0,
                start_error: None,
            },
            Err(err) => {
                log::error!("failed to enumerate cameras: {err:?}");
                CameraState::Unavailable {
                    message: format!("没有可用摄像头: {err:#}"),
                }
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
        match state {
            CameraState::Unavailable { message } => div()
                .child(div().child("没有可用摄像头"))
                .child(div().child(message.clone()))
                .into_any_element(),
            CameraState::Selection {
                options,
                selected,
                start_error,
            } => {
                if options.len() == 1 && self.camera_handle.is_none() && start_error.is_none() {
                    match self.start_camera_thread(&options[0]) {
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

                let mut container = div()
                    .child(div().child("选择摄像头"))
                    .child(div().child("检测到多个摄像头，请选择要使用的设备："));

                for (idx, device) in options.iter().enumerate() {
                    let is_selected = *selected == idx;
                    let label = device.label.clone();
                    let option = div()
                        .id(SharedString::from(format!("camera-option-{idx}")))
                        .child(format!(
                            "{} {label}",
                            if is_selected { "●" } else { "○" }
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_camera(idx);
                            cx.notify();
                        }));
                    container = container.child(option);
                }

                let mut actions = div().child(
                    div()
                        .id(SharedString::from("camera-start"))
                        .child("使用所选摄像头")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.start_selected_camera();
                            cx.notify();
                        })),
                );

                if let Some(err) = start_error {
                    actions = actions.child(div().child(format!("无法启动摄像头: {err}")));
                }

                container.child(actions).into_any_element()
            }
            CameraState::Ready => div()
                .child(div().child("正在启动摄像头..."))
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
            }
        }
    }

    fn start_camera_thread(&mut self, device: &CameraDevice) -> Result<(), String> {
        if self.camera_handle.is_some() {
            return Ok(());
        }

        camera::start_camera_stream(
            device.index.clone(),
            self.frame_tx.clone(),
            self.frame_to_rec_tx.clone(),
        )
        .map(|handle| {
            self.camera_handle = Some(handle);
        })
        .map_err(|err| format!("{err:#}"))
    }

    fn start_selected_camera(&mut self) {
        let selected_device = match &self.screen {
            Screen::Camera(CameraState::Selection { options, selected, .. }) => {
                options.get(*selected).cloned()
            }
            _ => None,
        };

        let Some(device) = selected_device else {
            if let Screen::Camera(CameraState::Selection { start_error, .. }) = &mut self.screen {
                *start_error = Some("无法找到所选摄像头".to_string());
            }
            return;
        };

        match self.start_camera_thread(&device) {
            Ok(()) => {
                self.screen = Screen::Download(DownloadState::new());
            }
            Err(err) => {
                if let Screen::Camera(CameraState::Selection { start_error, .. }) =
                    &mut self.screen
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

    fn render_download_view(&self, state: &DownloadState) -> AnyElement {
        let bar = progress_bar_string(state.downloaded, state.total);
        let detail = match (state.total, state.finished) {
            (_, true) => "Done".to_string(),
            (Some(total), false) if total > 0 => {
                let percent =
                    (state.downloaded as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
                format!("{percent:.1}%")
            }
            _ => format!("Downloaded {} KB", state.downloaded / 1024),
        };

        let mut container = div()
            .child(div().child("Preparing handpose model..."))
            .child(div().child(bar))
            .child(div().child(detail))
            .child(div().child(state.message.clone()));

        if let Some(err) = &state.error {
            container = container.child(div().child(format!("错误: {err}")));
        }

        container.into_any_element()
    }

    fn render_main(&mut self, _window: &mut Window, cx: &mut Context<'_, Self>) -> AnyElement {
        if let Some(rx) = self.frame_rx.as_ref() {
            while let Ok(frame) = rx.try_recv() {
                if let Some(image) = frame_to_image(
                    &frame,
                    self.latest_result
                        .as_ref()
                        .and_then(|r| r.landmarks.as_ref().map(|v| v.as_slice())),
                ) {
                    self.latest_image = Some(image);
                }
                self.latest_frame = Some(frame);
            }
        }

        if let Some(rx) = self.result_rx.as_ref() {
            while let Ok(result) = rx.try_recv() {
                self.latest_result = Some(result);
            }
        }

        let frame_status = self
            .latest_frame
            .as_ref()
            .map(|f| format!("摄像头: {}x{} (最新)", f.width, f.height))
            .unwrap_or_else(|| "摄像头: 等待画面...".to_string());

        let gesture_text = self
            .latest_result
            .as_ref()
            .map(|g| format!("手势: {}", g.display_text()))
            .unwrap_or_else(|| "手势: ...".to_string());

        let camera_width = if self.camera_expanded { px(520.0) } else { px(240.0) };
        let camera_height = if self.camera_expanded { px(390.0) } else { px(180.0) };

        let frame_view: AnyElement = if let Some(image) = &self.latest_image {
            img(image.clone())
                .size_full()
                .object_fit(ObjectFit::Cover)
                .into_any_element()
        } else {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(rgb(0x9ca3af))
                .child("等待摄像头...")
                .into_any_element()
        };

        let camera_shell = div()
            .w(camera_width)
            .h(camera_height)
            .rounded_md()
            .border_1()
            .border_color(rgb(0x2a2a2a))
            .overflow_hidden()
            .bg(rgb(0x0b0b0f))
            .child(frame_view);

        let toggle_label = if self.camera_expanded {
            "缩小画面"
        } else {
            "放大画面"
        };

        let camera_panel = div()
            .absolute()
            .top(px(12.0))
            .left(px(12.0))
            .p_2()
            .gap_2()
            .flex()
            .flex_col()
            .items_start()
            .bg(rgba(0x0b1220dd))
            .rounded_md()
            .shadow_lg()
            .border_1()
            .border_color(rgb(0x1f2937))
            .child(camera_shell)
            .child(
                div()
                    .w(camera_width)
                    .flex()
                    .justify_between()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0xe5e7eb))
                            .child(frame_status.clone()),
                    )
                    .child(
                        div()
                            .id(SharedString::from("camera-size-toggle"))
                            .px_3()
                            .py_2()
                            .rounded_sm()
                            .bg(rgb(0x2563eb))
                            .text_color(rgb(0xffffff))
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.camera_expanded = !this.camera_expanded;
                                cx.notify();
                            }))
                            .child(toggle_label),
                    ),
            );

        let main_panel = div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .bg(rgb(0x0f172a))
            .text_color(rgb(0xe5e7eb))
            .child(div().text_2xl().child(gesture_text))
            .child(div().text_sm().text_color(rgb(0x9ca3af)).child(frame_status));

        div()
            .relative()
            .size_full()
            .child(camera_panel)
            .child(main_panel)
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
                let view = self.render_download_view(&state);
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

    let line_color = [255u8, 142u8, 82u8, 255u8];
    for &(a, b) in CONNECTIONS {
        if let (Some(pa), Some(pb)) = (points.get(a), points.get(b)) {
            draw_line(buffer, width, height, pa, pb, line_color);
        }
    }

    let point_color = [56u8, 163u8, 255u8, 255u8];
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
) {
    let (mut x0, mut y0) = (p0.0 as i32, p0.1 as i32);
    let (x1, y1) = (p1.0 as i32, p1.1 as i32);
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        put_pixel_safe(buffer, width, height, x0, y0, color);
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
