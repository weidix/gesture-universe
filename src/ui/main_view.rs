use super::render_util::frame_to_image;
use super::{
    ActiveTheme, AnyElement, AppView, Button, Context, DEFAULT_CAMERA_RATIO, FluentBuilder,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ObjectFit, PanelResizeState, ParentElement, RIGHT_PANEL_MAX_WIDTH, RIGHT_PANEL_MIN_WIDTH,
    SharedString, Styled, StyledImage, Window, h_flex, v_flex,
};
use crate::pipeline::CompositedFrame;
use crate::types::{FingerState, GestureMotion};
use gpui_component::StyledExt;
use std::sync::Arc;

impl AppView {
    pub(super) fn render_main(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let composited_rx = self.composited_rx.take();
        if let Some(rx) = composited_rx.as_ref() {
            let mut frames = Vec::new();
            while let Ok(frame) = rx.try_recv() {
                frames.push(frame);
            }

            for frame in frames {
                let CompositedFrame { frame, result } = frame;

                self.latest_result = Some(result);

                if let Some(image) = frame_to_image(&frame, None) {
                    self.replace_latest_image(image, window, cx);
                }
                self.latest_frame = Some(frame);
                if let Some(ts) = self.latest_frame.as_ref().map(|f| f.timestamp) {
                    self.update_fps(ts);
                }
            }
        }
        self.composited_rx = composited_rx;

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
        let fps_text = self
            .latest_fps
            .as_ref()
            .map(|v| format!("{:.1} fps", v))
            .unwrap_or_else(|| "-- fps".to_string());

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

        let metrics = h_flex()
            .gap_3()
            .items_center()
            .child(
                super::div()
                    .text_xs()
                    .text_color(gpui::rgb(0xa0aab8))
                    .child(format!("置信度: {confidence_text}")),
            )
            .child(
                super::div()
                    .text_xs()
                    .text_color(gpui::rgb(0xa0aab8))
                    .child(format!("帧率: {fps_text}")),
            );

        let mut info_row = h_flex()
            .justify_between()
            .items_center()
            .gap_2()
            .child(metrics);

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

        let gesture_panel = self.render_gesture_panel(panel_width, cx);

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
            .child(v_flex().gap_3().child(camera_card).child(gesture_panel))
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

    fn render_gesture_panel(&self, panel_width: f32, cx: &mut Context<'_, Self>) -> AnyElement {
        let theme = cx.theme();
        let finger_labels = ["拇指", "食指", "中指", "无名指", "小指"];

        let (
            primary_text,
            secondary_text,
            confidence_text,
            handedness_text,
            motion_state,
            finger_states,
        ) = match &self.latest_result {
            Some(result) => {
                let detail = result.detail.as_ref();
                let primary = detail
                    .map(|d| format!("{}{}", d.primary.emoji(), d.primary.display_name()))
                    .unwrap_or_else(|| result.label.clone());
                let secondary = detail.and_then(|d| {
                    d.secondary
                        .map(|s| format!("也可能是 {}{}", s.emoji(), s.display_name()))
                });
                let motion = detail.map(|d| d.motion).unwrap_or(GestureMotion::Steady);
                let handedness = detail
                    .map(|d| d.handedness.label().to_string())
                    .unwrap_or_else(|| "--".to_string());
                let states = detail.map(|d| d.finger_states);
                let conf = format!("{:.0}%", (result.confidence * 100.0).clamp(0.0, 100.0));
                (primary, secondary, conf, handedness, motion, states)
            }
            None => (
                "等待手部进入画面".to_string(),
                None,
                "--".to_string(),
                "--".to_string(),
                GestureMotion::Steady,
                None,
            ),
        };

        let status_color = if finger_states.is_some() {
            theme.success
        } else {
            theme.muted_foreground
        };

        let motion_chip = match motion_state {
            GestureMotion::Fanning => self.stat_chip("状态", "扇风/摇动", gpui::rgb(0x22c55e)),
            GestureMotion::VerticalWave => self.stat_chip("状态", "上下挥动", gpui::rgb(0xf97316)),
            GestureMotion::Moving => self.stat_chip("状态", "移动中", gpui::rgb(0xfbbf24)),
            GestureMotion::Steady => self.stat_chip("状态", "保持", theme.muted_foreground),
        };

        let finger_block: AnyElement = if let Some(states) = finger_states {
            let mut first_row = h_flex().gap_2();
            let mut second_row = h_flex().gap_2();
            for (idx, name) in finger_labels.iter().enumerate() {
                let chip = self.finger_chip(name, states[idx]);
                if idx < 3 {
                    first_row = first_row.child(chip);
                } else {
                    second_row = second_row.child(chip);
                }
            }
            v_flex()
                .gap_1()
                .child(first_row)
                .child(second_row)
                .into_any_element()
        } else {
            super::div()
                .text_xs()
                .text_color(gpui::rgb(0x6b7280))
                .child("等检测到手势后，这里会展示各手指的状态与动作")
                .into_any_element()
        };

        let mut container = v_flex()
            .w(super::px(panel_width))
            .gap_3()
            .p_4()
            .rounded_lg()
            .bg(gpui::rgb(0x0f172a))
            .border_1()
            .border_color(gpui::rgba(0xffffff1a))
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                super::div()
                                    .w(super::px(8.0))
                                    .h(super::px(8.0))
                                    .rounded_full()
                                    .bg(status_color),
                            )
                            .child(
                                super::div()
                                    .text_sm()
                                    .font_semibold()
                                    .text_color(gpui::rgb(0xffffff))
                                    .child("当前手势"),
                            ),
                    )
                    .child(
                        super::div()
                            .text_xs()
                            .text_color(gpui::rgb(0x94a3b8))
                            .child("实时更新"),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .child(
                        super::div()
                            .text_3xl()
                            .font_bold()
                            .text_color(gpui::rgb(0xe0f2fe))
                            .child(primary_text.clone()),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                super::div()
                                    .text_sm()
                                    .text_color(gpui::rgb(0xa5b4fc))
                                    .child("检测结果"),
                            )
                            .when(secondary_text.is_some(), |this| {
                                this.child(
                                    super::div()
                                        .text_xs()
                                        .text_color(gpui::rgb(0x94a3b8))
                                        .child(secondary_text.clone().unwrap_or_default()),
                                )
                            }),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(self.stat_chip("置信度", &confidence_text, theme.success))
                    .child(self.stat_chip("惯用手", &handedness_text, gpui::rgb(0x38bdf8)))
                    .child(motion_chip),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        super::div()
                            .text_xs()
                            .text_color(gpui::rgb(0x94a3b8))
                            .child("手指展开度"),
                    )
                    .child(finger_block),
            );

        if finger_states.is_none() {
            container = container.child(
                super::div()
                    .pt_1()
                    .text_xs()
                    .text_color(gpui::rgb(0x6b7280))
                    .child("让手掌进入画面，尝试各种手势（打电话、点赞、OK、握拳、和平、摇滚等），基于HAGRID数据集的模型识别"),
            );
        }

        container.into_any_element()
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

    fn stat_chip<C>(&self, label: &str, value: &str, color: C) -> AnyElement
    where
        C: Into<gpui::Rgba>,
    {
        let color = color.into();
        super::div()
            .px(super::px(10.0))
            .py(super::px(6.0))
            .rounded_md()
            .bg(gpui::rgba(0xffffff14))
            .border_1()
            .border_color(gpui::rgba(0xffffff12))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        super::div()
                            .text_xs()
                            .text_color(gpui::rgb(0x9ca3af))
                            .child(label.to_string()),
                    )
                    .child(
                        super::div()
                            .text_sm()
                            .font_semibold()
                            .text_color(color)
                            .child(value.to_string()),
                    ),
            )
            .into_any_element()
    }

    fn finger_chip(&self, label: &str, state: FingerState) -> AnyElement {
        let (bg, fg) = match state {
            FingerState::Extended => (gpui::rgba(0x15803d40), gpui::rgb(0x34d399)),
            FingerState::HalfBent => (gpui::rgba(0x1d4ed840), gpui::rgb(0x93c5fd)),
            FingerState::Folded => (gpui::rgba(0x7f1d1d40), gpui::rgb(0xfca5a5)),
        };

        super::div()
            .px(super::px(10.0))
            .py(super::px(6.0))
            .rounded_md()
            .bg(bg)
            .border_1()
            .border_color(gpui::rgba(0xffffff12))
            .child(
                super::div()
                    .text_xs()
                    .font_semibold()
                    .text_color(fg)
                    .child(format!("{label}: {}", state.label())),
            )
            .into_any_element()
    }
}
