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
