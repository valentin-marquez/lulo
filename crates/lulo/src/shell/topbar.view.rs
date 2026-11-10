//! Title bar: logo and what is running.

use gpui_kit::component::{ActiveTheme, TitleBar};
use gpui_kit::*;

use crate::assets::LOGO;

#[derive(IntoElement)]
pub struct Topbar;

impl RenderOnce for Topbar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        TitleBar::new().child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .justify_between()
                .pr_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(img(LOGO).size(px(18.)))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("lulo"),
                        ),
                )
                // ponytail: static until launching exists (roadmap step 6).
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .h(px(24.))
                        .px_3()
                        .rounded_full()
                        .border_1()
                        .border_color(theme.border)
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(div().size(px(6.)).rounded_full().bg(theme.muted_foreground))
                        .child("Nothing running"),
                ),
        )
    }
}
