//! "Play" strip: recent instances with a one-click launch.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Icon, Sizable};
use gpui_kit::*;

use crate::instance::Instance;
use crate::ui::FancyButton;

#[derive(IntoElement)]
pub struct Play {
    recent: Vec<Instance>,
}

impl Play {
    pub fn new(recent: Vec<Instance>) -> Self {
        Self { recent }
    }
}

fn card(instance: Instance, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let subtitle = format!(
        "Build {}  ·  {}",
        instance.build,
        instance.last_played.clone().unwrap_or_default()
    );

    div()
        .flex()
        .items_center()
        .gap_3()
        .p_3()
        .rounded(theme.radius_lg)
        .bg(theme.group_box)
        .border_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(44.))
                .rounded(theme.radius)
                .bg(theme.muted)
                .text_color(theme.muted_foreground)
                .child(Icon::new(IconName::Box).size_5()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(instance.name),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(subtitle),
                ),
        )
        .child(
            FancyButton::new(SharedString::from(format!("play-{}", instance.id)), "Play")
                .small()
                .icon(IconName::Play),
        )
        .child(
            Button::new(SharedString::from(format!("more-{}", instance.id)))
                .ghost()
                .small()
                .icon(IconName::EllipsisVertical),
        )
}

impl RenderOnce for Play {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().font_weight(FontWeight::BOLD).child("Play"))
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap_3()
                    .children(self.recent.into_iter().map(|i| card(i, cx))),
            )
    }
}
