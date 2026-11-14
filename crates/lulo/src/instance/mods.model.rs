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
    match version {
        Some(version) => version.build >= 42,
        // Not run yet: Build 42 ships one jar, Build 41 loose class files.
        None => instance
            .game()
            .is_none_or(|game| game.join("projectzomboid/projectzomboid.jar").is_file()),
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

/// What the Workshop says about item `id`, with its picture.
pub async fn lookup(id: u64) -> Result<Listing, String> {
    // Next to the instances, like the avatars: pictures are only a cache.
    let cache = super::store::root()
        .with_file_name("cache")
        .join("workshop");
    let found = crate::runtime::spawn(async move {
        let http = steam::http();
        let mut client = steam::Client::anonymous(&http).await?;
        let item = steam::workshop::details(&mut client, &[id])
            .await?
            .into_iter()
            .next()
            .filter(|item| item.app == APP && !item.title.is_empty());
        let Some(item) = item else { return Ok(None) };
        let mut picture = None;
        if let Some(url) = &item.preview
            && let Ok(bytes) = steam::workshop::preview(&http, url).await
            && let Some(ext) = picture_ext(&bytes)
        {
            let path = cache.join(format!("{id}.{ext}"));
            if std::fs::create_dir_all(&cache).is_ok() && std::fs::write(&path, bytes).is_ok() {
                picture = Some(path);
            }
        }
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
    /// Title of the item that was asked for.
    pub title: String,
    /// Titles of the Workshop items it requires that were installed with it.
    pub with: Vec<String>,
}

/// The most items one install follows through `requires`; a mod needs a handful at most.
const MAX_REQUIRED: usize = 50;

/// Downloads Workshop item `id` into the instance with the items its author marked as
/// required (and theirs), and turns all their mods on. Items the instance already has are
/// left alone. `progress` gets the title being downloaded with the bytes done and in total,
/// over everything. Meant to run on the background runtime as one task, so aborting it stops
/// the download.
// ponytail: each instance keeps its own copy of a mod; share them like game builds if disk
// use becomes a problem.
pub async fn install(
    instance: Instance,
    id: u64,
    mut progress: impl FnMut(&str, u64, u64) + Send,
) -> Result<Installed, String> {
    let fail = |err: steam::Error| err.to_string();
    let incoming = instance.dir.join(".incoming");
    let http = steam::http();
    let mut client = steam::Client::anonymous(&http).await.map_err(fail)?;

    // The item first, then what it requires, each once; what is already here is skipped.
    let have: Vec<u64> = read_pins(&instance)
        .items
        .iter()
        .map(|pin| pin.id)
        .collect();
    let mut seen = vec![id];
    let mut queue = vec![id];
    let mut items = Vec::new();
    while !queue.is_empty() && seen.len() <= MAX_REQUIRED {
        let found = steam::workshop::details(&mut client, &queue)
            .await
            .map_err(fail)?;
        queue.clear();
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
    if items.first().is_none_or(|item| item.id != id) {
        return Err(
            "That isn't a Project Zomboid mod. Collections are added when creating an instance"
                .into(),
        );
    }

    let _ = std::fs::remove_dir_all(&incoming);
    let total: u64 = items.iter().map(|item| item.size).sum();
    let mut before = 0;
    for item in &items {
        progress(&item.title, before, total);
        let staging = incoming.join(item.id.to_string());
        steam::workshop::download(&mut client, &http, item, &staging, |done| {
            progress(&item.title, before + done, total)
        })
        .await
        .map_err(fail)?;
        before += item.size;
    }
    drop(client);

    let placed = crate::runtime::blocking(move || {
        let mut pins = read_pins(&instance);
        let mut placed = Vec::new();
        for item in &items {
            let folders = adopt(&instance, &incoming.join(item.id.to_string()))?;
            pins.items.retain(|pin| pin.id != item.id);
            pins.items.push(Pin {
                id: item.id,
                manifest: item.manifest,
                title: item.title.clone(),
                folders: folders.clone(),
            });
            placed.extend(folders);
        }
        let _ = std::fs::remove_dir_all(&incoming);
        write_pins(&instance, &pins)?;
        // Once everything is on disk, so a mod finds the ones it needs when it is turned on.
        for found in list(&instance)
            .iter()
            .filter(|m| placed.contains(&m.folder))
        {
            set_enabled(&instance, &found.id, true)?;
        }
        let mut titles = items.into_iter().map(|item| item.title);
        Ok::<_, io::Error>(Installed {
            title: titles.next().unwrap_or_default(),
            with: titles.collect(),
        })
    })
    .await?;
    placed.map_err(|err| err.to_string())
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
                .block_on(install(instance.clone(), id, move |_, done, total| {
                    *report.lock().unwrap() = (done, total);
                }));
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
