//! Home: every instance in one list, launchable from here, like a launcher's installations page.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::*;

use crate::instance::{self, Instance, Status};
use crate::shell::{Page, Shell};
use crate::ui::{FancyButton, Meta};

fn row(ix: usize, instance: &Instance, cx: &mut Context<Shell>) -> Stateful<Div> {
    let theme = cx.theme();
    let played = match &instance.last_played {
        Some(when) => format!("Played {when}"),
        None => "Never played".into(),
    };
    let status = match instance.status {
        Status::Ready => None,
        Status::Outdated => Some(("Update available".to_string(), theme.primary)),
        Status::Downloading(pct) => Some((format!("Downloading {pct}%"), theme.muted_foreground)),
    };

    div()
        .id(instance.id.clone())
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
                ),
        )
        .children(status.map(|(text, color)| div().text_sm().text_color(color).child(text)))
        .child(
            Button::new(SharedString::from(format!("folder-{}", instance.id)))
                .ghost()
                .icon(IconName::FolderOpen)
                .tooltip("Open folder")
                .cursor_pointer(),
        )
        .child(
            FancyButton::new(SharedString::from(format!("play-{}", instance.id)), "Play")
                .small()
                .icon(IconName::Play),
        )
        .on_click(cx.listener(move |shell, _, _, cx| shell.go(Page::Instance(ix), cx)))
}

pub fn render(shell: &Shell, cx: &mut Context<Shell>) -> impl IntoElement {
    let mut rows = Vec::new();
    for (ix, instance) in shell.instances().iter().enumerate() {
        rows.push(row(ix, instance, cx));
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
                        .on_click(|_, window, cx| instance::create::open(window, cx)),
                ),
        )
        .child(
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
}
