//! Window frame: title bar, navigation and instances on the left, the current page on the right.

#[path = "sidebar.view.rs"]
mod sidebar;
#[path = "topbar.view.rs"]
mod topbar;

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::instance::{self, Instance, detail::Detail};
use crate::ui::Loader;
use crate::{home, screenshots};
use topbar::Topbar;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Home,
    Screenshots,
    Instance(usize),
}

pub struct Shell {
    instances: Vec<Instance>,
    page: Page,
    detail: Entity<Detail>,
    /// Shows the loader under the title bar while the page's data loads.
    // ponytail: nothing loads yet; instances from disk (roadmap step 2) will set it.
    loading: bool,
}

impl Shell {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let instances = instance::samples();
        let first = instances[0].clone();
        Self {
            instances,
            page: Page::Home,
            detail: cx.new(|_| Detail::new(first)),
            loading: false,
        }
    }

    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    pub fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        if let Page::Instance(ix) = page {
            let instance = self.instances[ix].clone();
            self.detail
                .update(cx, |detail, cx| detail.show(instance, cx));
        }
        self.page = page;
        cx.notify();
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = match self.page {
            Page::Home => home::render(self, cx).into_any_element(),
            Page::Screenshots => screenshots::render().into_any_element(),
            Page::Instance(_) => self.detail.clone().into_any_element(),
        };

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
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .rounded_tl(px(14.))
                            .border_t_1()
                            .border_l_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().muted)
                            .overflow_hidden()
                            .when(self.loading, |this| this.child(Loader))
                            .child(div().flex_1().min_h_0().child(page)),
                    ),
            )
    }
}
