use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// "When a notification mentions `app`, press its `action` button." Both are always required.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub app: String,
    pub action: String,
}

impl Rule {
    pub fn new(app: &str, action: &str) -> Self {
        Rule { app: app.trim().to_string(), action: action.trim().to_string() }
    }

    pub fn id(&self) -> String {
        format!("{}|{}", self.app.to_lowercase(), self.action.to_lowercase())
    }

    pub fn is_valid(&self) -> bool {
        !self.app.is_empty() && !self.action.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub log: LogSettings,
}

/// Saving the clicks to a file: off by default. No path means the default one of the OS.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogSettings {
    #[serde(default)]
    pub save: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

fn yes() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config { enabled: true, rules: Vec::new(), log: LogSettings::default() }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum AddResult {
    Added,
    Invalid,
    Duplicate,
}

impl Config {
    pub fn add(&mut self, rule: Rule) -> AddResult {
        if !rule.is_valid() {
            return AddResult::Invalid;
        }
        if self.rules.iter().any(|r| r.id() == rule.id()) {
            return AddResult::Duplicate;
        }
        self.rules.push(rule);
        AddResult::Added
    }
}

#[derive(Clone, Debug)]
pub struct ConfigStore {
    pub path: PathBuf,
}

impl ConfigStore {
    /// `~/Library/Application Support/yesyesyes` on macOS, `%APPDATA%\yesyesyes` on Windows,
    /// `~/.config/yesyesyes` on Linux.
    pub fn default_location() -> Self {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        ConfigStore { path: base.join("yesyesyes").join("config.json") }
    }

    /// A missing or unreadable file gives the default (active, no rules).
    pub fn load(&self) -> Config {
        std::fs::read(&self.path)
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, config: &Config) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(config)?)?;
        std::fs::rename(tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_rejects_invalid_and_duplicate() {
        let mut c = Config::default();
        assert_eq!(c.add(Rule::new(" ", "x")), AddResult::Invalid);
        assert_eq!(c.add(Rule::new("Claude", "Allow once")), AddResult::Added);
        assert_eq!(c.add(Rule::new("claude", " allow once ")), AddResult::Duplicate);
        assert_eq!(c.rules.len(), 1);
    }

    #[test]
    fn store_round_trip_and_missing_file() {
        let dir = std::env::temp_dir().join(format!("yyy-{}-{:?}", std::process::id(), std::thread::current().id()));
        let store = ConfigStore { path: dir.join("config.json") };
        assert_eq!(store.load(), Config::default());
        let log = LogSettings { save: true, path: Some("~/clicks.jsonl".into()) };
        let c = Config { enabled: false, rules: vec![Rule::new("Claude", "Allow once")], log };
        store.save(&c).unwrap();
        assert_eq!(store.load(), c);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_the_swift_app_format() {
        let c: Config = serde_json::from_str(r#"{"rules":[{"action":"Allow once","app":"Claude"}]}"#).unwrap();
        assert!(c.enabled);
        assert!(!c.log.save, "the log file is off by default");
        assert_eq!(c.rules, vec![Rule::new("Claude", "Allow once")]);
    }
}
