use super::{
    default_model_path, ensure_model_available_with_callback, h_flex, v_flex, ActiveTheme, AppView,
    AnyElement, Context, DownloadEvent, DownloadMessage, DownloadState, IntoElement, ParentElement,
    RecognizerBackend, Sender, Styled, StyledExt, Tag, div, thread,
};

impl AppView {
    pub(super) fn poll_download_events(&mut self, state: &mut DownloadState) {
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

    pub(super) fn render_download_view(
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
}

pub(super) fn spawn_model_download(
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
