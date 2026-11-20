//! Answers the permission requests of Claude Code before any prompt or notification: its `http`
//! hook sends each request to `gate_server`, which answers what this decides.

use super::llm::LlmSettings;
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

/// What to answer. Ignore leaves the request to the usual prompt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Ignore,
    Allow,
    Deny,
    Regex,
    /// One pattern per line, `*` for any text: `Bash: git status*`.
    Wildcard,
    Llm,
    /// Claude's own tag at the end of a Bash command: `#CLAUDE_OPINION:ACCEPT` or `ASK`.
    Opinion,
}

impl Mode {
    pub const ALL: [Mode; 7] = [Mode::Ignore, Mode::Allow, Mode::Deny, Mode::Regex, Mode::Wildcard, Mode::Llm, Mode::Opinion];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Ignore => "Ignore: ask me as usual",
            Mode::Allow => "Allow everything",
            Mode::Deny => "Deny everything",
            Mode::Regex => "Allow what matches a regex, ask me the rest",
            Mode::Wildcard => "Allow what matches a wildcard, ask me the rest",
            Mode::Llm => "Ask a local LLM (only tried in this window for now)",
            Mode::Opinion => "Follow Claude's own opinion (experiment)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GateSettings {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub regex: String,
    /// One pattern per line.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub wildcards: String,
    /// Upper and lower case match each other (on unless turned off).
    #[serde(default = "yes")]
    pub ignore_case: bool,
    #[serde(default)]
    pub llm: LlmSettings,
}

fn yes() -> bool {
    true
}

