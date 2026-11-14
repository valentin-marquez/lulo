//! The game's console. PZ rewrites `console.txt` in the cachedir on every launch and keeps it
//! after exit, so following that file is the live view; each finished session is also saved as
//! `Logs/<dd-mm-yy_HH-MM-SS>_DebugLog.txt` (older days under `Logs/logs_<dd-mm>/`).

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use gpui_kit::SharedString;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}

impl Level {
    /// PZ prefixes each entry with its level: `ERROR: `, `WARN : `, `LOG  : `, `DEBUG: `.
    fn of(line: &str) -> Option<Level> {
        let (tag, _) = line.split_once(':')?;
        Some(match tag.trim_end() {
            "ERROR" | "SEVERE" => Level::Error,
            "WARN" => Level::Warn,
            "LOG" | "INFO" => Level::Info,
            "DEBUG" | "TRACE" => Level::Debug,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
        }
    }
}

pub struct Line {
    pub level: Level,
    /// Local `HH:MM:SS`. `None` on continuation lines, and on Build 42's live console, which
    /// writes no time.
    pub time: Option<SharedString>,
    /// PZ's logger name, e.g. `General`, `Mod`, `Lua`; `None` on continuation lines (stack
    /// traces, wrapped messages), which is how an entry is told from one.
    pub category: Option<SharedString>,
    pub message: SharedString,
    /// The line as written, for copying.
    pub raw: SharedString,
}

/// One log file to show: the live console or a saved session.
#[derive(Clone, PartialEq)]
pub struct Session {
    pub name: SharedString,
    pub path: PathBuf,
    pub live: bool,
}

/// The live console first, then saved sessions, newest first.
pub fn sessions(cachedir: &Path) -> Vec<Session> {
    let mut saved: Vec<(String, PathBuf)> = Vec::new();
    let logs = cachedir.join("Logs");
    let mut dirs = vec![logs.clone()];
    if let Ok(entries) = std::fs::read_dir(&logs) {
        dirs.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
    }
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.flatten().map(|e| e.path()) {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if let Some(stamp) = name.strip_suffix("_DebugLog.txt") {
                saved.push((stamp.to_owned(), path.clone()));
            }
        }
    }
    saved.sort_by_key(|(stamp, _)| std::cmp::Reverse(sortable(stamp)));
    let mut all = vec![Session {
        name: "Live".into(),
        path: cachedir.join("console.txt"),
        live: true,
    }];
    all.extend(saved.into_iter().map(|(stamp, path)| Session {
        name: session_name(&stamp).into(),
        path,
        live: false,
    }));
    all
}

/// `dd-mm-yy_HH-MM-SS` reordered as `yymmddHHMMSS`, which sorts by time.
fn sortable(stamp: &str) -> String {
    let parts: Vec<&str> = stamp.split(['-', '_']).collect();
    match parts.as_slice() {
        [d, m, y, rest @ ..] => format!("{y}{m}{d}{}", rest.concat()),
        _ => stamp.to_owned(),
    }
}

/// `12-11-26_20-07-52` as "Nov 12 20:07".
fn session_name(stamp: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let parts: Vec<&str> = stamp.split(['-', '_']).collect();
    if let [d, m, _, hh, mm, ..] = parts.as_slice()
        && let Some(month) = m
            .parse::<usize>()
            .ok()
            .and_then(|m| MONTHS.get(m.wrapping_sub(1)))
    {
        return format!("{month} {} {hh}:{mm}", d.trim_start_matches('0'));
    }
    stamp.to_owned()
}

/// Follows one log file: each `poll` returns the lines appended since the last one.
pub struct Console {
    path: PathBuf,
    read: u64,
    /// Bytes after the last newline, waiting for the rest of their line.
    partial: Vec<u8>,
    last: Level,
}

impl Console {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            read: 0,
            partial: Vec::new(),
            last: Level::Info,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// New complete lines, and whether the file started over (a new launch) so the caller
    /// drops what it had.
    // ponytail: a restart is noticed when the file shrinks; a new session that outgrows the old
    // one between two polls (half a second) would be read from the old offset.
    pub fn poll(&mut self) -> (Vec<Line>, bool) {
        let Ok(len) = std::fs::metadata(&self.path).map(|m| m.len()) else {
            return (Vec::new(), false);
        };
        let restarted = len < self.read;
        if restarted {
            (self.read, self.last) = (0, Level::Info);
            self.partial.clear();
        }
        if len == self.read {
            return (Vec::new(), restarted);
        }
        let mut bytes = std::mem::take(&mut self.partial);
        let got = std::fs::File::open(&self.path).and_then(|mut file| {
            file.seek(SeekFrom::Start(self.read))?;
            file.take(len - self.read).read_to_end(&mut bytes)
        });
        let Ok(got) = got else {
            self.partial = bytes;
            return (Vec::new(), restarted);
        };
        self.read += got as u64;
        let complete = bytes.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
        self.partial = bytes.split_off(complete);
        let text = String::from_utf8_lossy(&bytes);
        let lines = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let parsed = parse(line, self.last);
                self.last = parsed.level;
                parsed
            })
            .collect();
        (lines, restarted)
    }
}

