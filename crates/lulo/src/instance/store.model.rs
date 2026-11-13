//! Instances on disk: `<root>/<id>/instance.json` and `zomboid/` (passed to the game as
//! `-cachedir`, so saves, mods and options stay per instance). Game files are shared: each build
//! is stored once under `builds/<build id>/` next to the root, unless an instance has its own
//! `game/` (detached, so it can change game files without touching other instances).

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::{Icon, Instance};

const FILE: &str = "instance.json";

#[derive(Serialize, Deserialize)]
struct Manifest {
    name: String,
    branch: String,
    /// Steam build id the instance runs; absent until the first install.
    #[serde(default)]
    build: Option<u32>,
    #[serde(default)]
    last_played: Option<u64>,
    created: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<Icon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    memory_gb: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    jvm_args: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    launch_args: String,
}

/// Where instances live: `$LULO_HOME/instances`, else the OS data dir.
pub fn root() -> PathBuf {
    std::env::var_os("LULO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("lulo")))
        .unwrap_or_else(|| PathBuf::from("lulo"))
        .join("instances")
}

/// Shared game builds, next to the instances root.
pub fn builds(root: &Path) -> PathBuf {
    root.with_file_name("builds")
}

/// Written into a build folder once every file is in place; only complete builds are reused
/// or pruned, so a cancelled install is resumed rather than deleted.
const DONE: &str = ".lulo-done";

pub fn complete(build: &Path) -> bool {
    build.join(DONE).is_file()
}

pub fn set_complete(build: &Path) -> io::Result<()> {
    std::fs::write(build.join(DONE), b"")
}

/// Moves each installed instance's own `game/` (from before builds were shared) into `builds/`,
/// and drops empty ones. A copy whose build is already shared stays, as a detached instance.
pub fn migrate(root: &Path) {
    for instance in list(root) {
        let own = instance.dir.join("game");
        // Only succeeds when empty: an instance created but never installed.
        if std::fs::remove_dir(&own).is_ok() || !own.is_dir() {
            continue;
        }
        let Some(build) = instance.build else {
            continue;
        };
        let shared = builds(root).join(build.to_string());
        if !shared.exists()
            && std::fs::create_dir_all(builds(root)).is_ok()
            && std::fs::rename(&own, &shared).is_ok()
        {
            let _ = set_complete(&shared);
        }
    }
}

/// Deletes complete builds that no instance runs any more.
pub fn prune(root: &Path) -> io::Result<()> {
    let used: Vec<PathBuf> = list(root).iter().filter_map(Instance::game).collect();
    let Ok(entries) = std::fs::read_dir(builds(root)) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if complete(&dir) && !used.contains(&dir) {
            std::fs::remove_dir_all(&dir)?;
        }
    }
    Ok(())
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Every readable instance under `root`, most recently played first. Broken folders are skipped.
pub fn list(root: &Path) -> Vec<Instance> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<(Instance, u64)> = entries
        .flatten()
        .filter_map(|entry| {
            let dir = entry.path();
            let text = std::fs::read_to_string(dir.join(FILE)).ok()?;
            let manifest: Manifest = serde_json::from_str(&text).ok()?;
            let id = dir.file_name()?.to_string_lossy().into_owned();
            Some((instance(id, dir, &manifest), manifest.created))
        })
        .collect();
    out.sort_by(|(a, ca), (b, cb)| {
        b.last_played
            .cmp(&a.last_played)
            .then(cb.cmp(ca))
            .then_with(|| a.name.cmp(&b.name))
    });
    out.into_iter().map(|(i, _)| i).collect()
}

pub fn create(root: &Path, name: &str, branch: &str) -> io::Result<Instance> {
    let base = slug(name);
    let (id, dir) = (1..)
        .map(|n| {
            if n == 1 {
                base.clone()
            } else {
                format!("{base}-{n}")
            }
        })
        .map(|id| (id.clone(), root.join(id)))
        .find(|(_, dir)| !dir.exists())
        .expect("an unused name exists");

    std::fs::create_dir_all(dir.join("zomboid"))?;
    let manifest = Manifest {
        name: name.to_owned(),
        branch: branch.to_owned(),
        build: None,
        last_played: None,
        created: now(),
        icon: None,
        memory_gb: None,
        jvm_args: String::new(),
        launch_args: String::new(),
    };
    write(&dir, &manifest)?;
    Ok(instance(id, dir, &manifest))
}

