//! The selected instance: header with Play, and its Mods / Worlds / Logs tabs.

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::button::Button;
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

    // ponytail: empty states until #16 fills the Mods and Worlds tabs.
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
    /// Made on first view; it needs a window for its search field.
    logs: Option<Entity<super::logs_view::Logs>>,
}

impl Detail {
    pub fn new(instance: Instance, shell: WeakEntity<Shell>) -> Self {
        Self {
            instance,
            tab: Tab::Mods,
            shell,
            logs: None,
        }
    }

    pub fn id(&self) -> SharedString {
        self.instance.id.clone()
    }

    pub fn show(&mut self, instance: Instance, cx: &mut Context<Self>) {
        if instance.id != self.instance.id {
            self.logs = None;
        }
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
            .child(super::art::art(instance, 56., theme.radius_lg, cx))
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
            .child({
                let (shell, id) = (self.shell.clone(), instance.id.clone());
                Button::new("settings")
                    .outline()
                    .cursor_pointer()
                    .icon(IconName::Settings)
                    .size(px(40.))
                    .rounded(px(10.))
                    .on_click(move |_, window, cx| {
                        Shell::settings(shell.clone(), id.clone(), window, cx)
                    })
            })
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

    /// Mods, Worlds, Logs: the underline slides between them on a spring (gpui-component's
    /// TabBar measures each tab and keeps the indicator interruptible).
    fn tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::tab::{Tab as Item, TabBar};
        let selected = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        TabBar::new("instance-tabs")
            .underline()
            .selected_index(selected)
            .children(Tab::ALL.map(|tab| Item::new().label(tab.label()).cursor_pointer()))
            .on_click(cx.listener(|this, ix: &usize, _, cx| {
                this.tab = Tab::ALL[*ix];
                cx.notify();
            }))
    }
}

impl Render for Detail {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.tab {
            Tab::Logs => {
                let cachedir = self.instance.dir.join("zomboid");
                self.logs
                    .get_or_insert_with(|| {
                        cx.new(|cx| super::logs_view::Logs::new(&cachedir, window, cx))
                    })
                    .clone()
                    .into_any_element()
            }
            tab => {
                let (icon, title, line) = tab.empty();
                Empty::new(icon, title, line).into_any_element()
            }
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .gap_5()
            .p_6()
            .child(self.header(cx))
            .child(self.tabs(cx))
            .child(body)
    }
}
