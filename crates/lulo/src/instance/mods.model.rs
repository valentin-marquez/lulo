//! The mods an instance holds: what is in `zomboid/mods/`, which of them the game loads
//! (`default.txt`), and installing one from the Workshop.
//!
//! The game reads `<cachedir>/mods/<folder>`. Build 42 uses `common/` plus the highest version
//! folder (`42`, `42.13`) that is not above the running game; Build 41 uses the folder itself.
//! A mod is active when its id is listed in `mods/default.txt`, which the game empties unless a
//! `reset-mods-*.txt` sits beside it. Checked by hand and read from the game's
//! `ZomboidFileSystem` (notes in `research/game/MODS.md`).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use super::{APP, Instance};

/// A game or mod-folder version, down to what the game compares: build and major (`42.13`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub build: u32,
    pub major: u32,
}

impl Version {
    /// `42` is 42.0, `42.13` is 42.13 and `42.13.1` is 42.13; anything else is no version.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.');
        let build = parts.next()?.parse().ok()?;
        let major = match parts.next() {
            Some(major) => major.parse().ok()?,
            None => 0,
        };
        parts
            .all(|part| part.parse::<u32>().is_ok())
            .then_some(Self { build, major })
    }
}

/// The version of the game the instance last ran, from the `version.txt` it leaves in the
/// cachedir (`42.22.2 3d39148846`). `None` before the first run.
pub fn game_version(cachedir: &Path) -> Option<Version> {
    let text = std::fs::read_to_string(cachedir.join("version.txt")).ok()?;
    Version::parse(text.split_whitespace().next()?)
}

/// Whether the instance's game uses Build 42's mod layout and `default.txt` format.
fn modern(instance: &Instance, version: Option<Version>) -> bool {
    match (version, instance.game()) {
        (Some(version), _) => version.build >= 42,
        // Not run yet: Build 42 ships one jar, Build 41 loose class files.
        (None, Some(game)) => game.join("projectzomboid/projectzomboid.jar").is_file(),
        (None, None) => branch_is_modern(&instance.branch),
    }
}

/// Whether Steam branch `branch` is Build 42 or newer. `public` and `pre-release` are, as
/// are the numbered and unstable ones; only the legacy branch is 41.
pub fn branch_is_modern(branch: &str) -> bool {
    !branch.contains("41")
}

/// Whether a Workshop item with these tags is made for the instance's build.
pub fn fits(instance: &Instance, tags: &[String]) -> bool {
    for_build(tags, is_modern(instance))
}

