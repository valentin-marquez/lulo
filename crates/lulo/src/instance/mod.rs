//! A game install with its own build, mods and settings.

#[path = "art.view.rs"]
pub mod art;
#[path = "branches.model.rs"]
pub mod branches;
#[path = "create.view.rs"]
pub mod create;
#[path = "detail.view.rs"]
pub mod detail;
#[path = "install.model.rs"]
pub mod install;
#[path = "launch.model.rs"]
pub mod launch;
#[path = "logs.model.rs"]
pub mod logs;
#[path = "logs.view.rs"]
mod logs_view;
#[path = "store.model.rs"]
pub mod store;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use gpui_kit::assets::IconName;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::progress::Progress;
use gpui_kit::{App, IntoElement, SharedString, Styled};

/// Project Zomboid's Steam app id.
pub const APP: u32 = 108600;

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
    /// Chosen icon; `None` shows the one picked from the id.
    pub icon: Option<Icon>,
}

/// An instance's picture, stored in its `instance.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Icon {
    /// An item icon from the installed game (`Item_Axe`) on one of the palette colors.
    Item { item: String, color: u32 },
    /// A picture the user chose, copied into the instance folder under this name.
    Image { file: String },
}

/// Backgrounds for item icons, top and bottom of a vertical gradient: vivid enough for the
/// game's dark, detailed sprites (taken from Modrinth's instance icon backgrounds). An icon
/// stores the top color.
pub const BACKGROUNDS: [(u32, u32); 12] = [
    (0xD93263, 0xF65862),
    (0xFF902C, 0xFFB14E),
    (0xFFC92C, 0xFFEA4F),
    (0x76DD21, 0xC3FC4B),
    (0x10A324, 0x49CD47),
    (0x493DFF, 0x636BFF),
    (0x2683FF, 0x59BCFF),
    (0xBF59FD, 0xB985FF),
    (0xF744C3, 0xFE76ED),
    (0xB1B1B1, 0xD6D6D6),
    (0x383D4D, 0x4A4E57),
    (0x1C1E2A, 0x242630),
];

/// The gradient's bottom color for a stored top color; colors from older palettes get a
/// lighter shade of themselves.
pub fn background_bottom(top: u32) -> u32 {
    BACKGROUNDS
        .iter()
        .find(|(t, _)| *t == top)
        .map(|(_, bottom)| *bottom)
        .unwrap_or_else(|| {
            let lift = |c: u32| c + (255 - c) / 5;
            let [_, r, g, b] = top.to_be_bytes();
            u32::from_be_bytes([
                0,
                lift(r.into()) as u8,
                lift(g.into()) as u8,
                lift(b.into()) as u8,
            ])
        })
}

impl Icon {
    /// A stable pick for `seed`, so an instance keeps its look until someone chooses one.
    pub fn picked(seed: &str) -> Icon {
        // FNV-1a: tiny, stable across runs and platforms (std's hasher is randomly seeded).
        let hash = seed.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });
        let picks: Vec<&str> = crate::game::items().collect();
        Icon::Item {
            item: picks[hash as usize % picks.len()].to_owned(),
            color: BACKGROUNDS[(hash >> 32) as usize % BACKGROUNDS.len()].0,
        }
    }
}

// ponytail: Downloading lives in the shell (installs in progress), not on disk.
#[derive(Clone, Copy, PartialEq)]
pub enum Status {
    NotInstalled,
    /// Percent done.
    Installing(u8),
    Ready,
    Running,
    Outdated,
}

impl Instance {
    pub fn icon(&self) -> Icon {
        self.icon.clone().unwrap_or_else(|| Icon::picked(&self.id))
    }

    /// The game files this instance runs: its own `game/` when detached, else its shared build.
    pub fn game(&self) -> Option<PathBuf> {
        let build = self.build?;
        let own = self.dir.join("game");
        if own.is_dir() {
            return Some(own);
        }
        Some(store::builds(self.dir.parent()?).join(build.to_string()))
    }

    /// `latest` is the branch's newest build id, when known; without it an install is `Ready`.
    pub fn status(&self, latest: Option<u32>) -> Status {
        match (self.build, latest) {
            (None, _) => Status::NotInstalled,
            (Some(have), Some(newest)) if have != newest => Status::Outdated,
            _ => Status::Ready,
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
    pub fn label(self) -> String {
        match self {
            Status::NotInstalled => "Not installed".into(),
            Status::Installing(pct) => format!("Installing {pct}%"),
            Status::Ready => "Ready".into(),
            Status::Outdated => "Update available".into(),
            Status::Running => "Running".into(),
        }
    }

    /// The main button: label, icon, and whether it can be clicked.
    pub fn action(self) -> (String, IconName, bool) {
        match self {
            Status::NotInstalled => ("Install".into(), IconName::Download, true),
            // Progress shows next to it (bar and download manager), so the button offers the way out.
            Status::Installing(_) => ("Cancel".into(), IconName::Close, true),
            // Updating is its own button, after the user confirms; Play still works meanwhile.
            Status::Ready | Status::Outdated => ("Play".into(), IconName::Play, true),
            Status::Running => ("Stop".into(), IconName::Square, true),
        }
    }
}

/// The install's progress under its name, while it downloads.
pub fn bar(id: &SharedString, status: Status, cx: &App) -> Option<impl IntoElement + use<>> {
    let Status::Installing(pct) = status else {
        return None;
    };
    Some(
        Progress::new(SharedString::from(format!("progress-{id}")))
            .value(pct as f32)
            .color(cx.theme().primary)
            .mt_1(),
    )
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

#[cfg(test)]
mod tests {
    use super::{Instance, Status, ago};

    fn installed(build: Option<u32>) -> Instance {
        Instance {
            id: "a".into(),
            dir: Default::default(),
            name: "A".into(),
            branch: "public".into(),
            build,
            last_played: None,
            icon: None,
        }
    }

    #[test]
    fn picks_a_stable_icon_per_instance() {
        use super::Icon;
        assert_eq!(Icon::picked("new-instance"), Icon::picked("new-instance"));
        let picks: std::collections::HashSet<String> = ["a", "b", "c", "d", "e", "f", "g", "h"]
            .iter()
            .map(|id| format!("{:?}", Icon::picked(id)))
            .collect();
        assert!(picks.len() > 3, "ids spread over the set: {picks:?}");
    }

    #[test]
    fn marks_outdated_only_when_installed_and_behind() {
        assert!(installed(None).status(Some(2)) == Status::NotInstalled);
        assert!(installed(Some(2)).status(Some(2)) == Status::Ready);
        assert!(installed(Some(1)).status(Some(2)) == Status::Outdated);
        assert!(installed(Some(1)).status(None) == Status::Ready);
    }

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
