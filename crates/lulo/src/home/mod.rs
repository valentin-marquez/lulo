//! Home: every instance in one list, launchable from here, like a launcher's installations page.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::instance::{Instance, Status};
use crate::shell::{Page, Shell};
use crate::ui::{Empty, FancyButton, Meta};

fn row(ix: usize, instance: &Instance, shell: &Shell, cx: &mut Context<Shell>) -> Stateful<Div> {
    let theme = cx.theme();
    let played = instance.played();
    let current = shell.status(instance);
    let status = (current != Status::Ready).then(|| current.label());
    let (action, icon, enabled) = current.action();
    let (handle, id) = (cx.entity().downgrade(), instance.id.clone());
    let dir = instance.dir.clone();

    div()
        // Not the bare instance id: the sidebar row uses it, and twin ids share click state.
        .id(SharedString::from(format!("home-{}", instance.id)))
        .flex()
        .items_center()
        .gap_4()
        .px_4()
        .py_3()
        .border_b_1()
        .border_color(theme.border)
        .cursor_pointer()
        .hover(|s| s.bg(theme.muted))
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(44.))
                .rounded(theme.radius)
                .bg(theme.muted)
                .border_1()
                .border_color(theme.border)
                .text_color(theme.muted_foreground)
                .child(Icon::new(IconName::Box).size_5()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(instance.name.clone()),
                )
                .child(
                    div()
                        .flex()
                        .gap_4()
                        .child(Meta::new(IconName::GitBranch, instance.branch.clone()))
                        .child(Meta::new(IconName::Clock, played)),
                )
                .children(crate::instance::bar(&instance.id, current, cx)),
        )
        .children(status.map(|text| {
            div()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(text)
        }))
        .child(
            Button::new(SharedString::from(format!("folder-{}", instance.id)))
                .ghost()
                .icon(IconName::FolderOpen)
                .tooltip("Open folder")
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    cx.open_with_system(&dir);
                }),
        )
        .child(
            FancyButton::new(SharedString::from(format!("play-{}", instance.id)), action)
                .small()
                .icon(icon)
                .when(enabled, |button| {
                    button.on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        Shell::play(handle.clone(), id.clone(), window, cx);
                    })
                }),
        )
        .on_click(cx.listener(move |shell, _, _, cx| shell.go(Page::Instance(ix), cx)))
}

pub fn render(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    let handle = cx.entity().downgrade();
    let mut rows = Vec::new();
    for (ix, instance) in shell.instances().iter().enumerate() {
        rows.push(row(ix, instance, shell, cx));
    }
    let theme = cx.theme();

    div()
        .flex()
        .flex_col()
        .size_full()
        .gap_5()
        .p_6()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::BOLD)
                        .child("Instances"),
                )
                .child(
                    FancyButton::new("home-new", "New instance")
                        .small()
                        .icon(IconName::Plus)
                        .on_click(move |_, window, cx| Shell::create(handle.clone(), window, cx)),
                ),
        )
        .when(rows.is_empty(), |this| {
            this.child(Empty::new(
                IconName::Box,
                "No instances yet",
                "Create one to pick a game version and start playing.",
            ))
        })
        .when(!rows.is_empty(), |this| {
            this.child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(theme.radius_lg)
                    .bg(theme.group_box)
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
                    .children(rows),
            )
        })
}
