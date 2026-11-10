//! The selected instance: header with Play, and its Mods / Worlds / Logs tabs.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Instance;
use crate::ui::FancyButton;

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
}

impl Detail {
    pub fn new(instance: Instance) -> Self {
        Self {
            instance,
            tab: Tab::Mods,
        }
    }

    pub fn show(&mut self, instance: Instance, cx: &mut Context<Self>) {
        self.instance = instance;
        cx.notify();
    }

    fn header(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let instance = &self.instance;
        let played = match &instance.last_played {
            Some(when) => format!("Last played {when}"),
            None => "Never played".into(),
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
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(format!("{}  ·  {played}", instance.branch)),
                    ),
            )
            .child(
                Button::new("settings")
                    .outline()
                    .icon(IconName::Settings)
                    .size(px(40.))
                    .rounded(px(10.)),
            )
            .child(FancyButton::new("play", "Play").icon(IconName::Play))
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

    fn empty(&self, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let (icon, title, line) = self.tab.empty();
        div()
            .flex()
            .flex_col()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_2()
            .rounded(theme.radius_lg)
            .bg(theme.group_box)
            .border_1()
            .border_color(theme.border)
            .child(Icon::new(icon).size_8().text_color(theme.muted_foreground))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(line),
            )
            .when(self.tab == Tab::Logs, |this| {
                this.font_family(theme.mono_font_family.clone())
            })
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
            .child(self.empty(cx))
    }
}