/// Whether a Workshop item with these tags is made for a game of this build line. Items are
/// tagged `Build 41`, `Build 42` or both; one with neither says nothing and passes.
fn for_build(tags: &[String], modern: bool) -> bool {
    let has = |tag: &str| tags.iter().any(|t| t == tag);
    match (has("Build 41"), has("Build 42")) {
        (false, false) | (true, true) => true,
        (_, for_42) => for_42 == modern,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mod {
    pub id: String,
    pub name: String,
    pub author: String,
    pub description: String,
    /// Its folder under `zomboid/mods/`.
    pub folder: String,
    /// That folder on disk.
    pub dir: PathBuf,
    /// The version folder the game reads it from (`42.13`); `None` for `common/` or Build 41.
    pub version: Option<String>,
    /// The Workshop item it was installed from, when lulo installed it.
    pub workshop: Option<u64>,
    /// `icon=` (or the first `poster=`) of its `mod.info`, when the file exists.
    pub icon: Option<PathBuf>,
    /// Ids of the mods it needs.
    pub requires: Vec<String>,
    pub enabled: bool,
}

/// What a `mod.info` says, with only the keys lulo uses.
#[derive(Debug, Default, PartialEq)]
struct Info {
    id: String,
    name: String,
    author: String,
    description: String,
    icon: Option<String>,
    requires: Vec<String>,
}

fn parse_info(text: &str) -> Option<Info> {
    let mut info = Info::default();
    let mut poster = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "id" => info.id = value.to_owned(),
            "name" => info.name = value.to_owned(),
            "author" => info.author = value.to_owned(),
            "description" => info.description = value.to_owned(),
            "icon" if !value.is_empty() => info.icon = Some(value.to_owned()),
            "poster" if poster.is_none() && !value.is_empty() => poster = Some(value.to_owned()),
            "require" => {
                info.requires = value
                    .split(',')
                    // Build 42 writes required ids with a leading backslash.
                    .map(|id| id.trim().trim_start_matches('\\').to_owned())
                    .filter(|id| !id.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
    info.icon = info.icon.or(poster);
    if info.name.is_empty() {
        info.name = info.id.clone();
    }
    (!info.id.is_empty()).then_some(info)
}

/// The folder of `dir` whose `mod.info` the game reads, or `None` when this game can't load
/// the mod. `version` is the game's, when known; unknown takes the newest folder.
fn info_dir(dir: &Path, modern: bool, version: Option<Version>) -> Option<PathBuf> {
    if !modern {
        return dir.join("mod.info").is_file().then(|| dir.to_owned());
    }
    let floor = Version {
        build: 42,
        major: 0,
    };
    let chosen = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let found = Version::parse(&entry.file_name().to_string_lossy())?;
            (found >= floor && version.is_none_or(|game| found <= game))
                .then(|| (found, entry.path()))
        })
        .max_by_key(|(found, _)| *found)
        .map(|(_, path)| path);
    // The chosen version folder wins; `common/` may carry the `mod.info` instead.
    chosen
        .into_iter()
        .chain([dir.join("common")])
        .find(|folder| folder.join("mod.info").is_file())
}

/// The ids `default.txt` lists, and the maps block kept as it was.
#[derive(Debug, Default, PartialEq)]
struct Active {
    mods: Vec<String>,
    maps: Vec<String>,
}

impl Active {
    fn parse(text: &str) -> Self {
        let mut active = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_end_matches(',').trim();
            let value = value.trim_start_matches('\\').to_owned();
            match key.trim() {
                "mod" if !value.is_empty() => active.mods.push(value),
                "map" if !value.is_empty() => active.maps.push(value),
                _ => {}
            }
        }
        active
    }

    /// The file as the game writes it; Build 42 puts a backslash before each mod id.
    fn write(&self, modern: bool) -> String {
        let prefix = if modern { "\\" } else { "" };
        let block = |name: &str, key: &str, prefix: &str, entries: &[String]| {
            let lines: String = entries
                .iter()
                .map(|entry| format!("    {key} = {prefix}{entry},\n"))
                .collect();
            format!("{name}\n{{\n{lines}}}\n")
        };
        format!(
            "VERSION = 1,\n\n{}\n{}",
            block("mods", "mod", prefix, &self.mods),
            block("maps", "map", "", &self.maps)
        )
    }
}

fn mods_dir(instance: &Instance) -> PathBuf {
    instance.dir.join("zomboid/mods")
}

fn read_active(instance: &Instance) -> Active {
    std::fs::read_to_string(mods_dir(instance).join("default.txt"))
        .map(|text| Active::parse(&text))
        .unwrap_or_default()
}

fn write_active(instance: &Instance, active: &Active, modern: bool) -> io::Result<()> {
    let dir = mods_dir(instance);
    std::fs::create_dir_all(&dir)?;
    // Without a `reset-mods-*.txt` the game empties `default.txt` when it starts. One it left
    // itself is the right one; before the first run, write the name its build looks for.
    // ponytail: the name is fixed per build line (42_00, 41_51); a future build that looks for
    // another one resets the list once, and after that run its own file is there.
    let has_reset = std::fs::read_dir(&dir)?.flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.starts_with("reset-mods-") && name.ends_with(".txt")
    });
    if !has_reset {
        let name = if modern {
            "reset-mods-42_00.txt"
        } else {
            "reset-mods-41_51.txt"
        };
        std::fs::write(
            dir.join(name),
            "If this file does not exist, default.txt will be reset to empty (no mods active).",
        )?;
    }
    std::fs::write(dir.join("default.txt"), active.write(modern))
}

/// The mods in the instance that its game can load, by name.
pub fn list(instance: &Instance) -> Vec<Mod> {
    let version = game_version(&instance.dir.join("zomboid"));
    let modern = modern(instance, version);
    let active = read_active(instance);
    let pins = read_pins(instance);
    let Ok(entries) = std::fs::read_dir(mods_dir(instance)) else {
        return Vec::new();
    };
    let mut mods: Vec<Mod> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let folder = entry.file_name().to_string_lossy().into_owned();
            // The game skips its own template the same way.
            if folder.eq_ignore_ascii_case("examplemod") {
                return None;
            }
            let from = info_dir(&entry.path(), modern, version)?;
            let info = parse_info(&std::fs::read_to_string(from.join("mod.info")).ok()?)?;
            let icon = info
                .icon
                .map(|file| from.join(file))
                .filter(|path| path.is_file());
            let version = from
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| Version::parse(name).is_some());
            Some(Mod {
                enabled: active.mods.contains(&info.id),
                id: info.id,
                name: info.name,
                author: info.author,
                description: info.description,
                workshop: pins
                    .items
                    .iter()
                    .find(|pin| pin.folders.contains(&folder))
                    .map(|pin| pin.id),
                dir: entry.path(),
                version,
                folder,
                icon,
                requires: info.requires,
            })
        })
        .collect();
    mods.sort_by_key(|m| m.name.to_lowercase());
    mods
}

