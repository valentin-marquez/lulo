//! Finding a Project Zomboid that Steam already installed, to adopt instead of downloading it
//! again: where Steam is, its libraries, and what its `appmanifest` says about the game.

// Tests don't call `find` either, so this holds in both builds until the dialog uses it.
#![expect(dead_code, reason = "the import dialog (#23) will call it")]

use std::path::{Path, PathBuf};

use steam::keyvalues::{self, Value};

use super::APP;

/// The game as Steam installed it on this computer.
#[derive(Debug, PartialEq)]
pub struct SteamInstall {
    /// The game's own folder, `steamapps/common/<installdir>` in its library.
    pub dir: PathBuf,
    pub build: u32,
    /// The Steam branch, `public` when Steam records none.
    pub branch: String,
}

/// The first complete Steam install of the game, searching every library of the Steam found
/// here. `None` when there is no Steam, no game, or the game is still being updated.
pub fn find() -> Option<SteamInstall> {
    steam_roots()
        .iter()
        .flat_map(|root| libraries(root))
        .find_map(|library| read(&library))
}

/// Where the Steam client may live on this system; only folders that have a `steamapps`.
fn steam_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    #[cfg(windows)]
    {
        roots.extend(registry_steam_path());
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            roots.extend(std::env::var_os(var).map(|dir| PathBuf::from(dir).join("Steam")));
        }
    }
    #[cfg(target_os = "macos")]
    roots.extend(dirs::home_dir().map(|home| home.join("Library/Application Support/Steam")));
    #[cfg(target_os = "linux")]
    if let Some(home) = dirs::home_dir() {
        for relative in [
            ".steam/steam",
            ".local/share/Steam",
            ".var/app/com.valvesoftware.Steam/.local/share/Steam",
            "snap/steam/common/.local/share/Steam",
        ] {
            roots.push(home.join(relative));
        }
    }
    roots.retain(|root| root.join("steamapps").is_dir());
    unique(roots)
}

/// `paths` without repeats, keeping the first spelling of each: the registry says
/// `c:/program files (x86)/steam` where Windows says `C:\Program Files (x86)\Steam`.
fn unique(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = Vec::new();
    paths
        .into_iter()
        .filter(|path| {
            let real = path.canonicalize().unwrap_or_else(|_| path.clone());
            let new = !seen.contains(&real);
            seen.push(real);
            new
        })
        .collect()
}

/// The Steam folder the Windows registry records for the current user.
#[cfg(windows)]
fn registry_steam_path() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("reg")
        .args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    parse_reg_path(&String::from_utf8_lossy(&out.stdout))
}

/// The value of `SteamPath` in the output of `reg query`.
#[cfg(any(windows, test))]
fn parse_reg_path(text: &str) -> Option<PathBuf> {
    let line = text
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("SteamPath"))?;
    let value = line.split_once("REG_SZ")?.1.trim();
    (!value.is_empty()).then(|| PathBuf::from(value))
}

/// The Steam folder itself and every library `libraryfolders.vdf` lists.
fn libraries(root: &Path) -> Vec<PathBuf> {
    let text =
        std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")).unwrap_or_default();
    let mut found = vec![root.to_owned()];
    found.extend(library_paths(&text));
    unique(found)
}

/// The paths in `libraryfolders.vdf`: each entry holds a `path` (current format), or is the
/// path itself under a numbered key (older Steam).
fn library_paths(text: &str) -> Vec<PathBuf> {
    let Ok(file) = keyvalues::parse(text) else {
        return Vec::new();
    };
    let Some(folders) = file.map("libraryfolders") else {
        return Vec::new();
    };
    folders
        .iter()
        .filter(|(key, _)| key.bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|(_, value)| match value {
            Value::Map(entry) => entry.str("path"),
            Value::Str(path) => Some(path.as_str()),
        })
        .map(PathBuf::from)
        .collect()
}

/// The game in one library, if it is installed there and its folder exists.
fn read(library: &Path) -> Option<SteamInstall> {
    let text =
        std::fs::read_to_string(library.join(format!("steamapps/appmanifest_{APP}.acf"))).ok()?;
    let install = from_manifest(&text, library)?;
    install.dir.is_dir().then_some(install)
}

fn from_manifest(text: &str, library: &Path) -> Option<SteamInstall> {
    let manifest = keyvalues::parse(text).ok()?;
    let app = manifest.map("AppState")?;
    if app.str("appid")?.parse::<u32>().ok()? != APP {
        return None;
    }
    if !complete(app.str("StateFlags")?.parse().ok()?) {
        return None;
    }
    // The folder name comes from a file on disk: it must be one plain name, not a path.
    let folder = app.str("installdir")?;
    if Path::new(folder).file_name() != Some(std::ffi::OsStr::new(folder)) {
        return None;
    }
    // What is mounted is what is on disk; the user's choice may not be downloaded yet.
    let branch = ["MountedConfig", "UserConfig"]
        .iter()
        .find_map(|section| app.map(section)?.str("BetaKey"))
        .filter(|branch| !branch.is_empty())
        .unwrap_or("public");
    Some(SteamInstall {
        dir: library.join("steamapps/common").join(folder),
        build: app.str("buildid")?.parse().ok()?,
        branch: branch.to_owned(),
    })
}

