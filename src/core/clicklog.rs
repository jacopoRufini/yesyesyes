//! The log file: one JSON object per click, appended. Only written when the user turns it on.

use super::ClickEvent;
use std::io::Write;
use std::path::{Path, PathBuf};

const FILE: &str = "clicks.jsonl";

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
        "text": event.full_text(),
    })
    .to_string()
}

/// Appends the events, oldest first. The file is readable by its owner only (macOS and Linux).
pub fn append(path: &Path, events: &[ClickEvent]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    let text: String = events.iter().map(|e| line(e) + "\n").collect();
    file.write_all(text.as_bytes())
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
    fn resolves_the_path() {
        assert_eq!(resolve(None), default_path());
        assert_eq!(resolve(Some("  ")), default_path());
        assert!(default_path().ends_with(Path::new("yesyesyes").join(FILE)));
        let home = dirs::home_dir().unwrap();
        assert_eq!(resolve(Some("~/logs/c.jsonl")), home.join("logs/c.jsonl"));
        assert_eq!(resolve(Some("/tmp/c.jsonl")), PathBuf::from("/tmp/c.jsonl"));
    }
}