/// Records the Steam build the instance now runs.
pub fn set_build(dir: &Path, build: u32) -> io::Result<()> {
    let path = dir.join(FILE);
    let mut manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    manifest.build = Some(build);
    write(dir, &manifest)
}

/// Records the instance's icon; `None` goes back to the picked one.
pub fn set_icon(dir: &Path, icon: Option<Icon>) -> io::Result<()> {
    let path = dir.join(FILE);
    let mut manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    manifest.icon = icon;
    write(dir, &manifest)
}

/// What the instance settings dialog edits.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub name: String,
    /// Most memory the game may use, in GB; `None` leaves the game's own default.
    pub memory_gb: Option<u32>,
    pub jvm_args: String,
    pub launch_args: String,
}

/// Saves the instance's settings. Renaming keeps the folder and its id, which other things
/// refer to.
pub fn configure(dir: &Path, settings: &Settings) -> io::Result<()> {
    let name = settings.name.trim();
    if name.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the name is empty",
        ));
    }
    let path = dir.join(FILE);
    let mut manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    manifest.name = name.to_owned();
    manifest.memory_gb = settings.memory_gb;
    manifest.jvm_args = settings.jvm_args.trim().to_owned();
    manifest.launch_args = settings.launch_args.trim().to_owned();
    write(dir, &manifest)
}

/// Deletes the instance with its saves and mods, and any shared build nothing else runs.
pub fn delete(root: &Path, dir: &Path) -> io::Result<()> {
    // Only an instance folder directly under the root: never an arbitrary path.
    if dir.parent() != Some(root) || !dir.join(FILE).is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not an instance folder",
        ));
    }
    // The manifest goes first: from then on the instance is no longer listed, however long
    // the rest takes or if it is cut short.
    std::fs::remove_file(dir.join(FILE))?;
    std::fs::remove_dir_all(dir)?;
    prune(root)
}

/// Records that the instance was just played.
pub fn set_last_played(dir: &Path, at: u64) -> io::Result<()> {
    let path = dir.join(FILE);
    let mut manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    manifest.last_played = Some(at);
    write(dir, &manifest)
}

fn write(dir: &Path, manifest: &Manifest) -> io::Result<()> {
    // Write then rename, so a crash never leaves a half-written manifest.
    let tmp = dir.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, serde_json::to_vec_pretty(manifest)?)?;
    std::fs::rename(&tmp, dir.join(FILE))
}

fn instance(id: String, dir: PathBuf, m: &Manifest) -> Instance {
    Instance {
        id: id.into(),
        dir,
        name: m.name.clone().into(),
        branch: m.branch.clone().into(),
        build: m.build,
        last_played: m.last_played,
        icon: m.icon.clone(),
        memory_gb: m.memory_gb,
        jvm_args: m.jvm_args.clone(),
        launch_args: m.launch_args.clone(),
    }
}

