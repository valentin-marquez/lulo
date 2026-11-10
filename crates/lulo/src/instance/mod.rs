//! A game install with its own build, mods and settings.

#[path = "create.view.rs"]
pub mod create;
#[path = "detail.view.rs"]
pub mod detail;

use gpui_kit::SharedString;

#[derive(Clone)]
pub struct Instance {
    pub id: SharedString,
    pub name: SharedString,
    /// Steam branch the instance installs from, e.g. `public` or `legacy41`.
    pub branch: SharedString,
    pub status: Status,
    pub last_played: Option<SharedString>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Status {
    Ready,
    /// The branch has a newer build than the one installed.
    Outdated,
    /// Installing; percent done.
    Downloading(u8),
}

// ponytail: placeholder data until instances are read from disk.
pub fn samples() -> Vec<Instance> {
    let make = |id: &str, name: &str, branch: &str, status, last: Option<&str>| Instance {
        id: id.to_owned().into(),
        name: name.to_owned().into(),
        branch: branch.to_owned().into(),
        status,
        last_played: last.map(|l| l.to_owned().into()),
    };
    vec![
        make(
            "vanilla",
            "Vanilla",
            "public",
            Status::Ready,
            Some("2 days ago"),
        ),
        make(
            "brita",
            "Brita's Armory",
            "public",
            Status::Outdated,
            Some("last week"),
        ),
        make(
            "legacy",
            "Legacy MP",
            "legacy41",
            Status::Ready,
            Some("last month"),
        ),
        make(
            "fresh",
            "New instance",
            "public",
            Status::Downloading(38),
            None,
        ),
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
