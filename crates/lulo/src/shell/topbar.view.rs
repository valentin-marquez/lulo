//! Title bar: logo and what is running.

use gpui_kit::component::{ActiveTheme, Icon, TitleBar};
use gpui_kit::*;

use crate::assets::LOGO;

/// Taller than gpui-component's 34px default so the header carries the window.
pub const HEIGHT: Pixels = px(48.);

#[derive(IntoElement)]
pub struct Topbar {
    /// Name of the game running, if any.
    pub running: Option<SharedString>,
    pub downloads: super::downloads::Pill,
}

impl RenderOnce for Topbar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        let status = self.status(theme);
        TitleBar::new().h(HEIGHT).child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_between()
                .pr_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(img(LOGO).size(px(24.)))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("lulo"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(self.downloads)
                        .child(status),
                ),
        )
    }
}

impl Topbar {
    fn status(&self, theme: &gpui_kit::component::Theme) -> impl IntoElement + use<> {
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(28.))
            .px_3()
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(match self.running {
                Some(_) => Icon::empty()
                    .path("icons/gamepad.svg")
                    .size_4()
                    .text_color(theme.primary),
                None => Icon::empty()
                    .path("icons/moon-sleep.svg")
                    .size_4()
                    .text_color(theme.muted_foreground),
            })
            .child(match &self.running {
                Some(name) => format!("Running {name}"),
                None => "Nothing running".into(),
            })
    }
}
