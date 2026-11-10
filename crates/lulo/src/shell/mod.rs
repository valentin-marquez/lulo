//! Window frame: title bar, instance list on the left, the selected instance on the right.

#[path = "sidebar.view.rs"]
mod sidebar;
#[path = "topbar.view.rs"]
mod topbar;

use gpui_kit::component::ActiveTheme;
use gpui_kit::*;

use crate::instance::{self, Instance, detail::Detail};
use topbar::Topbar;

pub struct Shell {
    instances: Vec<Instance>,
    selected: usize,
    detail: Entity<Detail>,
}

impl Shell {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let instances = instance::samples();
        let first = instances[0].clone();
        Self {
            instances,
            selected: 0,
            detail: cx.new(|_| Detail::new(first)),
        }
    }

    fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.selected = ix;
        let instance = self.instances[ix].clone();
        self.detail
            .update(cx, |detail, cx| detail.show(instance, cx));
        cx.notify();
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().title_bar)
            .text_color(cx.theme().foreground)
            .child(Topbar)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(sidebar::render(self, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .rounded_tl(px(14.))
                            .border_t_1()
                            .border_l_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().muted)
                            .overflow_hidden()
                            .child(self.detail.clone()),
                    ),
            )
    }
}
