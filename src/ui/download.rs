use super::{
    AnyElement, AppView, Context, DownloadEvent, DownloadMessage, DownloadState, IntoElement,
    ParentElement, RecognizerBackend, Sender, Styled, StyledExt, div,
    ensure_model_available_with_callback, h_flex, thread, v_flex,
};
use gpui::{SharedString, px};

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
        _cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let bar = progress_bar_string(state.downloaded, state.total);
        let detail = match (state.total, state.finished) {
            (_, true) => "下载完成".to_string(),
            (Some(total), false) if total > 0 => {
                let percent = (state.downloaded as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
                format!("{percent:.1}%")
            }
            _ => format!("{:.1} MB", state.downloaded as f64 / 1024.0 / 1024.0),
        };

        let (status_icon, status_text, status_color) = if state.finished && state.error.is_none() {
            ("✓", "模型就绪", gpui::rgb(0x4ade80))
        } else if state.error.is_some() {
            ("✕", "下载失败", gpui::rgb(0xf87171))
        } else {
            ("⟳", "正在下载模型...", gpui::rgb(0xe2e8f0))
        };

        let mut container = v_flex()
            .w(px(420.0))
            .gap_4()
            .p_6()
            .rounded_xl()
            .bg(gpui::rgb(0x0a0a0a))
            .border_1()
            .border_color(gpui::rgb(0x262626))
            .shadow_xl()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .child(div().text_xl().text_color(status_color).child(status_icon))
                            .child(
                                div()
                                    .text_base()
                                    .font_semibold()
                                    .text_color(gpui::rgb(0xffffff))
                                    .child(status_text),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(gpui::rgb(0x525252))
                            .child(detail),
                    ),
            );

        if state.error.is_none() {
            container = container
                .child(
                    div()
                        .w_full()
                        .p_3()
                        .rounded_lg()
                        .bg(gpui::rgb(0x171717))
                        .border_1()
                        .border_color(gpui::rgb(0x262626))
                        .child(
                            div()
                                .text_xs()
                                .font_family(SharedString::from("Menlo"))
                                .text_color(gpui::rgb(0x22d3ee))
                                .whitespace_nowrap()
                                .child(bar),
                        ),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(gpui::rgb(0xa3a3a3))
                        .child(state.message.clone()),
                );
        } else if let Some(err) = &state.error {
            container = container.child(
                v_flex()
                    .w_full()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .bg(gpui::rgba(0x7f1d1d33))
                    .border_1()
                    .border_color(gpui::rgba(0xef444466))
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(gpui::rgb(0xfca5a5))
                            .child("错误详情"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::rgb(0xfecaca))
                            .whitespace_normal()
                            .child(err.clone()),
                    ),
            );
        }

        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .bg(gpui::rgb(0x1a2332))
            .child(container)
            .into_any_element()
    }
}

pub(super) fn spawn_model_download(
    backend: RecognizerBackend,
    tx: Sender<DownloadMessage>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let model_path = backend.model_path();

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
