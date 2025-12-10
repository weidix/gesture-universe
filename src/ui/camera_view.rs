use super::{
    camera, div, v_flex, ActiveTheme, AnyElement, AppView, Button, ButtonVariants, CameraDevice,
    CameraState, Context, DownloadState, IntoElement, ParentElement, Screen, Selectable,
    SharedString, Styled, StyledExt, Tag,
};

impl AppView {
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
