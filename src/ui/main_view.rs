use super::render_util::frame_to_image;
use super::{
    ActiveTheme, AnyElement, AppView, Button, Context, DEFAULT_CAMERA_RATIO, FluentBuilder,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ObjectFit, PanelResizeState, ParentElement, RIGHT_PANEL_MAX_WIDTH, RIGHT_PANEL_MIN_WIDTH,
    SharedString, Styled, StyledImage, Window, h_flex, v_flex,
};
use std::sync::Arc;

impl AppView {
    pub(super) fn render_main(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let result_rx = self.result_rx.take();
        if let Some(rx) = result_rx.as_ref() {
            while let Ok(result) = rx.try_recv() {
                self.latest_result = Some(result);
            }
        }
        self.result_rx = result_rx;

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
        let camera_height =
            (panel_width / ratio).clamp(super::CAMERA_MIN_SIZE.1, super::CAMERA_MAX_SIZE.1);

        let frame_view: AnyElement = if let Some(image) = &self.latest_image {
            super::img(image.clone())
                .size_full()
                .object_fit(ObjectFit::Contain)
                .rounded_t_lg()
                .into_any_element()
        } else {
            super::div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(gpui::rgb(0x8b95a5))
                .rounded_t_lg()
                .child("等待摄像头...")
                .into_any_element()
        };

        let camera_shell = super::div()
            .relative()
            .w(super::px(panel_width))
            .h(super::px(camera_height))
            .overflow_hidden()
            .rounded_t_lg()
            .bg(gpui::rgb(0x000000))
            .child(frame_view);

        let mut picker_panel: Option<AnyElement> = None;
        if self.camera_picker_open && !self.available_cameras.is_empty() {
            picker_panel = Some(self.render_camera_picker_main(cx));
        } else if let Some(err) = &self.camera_error {
            picker_panel = Some(
                h_flex()
                    .gap_2()
                    .items_center()
                    .p_3()
                    .rounded_lg()
                    .bg(gpui::rgba(0xef444433))
                    .border_1()
                    .border_color(gpui::rgba(0xef4444ff))
                    .child(super::div().text_base().child("⚠️"))
                    .child(
                        super::div()
                            .text_xs()
                            .text_color(gpui::rgb(0xfca5a5))
                            .child(err.clone()),
                    )
                    .into_any_element(),
            );
        }

        let mut info_row = h_flex().justify_between().items_center().gap_2().child(
            super::div()
                .text_xs()
                .text_color(gpui::rgb(0xa0aab8))
                .child(format!("置信度: {confidence_text}")),
        );

        if self.available_cameras.len() > 1 {
            let picker_label = if self.camera_picker_open {
                "◉ 关闭"
            } else {
                "◉ 切换"
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

        let mut camera_card = super::div().relative().w(super::px(panel_width)).child(
            v_flex()
                .w_full()
                .rounded_lg()
                .overflow_hidden()
                .bg(gpui::rgb(0x0f1419))
                .child(camera_shell)
                .child(
                    v_flex().gap_2().p_3().child(info_row).child(
                        super::div()
                            .text_xs()
                            .text_color(gpui::rgb(0x8b95a5))
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(frame_status.clone()),
                    ),
                ),
        );

        if let Some(picker) = picker_panel {
            camera_card = camera_card.child(
                super::div()
                    .absolute()
                    .top(super::px(16.0))
                    .left_1_2()
                    .w(super::px((panel_width * 0.85).min(400.0)))
                    .child(
                        super::div()
                            .relative()
                            .left(super::px(-(panel_width * 0.85).min(400.0) / 2.0))
                            .child(picker),
                    ),
            );
        }

        let theme = cx.theme();

        let (camera_icon, camera_text, camera_color) = if self.latest_frame.is_some() {
            ("●", "摄像头就绪", theme.success)
        } else {
            ("○", "等待摄像头", theme.muted_foreground)
        };

        let (recognizer_icon, recognizer_text, recognizer_color) =
            if self.recognizer_handle.is_some() {
                ("●", "识别运行中", theme.success)
            } else {
                ("○", "正在初始化", theme.muted_foreground)
            };

        let placeholder_block = super::div()
            .w(super::px(panel_width))
            .flex_1()
            .min_h(super::px(40.0))
            .rounded_lg()
            .bg(gpui::rgb(0x0f1419))
            .flex()
            .items_center()
            .justify_center()
            .child(
                super::div()
                    .text_xs()
                    .text_color(gpui::rgb(0x4a5568))
                    .child("预留区域"),
            );

        let panel_handle = super::div()
            .absolute()
            .left(super::px(-6.0))
            .top(super::px(0.0))
            .bottom(super::px(0.0))
            .w(super::px(12.0))
            .cursor_ew_resize()
            .bg(gpui::rgba(0x00000000))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::start_panel_resize))
            .on_mouse_move(cx.listener(Self::update_panel_resize))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::finish_panel_resize))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::finish_panel_resize));

        let right_panel = super::div()
            .relative()
            .w(super::px(panel_width))
            .h_full()
            .overflow_hidden()
            .child(v_flex().gap_3().child(camera_card).child(placeholder_block))
            .child(panel_handle);

        let titlebar = self.render_titlebar(
            recognizer_color,
            recognizer_icon,
            recognizer_text,
            camera_color,
            camera_icon,
            camera_text,
            window,
            cx,
        );

        v_flex()
            .size_full()
            .bg(gpui::rgb(0x1a2332))
            .when(self.panel_resize_state.is_some(), |this| {
                this.cursor_ew_resize()
            })
            .on_mouse_move(cx.listener(Self::update_panel_resize))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::finish_panel_resize))
            .child(titlebar)
            .child(
                h_flex()
                    .flex_1()
                    .gap_3()
                    .p_4()
                    .items_start()
                    .child(super::div().flex_1())
                    .child(right_panel),
            )
            .into_any_element()
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
            let new_width = target_width.clamp(RIGHT_PANEL_MIN_WIDTH, RIGHT_PANEL_MAX_WIDTH);
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

    fn replace_latest_image(
        &mut self,
        new_image: Arc<super::RenderImage>,
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