/// Folder name from a display name: lowercase ASCII letters and digits joined by `-`.
fn slug(name: &str) -> String {
    let slug = name
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "instance".into()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        // Nested, so the builds folder next to the root stays inside this test's folder too.
        let base = std::env::temp_dir().join(format!("lulo-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        base.join("instances")
    }

    #[test]
    fn creates_and_lists_instances() {
        let root = temp_root("list");
        let a = create(&root, "Brita's Armory", "public").unwrap();
        let b = create(&root, "Brita's Armory", "legacy41").unwrap();
        std::fs::create_dir_all(root.join("broken")).unwrap(); // no manifest: skipped

        assert_eq!(a.id.as_ref(), "brita-s-armory");
        assert_eq!(b.id.as_ref(), "brita-s-armory-2");
        assert!(a.dir.join("zomboid").is_dir());

        set_build(&a.dir, 26486631).unwrap();
        set_last_played(&a.dir, 1_800_000_000).unwrap();

        let listed = list(&root);
        assert!(
            listed
                .iter()
                .any(|i| i.id == a.id && i.build == Some(26486631))
        );
        assert_eq!(listed.len(), 2);
        assert!(
            listed
                .iter()
                .any(|i| i.branch.as_ref() == "legacy41" && i.build.is_none())
        );
        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn shares_builds_and_prunes_unused_ones() {
        let root = temp_root("builds");
        let builds = builds(&root);
        let reload = |id: &str| {
            list(&root)
                .into_iter()
                .find(|i| i.id.as_ref() == id)
                .unwrap()
        };

        // Installed before builds were shared: its game/ moves into builds/.
        let old = create(&root, "Old", "legacy41").unwrap();
        std::fs::create_dir_all(old.dir.join("game")).unwrap();
        std::fs::write(old.dir.join("game/projectzomboid.sh"), "").unwrap();
        set_build(&old.dir, 1).unwrap();
        // Same build, its own copy too: kept as a detached instance.
        let twin = create(&root, "Twin", "legacy41").unwrap();
        std::fs::create_dir_all(twin.dir.join("game")).unwrap();
        std::fs::write(twin.dir.join("game/projectzomboid.sh"), "").unwrap();
        set_build(&twin.dir, 1).unwrap();
        // Created, never installed: the empty game/ goes away.
        let fresh = create(&root, "Fresh", "public").unwrap();
        std::fs::create_dir_all(fresh.dir.join("game")).unwrap();

        migrate(&root);
        assert!(complete(&builds.join("1")));
        assert!(builds.join("1/projectzomboid.sh").is_file());
        assert_eq!(reload("old").game(), Some(builds.join("1")));
        assert_eq!(reload("twin").game(), Some(twin.dir.join("game")));
        assert_eq!(reload("fresh").game(), None);

        // An unused complete build is pruned; one in use or still downloading is kept.
        for build in ["2", "3"] {
            std::fs::create_dir_all(builds.join(build)).unwrap();
        }
        set_complete(&builds.join("2")).unwrap();
        prune(&root).unwrap();
        assert!(builds.join("1").is_dir());
        assert!(!builds.join("2").exists());
        assert!(builds.join("3").is_dir());

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn saves_the_settings_keeping_the_folder() {
        let root = temp_root("configure");
        let a = create(&root, "Old name", "public").unwrap();
        let plain = std::fs::read_to_string(a.dir.join(FILE)).unwrap();
        assert!(!plain.contains("launch_args") && !plain.contains("memory_gb"));

        let mut settings = Settings {
            name: "  New name ".into(),
            memory_gb: Some(8),
            jvm_args: " -XX:+AlwaysPreTouch ".into(),
            launch_args: "  -debug -nosteam  ".into(),
        };
        configure(&a.dir, &settings).unwrap();
        let listed = list(&root);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, a.id);
        assert_eq!(listed[0].name.as_ref(), "New name");
        assert_eq!(listed[0].memory_gb, Some(8));
        assert_eq!(listed[0].jvm_args, "-XX:+AlwaysPreTouch");
        assert_eq!(listed[0].launch_args, "-debug -nosteam");

        settings.name = "   ".into();
        assert!(configure(&a.dir, &settings).is_err());
        assert_eq!(list(&root)[0].name.as_ref(), "New name");

        // Cleared settings leave the file as a new instance has it.
        let cleared = Settings {
            name: "New name".into(),
            memory_gb: None,
            jvm_args: String::new(),
            launch_args: String::new(),
        };
        configure(&a.dir, &cleared).unwrap();
        let text = std::fs::read_to_string(a.dir.join(FILE)).unwrap();
        assert!(!text.contains("launch_args") && !text.contains("memory_gb"));
        assert!(!text.contains("jvm_args"));

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn deletes_an_instance_and_prunes_its_unused_build() {
        let root = temp_root("delete");
        let builds = builds(&root);
        let (a, b) = (
            create(&root, "A", "public").unwrap(),
            create(&root, "B", "public").unwrap(),
        );
        for build in ["1", "2"] {
            std::fs::create_dir_all(builds.join(build)).unwrap();
            set_complete(&builds.join(build)).unwrap();
        }
        set_build(&a.dir, 1).unwrap();
        set_build(&b.dir, 2).unwrap();
        std::fs::write(a.dir.join("zomboid/save.txt"), "x").unwrap();

        delete(&root, &a.dir).unwrap();
        assert!(!a.dir.exists());
        assert!(!builds.join("1").exists());
        assert!(builds.join("2").is_dir());
        assert_eq!(list(&root).len(), 1);

        // Not an instance folder: the root itself, a stranger, a folder outside the root.
        let outside = root.parent().unwrap().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        for target in [root.clone(), root.join("nothing"), outside.clone()] {
            assert!(delete(&root, &target).is_err());
        }
        assert!(root.is_dir() && outside.is_dir());

        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn slugs_are_safe_folder_names() {
        assert_eq!(slug("Legacy MP (B41)"), "legacy-mp-b41");
        assert_eq!(slug("../../etc"), "etc");
        assert_eq!(slug("ñandú"), "and");
        assert_eq!(slug("ñ ü"), "instance");
    }
}
