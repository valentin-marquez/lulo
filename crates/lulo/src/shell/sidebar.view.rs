//! Left panel: pages, every instance with its state, then new instance, settings and account.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Page, Shell};
use crate::instance::Status;

const WIDTH: Pixels = px(248.);

fn row(id: impl Into<ElementId>, active: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_3()
        .p_2()
        .rounded(theme.radius)
        .border_1()
        .cursor_pointer()
        .map(|this| {
            if active {
                this.bg(theme.group_box).border_color(theme.border)
            } else {
                this.border_color(transparent_black())
                    .hover(|s| s.bg(theme.sidebar_accent))
            }
        })
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

fn label(text: impl Into<SharedString>) -> Div {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .truncate()
        .child(text.into())
}

fn status(status: Status, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .truncate()
        .child(status.label())
}

fn nav(
    page: Page,
    icon: IconName,
    text: &'static str,
    shell: &Shell,
    cx: &mut Context<Shell>,
) -> Stateful<Div> {
    row(text, shell.page == page, cx)
        .child(tile(icon, cx))
        .child(label(text))
        .on_click(cx.listener(move |shell, _, _, cx| shell.go(page, cx)))
}

pub fn render(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    let handle = cx.entity().downgrade();
    let home = nav(Page::Home, IconName::House, "Home", shell, cx);
    let screenshots = nav(Page::Screenshots, IconName::Image, "Screenshots", shell, cx);
    let theme = cx.theme();

    let instances = shell.instances.iter().enumerate().map(|(ix, instance)| {
        row(instance.id.clone(), shell.page == Page::Instance(ix), cx)
            .child(tile(IconName::Box, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(label(instance.name.clone()))
                    .child(status(instance.status(), cx)),
            )
            .on_click(cx.listener(move |shell, _, _, cx| shell.go(Page::Instance(ix), cx)))
    });

    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(WIDTH)
        .px_2()
        .py_2()
        .gap_1()
        .bg(theme.sidebar)
        .child(home)
        .child(screenshots)
        .child(
            div()
                .flex()
                .justify_between()
                .px_2()
                .pt_4()
                .pb_1()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.muted_foreground)
                .child("INSTANCES")
                .child(shell.instances.len().to_string()),
        )
        .children(instances)
        .child(div().flex_1())
        .child(
            row("new", false, cx)
                .child(tile(IconName::Plus, cx))
                .child(label("New instance"))
                .on_click(move |_, window, cx| Shell::create(handle.clone(), window, cx)),
        )
        .child(
            row("settings", false, cx)
                .child(tile(IconName::Settings, cx))
                .child(div().text_sm().child("Settings")),
        )
        // ponytail: placeholder until Steam login (roadmap step 4).
        .child(
            row("account", false, cx)
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
