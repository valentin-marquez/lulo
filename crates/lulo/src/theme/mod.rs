//! Lulo's light and dark themes, switched with the system appearance.

use std::borrow::Cow;

use gpui_kit::component::{Theme, ThemeMode, ThemeRegistry};
use gpui_kit::*;

const THEMES: &str = include_str!("lulo.theme.json");

// Bundled so text looks the same on every OS; the themes name it as `font.family`.
const FONTS: [&[u8]; 4] = [
    include_bytes!("../../../../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../../../../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../../../../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../../../../assets/fonts/Inter-Bold.ttf"),
];
const LIGHT: &str = "Lulo Light";
const DARK: &str = "Lulo Dark";

/// Registers the bundled themes and applies the one matching the system.
pub fn init(cx: &mut App) {
    cx.text_system()
        .add_fonts(FONTS.map(Cow::Borrowed).to_vec())
        .expect("bundled fonts are valid");

    let registry = ThemeRegistry::global_mut(cx);
    registry
        .load_themes_from_str(THEMES)
        .expect("bundled theme file is valid");
    let light = registry.themes()[LIGHT].clone();
    let dark = registry.themes()[DARK].clone();

    Theme::update(cx, |theme| {
        theme.light_theme = light;
        theme.dark_theme = dark;
    });
    Theme::change(cx.window_appearance(), None, cx);
}

/// Keeps the theme in step with the window's light/dark appearance.
///
/// `LULO_THEME=light|dark` pins one mode instead, to work on a theme the OS isn't using.
pub fn follow_system(window: &mut Window, cx: &mut App) {
    match std::env::var("LULO_THEME").as_deref() {
        Ok("light") => return Theme::change(ThemeMode::Light, None, cx),
        Ok("dark") => return Theme::change(ThemeMode::Dark, None, cx),
        _ => {}
    }
    Theme::sync_system_appearance(Some(window), cx);
    window
        .observe_window_appearance(|window, cx| Theme::sync_system_appearance(Some(window), cx))
        .detach();
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::ThemeSet;

    #[test]
    fn bundles_light_and_dark() {
        let set: ThemeSet = serde_json::from_str(super::THEMES).expect("theme json parses");
        let modes: Vec<_> = set
            .themes
            .iter()
            .map(|t| (t.name.as_ref(), t.mode.is_dark()))
            .collect();
        assert_eq!(modes, [(super::LIGHT, false), (super::DARK, true)]);
    }
}
