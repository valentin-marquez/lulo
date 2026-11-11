//! A game install with its own build, mods and settings.

#[path = "create.view.rs"]
pub mod create;
#[path = "detail.view.rs"]
pub mod detail;
#[path = "store.model.rs"]
pub mod store;

use std::path::PathBuf;

use gpui_kit::SharedString;

#[derive(Clone)]
pub struct Instance {
    /// Folder name under the instances root.
    pub id: SharedString,
    pub dir: PathBuf,
    pub name: SharedString,
    /// Steam branch the instance installs from, e.g. `public` or `legacy41`.
    pub branch: SharedString,
    /// Installed Steam build id; `None` until the game is installed.
    pub build: Option<u32>,
    /// Unix time of the last launch.
    pub last_played: Option<u64>,
}

// ponytail: Outdated and Downloading come back with installs (roadmap 5) and updates (9).
#[derive(Clone, Copy, PartialEq)]
pub enum Status {
    NotInstalled,
    Ready,
}

impl Instance {
    pub fn status(&self) -> Status {
        if self.build.is_some() {
            Status::Ready
        } else {
            Status::NotInstalled
        }
    }

    pub fn played(&self) -> String {
        match self.last_played {
            Some(at) => format!("Played {}", ago(store::now().saturating_sub(at))),
            None => "Never played".into(),
        }
    }
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::NotInstalled => "Not installed",
            Status::Ready => "Ready",
        }
    }
}

/// "just now", "5 minutes ago", "yesterday", "3 weeks ago"...
fn ago(secs: u64) -> String {
    const UNITS: [(u64, &str); 5] = [
        (60, "minute"),
        (3600, "hour"),
        (86_400, "day"),
        (604_800, "week"),
        (2_592_000, "month"),
    ];
    if secs < 60 {
        return "just now".into();
    }
    if (86_400..172_800).contains(&secs) {
        return "yesterday".into();
    }
    let (unit, name) = UNITS
        .iter()
        .rev()
        .find(|(unit, _)| secs >= *unit)
        .copied()
        .expect("secs >= 60");
    let n = secs / unit;
    format!("{n} {name}{} ago", if n == 1 { "" } else { "s" })
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

#[cfg(test)]
mod tests {
    use super::ago;

    #[test]
    fn says_how_long_ago() {
        assert_eq!(ago(10), "just now");
        assert_eq!(ago(60), "1 minute ago");
        assert_eq!(ago(2 * 3600 + 5), "2 hours ago");
        assert_eq!(ago(86_400 + 3600), "yesterday");
        assert_eq!(ago(3 * 86_400), "3 days ago");
        assert_eq!(ago(15 * 86_400), "2 weeks ago");
        assert_eq!(ago(400 * 86_400), "13 months ago");
    }
}