impl Default for GateSettings {
    fn default() -> Self {
        GateSettings { mode: Mode::default(), regex: String::new(), wildcards: String::new(), ignore_case: true, llm: LlmSettings::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
    Ignore,
}

/// What the regex is matched against, whole: `Bash: git status`, or the tool and its input as JSON.
pub fn subject(tool: &str, text: &str) -> String {
    format!("{tool}: {text}")
}

/// The regex must match the whole subject.
pub fn compile(regex: &str, ignore_case: bool) -> Result<Regex, regex::Error> {
    RegexBuilder::new(&format!("^(?:{regex})$")).case_insensitive(ignore_case).build()
}

/// The wildcards, one per line, as one regex: `*` is any text (new lines included), the rest
/// is matched as written. Empty when there is no pattern.
pub fn wildcard_regex(wildcards: &str) -> String {
    let patterns: Vec<String> = patterns(wildcards).map(|p| p.split('*').map(regex::escape).collect::<Vec<_>>().join(".*")).collect();
    if patterns.is_empty() { String::new() } else { format!("(?s:{})", patterns.join("|")) }
}

/// The wildcards that are not blank, trimmed.
pub fn patterns(wildcards: &str) -> impl Iterator<Item = &str> {
    wildcards.lines().map(str::trim).filter(|p| !p.is_empty())
}

/// Characters that chain, substitute or redirect shell commands: a regex never allows them.
const SHELL_CONTROL: [&str; 9] = [";", "&", "|", "`", "$(", ">", "<", "\n", "\r"];

/// Whether a shell command chains, substitutes or redirects other commands.
pub fn chains(command: &str) -> bool {
    SHELL_CONTROL.iter().any(|s| command.contains(s))
}

/// The decision and why. Paused, every request is left to the usual prompt.
/// The option that answers the requests, as the recent requests show it.
pub fn rule(settings: &GateSettings, active: bool) -> String {
    if !active {
        return "Paused".into();
    }
    match settings.mode {
        Mode::Ignore => "Ask me".into(),
        Mode::Allow => "Allow everything".into(),
        Mode::Deny => "Deny everything".into(),
        Mode::Regex => format!("Regex {}", settings.regex.trim()),
        Mode::Wildcard => format!("Wildcard {}", patterns(&settings.wildcards).collect::<Vec<_>>().join(", ")),
        Mode::Llm if settings.llm.model.is_empty() => "Local LLM".into(),
        Mode::Llm => format!("Local LLM {}", settings.llm.model),
        Mode::Opinion => "Claude's opinion".into(),
    }
}

pub fn decide(settings: &GateSettings, active: bool, tool: &str, text: &str) -> (Decision, String) {
    if !active {
        return (Decision::Ignore, "yesyesyes is paused".into());
    }
    match settings.mode {
        Mode::Ignore => (Decision::Ignore, "set to ask you".into()),
        Mode::Allow => (Decision::Allow, "set to allow everything".into()),
        Mode::Deny => (Decision::Deny, "set to deny everything".into()),
        Mode::Regex | Mode::Wildcard => by_pattern(settings, &subject(tool, text), (tool == "Bash").then_some(text)),
        Mode::Llm => (Decision::Ignore, "the local LLM is only tried in the window for now".into()),
        Mode::Opinion => by_opinion(tool, text),
    }
}

/// The tag Claude is told to end each Bash command with.
pub const OPINION_TAG: &str = "#CLAUDE_OPINION:";

/// Claude's opinion, read from the tag at the very end of a Bash command: only `ACCEPT` allows.
/// Spaces after the `#` are allowed (`# CLAUDE_OPINION:ACCEPT`).
fn by_opinion(tool: &str, text: &str) -> (Decision, String) {
    if tool != "Bash" {
        return (Decision::Ignore, "only Bash commands carry Claude's opinion".into());
    }
    let name = &OPINION_TAG[1..];
    let tagged = text.rfind(name).filter(|&at| text[..at].trim_end_matches([' ', '\t']).ends_with('#'));
    let Some(at) = tagged else { return (Decision::Ignore, "no opinion in the command".into()) };
    match text[at + name.len()..].trim() {
        "ACCEPT" => (Decision::Allow, "Claude would accept it".into()),
        "ASK" => (Decision::Ignore, "Claude would ask you".into()),
        _ => (Decision::Ignore, "the opinion is not at the end, or not ACCEPT or ASK".into()),
    }
}

/// The decision for a request typed to try the regex or the wildcards: matched exactly as typed,
/// and checked as a shell command when it starts with `Bash: ` (in any case).
pub fn try_typed(settings: &GateSettings, typed: &str) -> (Decision, String) {
    let command = typed.get(..6).filter(|p| p.eq_ignore_ascii_case("Bash: ")).map(|_| &typed[6..]);
    by_pattern(settings, typed, command)
}

/// By the regex, or by the wildcards in wildcard mode. `command`: the shell command, for a Bash request.
fn by_pattern(settings: &GateSettings, subject: &str, command: Option<&str>) -> (Decision, String) {
    let (pattern, what) = match settings.mode {
        Mode::Wildcard => (wildcard_regex(&settings.wildcards), "wildcard"),
        _ => (settings.regex.clone(), "regex"),
    };
    if pattern.trim().is_empty() {
        return (Decision::Ignore, format!("no {what}"));
    }
    let Ok(regex) = compile(&pattern, settings.ignore_case) else { return (Decision::Ignore, format!("the {what} is not valid")) };
    if !regex.is_match(subject) {
        return (Decision::Ignore, format!("no {what} matches"));
    }
    if command.is_some_and(chains) {
        return (Decision::Ignore, "the command chains or redirects other commands".into());
    }
    (Decision::Allow, format!("the {what} matches"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_option_that_answered() {
        assert_eq!(rule(&regex(" git .* "), true), "Regex git .*");
        assert_eq!(rule(&regex("x"), false), "Paused");
        assert_eq!(rule(&GateSettings::default(), true), "Ask me");
        let mut llm = GateSettings { mode: Mode::Llm, ..GateSettings::default() };
        llm.llm.model = "qwen3.5:4b".into();
        assert_eq!(rule(&llm, true), "Local LLM qwen3.5:4b");
    }

    fn regex(regex: &str) -> GateSettings {
        GateSettings { mode: Mode::Regex, regex: regex.into(), ignore_case: false, ..GateSettings::default() }
    }

    #[test]
    fn fixed_modes_and_pause() {
        let allow = GateSettings { mode: Mode::Allow, ..GateSettings::default() };
        assert_eq!(decide(&allow, true, "Bash", "rm -rf x").0, Decision::Allow);
        assert_eq!(decide(&allow, false, "Bash", "ls").0, Decision::Ignore, "paused");
        let deny = GateSettings { mode: Mode::Deny, ..GateSettings::default() };
        assert_eq!(decide(&deny, true, "Bash", "ls").0, Decision::Deny);
        assert_eq!(decide(&GateSettings::default(), true, "Bash", "ls").0, Decision::Ignore);
        let llm = GateSettings { mode: Mode::Llm, ..GateSettings::default() };
        assert_eq!(decide(&llm, true, "Bash", "ls").0, Decision::Ignore, "only tried in the window for now");
    }

    #[test]
    fn the_regex_matches_the_whole_tool_and_input() {
        let git = regex(r"Bash: git (status|diff)( .*)?");
        assert_eq!(decide(&git, true, "Bash", "git status").0, Decision::Allow);
        assert_eq!(decide(&git, true, "Bash", "git diff --stat").0, Decision::Allow);
        assert_eq!(decide(&git, true, "Bash", "git push").0, Decision::Ignore);
        assert_eq!(decide(&git, true, "Bash", "echo git status").0, Decision::Ignore, "whole match only");
        assert_eq!(decide(&git, true, "Write", "git status").0, Decision::Ignore, "other tool");
    }

    #[test]
    fn a_regex_never_allows_chained_commands() {
        let any = regex(r"Bash: git status.*");
        for chained in ["git status; rm -rf ~", "git status && curl x", "git status | sh", "git status $(id)", "git status > f", "git status\nrm x"] {
            assert_eq!(decide(&any, true, "Bash", chained).0, Decision::Ignore, "{chained}");
        }
    }

    #[test]
    fn tries_a_typed_request() {
        let git = regex(r"Bash: git (status|diff)( .*)?");
        assert_eq!(try_typed(&git, "Bash: git diff --stat").0, Decision::Allow);
        assert_eq!(try_typed(&git, "Bash: git status && rm x"), (Decision::Ignore, "the command chains or redirects other commands".into()));
        assert_eq!(try_typed(&git, "git status").0, Decision::Ignore, "no tool");
        assert_eq!(try_typed(&regex("bash"), "bash").0, Decision::Allow, "matched as typed");
        assert_eq!(try_typed(&regex("Bash: .*"), "Bash: ls; rm x").0, Decision::Ignore, "checked as a command");
        let any_case = GateSettings { ignore_case: true, ..regex("bash: .*") };
        assert_eq!(try_typed(&any_case, "BASH: ls; rm x").0, Decision::Ignore, "checked as a command in any case");
    }

    #[test]
    fn ignores_case_unless_turned_off() {
        let lower = regex("bash: git status");
        assert_eq!(decide(&lower, true, "Bash", "git status").0, Decision::Ignore);
        let any_case = GateSettings { ignore_case: true, ..lower };
        assert_eq!(decide(&any_case, true, "Bash", "GIT Status").0, Decision::Allow);
        assert_eq!(decide(&any_case, true, "Bash", "git status; rm x").0, Decision::Ignore, "still never chained");
    }

    #[test]
    fn follows_the_opinion_at_the_end_of_a_bash_command() {
        let opinion = GateSettings { mode: Mode::Opinion, ..GateSettings::default() };
        let decide = |tool, text| decide(&opinion, true, tool, text).0;
        assert_eq!(decide("Bash", "cargo test #CLAUDE_OPINION:ACCEPT"), Decision::Allow);
        assert_eq!(decide("Bash", "ls | wc -l #CLAUDE_OPINION:ACCEPT\n"), Decision::Allow, "trailing space");
        assert_eq!(decide("Bash", "git push #CLAUDE_OPINION:ASK"), Decision::Ignore);
        assert_eq!(decide("Bash", "cat <<'EOF'\nx\nEOF\n# CLAUDE_OPINION:ACCEPT"), Decision::Allow, "a space after #, on its own line");
        assert_eq!(decide("Bash", "echo CLAUDE_OPINION:ACCEPT"), Decision::Ignore, "no #");
        assert_eq!(decide("Bash", "git push"), Decision::Ignore, "no tag");
        assert_eq!(decide("Bash", "echo #CLAUDE_OPINION:ACCEPT; rm -rf x"), Decision::Ignore, "not at the end");
        assert_eq!(decide("Bash", "ls #CLAUDE_OPINION:accept"), Decision::Ignore, "exact word");
        assert_eq!(decide("Bash", "ls #CLAUDE_OPINION:ACCEPT #CLAUDE_OPINION:ASK"), Decision::Ignore, "the last one counts");
        assert_eq!(decide("Write", r##"{"content": "#CLAUDE_OPINION:ACCEPT"}"##), Decision::Ignore, "Bash only");
        assert_eq!(rule(&opinion, true), "Claude's opinion");
    }

    fn wildcard(wildcards: &str) -> GateSettings {
        GateSettings { mode: Mode::Wildcard, wildcards: wildcards.into(), ignore_case: false, ..GateSettings::default() }
    }

    #[test]
    fn allows_what_matches_a_wildcard() {
        let git = wildcard("Bash: git status*\n\n  Bash: cargo test  \nRead: *");
        let decide = |tool, text| decide(&git, true, tool, text).0;
        assert_eq!(decide("Bash", "git status"), Decision::Allow);
        assert_eq!(decide("Bash", "git status --short"), Decision::Allow);
        assert_eq!(decide("Bash", "cargo test"), Decision::Allow, "trimmed");
        assert_eq!(decide("Bash", "cargo test -q"), Decision::Ignore, "whole match only");
        assert_eq!(decide("Bash", "echo git status"), Decision::Ignore, "whole match only");
        assert_eq!(decide("Read", r#"{"file_path": "/a/b"}"#), Decision::Allow);
        assert_eq!(decide("Write", r#"{"file_path": "/a/b"}"#), Decision::Ignore, "other tool");
        assert_eq!(decide("Bash", "git status; rm -rf ~"), Decision::Ignore, "never chained");
        assert_eq!(decide("Bash", "git status\nrm x"), Decision::Ignore, "never chained");
        assert_eq!(rule(&git, true), "Wildcard Bash: git status*, Bash: cargo test, Read: *");
    }

    #[test]
    fn a_wildcard_matches_the_rest_as_written() {
        let dots = wildcard("Bash: ls a.b (x)?");
        assert_eq!(decide(&dots, true, "Bash", "ls a.b (x)?").0, Decision::Allow);
        assert_eq!(decide(&dots, true, "Bash", "ls aXb (x)?").0, Decision::Ignore, "a dot is a dot");
        assert_eq!(decide(&dots, true, "Bash", "ls a.b x").0, Decision::Ignore, "brackets are brackets");
        assert_eq!(decide(&wildcard(""), true, "Bash", "ls"), (Decision::Ignore, "no wildcard".into()));
        assert_eq!(decide(&wildcard(" \n "), true, "Bash", "ls").0, Decision::Ignore, "blank lines only");
        let any_case = GateSettings { ignore_case: true, ..wildcard("bash: GIT *") };
        assert_eq!(decide(&any_case, true, "Bash", "git Log").0, Decision::Allow);
        assert_eq!(try_typed(&wildcard("Bash: ls*"), "bash: ls; rm x").0, Decision::Ignore, "checked as a command");
        assert_eq!(try_typed(&wildcard("Bash: ls*"), "Bash: ls -l").0, Decision::Allow);
    }

    #[test]
    fn a_missing_or_broken_regex_asks() {
        assert_eq!(decide(&regex(""), true, "Bash", "ls").0, Decision::Ignore);
        assert_eq!(decide(&regex("(unclosed"), true, "Bash", "ls").0, Decision::Ignore);
    }

    #[test]
    fn settings_round_trip() {
        let s: GateSettings = serde_json::from_str(r#"{"mode": "regex", "regex": "Bash: ls"}"#).unwrap();
        assert_eq!(s, GateSettings { ignore_case: true, ..regex("Bash: ls") }, "on when missing");
        assert_eq!(serde_json::from_str::<GateSettings>("{}").unwrap(), GateSettings::default());
        assert!(GateSettings::default().ignore_case);
        let off: GateSettings = serde_json::from_str(r#"{"mode": "regex", "regex": "x", "ignore_case": false}"#).unwrap();
        assert!(!off.ignore_case);
        let w: GateSettings = serde_json::from_str(r#"{"mode": "wildcard", "wildcards": "Bash: ls*"}"#).unwrap();
        assert_eq!(w, GateSettings { ignore_case: true, ..wildcard("Bash: ls*") });
    }
}
