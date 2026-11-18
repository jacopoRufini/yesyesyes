use super::{Config, Hit, Node, Outcome, find_all, press};
use chrono::{DateTime, Local};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// What was clicked, with the text of the notification (kept in memory only).
#[derive(Clone, Debug, PartialEq)]
pub struct ClickEvent {
    pub id: u64,
    pub at: DateTime<Local>,
    pub app: String,
    pub action: String,
    pub result: ClickResult,
    pub texts: Vec<String>,
    /// The whole request the notification shows cut, when a hook logged it (see `requests`).
    pub request: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickResult {
    Pressed,
    Failed,
    /// Seen while paused: logged, not pressed.
    Ignored,
    /// A Claude Code request answered by the gate.
    Allowed,
    Denied,
}

impl ClickResult {
    pub fn label(self) -> &'static str {
        match self {
            ClickResult::Pressed => "pressed",
            ClickResult::Failed => "failed",
            ClickResult::Ignored => "ignored",
            ClickResult::Allowed => "allowed",
            ClickResult::Denied => "denied",
        }
    }
}

const SNIPPET: usize = 60;

impl ClickEvent {
    /// The longest line of the notification (usually the command), on one line, cut to fit.
    /// The whole request instead, when it is known.
    pub fn snippet(&self) -> String {
        let longest = self.texts.iter().max_by_key(|t| t.chars().count()).map(String::as_str).unwrap_or("");
        let line = self.request.as_deref().unwrap_or(longest).split_whitespace().collect::<Vec<_>>().join(" ");
        if line.chars().count() <= SNIPPET {
            return line;
        }
        let cut: String = line.chars().take(SNIPPET - 3).collect();
        format!("{}...", cut.trim_end())
    }

    pub fn full_text(&self) -> String {
        self.texts.join("\n")
    }

    /// What Claude asked, whole, when known; else the text of the notification.
    pub fn shown_text(&self) -> String {
        self.request.clone().unwrap_or_else(|| self.full_text())
    }
}

pub struct Engine {
    cooldown: Duration,
    last: HashMap<String, Instant>,
    next_id: u64,
    /// Notifications logged as ignored that are still on screen: logged once each.
    ignored: HashSet<String>,
}

impl Engine {
    pub fn new(cooldown: Duration) -> Self {
        Engine { cooldown, last: HashMap::new(), next_id: 0, ignored: HashSet::new() }
    }

