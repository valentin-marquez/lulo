//! Starting lulo when the user signs in to the system: the registry on Windows, a LaunchAgent
//! on macOS, an autostart entry on Linux. The system holds the state, lulo doesn't keep a copy.

use auto_launch::{AutoLaunch, AutoLaunchBuilder};

fn launcher() -> Result<AutoLaunch, String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    AutoLaunchBuilder::new()
        .set_app_name("Lulo")
        .set_app_path(&exe.to_string_lossy())
        .build()
        .map_err(|err| err.to_string())
}

/// Whether the system starts lulo at sign in; `false` when it can't be told.
pub fn enabled() -> bool {
    launcher()
        .and_then(|launcher| launcher.is_enabled().map_err(|err| err.to_string()))
        .unwrap_or(false)
}

pub fn set(on: bool) -> Result<(), String> {
    let launcher = launcher()?;
    if on {
        launcher.enable()
    } else {
        launcher.disable()
    }
    .map_err(|err| err.to_string())
}
