//! Shared building blocks used across features.

#[path = "empty.component.rs"]
mod empty;
#[path = "fancy-button.component.rs"]
mod fancy_button;
#[path = "loader.component.rs"]
mod loader;
#[path = "meta.component.rs"]
mod meta;

use std::time::Duration;

use gpui_kit::base::Spring;
use gpui_kit::component::ActiveTheme;
use gpui_kit::{App, Hsla};

pub use empty::Empty;
pub use fancy_button::FancyButton;
pub use loader::Loader;
pub use meta::Meta;

/// Default motion for anything that reacts to the user: critically damped, no overshoot.
/// Interactions animate with springs, never with linear timing.
pub const SPRING: Spring = Spring::new(Duration::from_millis(300));

/// Hover and press feedback: quicker than [`SPRING`] so the UI answers the pointer at once.
pub const SPRING_FAST: Spring = Spring::new(Duration::from_millis(150));

/// Press feedback (the face sinking under the pointer): quicker still, about Modrinth's 100 ms.
pub const SPRING_PRESS: Spring = Spring::new(Duration::from_millis(100));

/// One-shot entrances (a step, a panel, a new icon): transitions.dev's 200 ms page slide.
/// Springs need a target that changes; an element that just appeared has none, so these use a
/// keyed `Animation` that restarts whenever its id changes.
pub const ENTER: Duration = Duration::from_millis(200);

/// transitions.dev's `cubic-bezier(0.22, 1, 0.36, 1)`: fast start, long soft landing.
pub fn ease_out_quint(t: f32) -> f32 {
    1. - (1. - t).powi(5)
}

/// The color a surface takes under the pointer: darker in light themes, lighter in dark ones,
/// like Modrinth's `brightness()` hover (0.9 and 1.25). GPUI has no filters, so it is computed.
pub fn hovered(color: Hsla, cx: &App) -> Hsla {
    let factor = if cx.theme().is_dark() { 1.25 } else { 0.9 };
    Hsla {
        l: (color.l * factor).clamp(0., 1.),
        ..color
    }
}

/// `from` moved `t` of the way to `to`; springs drive `t`, so colors ease without linear timing.
pub fn mix(from: Hsla, to: Hsla, t: f32) -> Hsla {
    let t = t.clamp(0., 1.);
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    Hsla {
        h: lerp(from.h, to.h),
        s: lerp(from.s, to.s),
        l: lerp(from.l, to.l),
        a: lerp(from.a, to.a),
    }
}

/// A section in a dialog's left navigation (settings, instance settings): icon and label,
/// tinted with the accent when it is the one shown.
pub fn nav_item(
    label: &'static str,
    icon: gpui_kit::assets::IconName,
    active: bool,
    cx: &App,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    use gpui_kit::component::Icon;
    use gpui_kit::prelude::FluentBuilder as _;
    use gpui_kit::*;
    let theme = cx.theme();
    div()
        .id(label)
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_2()
        .rounded(theme.radius)
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .when(active, |this| {
            this.bg(theme.primary.opacity(0.15))
                .text_color(theme.primary)
        })
        .when(!active, |this| {
            this.text_color(theme.muted_foreground)
                .hover(|style| style.bg(theme.sidebar_accent))
        })
        .child(Icon::new(icon).size_4())
        .child(label)
}

/// The quiet button beside a primary one (Cancel, Keep, Back): no fill or border, and the
/// small `FancyButton`'s height and corners so the two line up.
pub fn secondary(id: &'static str, label: &'static str) -> gpui_kit::component::button::Button {
    use gpui_kit::component::button::{Button, ButtonVariants};
    use gpui_kit::*;
    Button::new(id)
        .ghost()
        .cursor_pointer()
        .label(label)
        .h(px(36.))
        .px_3()
        .rounded(px(8.))
}

/// The tray a row of pill tabs sits in: the corners of an input, with its own fill and border,
/// so the tabs read as one control and not as loose words.
pub fn tab_tray(tabs: impl gpui_kit::IntoElement, cx: &App) -> gpui_kit::Div {
    use gpui_kit::*;
    let theme = cx.theme();
    div()
        .flex_none()
        .p_1()
        .rounded(theme.radius)
        .bg(theme.background)
        .border_1()
        .border_color(theme.border)
        .child(tabs)
}

/// One-shot entrance for content that replaces other content in place (a dialog's section):
/// fades in rising a few px. `key` changes to replay it.
pub fn enter(content: gpui_kit::Div, key: &'static str) -> impl gpui_kit::IntoElement {
    use gpui_kit::*;
    content.with_animation(
        key,
        Animation::new(crate::ui::ENTER).with_easing(crate::ui::ease_out_quint),
        |content, t| content.relative().top(px(6. * (1. - t))).opacity(t),
    )
}

/// Toasts by what they say, each with its own icon. They talk like a friend would, the way
/// Discord does: short, warm, a little playful, and still saying what happened and what to do.
pub mod toast {
    use gpui_kit::SharedString;
    pub use gpui_kit::component::notification::Notification;

    pub fn done(text: impl Into<SharedString>) -> Notification {
        Notification::success(text)
    }

    pub fn failed(text: impl Into<SharedString>) -> Notification {
        Notification::error(text)
    }

    /// What failed on the first line, and why (the error itself) under it.
    pub fn failed_because(what: &'static str, why: impl ToString) -> Notification {
        Notification::error(why.to_string()).title(what)
    }

    /// Play was pressed with Steam closed: says so, with a button that opens it.
    pub fn steam_closed() -> Notification {
        use gpui_kit::component::button::Button;
        use gpui_kit::*;
        Notification::warning("Wake it up, then hit Play again")
            .title("Oops! Steam is taking a nap")
            .action(|_, _, cx| {
                Button::new("open-steam")
                    .cursor_pointer()
                    .label("Open Steam")
                    .on_click(cx.listener(|toast, _, window, cx| {
                        cx.open_url("steam://open/main");
                        toast.dismiss(window, cx);
                    }))
            })
            // A toast with a button stays by default; this one is only a reminder.
            .autohide(true)
    }

    /// Something worked, with a catch the user should know about.
    pub fn heads_up(what: &'static str, detail: impl ToString) -> Notification {
        Notification::warning(detail.to_string()).title(what)
    }

    pub fn working(text: impl Into<SharedString>) -> Notification {
        Notification::info(text)
    }
}

#[cfg(test)]
mod tests {
    use super::mix;
    use gpui_kit::hsla;

    #[test]
    fn mixes_colors_by_the_spring_value() {
        let (a, b) = (hsla(0., 0., 0.2, 1.), hsla(0., 0., 0.6, 0.5));
        assert_eq!(mix(a, b, 0.), a);
        assert_eq!(mix(a, b, 1.), b);
        let half = mix(a, b, 0.5);
        assert!((half.l - 0.4).abs() < 1e-6 && (half.a - 0.75).abs() < 1e-6);
        assert_eq!(mix(a, b, 2.), b, "springs may overshoot; colors don't");
    }
}
