//! A game install with its own build, mods and settings.

#[path = "create.view.rs"]
pub mod create;

use gpui_kit::SharedString;

#[derive(Clone)]
pub struct Instance {
    pub id: SharedString,
    pub name: SharedString,
    /// Human-readable game build, e.g. `42.12`.
    pub build: SharedString,
    pub mods: usize,
    pub last_played: Option<SharedString>,
}

// ponytail: placeholder data until instances are read from disk.
pub fn samples() -> Vec<Instance> {
    let make = |id: &str, name: &str, build: &str, mods, last: Option<&str>| Instance {
        id: id.to_owned().into(),
        name: name.to_owned().into(),
        build: build.to_owned().into(),
        mods,
        last_played: last.map(|l| l.to_owned().into()),
    };
    vec![
        make("vanilla", "Vanilla", "42.12", 0, Some("2 days ago")),
        make("brita", "Brita's Armory", "42.12", 38, Some("last week")),
        make("legacy", "Legacy MP", "41.78.21", 112, Some("last month")),
        make("fresh", "New instance", "42.12", 0, None),
    ]
}

/// A Steam branch of the game; picking one picks the build an instance installs.
#[derive(Clone)]
pub struct Branch {
    pub name: &'static str,
    pub description: &'static str,
    pub build: &'static str,
    pub updated: &'static str,
}

// ponytail: snapshot of PICS appinfo for app 108600 (2026-11-10); read it live once the
// Steam client lands.
pub fn branches() -> Vec<Branch> {
    let make = |name, description, build, updated| Branch {
        name,
        description,
        build,
        updated,
    };
    vec![
        make("public", "Latest release", "26486631", "Nov 6, 2026"),
        make(
            "pre-release",
            "Pre-release testing",
            "26486631",
            "Nov 6, 2026",
        ),
        make("42.21", "Temporary fall-back", "25485521", "Sep 23, 2026"),
        make("42.19", "Build 42.19.2", "24929653", "Aug 25, 2026"),
        make("legacy41", "Build 41.78.21", "24928713", "Aug 25, 2026"),
    ]
}