    /// One look at the notifications. When paused nothing is pressed: a match is logged as
    /// ignored, once while it stays on screen.
    pub fn tick<N: Node>(&mut self, config: &Config, roots: &[N], now: Instant) -> Vec<ClickEvent> {
        let mut events = Vec::new();
        let paused = !config.enabled;
        let mut on_screen = HashSet::new();
        for rule in config.rules.iter().filter(|r| r.is_valid()) {
            let id = rule.id();
            if !paused && self.last.get(&id).is_some_and(|t| now.duration_since(*t) < self.cooldown) {
                continue;
            }
            let hits: Vec<Hit> = if paused {
                roots.iter().flat_map(|root| find_all(root, rule)).collect()
            } else {
                roots.iter().map(|root| press(root, rule)).find(|h| h.outcome != Outcome::NotFound).into_iter().collect()
            };
            for hit in hits {
                let result = match hit.outcome {
                    Outcome::Pressed => ClickResult::Pressed,
                    Outcome::Failed => ClickResult::Failed,
                    Outcome::Found | Outcome::NotFound => ClickResult::Ignored,
                };
                if paused {
                    let key = format!("{id}|{}", hit.texts.join("\n"));
                    let new = !self.ignored.contains(&key);
                    on_screen.insert(key);
                    if !new {
                        continue;
                    }
                } else {
                    self.last.insert(id.clone(), now);
                }
                self.next_id += 1;
                events.push(ClickEvent {
                    id: self.next_id,
                    at: Local::now(),
                    app: rule.app.clone(),
                    action: rule.action.clone(),
                    result,
                    texts: hit.texts,
                    request: None,
                });
            }
        }
        self.ignored = on_screen;
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Rule;
    use crate::core::matcher::fake::FakeNode;

    fn config(enabled: bool) -> Config {
        Config { enabled, rules: vec![Rule::new("Claude", "Allow once")], ..Config::default() }
    }

    #[test]
    fn paused_logs_ignored_once_while_on_screen() {
        let banner = FakeNode::banner("Claude", &["Allow once"]);
        let mut engine = Engine::new(Duration::from_secs(3));
        let start = Instant::now();
        let events = engine.tick(&config(false), std::slice::from_ref(&banner), start);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].result, ClickResult::Ignored);
        assert_eq!(events[0].texts, vec!["Claude, run it?"]);
        assert!(engine.tick(&config(false), std::slice::from_ref(&banner), start + Duration::from_secs(10)).is_empty());
        assert!(banner.performed().is_empty());
        let mut other = FakeNode::banner("Claude", &["Allow once"]);
        other.labels = vec!["Claude, run that?".into()];
        assert_eq!(engine.tick(&config(false), &[banner.clone(), other], start).len(), 1, "a new notification");
        assert!(engine.tick(&config(false), &[] as &[FakeNode], start).is_empty());
        assert_eq!(engine.tick(&config(false), std::slice::from_ref(&banner), start).len(), 1, "shown again");
    }

    #[test]
    fn cooldown_and_events() {
        let banner = FakeNode::banner("Claude", &["Allow once"]);
        let mut engine = Engine::new(Duration::from_secs(3));
        let start = Instant::now();
        assert_eq!(engine.tick(&config(true), std::slice::from_ref(&banner), start).len(), 1);
        assert!(engine.tick(&config(true), std::slice::from_ref(&banner), start + Duration::from_secs(1)).is_empty());
        let events = engine.tick(&config(true), std::slice::from_ref(&banner), start + Duration::from_secs(4));
        assert_eq!(banner.performed().len(), 2);
        assert_eq!((events[0].app.as_str(), events[0].action.as_str(), events[0].result), ("Claude", "Allow once", ClickResult::Pressed));
        assert_eq!(events[0].texts, vec!["Claude, run it?"]);
        assert_eq!(events[0].id, 2);
    }

    fn event(texts: &[&str]) -> ClickEvent {
        ClickEvent {
            id: 1,
            at: Local::now(),
            app: "Claude".into(),
            action: "Allow once".into(),
            result: ClickResult::Pressed,
            texts: texts.iter().map(|t| t.to_string()).collect(),
            request: None,
        }
    }

    #[test]
    fn snippet_is_the_longest_line_cut_to_fit() {
        assert_eq!(event(&["Claude", "echo  hi\nthere"]).snippet(), "echo hi there");
        let long = format!("Run {}", "x".repeat(100));
        let snippet = event(&["Claude", &long]).snippet();
        assert_eq!(snippet.chars().count(), SNIPPET);
        assert!(snippet.starts_with("Run xxx") && snippet.ends_with("..."));
        assert_eq!(event(&[]).snippet(), "");
        assert_eq!(event(&["a", "b c"]).full_text(), "a\nb c");
    }

    #[test]
    fn looks_in_every_root() {
        let other = FakeNode::banner("Slack", &["Reply"]);
        let banner = FakeNode::banner("Claude", &["Allow once"]);
        let events = Engine::new(Duration::from_secs(3)).tick(&config(true), &[other, banner.clone()], Instant::now());
        assert_eq!(events.len(), 1);
        assert_eq!(banner.performed(), vec!["Allow once"]);
    }

    #[test]
    fn failed_press_is_logged_once_per_cooldown() {
        let mut banner = FakeNode::banner("Claude", &["Allow once"]);
        banner.fails = true;
        let mut engine = Engine::new(Duration::from_secs(3));
        let start = Instant::now();
        let events = engine.tick(&config(true), std::slice::from_ref(&banner), start);
        assert_eq!(events[0].result, ClickResult::Failed);
        assert!(engine.tick(&config(true), &[banner], start + Duration::from_secs(1)).is_empty());
    }
}
