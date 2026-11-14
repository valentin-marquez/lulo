//! Installing an instance's branch: depots for this OS, downloaded and verified into the shared
//! build folder (or the instance's own `game/` when detached).

use std::time::Instant;

use gpui_kit::SharedString;
use steam::{content, pics};
use tokio::sync::mpsc::UnboundedSender;

use super::{APP, Instance, store};
use crate::account::Account;
use crate::runtime;

pub enum Event {
    Progress {
        done: u64,
        total: u64,
    },
    /// The build is recorded in the instance; reload it from disk.
    Done,
    Failed(String),
}

/// What a task in the download manager is doing.
#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Game,
    Mods,
    /// Deleting an instance: bytes freed instead of downloaded, and no cancel, since half a
    /// deleted instance is worth less than none.
    Delete,
}

/// An install (or a deletion) in progress, as the download manager shows it.
pub struct Download {
    pub name: SharedString,
    pub kind: Kind,
    pub done: u64,
    pub total: u64,
    /// Bytes per second over the last second or so; 0 until measured.
    pub speed: u64,
    sample: Option<(Instant, u64)>,
    cancel: Option<tokio::task::AbortHandle>,
}

impl Download {
    /// `cancel` stops it; `None` for what can't be stopped halfway.
    pub fn new(name: SharedString, kind: Kind, cancel: Option<tokio::task::AbortHandle>) -> Self {
        Self {
            name,
            kind,
            done: 0,
            total: 0,
            speed: 0,
            sample: None,
            cancel,
        }
    }

    /// Records progress; returns whether anything shown changed (percent or speed).
    pub fn progress(&mut self, done: u64, total: u64, now: Instant) -> bool {
        let before = (self.pct(), self.speed);
        (self.done, self.total) = (done, total);
        match self.sample {
            // The first report can be files kept from an earlier install, not downloaded now.
            None => self.sample = Some((now, done)),
            Some((at, from)) => {
                let elapsed = now.duration_since(at).as_secs_f64();
                if elapsed >= 1. {
                    self.speed = (done.saturating_sub(from) as f64 / elapsed) as u64;
                    self.sample = Some((now, done));
                }
            }
        }
        before != (self.pct(), self.speed)
    }

    /// Percent done, held at 99 until the build is recorded.
    pub fn pct(&self) -> u8 {
        (self.done * 100)
            .checked_div(self.total)
            .unwrap_or(0)
            .min(99) as u8
    }

    /// "4 min left", once the speed is known.
    pub fn left(&self) -> Option<String> {
        let secs = self
            .total
            .saturating_sub(self.done)
            .checked_div(self.speed)?;
        Some(match secs {
            0..60 => format!("{secs} s left"),
            60..3600 => format!("{} min left", secs.div_ceil(60)),
            _ => format!("{} h {} min left", secs / 3600, secs % 3600 / 60),
        })
    }

    pub fn can_cancel(&self) -> bool {
        self.cancel.is_some()
    }

    pub fn cancel(&self) {
        if let Some(cancel) = &self.cancel {
            cancel.abort();
        }
    }
}

/// "512 KB", "3.4 GB": decimal units, like Steam shows them.
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    if value < 1000. {
        return format!("{bytes} B");
    }
    let mut unit = 0;
    value /= 1000.;
    while value >= 1000. && unit < UNITS.len() - 1 {
        value /= 1000.;
        unit += 1;
    }
    if value < 10. {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

/// The OS name appinfo uses in `oslist`.
fn os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

/// Runs the install on the background runtime, reporting through `events`. Aborting the
/// returned handle cancels it; the files written so far are kept for the next install to reuse.
pub fn start(
    account: Account,
    instance: Instance,
    events: UnboundedSender<Event>,
) -> tokio::task::AbortHandle {
    runtime::spawn(async move {
        let event = match install(&account, &instance, &events).await {
            Ok(build) => match store::set_build(&instance.dir, build) {
                Ok(()) => {
                    // The build this instance ran before may now be unused.
                    if let Some(root) = instance.dir.parent() {
                        let _ = store::prune(root);
                    }
                    Event::Done
                }
                Err(err) => Event::Failed(format!("installed, but couldn't record it: {err}")),
            },
            Err(err) => Event::Failed(err),
        };
        let _ = events.send(event);
    })
    .abort_handle()
}

/// The lock for installing into `game`, shared by every install that targets that folder.
fn lock(game: &std::path::Path) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    use std::collections::HashMap;
    use std::sync::{Arc, LazyLock, Mutex};
    static LOCKS: LazyLock<Mutex<HashMap<std::path::PathBuf, Arc<tokio::sync::Mutex<()>>>>> =
        LazyLock::new(Default::default);
    let mut locks = LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    locks.entry(game.to_owned()).or_default().clone()
}