/// Steam's `StateFlags`: installed (4) and with nothing pending: no update required (2), files
/// missing (32) or corrupt (128), update running, paused or started (256, 512, 1024), nor
/// uninstalling (2048).
fn complete(flags: u32) -> bool {
    flags & 4 != 0 && flags & (2 | 32 | 128 | 256 | 512 | 1024 | 2048) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(flags: &str, extra: &str) -> String {
        format!(
            r#""AppState"
            {{
                "appid"        "108600"
                "installdir"   "ProjectZomboid"
                "StateFlags"   "{flags}"
                "buildid"      "26486631"
                {extra}
            }}"#
        )
    }

    fn library() -> PathBuf {
        PathBuf::from("lib")
    }

    #[test]
    fn reads_the_build_and_the_branch() {
        let install = from_manifest(&manifest("4", ""), &library()).unwrap();
        assert_eq!(
            install,
            SteamInstall {
                dir: library().join("steamapps/common/ProjectZomboid"),
                build: 26486631,
                branch: "public".into(),
            }
        );

        let user = r#""UserConfig" { "BetaKey" "legacy41" }"#;
        assert_eq!(
            from_manifest(&manifest("4", user), &library())
                .unwrap()
                .branch,
            "legacy41"
        );
        // What is mounted wins over what the user picked and hasn't been downloaded yet.
        let both =
            r#""UserConfig" { "BetaKey" "unstable" } "MountedConfig" { "BetaKey" "legacy41" }"#;
        assert_eq!(
            from_manifest(&manifest("4", both), &library())
                .unwrap()
                .branch,
            "legacy41"
        );
        let blank = r#""UserConfig" { "BetaKey" "" }"#;
        assert_eq!(
            from_manifest(&manifest("4", blank), &library())
                .unwrap()
                .branch,
            "public"
        );
    }

    #[test]
    fn only_a_complete_install_counts() {
        for flags in ["4", "68"] {
            assert!(
                from_manifest(&manifest(flags, ""), &library()).is_some(),
                "{flags}"
            );
        }
        // Not installed, update required, update running, files corrupt, uninstalling.
        for flags in ["1", "6", "260", "132", "2052", "nope"] {
            assert!(
                from_manifest(&manifest(flags, ""), &library()).is_none(),
                "{flags}"
            );
        }
    }

    #[test]
    fn refuses_what_is_not_this_game_or_not_a_folder_name() {
        let other = manifest("4", "").replace("108600", "220");
        assert!(from_manifest(&other, &library()).is_none());
        for name in ["..", ".", "a/b", "/etc", ""] {
            let bad = manifest("4", "").replace("ProjectZomboid", name);
            assert!(from_manifest(&bad, &library()).is_none(), "{name:?}");
        }
        assert!(from_manifest("not a manifest", &library()).is_none());
        assert!(from_manifest(r#""AppState" { "appid" "108600" }"#, &library()).is_none());
    }

    #[test]
    fn reads_every_library_in_both_formats() {
        let current = r#""libraryfolders"
        {
            "0" { "path" "C:\\Program Files (x86)\\Steam" "label" "" "apps" { "108600" "100" } }
            "1" { "path" "D:\\SteamLibrary" }
            "contentstatsid" "123"
        }"#;
        assert_eq!(
            library_paths(current),
            [
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from(r"D:\SteamLibrary")
            ]
        );
        let old = r#""LibraryFolders" { "TimeNextStatsReport" "1" "1" "/mnt/games/SteamLibrary" }"#;
        assert_eq!(
            library_paths(old),
            [PathBuf::from("/mnt/games/SteamLibrary")]
        );
        assert!(library_paths("garbage").is_empty());
    }

    #[test]
    fn the_same_folder_spelled_two_ways_counts_once() {
        let dir = std::env::temp_dir().join(format!("lulo-unique-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let again = dir.join("sub").join("..");
        let other = std::env::temp_dir();
        let kept = unique(vec![dir.clone(), again, other.clone(), dir.clone()]);
        assert_eq!(kept, [dir.clone(), other]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_the_steam_folder_from_reg_query() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\Valve\\Steam\r\n    SteamPath    REG_SZ    c:/program files (x86)/steam\r\n\r\n";
        assert_eq!(
            parse_reg_path(out),
            Some(PathBuf::from("c:/program files (x86)/steam"))
        );
        assert_eq!(parse_reg_path("ERROR: not found"), None);
    }

    #[test]
    fn finds_the_game_in_a_library_on_disk() {
        let root = std::env::temp_dir().join(format!("lulo-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let steamapps = root.join("steamapps");
        std::fs::create_dir_all(&steamapps).unwrap();
        assert!(read(&root).is_none(), "no manifest yet");

        std::fs::write(
            steamapps.join(format!("appmanifest_{APP}.acf")),
            manifest("4", ""),
        )
        .unwrap();
        assert!(read(&root).is_none(), "the game folder is missing");

        std::fs::create_dir_all(steamapps.join("common/ProjectZomboid")).unwrap();
        let install = read(&root).unwrap();
        assert_eq!(install.build, 26486631);
        assert_eq!(install.dir, steamapps.join("common/ProjectZomboid"));

        std::fs::write(
            steamapps.join("libraryfolders.vdf"),
            format!(
                r#""libraryfolders" {{ "1" {{ "path" "{}" }} }}"#,
                "/elsewhere"
            ),
        )
        .unwrap();
        assert_eq!(
            libraries(&root),
            [root.clone(), PathBuf::from("/elsewhere")]
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
