use super::{
    camera, div, h_flex, v_flex, ActiveTheme, AnyElement, AppView, Button, ButtonVariants, CameraDevice,
    CameraState, Context, DownloadState, FluentBuilder, IntoElement, InteractiveElement, ParentElement, Screen,
    SharedString, Styled, StyledExt,
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
            .gap_2()
            .p_4()
            .rounded_xl()
            .bg(gpui::rgba(0x0f1419f5))
            .border_1()
            .border_color(gpui::rgba(0x2d3748ff))
            .shadow_lg();

        let title_row = h_flex()
            .gap_2()
            .items_center()
            .w_full()
            .mb_2()
            .child(
                div()
                    .text_base()
                    .text_color(gpui::rgb(0xa5b4fc))
                    .child("◉")
            )
            .child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(gpui::rgb(0xe2e8f0))
                    .child("选择摄像头")
            );

        picker = picker.child(title_row);

        for (idx, device) in cameras.iter().enumerate() {
            let is_selected = selected_idx == idx;
            
            picker = picker.child(
                h_flex()
                    .w_full()
                    .gap_3()
                    .items_center()
                    .p_3()
                    .rounded_lg()
                    .cursor_pointer()
                    .bg(if is_selected {
                        gpui::rgba(0x2d374855)
                    } else {
                        gpui::rgba(0x1e293b00)
                    })
                    .border_1()
                    .border_color(if is_selected {
                        gpui::rgba(0x64748bff)
                    } else {
                        gpui::rgba(0x33415500)
                    })
                    .hover(|this| {
                        this.bg(gpui::rgba(0x2d374844))
                            .border_color(gpui::rgba(0x475569ff))
                    })
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _, _, cx| {
                        this.select_camera(idx);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_lg()
                            .flex_shrink_0()
                            .text_color(if is_selected {
                                gpui::rgb(0xa5b4fc)
                            } else {
                                gpui::rgb(0x94a3b8)
                            })
                            .child("●")
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(if is_selected {
                                gpui::rgb(0xe2e8f0)
                            } else {
                                gpui::rgb(0xcbd5e1)
                            })
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(device.label.clone())
                    )
                    .when(is_selected, |this| {
                        this.child(
                            div()
                                .text_sm()
                                .flex_shrink_0()
                                .text_color(gpui::rgb(0xa5b4fc))
                                .child("✓")
                        )
                    })
            );
        }

        if let Some(err) = error_msg {
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
                            .child("!")
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(gpui::rgb(0xfca5a5))
                            .overflow_hidden()
                            .child(err.to_string())
                    )
            );
        }

        picker = picker.child(
            Button::new(SharedString::from("camera-confirm"))
                .primary()
                .label("✓ 使用所选摄像头")
                .w_full()
                .mt_2()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.start_selected_camera();
                    cx.notify();
                }))
        );

        picker.into_any_element()
    }

    pub(super) fn render_camera_picker_main(
        &mut self,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let mut picker = v_flex()
            .gap_2()
            .p_4()
            .rounded_xl()
            .bg(gpui::rgba(0x0f1419f5))
            .border_1()
            .border_color(gpui::rgba(0x2d3748ff))
            .shadow_lg();

        let title_row = h_flex()
            .justify_between()
            .items_center()
            .w_full()
            .mb_2()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_base()
                            .text_color(gpui::rgb(0xa5b4fc))
                            .child("◉")
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(gpui::rgb(0xe2e8f0))
                            .child("选择摄像头")
                    )
            )
            .child(
                Button::new(SharedString::from("camera-picker-close"))
                    .label("×")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.camera_picker_open = false;
                        cx.notify();
                    }))
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
                        gpui::rgba(0x2d374855)
                    } else {
                        gpui::rgba(0x1e293b00)
                    })
                    .border_1()
                    .border_color(if is_selected {
                        gpui::rgba(0x64748bff)
                    } else {
                        gpui::rgba(0x33415500)
                    })
                    .hover(|this| {
                        this.bg(gpui::rgba(0x2d374844))
                            .border_color(gpui::rgba(0x475569ff))
                    })
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, _, _, cx| {
                        this.switch_camera(idx);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_lg()
                            .flex_shrink_0()
                            .text_color(if is_selected {
                                gpui::rgb(0xa5b4fc)
                            } else {
                                gpui::rgb(0x94a3b8)
                            })
                            .child("●")
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .text_color(if is_selected {
                                gpui::rgb(0xe2e8f0)
                            } else {
                                gpui::rgb(0xcbd5e1)
                            })
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(device.label.clone())
                    )
                    .when(is_selected, |this| {
                        this.child(
                            div()
                                .text_sm()
                                .flex_shrink_0()
                                .text_color(gpui::rgb(0xa5b4fc))
                                .child("✓")
                        )
                    })
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
                            .child("!")
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(gpui::rgb(0xfca5a5))
                            .overflow_hidden()
                            .child(err.clone())
                    )
            );
        }

        picker.into_any_element()
    }

    pub(super) fn initial_camera_state() -> (CameraState, Vec<CameraDevice>) {
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

    pub(super) fn render_camera_view(
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

                let error_msg = start_error.as_deref();
                let picker = self.render_camera_picker_startup(
                    options,
                    *selected,
                    error_msg,
                    cx,
                );

                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::rgba(0x1a233288))
                    .child(
                        div()
                            .w(super::px(450.0))
                            .child(picker)
                    )
                    .into_any_element()
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
}
