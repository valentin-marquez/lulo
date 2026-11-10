//! Thin orange bar that sweeps across the top of a view while it loads.

use std::time::Duration;

use gpui_kit::component::ActiveTheme;
use gpui_kit::*;

const WIDTH: f32 = 0.3;

#[derive(IntoElement)]
pub struct Loader;

impl RenderOnce for Loader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div().relative().w_full().h(px(2.)).overflow_hidden().child(
            div()
                .absolute()
                .top_0()
                .h_full()
                .w(relative(WIDTH))
                .rounded_full()
                .bg(cx.theme().primary)
                .with_animation(
                    "loader",
                    // Indeterminate, so it loops; eased so it never moves at a constant speed.
                    Animation::new(Duration::from_millis(1400))
                        .repeat()
                        .with_easing(ease_in_out),
                    |bar, t| bar.left(relative(-WIDTH + t * (1. + WIDTH))),
                ),
        )
    }
}
