use crate::backend::Access;
use crate::core::{ClickEvent, Config, ConfigStore, clicklog};
#[cfg(test)]
use crate::core::ClickResult;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

pub const LOG_SIZE: usize = 50;

/// Shared by the window, the tray menu and the engine thread.
pub struct Shared {
    pub config: Config,
    pub events: VecDeque<ClickEvent>,
    pub access: Access,
    pub show_window: bool,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct State {
    inner: Arc<Mutex<Shared>>,
    pub store: ConfigStore,
}

impl State {
    pub fn new(store: ConfigStore) -> Self {
        let shared = Shared {
            config: store.load(),
            events: VecDeque::new(),
            access: Access::Unknown,
            show_window: false,
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

    /// Keeps the events in memory and, when the user turned it on, appends them to the log file.
    pub fn record(&self, events: Vec<ClickEvent>) {
        let mut shared = self.lock();
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
        ClickEvent { id: 1, at: Local::now(), app: "Claude".into(), action: "Allow once".into(), result: ClickResult::Pressed, texts: vec!["Run ls".into()] }
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
}
