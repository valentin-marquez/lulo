//! The selected instance: header with Play, and its Mods / Worlds / Logs tabs.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Instance, Status};
use crate::shell::Shell;
use crate::ui::{Empty, FancyButton, Meta};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Mods,
    Worlds,
    Logs,
}

impl Tab {
    const ALL: [Tab; 3] = [Tab::Mods, Tab::Worlds, Tab::Logs];

    fn label(self) -> &'static str {
        match self {
            Tab::Mods => "Mods",
            Tab::Worlds => "Worlds",
            Tab::Logs => "Logs",
        }
    }

    // ponytail: empty states until step 7 of the roadmap fills the tabs.
    fn empty(self) -> (IconName, &'static str, &'static str) {
        match self {
            Tab::Mods => (
                IconName::Puzzle,
                "No mods yet",
                "Mods you add to this instance show up here.",
            ),
            Tab::Worlds => (
                IconName::Globe,
                "No worlds yet",
                "Saves from this instance show up here.",
            ),
            Tab::Logs => (
                IconName::SquareTerminal,
                "Nothing logged yet",
                "Start the instance to follow its console here.",
            ),
        }
    }
}

pub struct Detail {
    instance: Instance,
    tab: Tab,
    shell: WeakEntity<Shell>,
}

impl Detail {
    pub fn new(instance: Instance, shell: WeakEntity<Shell>) -> Self {
        Self {
            instance,
            tab: Tab::Mods,
            shell,
        }
    }

    pub fn id(&self) -> SharedString {
        self.instance.id.clone()
    }

    pub fn show(&mut self, instance: Instance, cx: &mut Context<Self>) {
        self.instance = instance;
        cx.notify();
    }

    fn header(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let instance = &self.instance;
        let played = instance.played();
        let latest = self
            .shell
            .upgrade()
            .and_then(|shell| shell.read(cx).latest(instance));
        let status = self
            .shell
            .upgrade()
            .map_or(instance.status(latest), |shell| {
                shell.read(cx).status(instance)
            });
        let note = match (status, instance.build, latest) {
            (Status::Installing(_), _, _) => Some(status.label()),
            (Status::Outdated, Some(have), Some(newest)) => {
                Some(format!("Update available: build {have}, newest {newest}"))
            }
            _ => None,
        };

        div()
            .flex()
            .items_center()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(56.))
                    .rounded(theme.radius_lg)
                    .bg(theme.group_box)
                    .border_1()
                    .border_color(theme.border)
                    .text_color(theme.muted_foreground)
                    .child(Icon::new(IconName::Box).size_6()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::BOLD)
                            .truncate()
                            .child(instance.name.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_4()
                            .child(Meta::new(IconName::GitBranch, instance.branch.clone()))
                            .child(Meta::new(IconName::Clock, played))
                            .children(note.map(|text| Meta::new(IconName::Download, text))),
                    )
                    .children(super::bar(&instance.id, status, cx)),
            )
            .child(
                Button::new("settings")
                    .outline()
                    .cursor_pointer()
                    .icon(IconName::Settings)
                    .size(px(40.))
                    .rounded(px(10.)),
            )
            .when(status == Status::Outdated, |this| {
                let (shell, id) = (self.shell.clone(), instance.id.clone());
                this.child(
                    Button::new("update")
                        .outline()
                        .cursor_pointer()
                        .icon(IconName::Download)
                        .label("Update")
                        .h(px(40.))
                        .rounded(px(10.))
                        .on_click(move |_, window, cx| {
                            Shell::confirm_update(shell.clone(), id.clone(), window, cx)
                        }),
                )
            })
            .child({
                let (label, icon, enabled) = status.action();
                let (shell, id) = (self.shell.clone(), instance.id.clone());
                FancyButton::new("play", label)
                    .icon(icon)
                    .when(enabled, |button| {
                        button.on_click(move |_, window, cx| {
                            Shell::play(shell.clone(), id.clone(), window, cx)
                        })
                    })
            })
    }

    fn tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .flex()
            .gap_1()
            .border_b_1()
            .border_color(theme.border)
            .children(Tab::ALL.map(|tab| {
                let active = tab == self.tab;
                div()
                    .id(tab.label())
                    .px_3()
                    .py_2()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .map(|this| {
                        if active {
                            this.text_color(theme.foreground)
                                .border_b_2()
                                .border_color(theme.primary)
                        } else {
                            this.text_color(theme.muted_foreground)
                                .hover(|s| s.text_color(theme.foreground))
                        }
                    })
                    .child(tab.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab = tab;
                        cx.notify();
                    }))
            }))
    }
}

impl Render for Detail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .gap_5()
            .p_6()
            .child(self.header(cx))
            .child(self.tabs(cx))
            .child({
                let (icon, title, line) = self.tab.empty();
                Empty::new(icon, title, line)
            })
    }
}
