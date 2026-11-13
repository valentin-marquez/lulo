//! Lulo's light and dark themes: the system's by default, or the one the user picked.

use std::borrow::Cow;

use gpui_kit::component::{Theme, ThemeMode, ThemeRegistry};
use gpui_kit::*;
use serde::{Deserialize, Serialize};

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

/// Which theme the app uses.
#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Whatever the system is set to, and it follows when that changes.
    #[default]
    System,
    Light,
    Dark,
}

impl Global for Mode {}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::System, Mode::Light, Mode::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Mode::System => "System",
            Mode::Light => "Light",
            Mode::Dark => "Dark",
        }
    }
}

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

/// The mode in use.
pub fn mode(cx: &App) -> Mode {
    *cx.global::<Mode>()
}

/// Switches to `mode` now and remembers it for the next start.
pub fn choose(mode: Mode, window: &mut Window, cx: &mut App) {
    preview(mode, Some(window), cx);
    crate::settings::update(|saved| saved.theme = mode);
}

/// Applies `mode` without saving it. Without a window, the system's appearance is read from
/// the app instead.
pub fn preview(mode: Mode, window: Option<&mut Window>, cx: &mut App) {
    cx.set_global(mode);
    match mode {
        Mode::System => Theme::sync_system_appearance(window, cx),
        Mode::Light => Theme::change(ThemeMode::Light, None, cx),
        Mode::Dark => Theme::change(ThemeMode::Dark, None, cx),
    }
}

/// Applies the saved mode and, in system mode, keeps the theme in step with the window's
/// light/dark appearance.
///
/// `LULO_THEME=light|dark` pins one mode instead, to work on a theme the OS isn't using.
pub fn follow_system(window: &mut Window, cx: &mut App) {
    let start = match std::env::var("LULO_THEME").as_deref() {
        Ok("light") => Mode::Light,
        Ok("dark") => Mode::Dark,
        _ => crate::settings::load().theme,
    };
    preview(start, Some(&mut *window), cx);
    window
        .observe_window_appearance(|window, cx| {
            if mode(cx) == Mode::System {
                Theme::sync_system_appearance(Some(window), cx)
            }
        })
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
