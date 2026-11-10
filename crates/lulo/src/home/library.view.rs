//! "Library" grid: every instance, searchable.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme, Icon, Sizable};
use gpui_kit::*;

use crate::instance::{self, Instance};
use crate::ui::FancyButton;

const TILE: Pixels = px(168.);

#[derive(IntoElement)]
pub struct Library {
    instances: Vec<Instance>,
    search: Entity<InputState>,
}

impl Library {
    pub fn new(instances: Vec<Instance>, search: Entity<InputState>) -> Self {
        Self { instances, search }
    }
}

fn tile(instance: Instance, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let detail = match instance.mods {
        0 => format!("Build {}", instance.build),
        n => format!("Build {}  ·  {n} mods", instance.build),
    };

    div()
        .id(instance.id.clone())
        .flex()
        .flex_col()
        .gap_2()
        .w(TILE)
        .p_2()
        .rounded(theme.radius_lg)
        .bg(theme.group_box)
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        .hover(|s| s.border_color(theme.primary))
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .h(TILE - px(16.))
                .rounded(theme.radius)
                .bg(theme.muted)
                .text_color(theme.muted_foreground)
                .child(Icon::new(IconName::Box).size(px(56.))),
        )
        .child(
            div()
                .px_1()
                .pb_1()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(instance.name),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .child(detail),
                ),
        )
}

impl RenderOnce for Library {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::BOLD)
                    .child("Library"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div().flex_1().child(
                            Input::new(&self.search)
                                .prefix(Icon::new(IconName::Search).size_4())
                                .cleanable(true),
                        ),
                    )
                    .child(
                        FancyButton::new("create", "Create instance")
                            .small()
                            .icon(IconName::Plus)
                            .on_click(|_, window, cx| instance::create::open(window, cx)),
                    ),
            )
            .child(
                div().flex().gap_2().child(
                    Button::new("sort")
                        .outline()
                        .small()
                        .icon(IconName::ArrowUpDown)
                        .label("Last played")
                        .dropdown_caret(true),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_3()
                    .children(self.instances.into_iter().map(|i| tile(i, cx))),
            )
    }
}