/// One line of the game's console, in either build's format:
/// - Build 41: `LOG  : General     , 1794524872665> message` (epoch milliseconds);
/// - Build 42: `LOG  : General      f:0> message`, or with where it was logged from,
///   `ERROR: General      f:0 at AngelCodeFont.parseFnt   > message` (no time at all).
///
/// Saved sessions put `[12-11-26 20:07:52.665] ` (local time) before either. Anything else
/// continues the entry above and keeps its level.
fn parse(line: &str, last: Level) -> Line {
    let raw: SharedString = line.trim_end().to_owned().into();
    let (stamp, rest) = match line.strip_prefix('[').and_then(|l| l.split_once("] ")) {
        Some((stamp, rest)) => (stamp.get(9..17), rest),
        None => (None, line),
    };
    let entry = Level::of(rest).and_then(|level| {
        let body = rest.split_once(':')?.1.trim_start();
        let (category, time, message) = entry_41(body, stamp).or_else(|| entry_42(body, stamp))?;
        Some(Line {
            level,
            time: time.map(SharedString::from),
            category: Some(category.to_owned().into()),
            message: message.into(),
            raw: raw.clone(),
        })
    });
    entry.unwrap_or_else(|| Line {
        level: Level::of(rest).unwrap_or(last),
        time: None,
        category: None,
        message: rest.trim_end().replace('\t', "    ").into(),
        raw,
    })
}

/// Category, time and message of a Build 41 entry, after its `LEVEL:`.
fn entry_41<'a>(body: &'a str, stamp: Option<&str>) -> Option<(&'a str, Option<String>, String)> {
    let (category, tail) = body.split_once(',')?;
    let (millis, message) = tail.trim_start().split_once("> ")?;
    let millis: i64 = millis.parse().ok()?;
    let time = match stamp {
        Some(stamp) => stamp.to_owned(),
        None => local_time(millis)?,
    };
    Some((category.trim(), Some(time), message.trim_end().to_owned()))
}

/// The same for Build 42: `General      f:0> text` or `General      f:0 at Where.from   > text`.
/// The frame counter is dropped; where it was logged from leads the message.
fn entry_42<'a>(body: &'a str, stamp: Option<&str>) -> Option<(&'a str, Option<String>, String)> {
    let (category, tail) = body.split_once(char::is_whitespace)?;
    let (head, message) = tail.trim_start().strip_prefix("f:")?.split_once('>')?;
    let (frame, from) = match head.split_once(" at ") {
        Some((frame, from)) => (frame, Some(from.trim())),
        None => (head, None),
    };
    if frame.trim().parse::<u64>().is_err() {
        return None;
    }
    let message = message.trim();
    let message = match from.filter(|from| !from.is_empty()) {
        Some(from) => format!("{from} > {message}"),
        None => message.to_owned(),
    };
    Some((category, stamp.map(str::to_owned), message))
}

