//! Answers the permission requests of Claude Code before any prompt or notification: its `http`
//! hook sends each request to `gate_server`, which answers what this decides.

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
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Ignore, Mode::Allow, Mode::Deny, Mode::Regex];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Ignore => "Ignore: ask me as usual",
            Mode::Allow => "Allow everything",
            Mode::Deny => "Deny everything",
            Mode::Regex => "Allow what matches a regex, ask me the rest",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateSettings {
    #[serde(default)]
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub regex: String,
    /// Upper and lower case match each other (on unless turned off).
    #[serde(default = "yes")]
    pub ignore_case: bool,
}

fn yes() -> bool {
    true
}

impl Default for GateSettings {
    fn default() -> Self {
        GateSettings { mode: Mode::default(), regex: String::new(), ignore_case: true }
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

/// Characters that chain, substitute or redirect shell commands: a regex never allows them.
const SHELL_CONTROL: [&str; 9] = [";", "&", "|", "`", "$(", ">", "<", "\n", "\r"];

/// The decision and why. Paused, every request is left to the usual prompt.
pub fn decide(settings: &GateSettings, active: bool, tool: &str, text: &str) -> (Decision, String) {
    if !active {
        return (Decision::Ignore, "yesyesyes is paused".into());
    }
    match settings.mode {
        Mode::Ignore => (Decision::Ignore, "set to ask you".into()),
        Mode::Allow => (Decision::Allow, "set to allow everything".into()),
        Mode::Deny => (Decision::Deny, "set to deny everything".into()),
        Mode::Regex => by_regex(settings, tool, text),
    }
}

/// The decision for a request typed to try a regex: matched exactly as typed, and checked as a
/// shell command when it starts with `Bash: ` (in any case).
pub fn try_regex(settings: &GateSettings, typed: &str) -> (Decision, String) {
    let command = typed.get(..6).filter(|p| p.eq_ignore_ascii_case("Bash: ")).map(|_| &typed[6..]);
    by_subject(settings, typed, command)
}

fn by_regex(settings: &GateSettings, tool: &str, text: &str) -> (Decision, String) {
    by_subject(settings, &subject(tool, text), (tool == "Bash").then_some(text))
}

/// `command`: the shell command, for a Bash request.
fn by_subject(settings: &GateSettings, subject: &str, command: Option<&str>) -> (Decision, String) {
    if settings.regex.trim().is_empty() {
        return (Decision::Ignore, "no regex".into());
    }
    let Ok(regex) = compile(&settings.regex, settings.ignore_case) else { return (Decision::Ignore, "the regex is not valid".into()) };
    if !regex.is_match(subject) {
        return (Decision::Ignore, "the regex does not match".into());
    }
    if command.is_some_and(|c| SHELL_CONTROL.iter().any(|s| c.contains(s))) {
        return (Decision::Ignore, "the command chains or redirects other commands".into());
    }
    (Decision::Allow, "the regex matches".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regex(regex: &str) -> GateSettings {
        GateSettings { mode: Mode::Regex, regex: regex.into(), ignore_case: false }
    }

    #[test]
    fn fixed_modes_and_pause() {
        let allow = GateSettings { mode: Mode::Allow, ..GateSettings::default() };
        assert_eq!(decide(&allow, true, "Bash", "rm -rf x").0, Decision::Allow);
        assert_eq!(decide(&allow, false, "Bash", "ls").0, Decision::Ignore, "paused");
        let deny = GateSettings { mode: Mode::Deny, ..GateSettings::default() };
        assert_eq!(decide(&deny, true, "Bash", "ls").0, Decision::Deny);
        assert_eq!(decide(&GateSettings::default(), true, "Bash", "ls").0, Decision::Ignore);
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
        assert_eq!(try_regex(&git, "Bash: git diff --stat").0, Decision::Allow);
        assert_eq!(try_regex(&git, "Bash: git status && rm x"), (Decision::Ignore, "the command chains or redirects other commands".into()));
        assert_eq!(try_regex(&git, "git status").0, Decision::Ignore, "no tool");
        assert_eq!(try_regex(&regex("bash"), "bash").0, Decision::Allow, "matched as typed");
        assert_eq!(try_regex(&regex("Bash: .*"), "Bash: ls; rm x").0, Decision::Ignore, "checked as a command");
        let any_case = GateSettings { ignore_case: true, ..regex("bash: .*") };
        assert_eq!(try_regex(&any_case, "BASH: ls; rm x").0, Decision::Ignore, "checked as a command in any case");
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
    }
}
