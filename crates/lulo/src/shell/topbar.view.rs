//! Title bar: logo, where you are, and what is running.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, TitleBar};
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
                        .child(img(LOGO).size(px(20.)))
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("lulo"),
                        )
                        .child(div().text_color(theme.muted_foreground).child("/"))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_sm()
                                .child(Icon::new(IconName::House).size_4())
                                .child("Home"),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("downloads")
                                .ghost()
                                .small()
                                .icon(IconName::Download),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .h(px(26.))
                                .px_3()
                                .rounded_full()
                                .border_1()
                                .border_color(theme.border)
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(div().size(px(6.)).rounded_full().bg(theme.muted_foreground))
                                .child("No instances running"),
                        ),
                ),
        )
    }
}