fn local_time(millis: i64) -> Option<String> {
    use chrono::TimeZone;
    let at = chrono::Local.timestamp_millis_opt(millis).single()?;
    Some(at.format("%H:%M:%S").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured from a legacy41 session's console.txt (paths shortened).
    const SAMPLE: &str = "\
DEBUG: General     , 1794524872596> LoggerManager.init                  > Initializing...
LOG  : General     , 1794524872665> cachedir set to \"/lulo/instances/new-instance/zomboid\"
ERROR: General     , 1794524330296> ExceptionLogger.logException> Exception thrown java.nio.file.NoSuchFileException: /zomboid/mods at UnixException.translateToIOException.
java.nio.file.NoSuchFileException: /zomboid/mods
\tat java.base/sun.nio.fs.UnixException.translateToIOException(Unknown Source)
WARN : General     , 1794524335184> MainScreen.onResolutionChange 1680x1050 -> 1920x1080
COMMAND QUEUE stallCount = 0 peakUsage = 352
";

    fn levels(lines: &[Line]) -> Vec<Level> {
        lines.iter().map(|l| l.level).collect()
    }

    // Captured from a Build 42.22 console.txt and its saved session (paths shortened).
    #[test]
    fn reads_build_42_entries() {
        let plain = parse("LOG  : General      f:0> RAM: 15907 Mb", Level::Info);
        assert_eq!(plain.level, Level::Info);
        assert_eq!(plain.category.as_deref(), Some("General"));
        assert_eq!(
            (plain.time.is_none(), plain.message.as_ref()),
            (true, "RAM: 15907 Mb")
        );

        let from = parse(
            "ERROR: General      f:0 at AngelCodeFont.parseFnt              > AngelCodeFont failed to load page 0 texture /media/mainfont_0.png",
            Level::Info,
        );
        assert_eq!(from.level, Level::Error);
        assert_eq!(
            from.message.as_ref(),
            "AngelCodeFont.parseFnt > AngelCodeFont failed to load page 0 texture /media/mainfont_0.png"
        );

        let saved = parse(
            "[13-11-26 20:38:33.898] WARN : Lua          f:12> ===== System specs =====.",
            Level::Info,
        );
        assert_eq!(saved.level, Level::Warn);
        assert_eq!(saved.time.as_deref(), Some("20:38:33"));
        assert_eq!(saved.category.as_deref(), Some("Lua"));

        // Not an entry: no frame counter where one belongs.
        let not = parse("LOG  : just some text > here", Level::Warn);
        assert_eq!((not.category.is_none(), not.level), (true, Level::Info));
    }

    #[test]
    fn splits_entries_into_columns() {
        let line = parse(SAMPLE.lines().nth(1).unwrap(), Level::Info);
        assert_eq!(line.level, Level::Info);
        assert_eq!(line.category.as_deref(), Some("General"));
        assert_eq!(
            line.message.as_ref(),
            "cachedir set to \"/lulo/instances/new-instance/zomboid\""
        );
        assert_eq!(line.time.as_deref(), local_time(1794524872665).as_deref());

        // Saved sessions carry their own local time in front.
        let saved = parse(
            "[12-11-26 20:07:52.665] WARN : Mod         , 1794524872665> missing texture",
            Level::Info,
        );
        assert_eq!(saved.time.as_deref(), Some("20:07:52"));
        assert_eq!(
            (saved.level, saved.message.as_ref()),
            (Level::Warn, "missing texture")
        );

        let trace = parse("\tat zombie.Lua(Unknown Source)", Level::Error);
        assert_eq!((trace.level, trace.time.is_none()), (Level::Error, true));
    }

    #[test]
    fn follows_the_file_across_partial_writes_and_restarts() {
        let dir = std::env::temp_dir().join(format!("lulo-logs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("console.txt");
        let mut console = Console::new(path.clone());
        assert!(console.poll().0.is_empty(), "no file yet");

        // The game is mid-line when we read: only complete lines come out.
        let cut = SAMPLE.find("\njava.nio").unwrap() + 5;
        std::fs::write(&path, &SAMPLE[..cut]).unwrap();
        let (first, restarted) = console.poll();
        assert!(!restarted);
        assert_eq!(levels(&first), [Level::Debug, Level::Info, Level::Error]);

        std::fs::write(&path, SAMPLE).unwrap();
        let (rest, _) = console.poll();
        use Level::*;
        // The stack trace stays an error; the queue line follows the warning above it.
        assert_eq!(levels(&rest), [Error, Error, Warn, Warn]);
        assert_eq!(
            rest[0].raw.as_ref(),
            "java.nio.file.NoSuchFileException: /zomboid/mods"
        );

        // A new launch rewrites the file from scratch.
        std::fs::write(&path, "LOG  : General     , 1> again\n").unwrap();
        let (again, restarted) = console.poll();
        assert!(restarted);
        assert_eq!(levels(&again), [Info]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn lists_live_then_saved_sessions_newest_first() {
        let dir = std::env::temp_dir().join(format!("lulo-sessions-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("Logs/logs_11-11")).unwrap();
        for file in [
            "Logs/12-11-26_20-07-52_DebugLog.txt",
            "Logs/logs_11-11/11-11-26_22-41-03_DebugLog.txt",
            "Logs/logs_11-11/12-11-26_19-58-47_DebugLog.txt",
            "Logs/12-11-26_20-07-52_other.txt",
        ] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        let names: Vec<String> = sessions(&dir).iter().map(|s| s.name.to_string()).collect();
        assert_eq!(
            names,
            ["Live", "Nov 12 20:07", "Nov 12 19:58", "Nov 11 22:41"]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
