//! Starting Project Zomboid from an instance's own game copy, with its own cachedir.

use std::io;
use std::path::Path;
use std::process::{Child, Command};

use super::{APP, Instance};

/// The command that starts the game in `game/` with saves, mods and options in `zomboid/`.
pub fn command(instance: &Instance) -> io::Result<Command> {
    let game = instance.dir.join("game");
    let cachedir = format!("-cachedir={}", instance.dir.join("zomboid").display());
    let missing = |what: &Path| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is missing; install the instance again", what.display()),
        )
    };

    let mut command = if cfg!(target_os = "macos") {
        let app = game.join("Project Zomboid.app");
        if !app.exists() {
            return Err(missing(&app));
        }
        // `open -W` waits for the app to quit, so the running state stays accurate.
        let mut open = Command::new("open");
        open.arg("-W").arg("-a").arg(app).arg("--args");
        open
    } else {
        let exe = game.join(if cfg!(windows) {
            "ProjectZomboid64.exe"
        } else {
            "projectzomboid.sh"
        });
        if !exe.exists() {
            return Err(missing(&exe));
        }
        Command::new(exe)
    };
    command.arg(cachedir).current_dir(&game);
    Ok(command)
}

// ponytail: the game's output is dropped; the Logs tab (#16) will read it.
pub fn start(instance: &Instance) -> io::Result<Child> {
    let mut command = command(instance)?;
    // Lets the Steam API find the app when the game isn't started by Steam itself.
    std::fs::write(
        instance.dir.join("game").join("steam_appid.txt"),
        APP.to_string(),
    )?;
    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn runs_the_instance_copy_with_its_cachedir() {
        let dir = std::env::temp_dir().join(format!("lulo-launch-{}", std::process::id()));
        let instance = Instance {
            id: "a".into(),
            dir: dir.clone(),
            name: "A".into(),
            branch: "public".into(),
            build: Some(1),
            last_played: None,
        };
        assert!(command(&instance).is_err(), "no game files yet");

        std::fs::create_dir_all(dir.join("game")).unwrap();
        std::fs::write(dir.join("game/projectzomboid.sh"), "#!/bin/sh\n").unwrap();
        let cmd = command(&instance).unwrap();
        assert_eq!(
            cmd.get_program(),
            dir.join("game/projectzomboid.sh").as_os_str()
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [format!("-cachedir={}", dir.join("zomboid").display())]
        );
        assert_eq!(cmd.get_current_dir(), Some(dir.join("game").as_path()));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
