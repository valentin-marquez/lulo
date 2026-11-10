//! Primary call-to-action button, modelled on AlignUI's FancyButton.
//!
//! Accent fill with a 1px accent ring and a soft drop shadow, a white gloss that fades
//! top to bottom (stronger on hover) and a faint highlight along the top edge.

use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

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
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let accent = cx.theme().primary;
        let foreground = cx.theme().primary_foreground;
        let radius = self.size.radius();
        let group = SharedString::from(format!("fancy-{}", self.id));

        div()
            .id(self.id)
            .group(group.clone())
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap_2()
            .h(self.size.height())
            .px(self.size.padding())
            .rounded(radius)
            .bg(accent)
            .text_color(foreground)
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .shadow(vec![
                BoxShadow {
                    color: accent,
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(1.),
                    inset: false,
                },
                BoxShadow {
                    color: hsla(220. / 360., 0.32, 0.08, 0.24),
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
                    .opacity(0.16)
                    .group_hover(group, |style| style.opacity(0.24)),
            )
            // ponytail: AlignUI masks a 1px gradient ring; a top border is the visible part of it.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(radius)
                    .border_t_1()
                    .border_color(white().opacity(0.12)),
            )
            .children(
                self.icon
                    .map(|icon| div().relative().mx(px(-4.)).child(icon.size_5())),
            )
            .child(div().relative().child(self.label))
            .when_some(self.on_click, |this, handler| this.on_click(handler))
    }
}
