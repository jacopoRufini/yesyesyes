//! The permission requests Claude Code sends (see `gate_server`), with the whole tool input.
//! Claude Desktop cuts the notification text; the request whose input holds the end of that cut
//! text, received shortly before, is the whole one.

use chrono::{DateTime, Duration, FixedOffset, Local};
use serde_json::Value;

/// How much of the end of the cut text must be found in the request.
const PROBE: usize = 40;
const MIN_PROBE: usize = 12;

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub at: DateTime<FixedOffset>,
    /// The tool Claude asked to use, e.g. Bash.
    pub tool: String,
    /// What to show: the command, else the input as indented JSON.
    pub text: String,
    /// Every string of the input, on one line: what the notification text is matched against.
    haystack: String,
}

/// From the JSON of a Claude Code hook (`tool_name`, `tool_input`).
pub fn from_hook(value: &Value, at: DateTime<FixedOffset>) -> Option<Request> {
    let tool = value.get("tool_name")?.as_str()?.to_string();
    let input = value.get("tool_input")?;
    let text = match input.get("command").and_then(Value::as_str) {
        Some(command) => command.to_string(),
        None => serde_json::to_string_pretty(input).ok()?,
    };
    let mut strings = Vec::new();
    collect_strings(input, &mut strings);
    Some(Request { at, tool, text, haystack: one_line(&strings.join(" ")) })
}

fn collect_strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(s) => out.push(s),
        Value::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
        Value::Object(map) => map.values().for_each(|v| collect_strings(v, out)),
        _ => {}
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The newest request received from 10 minutes before to 5 seconds after the notification whose
/// input holds the end of its cut text ("Allow Claude to run <command>…?").
pub fn find<'a>(requests: &'a [Request], at: DateTime<Local>, texts: &[String]) -> Option<&'a Request> {
    let probe = probe(texts)?;
    let (from, to) = (at - Duration::minutes(10), at + Duration::seconds(5));
    requests.iter().rev().find(|r| r.at >= from && r.at <= to && r.haystack.contains(&probe))
}

/// The end of the longest text, without the cut mark and the word before the request
/// ("run", or the name of the tool).
fn probe(texts: &[String]) -> Option<String> {
    let longest = texts.iter().max_by_key(|t| t.chars().count())?;
    let text = one_line(longest);
    let text = text.trim_end_matches('?').trim_end_matches(['…', '.']).trim_end();
    let asked = text.split_once("Allow Claude to ").map_or(text, |(_, rest)| rest);
    let request = asked.split_once(' ').map_or("", |(_, rest)| rest).trim();
    let chars: Vec<char> = request.chars().collect();
    if chars.len() < MIN_PROBE {
        return None;
    }
    Some(chars[chars.len().saturating_sub(PROBE)..].iter().collect())
}

#[cfg(test)]
pub fn sample(at: &str, tool: &str, input: &str) -> Request {
    let value = serde_json::from_str(&format!(r#"{{"tool_name": "{tool}", "tool_input": {input}}}"#)).unwrap();
    from_hook(&value, DateTime::parse_from_rfc3339(at).unwrap()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CUT: &str = "Claude, contact-less, notification autoclicker, Allow Claude to run touch ~/x && echo \"a long line that the notification\ncuts somewhere in the…?";

    fn at(time: &str) -> DateTime<Local> {
        DateTime::parse_from_rfc3339(time).unwrap().with_timezone(&Local)
    }

    #[test]
    fn reads_the_command_or_the_whole_input() {
        let bash = sample("2026-11-18T15:50:12+01:00", "Bash", r#"{"command": "ls -l"}"#);
        assert_eq!((bash.tool.as_str(), bash.text.as_str()), ("Bash", "ls -l"));
        let tool = sample("2026-11-18T15:50:12+01:00", "Recap", r#"{"recap": "Done.\nAll good"}"#);
        assert!(tool.text.contains("\"recap\": \"Done.\\nAll good\""));
        assert_eq!(tool.haystack, "Done. All good");
        let now = Local::now().fixed_offset();
        assert_eq!(from_hook(&serde_json::json!({"tool_input": {}}), now), None, "no tool");
        assert_eq!(from_hook(&serde_json::json!({"tool_name": "Bash"}), now), None, "no input");
    }

    #[test]
    fn finds_the_whole_request_of_a_cut_notification() {
        let whole = r#"{"command": "touch ~/x && echo \"a long line that the notification cuts somewhere in the middle\""}"#;
        let requests = vec![
            sample("2026-11-18T15:45:00+01:00", "Bash", whole),
            sample("2026-11-18T15:50:12+01:00", "Bash", r#"{"command": "ls"}"#),
        ];
        let texts = vec!["Claude".to_string(), CUT.to_string()];
        let found = find(&requests, at("2026-11-18T15:50:15+01:00"), &texts).unwrap();
        assert!(found.text.ends_with("in the middle\""));
        assert_eq!(find(&requests, at("2026-11-18T15:55:01+01:00"), &texts), None, "older than 10 minutes");
        assert_eq!(find(&requests, at("2026-11-18T15:44:00+01:00"), &texts), None, "received after it");
    }

    #[test]
    fn needs_enough_text_to_match() {
        let requests = vec![sample("2026-11-18T15:50:12+01:00", "Bash", r#"{"command": "ls"}"#)];
        let texts = vec!["Claude, Allow Claude to run ls?".to_string()];
        assert_eq!(find(&requests, at("2026-11-18T15:50:15+01:00"), &texts), None);
    }
}
