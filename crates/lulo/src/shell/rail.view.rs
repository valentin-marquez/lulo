//! Left navigation rail: pages, recent instances, settings.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::instance::{self, Instance};

const WIDTH: Pixels = px(60.);
const ITEM: Pixels = px(40.);

#[derive(IntoElement)]
pub struct Rail {
    recent: Vec<Instance>,
}

impl Rail {
    pub fn new(recent: Vec<Instance>) -> Self {
        Self { recent }
    }
}

fn item(id: impl Into<ElementId>, icon: IconName, active: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(ITEM)
        .rounded_full()
        .cursor_pointer()
        .map(|this| {
            if active {
                this.bg(theme.primary).text_color(theme.primary_foreground)
            } else {
                this.text_color(theme.sidebar_foreground)
                    .hover(|s| s.bg(theme.sidebar_accent))
            }
        })
        .child(Icon::new(icon).size_5())
}

fn divider(cx: &App) -> Div {
    div().w(px(24.)).h(px(1.)).my_1().bg(cx.theme().border)
}

impl RenderOnce for Rail {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();

        div()
            .flex()
            .flex_col()
            .flex_none()
            .items_center()
            .justify_between()
            .w(WIDTH)
            .py_2()
            .bg(theme.sidebar)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(item("home", IconName::Play, true, cx))
                    .child(item("discover", IconName::Compass, false, cx))
                    .child(divider(cx))
                    .children(self.recent.into_iter().map(|instance| {
                        div()
                            .id(instance.id.clone())
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(ITEM)
                            .rounded(px(10.))
                            .cursor_pointer()
                            .text_color(theme.muted_foreground)
                            .hover(|s| s.bg(theme.sidebar_accent))
                            .child(Icon::new(IconName::Box).size_5())
                    }))
                    .child(divider(cx))
                    .child(
                        item("new", IconName::Plus, false, cx)
                            .on_click(|_, window, cx| instance::create::open(window, cx)),
                    ),
            )
            .child(item("settings", IconName::Settings, false, cx))
    }
}
