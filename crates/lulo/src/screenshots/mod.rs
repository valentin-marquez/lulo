//! Screenshots taken in any instance, in one place.

use gpui_kit::assets::IconName;
use gpui_kit::*;

use crate::ui::Empty;

// ponytail: empty until instances live on disk; PZ saves them under each cachedir's Screenshots/.
pub fn render() -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .gap_5()
        .p_6()
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::BOLD)
                .child("Screenshots"),
        )
        .child(Empty::new(
            IconName::Image,
            "No screenshots yet",
            "Screenshots you take in any instance show up here.",
        ))
}
