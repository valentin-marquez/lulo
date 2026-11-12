//! Instances on disk: `<root>/<id>/instance.json`, plus `game/` (the install) and `zomboid/`
//! (passed to the game as `-cachedir`, so saves, mods and options stay per instance).

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::Instance;

const FILE: &str = "instance.json";

#[derive(Serialize, Deserialize)]
struct Manifest {
    name: String,
    branch: String,
    /// Steam build id installed in `game/`; absent until the first install.
    #[serde(default)]
    build: Option<u32>,
    #[serde(default)]
    last_played: Option<u64>,
    created: u64,
}

/// Where instances live: `$LULO_HOME/instances`, else the OS data dir.
pub fn root() -> PathBuf {
    std::env::var_os("LULO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("lulo")))
        .unwrap_or_else(|| PathBuf::from("lulo"))
        .join("instances")
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

    std::fs::create_dir_all(dir.join("game"))?;
    std::fs::create_dir_all(dir.join("zomboid"))?;
    let manifest = Manifest {
        name: name.to_owned(),
        branch: branch.to_owned(),
        build: None,
        last_played: None,
        created: now(),
    };
    write(&dir, &manifest)?;
    Ok(instance(id, dir, &manifest))
}

/// Records the Steam build now installed in the instance's `game/`.
pub fn set_build(dir: &Path, build: u32) -> io::Result<()> {
    let path = dir.join(FILE);
    let mut manifest: Manifest = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    manifest.build = Some(build);
    write(dir, &manifest)
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
        let root = std::env::temp_dir().join(format!("lulo-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn creates_and_lists_instances() {
        let root = temp_root("list");
        let a = create(&root, "Brita's Armory", "public").unwrap();
        let b = create(&root, "Brita's Armory", "legacy41").unwrap();
        std::fs::create_dir_all(root.join("broken")).unwrap(); // no manifest: skipped

        assert_eq!(a.id.as_ref(), "brita-s-armory");
        assert_eq!(b.id.as_ref(), "brita-s-armory-2");
        assert!(a.dir.join("game").is_dir() && a.dir.join("zomboid").is_dir());

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
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn slugs_are_safe_folder_names() {
        assert_eq!(slug("Legacy MP (B41)"), "legacy-mp-b41");
        assert_eq!(slug("../../etc"), "etc");
        assert_eq!(slug("ñandú"), "and");
        assert_eq!(slug("ñ ü"), "instance");
    }
}
