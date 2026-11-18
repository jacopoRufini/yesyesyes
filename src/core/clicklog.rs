//! The log file: one JSON object per click, appended; past `MAX_LINES` the oldest are dropped.
//! Only written when the user turns it on.

use super::{ClickEvent, ClickResult};
use chrono::{DateTime, Local};
use std::io::Write;
use std::path::{Path, PathBuf};

const FILE: &str = "clicks.jsonl";
/// The file keeps the newest lines only.
pub const MAX_LINES: usize = 100;
/// Ids of the events read back, apart from the engine's and the server's.
const READ_IDS: u64 = 1 << 61;

/// Where logs usually go: `~/Library/Logs/yesyesyes` on macOS (shown by Console),
/// `~/.local/state/yesyesyes` on Linux (XDG state), `%LOCALAPPDATA%\yesyesyes` on Windows.
pub fn default_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    let base = dirs::home_dir().map(|h| h.join("Library").join("Logs"));
    #[cfg(target_os = "linux")]
    let base = dirs::state_dir();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let base = dirs::data_local_dir();
    base.unwrap_or_else(|| PathBuf::from(".")).join("yesyesyes").join(FILE)
}

/// The path the user typed (a leading `~` is the home folder), or the default one.
pub fn resolve(path: Option<&str>) -> PathBuf {
    let path = path.map(str::trim).unwrap_or("");
    if path.is_empty() {
        return default_path();
    }
    match (path.strip_prefix("~/").or(path.strip_prefix("~\\")), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ if path == "~" => dirs::home_dir().map(|h| h.join(FILE)).unwrap_or_else(default_path),
        _ => PathBuf::from(path),
    }
}

pub fn line(event: &ClickEvent) -> String {
    serde_json::json!({
        "at": event.at.to_rfc3339(),
        "app": event.app,
        "button": event.action,
        "result": event.result.label(),
        "text": event.shown_text(),
    })
    .to_string()
}

/// Appends the events, oldest first, then drops the oldest lines past `MAX_LINES`. The file is
/// readable by its owner only (macOS and Linux).
pub fn append(path: &Path, events: &[ClickEvent]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text: String = events.iter().map(|e| line(e) + "\n").collect();
    open_private(path, true)?.write_all(text.as_bytes())?;
    trim(path)
}

fn open_private(path: &Path, append: bool) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true);
    if append { options.append(true) } else { options.write(true).truncate(true) };
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

/// Rewrites the file with its newest `MAX_LINES` lines, through a new file renamed over it.
fn trim(path: &Path) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path)?;
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= MAX_LINES {
        return Ok(());
    }
    let kept: String = lines[lines.len() - MAX_LINES..].iter().map(|l| format!("{l}\n")).collect();
    let tmp = path.with_extension("jsonl.tmp");
    open_private(&tmp, false)?.write_all(kept.as_bytes())?;
    std::fs::rename(tmp, path)
}

/// The clicks of the file, newest first, to show them again after a restart. Lines that cannot
/// be read are skipped; no file gives none.
pub fn read(path: &Path) -> Vec<ClickEvent> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let mut events: Vec<ClickEvent> = text.lines().filter_map(parse).collect();
    events.reverse();
    events.truncate(MAX_LINES);
    for (n, event) in events.iter_mut().enumerate() {
        event.id = READ_IDS + n as u64;
    }
    events
}

fn parse(line: &str) -> Option<ClickEvent> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let field = |name: &str| value.get(name).and_then(|v| v.as_str());
    let result = match field("result")? {
        "pressed" => ClickResult::Pressed,
        "failed" => ClickResult::Failed,
        "ignored" => ClickResult::Ignored,
        "allowed" => ClickResult::Allowed,
        "denied" => ClickResult::Denied,
        _ => return None,
    };
    Some(ClickEvent {
        id: 0,
        at: DateTime::parse_from_rfc3339(field("at")?).ok()?.with_timezone(&Local),
        app: field("app")?.to_string(),
        action: field("button")?.to_string(),
        result,
        texts: vec![field("text").unwrap_or_default().to_string()],
        request: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ClickResult;
    use chrono::Local;

    fn event(result: ClickResult) -> ClickEvent {
        ClickEvent {
            id: 1,
            at: Local::now(),
            app: "Claude".into(),
            action: "Allow once".into(),
            result,
            texts: vec!["Claude".into(), "Run echo \"hi\"".into()],
            request: None,
        }
    }

    #[test]
    fn one_json_object_per_line() {
        let value: serde_json::Value = serde_json::from_str(&line(&event(ClickResult::Failed))).unwrap();
        assert_eq!(value["app"], "Claude");
        assert_eq!(value["button"], "Allow once");
        assert_eq!(value["result"], "failed");
        assert_eq!(value["text"], "Claude\nRun echo \"hi\"");
        assert!(!line(&event(ClickResult::Ignored)).contains('\n'));
    }

    #[test]
    fn appends_to_a_private_file() {
        let dir = std::env::temp_dir().join(format!("yyy-log-{}", std::process::id()));
        let path = dir.join("sub").join(FILE);
        append(&path, &[event(ClickResult::Pressed)]).unwrap();
        append(&path, &[event(ClickResult::Pressed), event(ClickResult::Ignored)]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 3);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keeps_only_the_newest_lines() {
        let dir = std::env::temp_dir().join(format!("yyy-log-trim-{}", std::process::id()));
        let path = dir.join(FILE);
        for n in 0..MAX_LINES + 7 {
            let mut e = event(ClickResult::Pressed);
            e.texts = vec![format!("click {n}")];
            append(&path, &[e]).unwrap();
        }
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), MAX_LINES);
        assert!(text.lines().next().unwrap().contains("click 7"), "the oldest are dropped");
        assert!(text.lines().last().unwrap().contains(&format!("click {}", MAX_LINES + 6)));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_the_clicks_back_newest_first() {
        let dir = std::env::temp_dir().join(format!("yyy-log-read-{}", std::process::id()));
        let path = dir.join(FILE);
        let mut first = event(ClickResult::Allowed);
        first.request = Some("git status".into());
        append(&path, &[first, event(ClickResult::Ignored)]).unwrap();
        std::fs::write(&path, std::fs::read_to_string(&path).unwrap() + "not json\n").unwrap();
        let events = read(&path);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].result, ClickResult::Ignored);
        assert_eq!((events[1].result, events[1].shown_text().as_str()), (ClickResult::Allowed, "git status"));
        assert_ne!(events[0].id, events[1].id);
        assert!(read(&dir.join("missing.jsonl")).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resolves_the_path() {
        assert_eq!(resolve(None), default_path());
        assert_eq!(resolve(Some("  ")), default_path());
        assert!(default_path().ends_with(Path::new("yesyesyes").join(FILE)));
        let home = dirs::home_dir().unwrap();
        assert_eq!(resolve(Some("~/logs/c.jsonl")), home.join("logs/c.jsonl"));
        assert_eq!(resolve(Some("/tmp/c.jsonl")), PathBuf::from("/tmp/c.jsonl"));
    }
}
