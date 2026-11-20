use crate::backend::Access;
use crate::core::llm::LlmState;
use crate::core::requests::{self, Request};
use crate::core::{ClickEvent, Config, ConfigStore, clicklog};
#[cfg(test)]
use crate::core::ClickResult;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

/// Recent clicks kept: as many as the log file keeps.
pub const LOG_SIZE: usize = clicklog::MAX_LINES;

/// Shared by the window, the tray menu, the engine thread and the Claude Code server.
pub struct Shared {
    pub config: Config,
    pub events: VecDeque<ClickEvent>,
    /// The last Claude Code requests, oldest first, in memory only.
    pub requests: Vec<Request>,
    pub access: Access,
    pub show_window: bool,
    /// The local LLM, kept loaded by `llm_bridge`.
    pub llm: LlmState,
    /// Asked by the window: unload the other models so ours can load.
    pub llm_unload_others: bool,
    /// Bytes of memory the loaded model takes, when known.
    pub llm_memory: Option<u64>,
    /// When yesyesyes turns itself off (`autooff`).
    pub off_at: Option<chrono::DateTime<chrono::Local>>,
    /// When the last request or notification was recorded: `autooff` counts inactivity from it.
    pub last_activity: Option<std::time::Instant>,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct State {
    inner: Arc<Mutex<Shared>>,
    pub store: ConfigStore,
}

impl State {
    pub fn new(store: ConfigStore) -> Self {
        let config = store.load();
        // With the log file on, the recent clicks survive a restart.
        let events = if config.log.save { clicklog::read(&clicklog::resolve(config.log.path.as_deref())).into() } else { VecDeque::new() };
        let shared = Shared {
            config,
            events,
            requests: Vec::new(),
            access: Access::Unknown,
            show_window: false,
            llm: LlmState::Off,
            llm_unload_others: false,
            llm_memory: None,
            off_at: None,
            last_activity: None,
            error: None,
        };
        State { inner: Arc::new(Mutex::new(shared)), store }
    }

    pub fn lock(&self) -> MutexGuard<'_, Shared> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Changes the config and saves it; a failed save is shown in the window.
    pub fn update(&self, change: impl FnOnce(&mut Config)) {
        let mut shared = self.lock();
        change(&mut shared.config);
        shared.error = self.store.save(&shared.config).err().map(|e| format!("Could not save the rules: {e}"));
    }

    /// Keeps a Claude Code request, to show it whole with its notification.
    pub fn remember(&self, request: Request) {
        let requests = &mut self.lock().requests;
        requests.push(request);
        let extra = requests.len().saturating_sub(LOG_SIZE);
        requests.drain(..extra);
    }

    /// Keeps the events in memory and, when the user turned it on, appends them to the log file.
    /// Each one gets the whole Claude Code request it shows cut, when there is one.
    pub fn record(&self, mut events: Vec<ClickEvent>) {
        let mut shared = self.lock();
        if !events.is_empty() {
            shared.last_activity = Some(std::time::Instant::now());
        }
        for event in events.iter_mut().filter(|e| e.request.is_none()) {
            event.request = requests::find(&shared.requests, event.at, &event.texts).map(|r| r.text.clone());
        }
        if shared.config.log.save {
            let path = clicklog::resolve(shared.config.log.path.as_deref());
            if let Err(e) = clicklog::append(&path, &events) {
                shared.error = Some(format!("Could not write the log to {}: {e}", path.display()));
            }
        }
        for event in events {
            shared.events.push_front(event);
        }
        shared.events.truncate(LOG_SIZE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    fn event() -> ClickEvent {
        ClickEvent { id: 1, at: Local::now(), app: "Claude".into(), action: "Allow once".into(), result: ClickResult::Pressed, texts: vec!["Run ls".into()], request: None, rule: None, chat: None }
    }

    #[test]
    fn writes_the_log_file_only_when_turned_on() {
        let dir = std::env::temp_dir().join(format!("yyy-state-{}", std::process::id()));
        let log = dir.join("clicks.jsonl");
        let state = State::new(ConfigStore { path: dir.join("config.json") });
        state.record(vec![event()]);
        assert!(!log.exists());
        state.update(|c| {
            c.log.save = true;
            c.log.path = Some(log.display().to_string());
        });
        state.record(vec![event()]);
        assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 1);
        assert_eq!(state.lock().events.len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn attaches_the_whole_request() {
        let state = State::new(ConfigStore { path: std::env::temp_dir().join("yyy-state-req-unused.json") });
        let at = Local::now().to_rfc3339();
        state.remember(requests::sample(&at, "Bash", r#"{"command": "cargo test --workspace --all-features -- --nocapture"}"#));
        let mut cut = event();
        cut.texts = vec!["Claude, Allow Claude to run cargo test --workspace --all-feat…?".into()];
        state.record(vec![cut, event()]);
        let events = &state.lock().events;
        assert_eq!(events[1].request.as_deref(), Some("cargo test --workspace --all-features -- --nocapture"));
        assert_eq!(events[0].request, None);
    }

    #[test]
    fn remembers_only_the_last_requests() {
        let state = State::new(ConfigStore { path: std::env::temp_dir().join("yyy-state-many-unused.json") });
        for n in 0..LOG_SIZE + 5 {
            state.remember(requests::sample("2026-11-18T15:50:12+01:00", "Bash", &format!(r#"{{"command": "echo {n}"}}"#)));
        }
        let requests = &state.lock().requests;
        assert_eq!(requests.len(), LOG_SIZE);
        assert_eq!(requests[0].text, "echo 5");
    }
}
