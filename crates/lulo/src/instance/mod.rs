//! A game install with its own build, mods and settings.

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
