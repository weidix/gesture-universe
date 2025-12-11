use super::{
    ActiveTheme, AnyElement, AppView, CameraDevice, CameraState, Context, FluentBuilder,
    InteractiveElement, IntoElement, ParentElement, Screen, Styled, StyledExt, Window, camera, div,
    h_flex, v_flex,
};

impl AppView {
    fn render_camera_picker_startup(
        &mut self,
        cameras: &[CameraDevice],
        selected_idx: usize,
        error_msg: Option<&str>,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let mut picker = v_flex()
            .w(super::px(400.0))
            .gap_2()
            .p_4()
            .rounded_xl()
            .bg(gpui::rgb(0x0a0a0a))
            .border_1()
            .border_color(gpui::rgb(0x262626))
            .shadow_xl();

        let title_row = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .mb_2()
            .child(
                div()
                    .text_lg()
                    .font_bold()
                    .text_color(gpui::rgb(0xffffff))
                    .child("选择摄像头"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(gpui::rgb(0x525252))
                    .child(format!("可用设备: {}", cameras.len())),
            );

        picker = picker.child(title_row);

        for (idx, device) in cameras.iter().enumerate() {
            let is_selected = selected_idx == idx;

            picker = picker.child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_3()
                    .p_3()
                    .rounded_lg()
                    .cursor_pointer()
                    .bg(if is_selected {
                        gpui::rgb(0x171717)
                    } else {
                        gpui::rgb(0x0a0a0a)
                    })
                    .border_1()
                    .border_color(if is_selected {
                        gpui::rgb(0x525252)
                    } else {
                        gpui::rgb(0x0a0a0a)
                    })
                    .hover(|this| this.bg(gpui::rgb(0x262626)))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.select_camera(idx);
                            this.start_selected_camera();
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .text_lg()
                            .flex_shrink_0()
                            .text_color(if is_selected {
                                gpui::rgb(0xffffff)
                            } else {
                                gpui::rgb(0x525252)
                            })
                            .child("●"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .font_medium()
                            .text_color(if is_selected {
                                gpui::rgb(0xffffff)
                            } else {
                                gpui::rgb(0xa3a3a3)
                            })
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(device.label.clone()),
                    )
                    .when(is_selected, |this| {
                        this.child(
                            div()
                                .text_sm()
                                .flex_shrink_0()
                                .text_color(gpui::rgb(0xffffff))
                                .child("✓"),
                        )
                    }),
            );
        }

        if let Some(err) = error_msg {
            picker = picker.child(
                div()
                    .mt_2()
                    .text_xs()
                    .text_color(gpui::rgb(0xef4444))
                    .child(err.to_string()),
            );
        }

        picker.into_any_element()
    }

