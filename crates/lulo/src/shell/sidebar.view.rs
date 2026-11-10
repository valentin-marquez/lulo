//! Left panel: every instance with its state, then new instance, settings and account.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Shell;
use crate::instance::{self, Status};

const WIDTH: Pixels = px(248.);

fn row(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .p_2()
        .rounded(cx.theme().radius)
        .cursor_pointer()
}

fn tile(icon: IconName, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(34.))
        .rounded(px(8.))
        .bg(theme.muted)
        .border_1()
        .border_color(theme.border)
        .text_color(theme.muted_foreground)
        .child(Icon::new(icon).size_4())
}

fn status(status: Status, branch: &str, cx: &App) -> Div {
    let theme = cx.theme();
    let (text, color) = match status {
        Status::Ready => (format!("Ready · {branch}"), theme.muted_foreground),
        Status::Outdated => ("Update available".into(), theme.primary),
        Status::Downloading(pct) => (format!("Downloading · {pct}%"), theme.muted_foreground),
    };
    div().text_xs().text_color(color).truncate().child(text)
}

pub fn render(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    let theme = cx.theme();

    let instances = shell.instances.iter().enumerate().map(|(ix, instance)| {
        let selected = ix == shell.selected;
        row(instance.id.clone(), cx)
            .map(|this| {
                if selected {
                    this.bg(theme.group_box)
                        .border_1()
                        .border_color(theme.border)
                } else {
                    this.border_1()
                        .border_color(transparent_black())
                        .hover(|s| s.bg(theme.sidebar_accent))
                }
            })
            .child(tile(IconName::Box, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(instance.name.clone()),
                    )
                    .child(status(instance.status, &instance.branch, cx)),
            )
            .on_click(cx.listener(move |shell, _, _, cx| shell.select(ix, cx)))
    });

    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(WIDTH)
        .px_2()
        .pb_2()
        .gap_1()
        .bg(theme.sidebar)
        .child(
            div()
                .px_2()
                .pt_2()
                .pb_1()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .child(format!("INSTANCES · {}", shell.instances.len())),
        )
        .children(instances)
        .child(div().flex_1())
        .child(
            row("new", cx)
                .hover(|s| s.bg(theme.sidebar_accent))
                .child(tile(IconName::Plus, cx))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("New instance"),
                )
                .on_click(|_, window, cx| instance::create::open(window, cx)),
        )
        .child(
            row("settings", cx)
                .hover(|s| s.bg(theme.sidebar_accent))
                .child(tile(IconName::Settings, cx))
                .child(div().text_sm().child("Settings")),
        )
        // ponytail: placeholder until Steam login (roadmap step 4).
        .child(
            row("account", cx)
                .hover(|s| s.bg(theme.sidebar_accent))
                .child(tile(IconName::CircleUser, cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().text_sm().child("Not signed in"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Steam"),
                        ),
                ),
        )
}
