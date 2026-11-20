//! The model and the chat name of a Claude Code session, read from its transcript: the hook
//! request gives the transcript's path, not them. Nothing else of the transcript is read.

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Enough for the last answers of the model.
const TAIL: u64 = 256 * 1024;
const KEY: &str = "\"model\":\"";

/// The model of the last answer in the transcript at `path`, when it is a Claude Code
/// transcript under `claude_dir` (`~/.claude`).
pub fn last_model(path: &Path, claude_dir: &Path) -> Option<String> {
    let mut file = open(path, claude_dir)?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(TAIL))).ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    model_in(&String::from_utf8_lossy(&tail))
}

/// The transcript at `path`, when it is a Claude Code transcript under `claude_dir`.
fn open(path: &Path, claude_dir: &Path) -> Option<std::fs::File> {
    let path = path.canonicalize().ok()?;
    if !path.starts_with(claude_dir.canonicalize().ok()?) || path.extension()? != "jsonl" {
        return None;
    }
    std::fs::File::open(&path).ok()
}

/// The name of the chat: the last one given to it (`/rename`), else the last one Claude Code made up.
pub fn chat_name(path: &Path, claude_dir: &Path) -> Option<String> {
    title_in(BufReader::new(open(path, claude_dir)?))
}

const MAX_TITLE_CHARS: usize = 80;

fn title_in(reader: impl BufRead) -> Option<String> {
    let (mut given, mut made_up) = (None, None);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        // Only the title lines are parsed.
        if !line.contains("-title\"") {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&line) else { continue };
        let text = |key: &str| entry.get(key).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(|t| t.chars().take(MAX_TITLE_CHARS).collect());
        match entry.get("type").and_then(Value::as_str) {
            Some("custom-title") => given = text("customTitle").or(given),
            Some("ai-title") => made_up = text("aiTitle").or(made_up),
            _ => {}
        }
    }
    given.or(made_up)
}

/// The last model name in the text that looks like one (`claude-opus-5-5`).
pub fn model_in(text: &str) -> Option<String> {
    text.rmatch_indices(KEY).find_map(|(at, _)| {
        let rest = &text[at + KEY.len()..];
        let name = &rest[..rest.find('"')?];
        let fits = name.starts_with("claude-") && name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || "-._".contains(c));
        fits.then(|| name.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_last_claude_model() {
        let lines = r#"{"message":{"model":"claude-sonnet-5-5"}}
{"message":{"model":"claude-opus-5-5","content":"x"}}
{"message":{"model":"<synthetic>"}}"#;
        assert_eq!(model_in(lines).as_deref(), Some("claude-opus-5-5"));
        assert_eq!(model_in(r#"{"model":"claude-x; rm"}"#), None, "not a model name");
        assert_eq!(model_in("no model"), None);
    }

    #[test]
    fn the_given_name_wins_over_the_made_up_one() {
        let title = |text: &str| title_in(text.as_bytes());
        let made_up = r#"{"type":"ai-title","aiTitle":"Fix the build","sessionId":"s"}"#;
        let given = r#"{"type":"custom-title","customTitle":"yesyesyes","sessionId":"s"}"#;
        let renamed = r#"{"type":"custom-title","customTitle":"yyy repo","sessionId":"s"}"#;
        let message = r#"{"type":"user","message":{"content":"ai-title\" in a message"}}"#;
        assert_eq!(title(made_up).as_deref(), Some("Fix the build"));
        assert_eq!(title(&[given, made_up, message].join("\n")).as_deref(), Some("yesyesyes"));
        assert_eq!(title(&[given, renamed].join("\n")).as_deref(), Some("yyy repo"), "the last name");
        assert_eq!(title(message), None);
        let long = format!(r#"{{"type":"ai-title","aiTitle":"{}"}}"#, "x".repeat(200));
        assert_eq!(title(&long).map(|t| t.len()), Some(MAX_TITLE_CHARS));
    }

    #[test]
    fn reads_only_transcripts_under_the_claude_folder() {
        let dir = std::env::temp_dir().join(format!("yyy-transcript-{}", std::process::id()));
        let claude = dir.join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        let inside = claude.join("s.jsonl");
        std::fs::write(&inside, r#"{"message":{"model":"claude-opus-5-5"}}"#).unwrap();
        let outside = dir.join("s.jsonl");
        std::fs::write(&outside, r#"{"message":{"model":"claude-opus-5-5"}}"#).unwrap();
        assert_eq!(last_model(&inside, &claude).as_deref(), Some("claude-opus-5-5"));
        assert_eq!(last_model(&outside, &claude), None);
        assert_eq!(last_model(&claude.join("missing.jsonl"), &claude), None);
        std::fs::write(&outside, r#"{"type":"ai-title","aiTitle":"x"}"#).unwrap();
        assert_eq!(chat_name(&outside, &claude), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