async fn install(
    account: &Account,
    instance: &Instance,
    events: &UnboundedSender<Event>,
) -> Result<u32, String> {
    let http = steam::http();
    let mut client = account.client(&http).await.map_err(|e| e.to_string())?;
    let info = pics::app_info(&mut client, &http, APP)
        .await
        .map_err(|e| e.to_string())?;
    let branch = instance.branch.as_ref();
    let build = pics::branches(&info)
        .into_iter()
        .find(|b| b.name == branch)
        .ok_or_else(|| format!("Steam no longer has the branch {branch}"))?
        .build_id;
    let own = instance.dir.join("game");
    // An own `game/` with no recorded build is a download from before builds were shared, cut
    // short: it seeds the shared build and then goes away.
    let partial = own.is_dir() && instance.build.is_none();
    let detached = own.is_dir() && !partial;
    let game = match instance.dir.parent() {
        Some(root) if !detached => store::builds(root).join(build.to_string()),
        _ => own.clone(),
    };
    // One install per build folder at a time; a second one waits here, then finds it complete.
    let _writing = lock(&game).lock_owned().await;
    // Another instance already installed this build: nothing to download.
    if !detached && store::complete(&game) {
        if partial {
            let _ = std::fs::remove_dir_all(&own);
        }
        return Ok(build);
    }
    // Files that didn't change since the build the instance runs now are copied, not downloaded.
    let seed = match partial {
        true => Some(own.clone()),
        false => instance.game().filter(|old| *old != game && old.is_dir()),
    };
    let depots = pics::depots(&info, branch, os());
    if depots.is_empty() {
        return Err(format!("The {branch} branch has no files for {}", os()));
    }

    // Keys and manifests need the CM session; the files themselves come from the CDN.
    let hosts = content::cdn_hosts(&mut client)
        .await
        .map_err(|e| e.to_string())?;
    let mut plan = Vec::new();
    for depot in depots {
        let key = content::depot_key(&mut client, APP, depot.id)
            .await
            .map_err(|err| match err {
                steam::Error::Steam { code: 15, .. } => {
                    "This Steam account doesn't own Project Zomboid".to_owned()
                }
                other => other.to_string(),
            })?;
        let manifest = content::manifest(
            &mut client,
            &http,
            &hosts[0],
            (APP, depot.id, depot.manifest),
            branch,
            &key,
        )
        .await
        .map_err(|e| e.to_string())?;
        plan.push((depot.id, key, manifest));
    }
    drop(client);

    let total: u64 = plan
        .iter()
        .flat_map(|(_, _, m)| &m.files)
        .map(|f| f.size)
        .sum();
    let mut before = 0;
    for (depot, key, manifest) in &plan {
        let timings = content::install(
            &http,
            &hosts,
            (*depot, key),
            manifest,
            &game,
            seed.as_deref(),
            |done| {
                let _ = events.send(Event::Progress {
                    done: before + done,
                    total,
                });
            },
        )
        .await
        .map_err(|e| e.to_string())?;
        eprintln!("depot {depot}: {timings:?}");
        before += manifest.files.iter().map(|f| f.size).sum::<u64>();
    }
    if !detached {
        store::set_complete(&game).map_err(|e| e.to_string())?;
    }
    if partial {
        let _ = std::fs::remove_dir_all(&own);
    }
    Ok(build)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn sizes_read_like_steam() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(1_500), "1.5 KB");
        assert_eq!(size(42_000_000), "42 MB");
        assert_eq!(size(3_400_000_000), "3.4 GB");
    }

    #[test]
    fn speed_skips_kept_files_and_estimates_time_left() {
        let cancel = runtime::spawn(async {}).abort_handle();
        let mut download = Download::new("a".into(), Kind::Game, Some(cancel));
        let start = Instant::now();
        // 500 MB kept from an earlier install arrive at once; they are not download speed.
        download.progress(500_000_000, 1_000_000_000, start);
        assert_eq!((download.speed, download.left()), (0, None));
        download.progress(510_000_000, 1_000_000_000, start + Duration::from_secs(1));
        assert_eq!(download.speed, 10_000_000);
        assert_eq!(download.left().as_deref(), Some("49 s left"));
        assert_eq!(download.pct(), 51);
    }
}
