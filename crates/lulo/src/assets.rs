//! Asset source: our icons (Solar), the logo, and gpui-kit's bundled set as a fallback.

use std::borrow::Cow;

use gpui_kit::assets::{Assets, icon_assets};
use gpui_kit::*;

/// Solar icons (480 Design, CC BY 4.0, `assets/icons/ui/LICENSE-SOLAR.md`) saved under the
/// Lucide names gpui-kit and gpui-component ask for, so every icon in the app, the components'
/// own included, comes from one set: Bold for app icons, Linear for the small glyphs inside
/// controls (chevrons, check, close), where filled shapes read as blobs. Add one by saving the
/// Solar SVG as `assets/icons/ui/<lucide name>.svg` and listing it here.
macro_rules! ui_icons {
    ($($name:literal),* $(,)?) => {
        const UI_ICONS: &[&str] = &[$($name),*];

        fn ui_icon(path: &str) -> Option<&'static [u8]> {
            match path.strip_prefix("icons/")?.strip_suffix(".svg")? {
                $($name => Some(include_bytes!(concat!("../../../assets/icons/ui/", $name, ".svg"))),)*
                _ => None,
            }
        }
    };
}

ui_icons!(
    "arrow-down",
    "arrow-down-wide-narrow",
    "box",
    "check",
    "chevron-down",
    "chevron-left",
    "chevron-right",
    "chevron-up",
    "chevrons-up-down",
    "circle-check",
    "circle-user",
    "circle-x",
    "clock",
    "close",
    "compass",
    "copy",
    "cpu",
    "download",
    "external-link",
    "eye",
    "folder-open",
    "gamepad",
    "git-branch",
    "globe",
    "history",
    "house",
    "image",
    "info",
    "layers",
    "library",
    "loader-circle",
    "log-out",
    "minus",
    "moon-sleep",
    "palette",
    "play",
    "plus",
    "puzzle",
    "radio",
    "rocket",
    "search",
    "settings",
    "settings-2",
    "square",
    "square-terminal",
    "trash",
    "triangle-alert",
    "undo-2",
);

// Fallback for icons gpui-component draws that we haven't replaced yet.
icon_assets!(
    Icons,
    [
        Box, Clock, Compass, Download, GitBranch, House, Image, Puzzle
    ]
);

pub const LOGO: &str = "brand/lulo-64.png";

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == LOGO {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icons/png/lulo-64.png"
            ))));
        }
        if let Some(item) = path
            .strip_prefix("instance-icons/")
            .and_then(|p| p.strip_suffix(".png"))
            && let Some(bytes) = crate::game::icon(item)
        {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        if let Some(bytes) = ui_icon(path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        if let Some(bytes) = Icons.load(path)? {
            return Ok(Some(bytes));
        }
        Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = Assets.list(path)?;
        paths.extend(Icons.list(path)?);
        paths.extend(
            UI_ICONS
                .iter()
                .map(|name| format!("icons/{name}.svg"))
                .filter(|p| p.starts_with(path))
                .map(SharedString::from),
        );
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: the gpui glob brings its own `test` attribute, which would shadow Rust's.
    use super::ui_icon;

    #[test]
    fn every_icon_file_is_served() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/icons/ui");
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(icon) = name.strip_suffix(".svg") {
                assert!(
                    ui_icon(&format!("icons/{icon}.svg")).is_some(),
                    "{name} is not listed in ui_icons!"
                );
            }
        }
        assert!(ui_icon("icons/nope.svg").is_none());
    }
}
