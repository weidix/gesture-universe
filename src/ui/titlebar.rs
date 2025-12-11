use super::{
    AnyElement, AppView, Context, Hsla, InteractiveElement, IntoElement, ParentElement, Styled,
    Window, WindowControlArea, div, h_flex, px,
};

#[cfg(target_os = "windows")]
use super::SharedString;

impl AppView {
    pub(super) fn render_titlebar(
        &self,
        recognizer_color: Hsla,
        recognizer_icon: &str,
        recognizer_text: &str,
        camera_color: Hsla,
        camera_icon: &str,
        camera_text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let titlebar_height = px(32.0);
        let titlebar_bg = gpui::rgb(0x1a2332);

        #[cfg(target_os = "windows")]
        let controls = self.render_windows_controls(window, cx);

        #[cfg(target_os = "macos")]
        let controls = self.render_macos_controls(window, cx);

        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let controls = self.render_linux_controls(window, cx);

        h_flex()
            .window_control_area(WindowControlArea::Drag)
            .h(titlebar_height)
            .w_full()
            .items_center()
            .justify_between()
            .bg(titlebar_bg)
            .child(
                h_flex()
                    .gap_3()
                    .pl(px(80.0))
                    .pr_3()
                    .h_full()
                    .items_center()
                    .child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(gpui::rgba(0x00000033))
                            .text_xs()
                            .text_color(recognizer_color)
                            .child(format!("{} {}", recognizer_icon, recognizer_text)),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(gpui::rgba(0x00000033))
                            .text_xs()
                            .text_color(camera_color)
                            .child(format!("{} {}", camera_icon, camera_text)),
                    ),
            )
            .child(controls)
            .into_any_element()
    }

    #[cfg(target_os = "windows")]
    fn render_windows_controls(
        &self,
        window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        use gpui::Rgba;

        let button_height = px(32.0);
        let close_button_hover_color = Rgba {
            r: 232.0 / 255.0,
            g: 17.0 / 255.0,
            b: 32.0 / 255.0,
            a: 1.0,
        };

        let button_hover_color = gpui::rgb(0x404040);

        let font_family: SharedString = "Segoe Fluent Icons".into();

        div()
            .id("windows-window-controls")
            .font_family(font_family)
            .flex()
            .flex_row()
            .justify_center()
            .content_stretch()
            .max_h(button_height)
            .min_h(button_height)
            .child(
                div()
                    .id("minimize")
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .occlude()
                    .w(px(46.0))
                    .h_full()
                    .text_size(px(10.0))
                    .hover(|s| s.bg(button_hover_color))
                    .window_control_area(WindowControlArea::Min)
                    .child("\u{e921}"), // Minimize icon
            )
            .child(
                div()
                    .id("maximize-or-restore")
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .occlude()
                    .w(px(46.0))
                    .h_full()
                    .text_size(px(10.0))
                    .hover(|s| s.bg(button_hover_color))
                    .window_control_area(WindowControlArea::Max)
                    .child(if window.is_maximized() {
                        "\u{e923}" // Restore icon
                    } else {
                        "\u{e922}" // Maximize icon
                    }),
            )
            .child(
                div()
                    .id("close")
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .occlude()
                    .w(px(46.0))
                    .h_full()
                    .text_size(px(10.0))
                    .hover(|s| s.bg(close_button_hover_color))
                    .window_control_area(WindowControlArea::Close)
                    .child("\u{e8bb}"), // Close icon
            )
            .into_any_element()
    }

    #[cfg(target_os = "macos")]
    fn render_macos_controls(
        &self,
        _window: &mut Window,
        _cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        div().into_any_element()
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    fn render_linux_controls(
        &self,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> AnyElement {
        let button_size = px(28.0);
        let icon_size = px(16.0);
        let icon_color = gpui::rgb(0xc9d1d9);
        let hover_bg = gpui::rgb(0x1f2428);
        let close_hover_bg = gpui::rgb(0xe81123);

        h_flex()
            .gap_1()
            .px_2()
            .child(
                div()
                    .id("linux-minimize")
                    .size(button_size)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .window_control_area(WindowControlArea::Min)
                    .hover(|s| s.bg(hover_bg))
                    .child(
                        gpui::svg()
                            .size(icon_size)
                            .path("M 4,8 H 12")
                            .text_color(icon_color),
                    ),
            )
            .child(
                div()
                    .id("linux-maximize")
                    .size(button_size)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .window_control_area(WindowControlArea::Max)
                    .hover(|s| s.bg(hover_bg))
                    .child(
                        gpui::svg()
                            .size(icon_size)
                            .path("M 4,4 H 12 V 12 H 4 Z")
                            .text_color(icon_color),
                    ),
            )
            .child(
                div()
                    .id("linux-close")
                    .group("close")
                    .size(button_size)
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .cursor_pointer()
                    .window_control_area(WindowControlArea::Close)
                    .hover(|s| s.bg(close_hover_bg))
                    .child(
                        gpui::svg()
                            .size(icon_size)
                            .path("M 4,4 L 12,12 M 12,4 L 4,12")
                            .text_color(icon_color)
                            .group_hover("close", |s| s.text_color(gpui::rgb(0xffffff))),
                    ),
            )
            .into_any_element()
    }
}