    pub(super) fn render_camera_picker_main(&mut self, cx: &mut Context<'_, Self>) -> AnyElement {
        let mut picker = v_flex()
            .gap_2()
            .p_4()
            .rounded_xl()
            .bg(gpui::rgb(0x0a0a0a))
            .border_1()
            .border_color(gpui::rgb(0x262626))
            .shadow_xl();

        let title_row = h_flex()
            .justify_between()
            .items_center()
            .w_full()
            .mb_2()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().text_base().text_color(gpui::rgb(0xffffff)).child("◉"))
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(gpui::rgb(0xffffff))
                            .child("选择摄像头"),
                    ),
            )
            .child(
                div()
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    .w_6()
                    .h_6()
                    .rounded_md()
                    .hover(|this| this.bg(gpui::rgba(0xffffff1a)))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.camera_picker_open = false;
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(gpui::rgb(0x94a3b8))
                            .child("✕"),
                    ),
            );

        picker = picker.child(title_row);

        for (idx, device) in self.available_cameras.iter().enumerate() {
            let is_selected = self.selected_camera_idx == Some(idx);

            picker = picker.child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .items_center()
                    .p_3()
                    .rounded_lg()
                    .cursor_pointer()
                    .bg(if is_selected {
                        gpui::rgb(0x171717)
                    } else {
                        gpui::rgb(0x0a0a0a)
                    })
                    .border_1()
                    .border_color(if is_selected {
                        gpui::rgb(0x525252)
                    } else {
                        gpui::rgb(0x0a0a0a)
                    })
                    .hover(|this| this.bg(gpui::rgb(0x262626)))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.switch_camera(idx);
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .text_lg()
                            .flex_shrink_0()
                            .text_color(if is_selected {
                                gpui::rgb(0xffffff)
                            } else {
                                gpui::rgb(0x525252)
                            })
                            .child("●"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(if is_selected {
                                gpui::rgb(0xffffff)
                            } else {
                                gpui::rgb(0xa3a3a3)
                            })
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(device.label.clone()),
                    )
                    .when(is_selected, |this| {
                        this.child(
                            div()
                                .text_sm()
                                .flex_shrink_0()
                                .text_color(gpui::rgb(0xffffff))
                                .child("✓"),
                        )
                    }),
            );
        }

        if let Some(err) = &self.camera_error {
            picker = picker.child(
                h_flex()
                    .gap_2()
                    .items_start()
                    .mt_2()
                    .p_3()
                    .rounded_lg()
                    .bg(gpui::rgba(0x7f1d1d33))
                    .border_1()
                    .border_color(gpui::rgba(0xef4444aa))
                    .child(
                        div()
                            .text_sm()
                            .flex_shrink_0()
                            .text_color(gpui::rgb(0xfca5a5))
                            .child("!"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(gpui::rgb(0xfca5a5))
                            .overflow_hidden()
                            .child(err.clone()),
                    ),
            );
        }

        picker.into_any_element()
    }

    pub(super) fn initial_camera_state() -> (CameraState, Vec<CameraDevice>) {
        match camera::available_cameras() {
            Ok(cameras) if cameras.is_empty() => (
                CameraState::Unavailable {
                    message: String::new(),
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
                        message: format!("{err:#}"),
                    },
                    Vec::new(),
                )
            }
        }
    }

    pub(super) fn render_camera_view(
        &mut self,
        state: &mut CameraState,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let (cam_color, cam_icon, cam_text) = match state {
            CameraState::Unavailable { .. } => (gpui::hsla(0.0, 0.8, 0.5, 1.0), "!", "无设备"),
            CameraState::Selection { .. } => (gpui::hsla(0.1, 0.8, 0.5, 1.0), "●", "选择中"),
            CameraState::Ready => (gpui::hsla(0.3, 0.8, 0.5, 1.0), "●", "启动中"),
        };

        let titlebar = self.render_titlebar(
            gpui::hsla(0.0, 0.0, 0.5, 1.0),
            "○",
            "未启动",
            cam_color,
            cam_icon,
            cam_text,
            window,
            cx,
        );

        let content = match state {
            CameraState::Unavailable { message } => div()
                .flex_1()
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui::rgb(0x1a2332))
                .child(
                    v_flex()
                        .w(super::px(400.0))
                        .gap_2()
                        .p_4()
                        .rounded_xl()
                        .bg(gpui::rgb(0x0a0a0a))
                        .border_1()
                        .border_color(gpui::rgb(0x262626))
                        .shadow_xl()
                        .child(
                            h_flex()
                                .w_full()
                                .justify_between()
                                .items_center()
                                .mb_2()
                                .child(
                                    div()
                                        .text_lg()
                                        .font_bold()
                                        .text_color(gpui::rgb(0xffffff))
                                        .child("没有可用摄像头"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(gpui::rgb(0x525252))
                                        .child("请检查连接"),
                                ),
                        )
                        .when(!message.is_empty(), |this| {
                            this.child(
                                div()
                                    .w_full()
                                    .p_3()
                                    .rounded_lg()
                                    .bg(gpui::rgba(0x00000033))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(gpui::rgb(0xef4444))
                                            .child(message.clone()),
                                    ),
                            )
                        })
                        .child(
                            div()
                                .id("refresh-cameras-button")
                                .w_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .gap_2()
                                .p_2()
                                .rounded_lg()
                                .bg(gpui::rgb(0x0a0a0a))
                                .border_1()
                                .border_color(gpui::rgb(0x262626))
                                .cursor_pointer()
                                .hover(|this| this.bg(gpui::rgb(0x262626)))
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.is_refreshing_cameras = true;
                                        cx.notify();
                                    }),
                                )
                                .on_mouse_up(
                                    gpui::MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.refresh_cameras();
                                        this.is_refreshing_cameras = false;
                                        cx.notify();
                                    }),
                                )
                                .child(
                                    div()
                                        .text_base()
                                        .flex_shrink_0()
                                        .text_color(gpui::rgb(0x525252))
                                        .child("⟳"),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .font_medium()
                                        .text_color(if self.is_refreshing_cameras {
                                            gpui::rgb(0xffffff)
                                        } else {
                                            gpui::rgb(0xa3a3a3)
                                        })
                                        .child(if self.is_refreshing_cameras {
                                            "刷新中..."
                                        } else {
                                            "刷新摄像头列表"
                                        }),
                                ),
                        ),
                )
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

                let error_msg = start_error.as_deref();
                let picker = self.render_camera_picker_startup(options, *selected, error_msg, cx);

                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::rgb(0x1a2332))
                    .child(div().w(super::px(450.0)).child(picker))
                    .into_any_element()
            }
            CameraState::Ready => {
                let theme = cx.theme();
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::rgb(0x1a2332))
                    .child(
                        v_flex()
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
                            ),
                    )
                    .into_any_element()
            }
        };

        v_flex()
            .size_full()
            .child(titlebar)
            .child(content)
            .into_any_element()
    }

    pub(super) fn switch_camera(&mut self, idx: usize) {
        if idx >= self.available_cameras.len() {
            self.camera_error = Some("无法找到所选摄像头".to_string());
            return;
        }

        let device = self.available_cameras[idx].clone();
        match self.start_camera_for_device(&device) {
            Ok(()) => {
                self.selected_camera_idx = Some(idx);
                self.camera_error = None;
                self.camera_picker_open = false;
            }
            Err(err) => {
                self.camera_error = Some(format!("无法启动摄像头: {err}"));
            }
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
                self.start_recognizer_if_needed();
                self.screen = Screen::Main;
            }
            Err(err) => {
                if let Screen::Camera(CameraState::Selection { start_error, .. }) = &mut self.screen
                {
                    *start_error = Some(format!("无法启动摄像头: {err}"));
                }
            }
        }
    }

    pub(super) fn refresh_cameras(&mut self) {
        let (new_state, new_cameras) = Self::initial_camera_state();
        self.screen = Screen::Camera(new_state);
        self.available_cameras = new_cameras;
        self.selected_camera_idx = if self.available_cameras.is_empty() {
            None
        } else {
            Some(0)
        };
    }
}
