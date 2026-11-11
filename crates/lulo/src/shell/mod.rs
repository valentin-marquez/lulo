//! Window frame: title bar, navigation and instances on the left, the current page on the right.

#[path = "sidebar.view.rs"]
mod sidebar;
#[path = "topbar.view.rs"]
mod topbar;

use std::path::PathBuf;

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::instance::{self, Instance, detail::Detail, store};
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
    root: PathBuf,
    instances: Vec<Instance>,
    page: Page,
    detail: Option<Entity<Detail>>,
    /// Shows the loader under the title bar while the page's data loads.
    // ponytail: listing instances is instant; installs (roadmap 5) will set it.
    loading: bool,
}

impl Shell {
    pub fn new(_: &mut Context<Self>) -> Self {
        let root = store::root();
        Self {
            instances: store::list(&root),
            root,
            page: Page::Home,
            detail: None,
            loading: false,
        }
    }

    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    pub fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        if let Page::Instance(ix) = page {
            let Some(instance) = self.instances.get(ix).cloned() else {
                return;
            };
            match &self.detail {
                Some(detail) => detail.update(cx, |d, cx| d.show(instance, cx)),
                None => self.detail = Some(cx.new(|_| Detail::new(instance))),
            }
        }
        self.page = page;
        cx.notify();
    }

    /// Opens the create dialog; a created instance is listed and selected.
    pub fn create(shell: WeakEntity<Shell>, window: &mut Window, cx: &mut App) {
        let Ok(root) = shell.read_with(cx, |shell, _| shell.root.clone()) else {
            return;
        };
        instance::create::open(
            root,
            move |created, _, cx| {
                let _ = shell.update(cx, |shell, cx| {
                    shell.instances = store::list(&shell.root);
                    let ix = shell.instances.iter().position(|i| i.id == created.id);
                    shell.go(ix.map_or(Page::Home, Page::Instance), cx);
                });
            },
            window,
            cx,
        );
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = match (self.page, &self.detail) {
            (Page::Instance(_), Some(detail)) => detail.clone().into_any_element(),
            (Page::Screenshots, _) => screenshots::render().into_any_element(),
            _ => home::render(self, cx).into_any_element(),
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
