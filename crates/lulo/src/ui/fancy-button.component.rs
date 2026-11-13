//! Primary call-to-action button, modelled on AlignUI's FancyButton.
//!
//! Accent fill with a 1px accent ring and a soft drop shadow, a white gloss that fades
//! top to bottom (stronger on hover) and a faint highlight along the top edge. Pressing
//! shrinks the face to 97% like Modrinth's buttons: GPUI can't scale a subtree, so the fill
//! layer insets itself while the label stays put (3% of the text is under half a pixel).

use gpui_kit::base::spring;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{SPRING, SPRING_PRESS};

#[derive(Clone, Copy, Default)]
pub enum Size {
    #[default]
    Medium,
    Small,
}

impl Size {
    fn height(self) -> Pixels {
        match self {
            Size::Medium => px(40.),
            Size::Small => px(36.),
        }
    }

    fn padding(self) -> Pixels {
        match self {
            Size::Medium => px(14.),
            Size::Small => px(12.),
        }
    }

    fn radius(self) -> Pixels {
        match self {
            Size::Medium => px(10.),
            Size::Small => px(8.),
        }
    }
}

#[derive(IntoElement)]
pub struct FancyButton {
    id: ElementId,
    label: SharedString,
    icon: Option<Icon>,
    size: Size,
    on_click: Option<ClickHandler>,
}

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

impl FancyButton {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            size: Size::default(),
            on_click: None,
        }
    }

    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn small(mut self) -> Self {
        self.size = Size::Small;
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for FancyButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let hovered = window.use_keyed_state(
            SharedString::from(format!("fancy-hover-{}", self.id)),
            cx,
            |_, _| false,
        );
        let gloss = spring(
            SharedString::from(format!("fancy-gloss-{}", self.id)),
            if *hovered.read(cx) { 0.24 } else { 0.16 },
            SPRING,
            window,
            cx,
        );
        let pressed = window.use_keyed_state(
            SharedString::from(format!("fancy-pressed-{}", self.id)),
            cx,
            |_, _| false,
        );
        let press: f32 = spring(
            SharedString::from(format!("fancy-press-{}", self.id)),
            if *pressed.read(cx) { 1. } else { 0. },
            SPRING_PRESS,
            window,
            cx,
        );
        // 1.5% per side of the height, and about that of a typical width.
        let (inset_x, inset_y) = (px(1.8 * press), self.size.height() * (0.015 * press));
        let accent = cx.theme().primary;
        let foreground = cx.theme().primary_foreground;
        let radius = self.size.radius();
        div()
            .id(self.id)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap_2()
            .h(self.size.height())
            .px(self.size.padding())
            .text_color(foreground)
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .on_hover(move |is_hovered, window, cx| {
                hovered.update(cx, |h, _| *h = *is_hovered);
                window.refresh();
            })
            .on_mouse_down(MouseButton::Left, {
                let pressed = pressed.clone();
                move |_, window, cx| {
                    pressed.update(cx, |p, _| *p = true);
                    window.refresh();
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let pressed = pressed.clone();
                move |_, window, cx| {
                    pressed.update(cx, |p, _| *p = false);
                    window.refresh();
                }
            })
            .on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                pressed.update(cx, |p, _| *p = false);
                window.refresh();
            })
            // The face: fill, ring, shadow, gloss and highlight, inset while pressed.
            .child(
                div()
                    .absolute()
                    .top(inset_y)
                    .bottom(inset_y)
                    .left(inset_x)
                    .right(inset_x)
                    .rounded(radius)
                    .bg(accent)
                    .shadow(vec![
                        BoxShadow {
                            color: accent,
                            offset: point(px(0.), px(0.)),
                            blur_radius: px(0.),
                            spread_radius: px(1.),
                            inset: false,
                        },
                        // The drop shadow goes as the face sinks.
                        BoxShadow {
                            color: hsla(220. / 360., 0.32, 0.08, 0.24 * (1. - press)),
                            offset: point(px(0.), px(1.)),
                            blur_radius: px(2.),
                            spread_radius: px(0.),
                            inset: false,
                        },
                    ])
                    // Gloss: white fading to transparent, 16% at rest and 24% on hover.
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded(radius)
                            .bg(linear_gradient(
                                180.,
                                linear_color_stop(white(), 0.),
                                linear_color_stop(white().opacity(0.), 1.),
                            ))
                            .opacity(gloss * (1. - press)),
                    )
                    // ponytail: AlignUI masks a 1px gradient ring; a top border is the
                    // visible part of it.
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded(radius)
                            .border_t_1()
                            .border_color(white().opacity(0.12)),
                    ),
            )
            .children(
                self.icon
                    .map(|icon| div().relative().mx(px(-4.)).child(icon.size_5())),
            )
            .child(div().relative().child(self.label))
            .when_some(self.on_click, |this, handler| this.on_click(handler))
    }
}