/// When the instance's pins and active list last changed.
pub type Stamp = (Option<std::time::SystemTime>, Option<std::time::SystemTime>);

/// The [`Stamp`] of the instance now: two `stat` calls, cheap enough for the UI thread. Every
/// install, removal and turn on or off rewrites one of the two files.
pub fn stamp(instance: &Instance) -> Stamp {
    let changed = |path: PathBuf| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    (
        changed(instance.dir.join("mods.json")),
        changed(mods_dir(instance).join("default.txt")),
    )
}

/// Turns mod `id` on or off. Turning one on also turns on the installed mods it requires;
/// the ids it requires that aren't installed come back so the caller can say so.
pub fn set_enabled(instance: &Instance, id: &str, on: bool) -> io::Result<Vec<String>> {
    let version = game_version(&instance.dir.join("zomboid"));
    let modern = modern(instance, version);
    let installed = list(instance);
    let mut active = read_active(instance);
    let mut missing = Vec::new();
    if on {
        // Required mods go before the ones that need them.
        let mut wanted = vec![id.to_owned()];
        while let Some(next) = wanted.pop() {
            if active.mods.contains(&next) {
                continue;
            }
            match installed.iter().find(|m| m.id == next) {
                Some(found) => {
                    let at = active
                        .mods
                        .iter()
                        .position(|other| {
                            installed
                                .iter()
                                .any(|m| m.id == *other && m.requires.contains(&next))
                        })
                        .unwrap_or(active.mods.len());
                    active.mods.insert(at, next);
                    wanted.extend(found.requires.iter().cloned());
                }
                None => missing.push(next),
            }
        }
    } else {
        active.mods.retain(|other| other != id);
    }
    write_active(instance, &active, modern)?;
    Ok(missing)
}

/// Deletes the mod in `folder` and takes it off the active list.
pub fn remove(instance: &Instance, folder: &str) -> io::Result<()> {
    // One plain folder name, never a path out of `mods/`.
    if Path::new(folder).file_name() != Some(std::ffi::OsStr::new(folder)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a mod folder",
        ));
    }
    let gone: Vec<String> = list(instance)
        .into_iter()
        .filter(|m| m.folder == folder)
        .map(|m| m.id)
        .collect();
    std::fs::remove_dir_all(mods_dir(instance).join(folder))?;
    let version = game_version(&instance.dir.join("zomboid"));
    let mut active = read_active(instance);
    active.mods.retain(|id| !gone.contains(id));
    write_active(instance, &active, modern(instance, version))?;
    let mut pins = read_pins(instance);
    for pin in &mut pins.items {
        pin.folders.retain(|kept| kept != folder);
    }
    pins.items.retain(|pin| !pin.folders.is_empty());
    write_pins(instance, &pins)
}

/// A Workshop item installed in the instance, pinned to the version that was downloaded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    pub id: u64,
    pub manifest: u64,
    pub title: String,
    /// The folders under `zomboid/mods/` that came from it.
    pub folders: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Pins {
    #[serde(default)]
    items: Vec<Pin>,
}

fn read_pins(instance: &Instance) -> Pins {
    std::fs::read_to_string(instance.dir.join("mods.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_pins(instance: &Instance, pins: &Pins) -> io::Result<()> {
    let text = serde_json::to_string_pretty(pins).map_err(io::Error::other)?;
    std::fs::write(instance.dir.join("mods.json"), text)
}

/// Moves the mod folders of a downloaded Workshop item (`<item>/mods/<folder>`) into the
/// instance, replacing older copies, and returns their names.
fn adopt(instance: &Instance, item: &Path) -> io::Result<Vec<String>> {
    let target = mods_dir(instance);
    std::fs::create_dir_all(&target)?;
    let mut folders = Vec::new();
    for entry in std::fs::read_dir(item.join("mods"))?.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name();
        let to = target.join(&name);
        if to.exists() {
            std::fs::remove_dir_all(&to)?;
        }
        std::fs::rename(entry.path(), &to)?;
        folders.push(name.to_string_lossy().into_owned());
    }
    folders.sort();
    Ok(folders)
}

/// Whether the instance runs Build 42 or newer (its mod layout and Workshop tag).
pub fn is_modern(instance: &Instance) -> bool {
    modern(instance, game_version(&instance.dir.join("zomboid")))
}

/// A Workshop item as shown before installing it.
#[derive(Debug, Clone)]
pub struct Listing {
    pub item: steam::workshop::Item,
    /// Its Workshop picture, downloaded; `None` when it has none or it couldn't be fetched.
    pub picture: Option<PathBuf>,
}

/// The file extension of a picture, from how its bytes start.
fn picture_ext(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0xFF, 0xD8, 0xFF, ..] => Some("jpg"),
        [0x89, b'P', b'N', b'G', ..] => Some("png"),
        [b'G', b'I', b'F', b'8', ..] => Some("gif"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("webp"),
        _ => None,
    }
}

