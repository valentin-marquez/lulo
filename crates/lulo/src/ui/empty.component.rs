//! Empty state: icon, a title and one line saying what will show up here.

use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::*;

#[derive(IntoElement)]
pub struct Empty {
    icon: IconName,
    title: SharedString,
    line: SharedString,
}

impl Empty {
    pub fn new(
        icon: IconName,
        title: impl Into<SharedString>,
        line: impl Into<SharedString>,
    ) -> Self {
        Self {
            icon,
            title: title.into(),
            line: line.into(),
        }
    }
}

impl RenderOnce for Empty {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
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
            .child(
                Icon::new(self.icon)
                    .size_8()
                    .text_color(theme.muted_foreground),
            )
            .child(div().font_weight(FontWeight::SEMIBOLD).child(self.title))
            .child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(self.line),
            )
    }
}
