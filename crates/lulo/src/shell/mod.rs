//! Window frame: title bar on top, navigation rail on the left, the page on the right.

#[path = "rail.view.rs"]
mod rail;
#[path = "topbar.view.rs"]
mod topbar;

use gpui_kit::component::ActiveTheme;
use gpui_kit::*;

use crate::home::Home;
use crate::instance;
use rail::Rail;
use topbar::Topbar;

pub struct Shell {
    home: Entity<Home>,
}

impl Shell {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let instances = instance::samples();
        Self {
            home: cx.new(|cx| Home::new(instances, window, cx)),
        }
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let recent = self.home.read(cx).recent();

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
                    .child(Rail::new(recent))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .rounded_tl(px(14.))
                            .border_t_1()
                            .border_l_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().background)
                            .overflow_hidden()
                            .child(self.home.clone()),
                    ),
            )
    }
}