/// `item`'s Workshop picture, downloaded once into the cache next to the instances (like the
/// avatars). `None` when it has none, it couldn't be fetched, or it is in a format that
/// isn't shown.
pub async fn picture(http: &steam::reqwest::Client, url: Option<&str>) -> Option<PathBuf> {
    let cache = super::store::root()
        .with_file_name("cache")
        .join("workshop");
    let url = url?;
    // Named by the picture's own address, which changes when its author replaces it.
    let name: String = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()?
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(40)
        .collect();
    for ext in ["jpg", "png", "gif", "webp"] {
        let path = cache.join(format!("{name}.{ext}"));
        if path.is_file() {
            return Some(path);
        }
    }
    let bytes = steam::workshop::preview(http, url).await.ok()?;
    let path = cache.join(format!("{name}.{}", picture_ext(&bytes)?));
    std::fs::create_dir_all(&cache).ok()?;
    std::fs::write(&path, bytes).ok()?;
    Some(path)
}

/// What the Workshop says about item `id`, with its picture.
pub async fn lookup(id: u64) -> Result<Listing, String> {
    let found = crate::runtime::spawn(async move {
        let http = steam::http();
        let mut client = steam::Client::anonymous(&http).await?;
        let item = steam::workshop::details(&mut client, &[id])
            .await?
            .into_iter()
            .next()
            .filter(|item| item.app == APP && !item.title.is_empty());
        let Some(item) = item else { return Ok(None) };
        let picture = picture(&http, item.preview.as_deref()).await;
        Ok::<_, steam::Error>(Some(Listing { item, picture }))
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())?;
    found.ok_or_else(|| "Steam has no Project Zomboid item with that id".to_owned())
}

/// Turns several mods on or off; the ids they need that aren't installed come back.
pub fn set_enabled_all(instance: &Instance, ids: &[String], on: bool) -> io::Result<Vec<String>> {
    let mut missing = Vec::new();
    for id in ids {
        missing.extend(set_enabled(instance, id, on)?);
    }
    missing.sort();
    missing.dedup();
    Ok(missing)
}

/// What an install brought in.
#[derive(Debug, PartialEq)]
pub struct Installed {
    /// Title of the first item that was asked for.
    pub title: String,
    /// Titles of the other Workshop items installed with it: the rest of what was asked for,
    /// then what those require.
    pub with: Vec<String>,
}

/// The most items one install follows through `requires` beyond the ones asked for; a mod
/// needs a handful at most.
const MAX_REQUIRED: usize = 50;
/// Items asked of the Workshop in one request.
const BATCH: usize = 50;

/// Workshop items downloading at once. Each one already fetches 32 chunks at a time, so this
/// is about overlapping the per-item round trips with the downloads, not about bandwidth.
const PARALLEL_ITEMS: usize = 4;

/// What a download task ended with.
fn joined(done: Result<Result<(), String>, tokio::task::JoinError>) -> Result<(), String> {
    done.map_err(|err| err.to_string())?
}

/// Moves a downloaded item from `staging` into the instance, pins it and turns its mods on.
fn land(instance: &Instance, item: &steam::workshop::Item, staging: &Path) -> io::Result<()> {
    let folders = adopt(instance, staging)?;
    let _ = std::fs::remove_dir_all(staging);
    let mut pins = read_pins(instance);
    pins.items.retain(|pin| pin.id != item.id);
    pins.items.push(Pin {
        id: item.id,
        manifest: item.manifest,
        title: item.title.clone(),
        folders: folders.clone(),
    });
    write_pins(instance, &pins)?;
    // On as soon as it lands, so a cancelled install leaves what it got usable. A mod whose
    // requirement comes later is fine: turning that one on puts it first.
    // ponytail: each turn-on rescans the mods folder, quadratic over a collection (325 mods:
    // about 100k small reads); keep the list in memory if it shows.
    for found in list(instance)
        .iter()
        .filter(|m| folders.contains(&m.folder))
    {
        set_enabled(instance, &found.id, true)?;
    }
    Ok(())
}

/// Downloads Workshop items `ids` into the instance with the items their authors marked as
/// required (and theirs), and turns all their mods on. Items the instance already has are
/// left alone. `progress` gets the title being downloaded with the bytes done and in total,
/// over everything. Meant to run on the background runtime as one task, so aborting it stops
/// the download; each item is moved into the instance as soon as it is down, so what finished
/// before a cancel stays.
// ponytail: each instance keeps its own copy of a mod; share them like game builds if disk
// use becomes a problem.
pub async fn install(
    instance: Instance,
    ids: Vec<u64>,
    progress: impl FnMut(&str, u64, u64) + Send + 'static,
) -> Result<Installed, String> {
    let fail = |err: steam::Error| err.to_string();
    let incoming = instance.dir.join(".incoming");
    let http = steam::http();
    let mut client = steam::Client::anonymous(&http).await.map_err(fail)?;

    // The items asked for first (downloaded again if here, which updates them), then what
    // they require, each once; required items already here are skipped.
    let have: Vec<u64> = read_pins(&instance)
        .items
        .iter()
        .map(|pin| pin.id)
        .collect();
    let mut seen: Vec<u64> = Vec::new();
    for id in &ids {
        if !seen.contains(id) {
            seen.push(*id);
        }
    }
    let mut queue = seen.clone();
    let mut items = Vec::new();
    while !queue.is_empty() && seen.len() <= ids.len() + MAX_REQUIRED {
        let take = queue.len().min(BATCH);
        let batch: Vec<u64> = queue.drain(..take).collect();
        let found = steam::workshop::details(&mut client, &batch)
            .await
            .map_err(fail)?;
        for item in found {
            if item.app != APP || item.manifest == 0 {
                continue;
            }
            for needed in &item.requires {
                if !seen.contains(needed) && !have.contains(needed) {
                    seen.push(*needed);
                    queue.push(*needed);
                }
            }
            items.push(item);
        }
    }
    if !items.iter().any(|item| ids.contains(&item.id)) {
        return Err(if ids.len() == 1 {
            "That isn't a Project Zomboid mod. Collections are added when creating an instance"
                .into()
        } else {
            "None of those are Project Zomboid mods Steam still has".into()
        });
    }

    let _ = std::fs::remove_dir_all(&incoming);
    let total: u64 = items.iter().map(|item| item.size).sum();
    let source = steam::workshop::source(&mut client, APP)
        .await
        .map_err(fail)?;
    // Bytes done per item, summed for the overall progress by every download in flight.
    let report = Arc::new(Mutex::new((vec![0u64; items.len()], progress)));
    // Moving an item in and turning it on edits `mods.json` and `default.txt`: one at a time.
    let landing = Arc::new(Mutex::new(()));
    let mut running = tokio::task::JoinSet::new();
    for (ix, item) in items.iter().enumerate() {
        // The manifest needs the Steam client, so manifests come one by one while the items
        // before download. Per item that is most of the time: a 370 KB mod took 400 ms of
        // manifest and 40 ms of files.
        let manifest = steam::workshop::manifest(&mut client, &http, &source, item)
            .await
            .map_err(fail)?;
        if running.len() >= PARALLEL_ITEMS
            && let Some(done) = running.join_next().await
        {
            joined(done)?;
        }
        let (http, source, report, landing) = (
            http.clone(),
            source.clone(),
            report.clone(),
            landing.clone(),
        );
        let (instance, item, staging) = (
            instance.clone(),
            item.clone(),
            incoming.join(item.id.to_string()),
        );
        running.spawn(async move {
            let tell = |done: u64| {
                let mut report = report.lock().unwrap_or_else(PoisonError::into_inner);
                report.0[ix] = done;
                let sum = report.0.iter().sum();
                (report.1)(&item.title, sum, total);
            };
            steam::workshop::fetch(&http, &source, &manifest, &staging, &tell)
                .await
                .map_err(fail)?;
            tell(item.size);
            crate::runtime::blocking(move || {
                let _one = landing.lock().unwrap_or_else(PoisonError::into_inner);
                land(&instance, &item, &staging)
            })
            .await?
            .map_err(|err| err.to_string())
        });
    }
    while let Some(done) = running.join_next().await {
        joined(done)?;
    }
    drop(client);
    let _ = std::fs::remove_dir_all(&incoming);
    let mut titles = items.into_iter().map(|item| item.title);
    Ok(Installed {
        title: titles.next().unwrap_or_default(),
        with: titles.collect(),
    })
}

/// A Workshop collection as the create dialog shows it before installing.
#[derive(Debug, Clone)]
pub struct Bundle {
    pub id: u64,
    pub title: String,
    /// Its Workshop picture (or its first mod's, when it has none), downloaded, as
    /// [`Listing::picture`].
    pub picture: Option<PathBuf>,
    /// Its mods with the ones they require, each once.
    pub mods: Vec<steam::workshop::Item>,
}

impl Bundle {
    /// The mods made for a game of this build line, and the ones tagged for the other.
    pub fn split(
        &self,
        modern: bool,
    ) -> (Vec<&steam::workshop::Item>, Vec<&steam::workshop::Item>) {
        self.mods
            .iter()
            .filter(|item| item.app == APP && item.manifest != 0)
            .partition(|item| for_build(&item.tags, modern))
    }
}

/// What the Workshop says is in collection `id`.
pub async fn collection(id: u64) -> Result<Bundle, String> {
    crate::runtime::spawn(async move {
        let http = steam::http();
        let mut client = steam::Client::anonymous(&http).await?;
        let found = steam::workshop::collection(&mut client, id).await?;
        // Many collections have no picture of their own; the first mod with one stands in.
        let url = found
            .preview
            .as_deref()
            .or_else(|| found.mods.iter().find_map(|item| item.preview.as_deref()));
        let picture = picture(&http, url).await;
        Ok::<_, steam::Error>(Bundle {
            id: found.id,
            title: found.title,
            picture,
            mods: found.mods,
        })
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `mod.info` of Workshop item 3832274571, as downloaded.
    const INFO: &str = "name=Floor Attack Highlight Fix\nid=FloorAttackHighlightFix\nauthor=wget\ndescription=fixes misleading melee combat outlines on ground targets like crawlers and knocked-down zombies\nposter=poster.png\nicon=icon.png\n";

    /// `default.txt` as Build 42.19 left it with that mod active.
    const DEFAULT: &str =
        "VERSION = 1,\n\nmods\n{\n    mod = \\FloorAttackHighlightFix,\n}\n\nmaps\n{\n}\n";

    fn instance(dir: &Path) -> Instance {
        Instance {
            id: "a".into(),
            dir: dir.to_owned(),
            name: "A".into(),
            branch: "public".into(),
            build: None,
            last_played: None,
            icon: None,
            memory_gb: None,
            jvm_args: String::new(),
            launch_args: String::new(),
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lulo-mods-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(dir: &Path, file: &str, text: &str) {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn fits_an_item_to_the_instance_by_its_build_tags() {
        let dir = temp("fits");
        let tags = |list: &[&str]| list.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        // Never installed: the branch decides, and only the legacy one is Build 41.
        let mut new = instance(&dir);
        for (branch, modern) in [("public", true), ("pre-release", true), ("legacy41", false)] {
            new.branch = branch.into();
            assert_eq!(is_modern(&new), modern, "{branch}");
            assert_eq!(fits(&new, &tags(&["Build 42", "QOL"])), modern, "{branch}");
            assert_eq!(fits(&new, &tags(&["Build 41"])), !modern, "{branch}");
            assert!(fits(&new, &tags(&["Build 41", "Build 42"])), "{branch}");
            assert!(
                fits(&new, &tags(&["Maps"])),
                "no build tag passes: {branch}"
            );
        }
        // Once the game has run, what it ran wins over the branch's name.
        put(&dir, "zomboid/version.txt", "41.78.16 abc\n");
        new.branch = "public".into();
        assert!(!is_modern(&new));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_collection_keeps_the_mods_for_the_chosen_build() {
        let item = |id, tags: &[&str], manifest| steam::workshop::Item {
            id,
            app: APP,
            title: id.to_string(),
            manifest,
            file_url: None,
            size: 1,
            updated: None,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            preview: None,
            requires: Vec::new(),
            subscriptions: 0,
        };
        let bundle = Bundle {
            id: 1,
            title: "Pack".into(),
            picture: None,
            mods: vec![
                item(10, &["Build 42"], 5),
                item(11, &["Build 41"], 5),
                item(12, &["Maps"], 5),
                item(13, &["Build 41", "Build 42"], 5),
                // No manifest: nothing to download.
                item(14, &["Build 42"], 0),
            ],
        };
        let ids = |modern| {
            let (fit, other) = bundle.split(modern);
            let ids = |items: Vec<&steam::workshop::Item>| items.iter().map(|i| i.id).collect();
            (ids(fit), ids(other))
        };
        assert_eq!(ids(true), (vec![10, 12, 13], vec![11]));
        assert_eq!(ids(false), (vec![11, 12, 13], vec![10]));
    }

    #[test]
    fn tells_a_picture_by_its_first_bytes() {
        assert_eq!(picture_ext(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(picture_ext(b"\x89PNG\r\n"), Some("png"));
        assert_eq!(picture_ext(b"GIF89a"), Some("gif"));
        assert_eq!(picture_ext(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(picture_ext(b"<html>"), None);
        assert_eq!(picture_ext(&[]), None);
    }

    #[test]
    fn versions_keep_build_and_major() {
        let v = |build, major| Some(Version { build, major });
        assert_eq!(Version::parse("42"), v(42, 0));
        assert_eq!(Version::parse("42.13"), v(42, 13));
        assert_eq!(Version::parse("42.13.1"), v(42, 13));
        for not in ["", "common", "media", "42.x", "v42"] {
            assert_eq!(Version::parse(not), None, "{not}");
        }
        assert!(Version::parse("42.9") < Version::parse("42.13"));
    }

    #[test]
    fn reads_what_a_mod_info_says() {
        let info = parse_info(INFO).unwrap();
        assert_eq!(info.id, "FloorAttackHighlightFix");
        assert_eq!(info.name, "Floor Attack Highlight Fix");
        assert_eq!(info.icon.as_deref(), Some("icon.png"));
        assert_eq!(info.author, "wget");
        assert!(info.description.starts_with("fixes misleading"));
        assert!(info.requires.is_empty());

        let needy = parse_info("id=B\nrequire=\\A, C\nposter=one.png\nposter=two.png").unwrap();
        assert_eq!(needy.name, "B", "the id stands in for a missing name");
        assert_eq!(needy.requires, ["A", "C"]);
        assert_eq!(needy.icon.as_deref(), Some("one.png"));
        assert_eq!(parse_info("name=No id"), None);
    }

    #[test]
    fn reads_and_writes_the_active_list_in_both_formats() {
        let active = Active::parse(DEFAULT);
        assert_eq!(active.mods, ["FloorAttackHighlightFix"]);
        assert_eq!(active.write(true), DEFAULT, "Build 42 round trip");

        let old = "VERSION = 1,\n\nmods\n{\n    mod = A,\n    mod = B,\n}\n\nmaps\n{\n    map = Town,\n}\n";
        let active = Active::parse(old);
        assert_eq!(
            (active.mods.len(), active.maps.as_slice()),
            (2, &["Town".to_owned()][..])
        );
        assert_eq!(active.write(false), old, "Build 41 round trip");
        assert_eq!(
            Active::parse("").write(true),
            "VERSION = 1,\n\nmods\n{\n}\n\nmaps\n{\n}\n"
        );
    }

    #[test]
    fn picks_the_folder_the_game_would() {
        let dir = temp("folders");
        let game = |major| Some(Version { build: 42, major });
        for folder in ["42", "42.13", "42.22"] {
            put(&dir, &format!("multi/{folder}/mod.info"), "id=multi");
        }
        assert_eq!(
            info_dir(&dir.join("multi"), true, game(19)),
            Some(dir.join("multi/42.13"))
        );
        assert_eq!(
            info_dir(&dir.join("multi"), true, game(22)),
            Some(dir.join("multi/42.22"))
        );
        assert_eq!(
            info_dir(&dir.join("multi"), true, None),
            Some(dir.join("multi/42.22"))
        );

        // Only a newer folder: this game can't load it.
        put(&dir, "newer/42.21/mod.info", "id=newer");
        assert_eq!(info_dir(&dir.join("newer"), true, game(19)), None);

        // `mod.info` in common, files in a version folder.
        put(&dir, "shared/common/mod.info", "id=shared");
        put(&dir, "shared/42/media/x.lua", "");
        assert_eq!(
            info_dir(&dir.join("shared"), true, game(19)),
            Some(dir.join("shared/common"))
        );

        // Build 41 layout: only Build 41 sees it.
        put(&dir, "old/mod.info", "id=old");
        assert_eq!(info_dir(&dir.join("old"), true, game(19)), None);
        assert_eq!(
            info_dir(&dir.join("old"), false, None),
            Some(dir.join("old"))
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn lists_enables_and_removes_mods() {
        let dir = temp("list");
        let instance = instance(&dir);
        put(&dir, "zomboid/version.txt", "42.19.2 99198cd6ef\n");
        put(
            &dir,
            "zomboid/mods/FloorAttackHighlightFix/42/mod.info",
            INFO,
        );
        put(
            &dir,
            "zomboid/mods/FloorAttackHighlightFix/42/icon.png",
            "png",
        );
        put(
            &dir,
            "zomboid/mods/Lib/42/mod.info",
            "id=Lib\nname=A library",
        );
        put(
            &dir,
            "zomboid/mods/Needy/42/mod.info",
            "id=Needy\nname=Needy\nrequire=\\Lib,\\Gone",
        );
        put(&dir, "zomboid/mods/examplemod/42/mod.info", "id=examplemod");
        put(&dir, "zomboid/mods/TooNew/42.21/mod.info", "id=TooNew");

        let names: Vec<_> = list(&instance).into_iter().map(|m| m.id).collect();
        assert_eq!(names, ["Lib", "FloorAttackHighlightFix", "Needy"]);
        assert!(list(&instance).iter().all(|m| !m.enabled));

        // Turning one on writes the list and the file that keeps the game from emptying it.
        assert!(
            set_enabled(&instance, "FloorAttackHighlightFix", true)
                .unwrap()
                .is_empty()
        );
        let mods = dir.join("zomboid/mods");
        assert_eq!(
            std::fs::read_to_string(mods.join("default.txt")).unwrap(),
            DEFAULT
        );
        assert!(mods.join("reset-mods-42_00.txt").is_file());
        let fix = list(&instance)
            .into_iter()
            .find(|m| m.id == "FloorAttackHighlightFix")
            .unwrap();
        assert!(fix.enabled);
        assert_eq!((fix.version.as_deref(), fix.workshop), (Some("42"), None));
        assert_eq!(
            fix.icon,
            Some(mods.join("FloorAttackHighlightFix/42/icon.png"))
        );

        // What it requires comes on with it, before it; what isn't installed is reported.
        assert_eq!(set_enabled(&instance, "Needy", true).unwrap(), ["Gone"]);
        assert_eq!(
            read_active(&instance).mods,
            ["FloorAttackHighlightFix", "Lib", "Needy"]
        );

        set_enabled(&instance, "Lib", false).unwrap();
        assert_eq!(
            read_active(&instance).mods,
            ["FloorAttackHighlightFix", "Needy"]
        );

        remove(&instance, "Needy").unwrap();
        assert!(!mods.join("Needy").exists());
        assert_eq!(read_active(&instance).mods, ["FloorAttackHighlightFix"]);
        assert!(remove(&instance, "../zomboid").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn adopts_a_downloaded_item_replacing_the_old_copy() {
        let dir = temp("adopt");
        let instance = instance(&dir);
        put(&dir, "zomboid/mods/One/42/mod.info", "id=One\nname=Old");
        put(&dir, ".incoming/7/mods/One/42/mod.info", "id=One\nname=New");
        put(&dir, ".incoming/7/mods/Two/42/mod.info", "id=Two");
        assert_eq!(
            adopt(&instance, &dir.join(".incoming/7")).unwrap(),
            ["One", "Two"]
        );
        let names: Vec<_> = list(&instance).into_iter().map(|m| m.name).collect();
        assert_eq!(names, ["New", "Two"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Downloads a real, small Build 42 mod from Steam and finds it listed and on.
    #[test]
    #[ignore = "needs network access to Steam"]
    fn installs_a_workshop_mod_into_an_instance() {
        let dir = temp("install");
        let instance = instance(&dir);
        put(&dir, "zomboid/version.txt", "42.22.2 3d39148846\n");
        let run = |id| {
            let seen = std::sync::Arc::new(std::sync::Mutex::new((0, 0)));
            let report = seen.clone();
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(install(
                    instance.clone(),
                    vec![id],
                    move |_, done, total| {
                        *report.lock().unwrap() = (done, total);
                    },
                ));
            (result, *seen.lock().unwrap())
        };
        let (title, (done, total)) = run(3832274571);
        let title = title.expect("the mod installs");
        assert!(
            total > 0 && done == total,
            "progress reached the end: {done} of {total}"
        );
        assert_eq!(title.title, "floor attack highlight fix");
        assert!(title.with.is_empty(), "it requires nothing: {title:?}");

        let mods = list(&instance);
        assert_eq!(mods.len(), 1, "{mods:?}");
        assert_eq!(read_pins(&instance).items[0].id, 3832274571);
        assert_eq!(mods[0].id, "FloorAttackHighlightFix");
        assert!(mods[0].enabled && mods[0].icon.is_some());
        assert_eq!(mods[0].workshop, Some(3832274571));
        assert_eq!(read_pins(&instance).items[0].id, 3832274571);
        assert_ne!(read_pins(&instance).items[0].manifest, 0);
        assert!(!dir.join(".incoming").exists());

        // "Project Cook" needs a framework its author lists as required: it comes along, and
        // every mod it turned on finds what it needs.
        let (cook, _) = run(3490188370);
        let cook = cook.expect("the mod installs with what it requires");
        assert!(!cook.with.is_empty(), "{cook:?}");
        let installed = list(&instance);
        let ids: Vec<&str> = installed.iter().map(|m| m.id.as_str()).collect();
        for item in installed.iter().filter(|m| m.enabled) {
            for needed in &item.requires {
                assert!(
                    ids.contains(&needed.as_str()),
                    "{} needs {needed}: {ids:?}",
                    item.id
                );
            }
        }

        // A Workshop listing comes with its picture, saved where the dialog shows it from.
        let listing = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            // "Project Cook", whose picture is a PNG. The mod installed above has an animated
            // AVIF, which isn't one of the formats shown, so it gets the placeholder.
            .block_on(lookup(3490188370))
            .expect("the listing is found");
        assert!(
            listing.item.tags.iter().any(|tag| tag == "Build 42"),
            "{listing:?}"
        );
        let picture = listing
            .picture
            .clone()
            .unwrap_or_else(|| panic!("it has a picture: {listing:?}"));
        assert!(
            std::fs::metadata(&picture).unwrap().len() > 1000,
            "{picture:?}"
        );

        // A collection, or anything that isn't a mod, is refused.
        let (not, _) = run(3834236342);
        assert!(not.is_err(), "{not:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
