//! The calls to the local LLM server (Ollama or llama-server) over HTTP, and the thread that keeps
//! the chosen model loaded while the mode "Ask a local LLM" is on: loaded for the keep time,
//! refreshed every quarter of it, unloaded when the mode changes or yesyesyes quits. It never
//! loads a model while the server holds another one, unless the window asks to unload them.

use crate::core::gate::{Decision, Mode};
use crate::core::llm::{self, Backend, LlmSettings, LlmState};
use crate::core::{Channel, Config};
use crate::state::State;
use chrono::Local;
use eframe::egui;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_secs(1);
/// After a failure or while other models are in memory, the next try.
const RETRY: Duration = Duration::from_secs(10);
/// Loading a model from disk can take a while.
const LOAD: Duration = Duration::from_secs(180);
/// How often the memory of the loaded model is read again.
const MEASURE: Duration = Duration::from_secs(5);
const QUICK: Duration = Duration::from_secs(3);

pub struct Answer {
    pub decision: Decision,
    pub reason: String,
    pub took: Duration,
}

/// The JSON answer of a GET (no body) or POST. Never through a proxy: the server is on this computer.
fn call(url: &str, body: Option<&Value>, timeout: Duration) -> Result<Value, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(timeout)).http_status_as_error(false).proxy(None).build().into();
    let response = match body {
        Some(body) => agent.post(url).send_json(body),
        None => agent.get(url).call(),
    };
    let mut response = response.map_err(|e| match e {
        ureq::Error::Timeout(_) => "no answer in time.".to_string(),
        e => format!("cannot reach the server ({e}): is it running?"),
    })?;
    let status = response.status().as_u16();
    let text = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
    if !(200..300).contains(&status) {
        return Err(format!("the server answered {status}: {}", text.chars().take(200).collect::<String>()));
    }
    serde_json::from_str(&text).map_err(|_| "the server did not answer JSON.".to_string())
}

fn get(settings: &LlmSettings, path: &str, timeout: Duration) -> Result<Value, String> {
    call(&(settings.url()? + path), None, timeout)
}

fn post(settings: &LlmSettings, path: &str, body: &Value, timeout: Duration) -> Result<Value, String> {
    call(&(settings.url()? + path), Some(body), timeout)
}

/// Ollama calls `qwen3.5` what it shows as `qwen3.5:latest`.
fn same_name(a: &str, b: &str) -> bool {
    let full = |n: &str| if n.contains(':') { n.to_ascii_lowercase() } else { format!("{}:latest", n.to_ascii_lowercase()) };
    full(a) == full(b)
}

/// The models the server can run.
pub fn models(settings: &LlmSettings) -> Result<Vec<String>, String> {
    let path = if settings.backend == Backend::Ollama { "/api/tags" } else { "/v1/models" };
    Ok(llm::read_models(settings.backend, &get(settings, path, QUICK)?))
}

/// The models in memory; None for a llama-server started with one model, which keeps it itself.
pub fn loaded(settings: &LlmSettings) -> Result<Option<Vec<String>>, String> {
    let path = if settings.backend == Backend::Ollama { "/api/ps" } else { "/models" };
    Ok(llm::read_loaded(settings.backend, &get(settings, path, QUICK)?))
}

/// Bytes of memory the model takes: Ollama's own count, else the llama-server processes.
pub fn memory(settings: &LlmSettings) -> Option<u64> {
    match settings.backend {
        Backend::Ollama => ollama_size(&get(settings, "/api/ps", QUICK).ok()?, &settings.model),
        Backend::LlamaCpp => crate::memory::of_server(crate::llm_server::host_port(&settings.url().ok()?).1),
    }
}

fn ollama_size(list: &Value, model: &str) -> Option<u64> {
    list["models"].as_array()?.iter().find(|m| m["name"].as_str().is_some_and(|n| same_name(n, model)))?["size"].as_u64()
}

/// Loads the model, or refreshes it. Returns when the server unloads it without a refresh
/// (Ollama), None when it keeps it (llama-server).
pub fn load(settings: &LlmSettings) -> Result<Option<chrono::DateTime<Local>>, String> {
    match settings.backend {
        Backend::Ollama => {
            post(settings, "/api/generate", &settings.ollama_load(), LOAD)?;
            Ok(Some(Local::now() + settings.keep()))
        }
        Backend::LlamaCpp => {
            let Some(names) = loaded(settings)? else { return Ok(None) };
            if !names.contains(&settings.model) {
                post(settings, "/models/load", &json!({ "model": settings.model }), QUICK)?;
                let start = Instant::now();
                while !loaded(settings)?.unwrap_or_default().contains(&settings.model) {
                    if start.elapsed() > LOAD {
                        return Err("llama-server did not load it in time.".into());
                    }
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
            Ok(None)
        }
    }
}

pub fn unload(settings: &LlmSettings, model: &str) -> Result<(), String> {
    match settings.backend {
        Backend::Ollama => post(settings, "/api/generate", &llm::ollama_unload(model), QUICK).map(drop),
        Backend::LlamaCpp if loaded(settings)?.is_some() => post(settings, "/models/unload", &json!({ "model": model }), QUICK).map(drop),
        Backend::LlamaCpp => Ok(()),
    }
}

/// Asks the model about a request. The model must be loaded: Ollama would load it otherwise.
pub fn ask(settings: &LlmSettings, tool: &str, text: &str) -> Result<Answer, String> {
    let start = Instant::now();
    let answer = match settings.backend {
        Backend::Ollama => match post(settings, "/api/chat", &settings.ollama_chat(tool, text, Some(false)), settings.answer_timeout()) {
            // A model without a thinking switch refuses it.
            Err(e) if e.contains("think") => post(settings, "/api/chat", &settings.ollama_chat(tool, text, None), settings.answer_timeout()),
            other => other,
        },
        Backend::LlamaCpp => post(settings, "/v1/chat/completions", &settings.llamacpp_chat(tool, text), settings.answer_timeout()),
    }?;
    let (decision, reason) = llm::read_answer(settings.backend, &answer)?;
    Ok(Answer { decision, reason, took: start.elapsed() })
}

/// Unloads the model before quitting, when it is loaded, and stops the server yesyesyes started.
pub fn release(state: &State) {
    let (ready, settings) = {
        let shared = state.lock();
        (shared.llm.is_ready(), shared.config.gate.llm.clone())
    };
    if ready {
        let _ = unload(&settings, &settings.model);
    }
    crate::llm_server::stop();
}

fn active(config: &Config) -> bool {
    config.enabled && config.channel == Channel::ClaudeCode && config.gate.mode == Mode::Llm
}

pub fn start(state: State, ctx: egui::Context) {
    std::thread::spawn(move || {
        let mut keeper = Keeper::default();
        loop {
            if keeper.step(&state) {
                ctx.request_repaint();
            }
            std::thread::sleep(TICK);
        }
    });
}

#[derive(Default)]
struct Keeper {
    /// The loaded model and when it was last loaded or refreshed.
    held: Option<(LlmSettings, Instant)>,
    /// The settings of the last try, and when to try again.
    tried: Option<LlmSettings>,
    retry_at: Option<Instant>,
    /// When the memory of the model was last read.
    measured: Option<Instant>,
}

impl Keeper {
    /// One look at the settings, then at the memory of the model. Returns whether the status changed.
    fn step(&mut self, state: &State) -> bool {
        let changed = self.status_step(state);
        let (ready, settings, shown) = {
            let shared = state.lock();
            (shared.llm.is_ready(), shared.config.gate.llm.clone(), shared.llm_memory)
        };
        let memory = if !ready {
            self.measured = None;
            None
        } else if self.measured.is_some_and(|at| at.elapsed() < MEASURE) && !changed {
            return changed;
        } else {
            self.measured = Some(Instant::now());
            memory(&settings)
        };
        state.lock().llm_memory = memory;
        changed | (memory != shown)
    }

    /// Unload, load or refresh. Returns whether the status changed.
    fn status_step(&mut self, state: &State) -> bool {
        let (active, settings, unload_others) = {
            let mut shared = state.lock();
            (active(&shared.config), shared.config.gate.llm.clone(), std::mem::take(&mut shared.llm_unload_others))
        };
        let mut changed = false;
        if let Some((held, _)) = &self.held
            && (!active || !held.same_model(&settings))
        {
            let _ = unload(held, &held.model);
            self.held = None;
            changed |= set(state, LlmState::Off);
        }
        if unload_others || !self.tried.as_ref().is_some_and(|t| t.same_model(&settings)) {
            self.tried = Some(settings.clone());
            self.retry_at = None;
        }
        if !active {
            return changed | set(state, LlmState::Off);
        }
        match self.keep(state, &settings, unload_others) {
            Some(status) => changed | set(state, status),
            None => changed,
        }
    }

    /// The new status, or None when nothing was due.
    fn keep(&mut self, state: &State, settings: &LlmSettings, unload_others: bool) -> Option<LlmState> {
        let now = Instant::now();
        let refresh = match &self.held {
            Some((_, refreshed)) if refreshed.elapsed() < settings.refresh_every() => return None,
            Some(_) => true,
            None if self.retry_at.is_some_and(|at| now < at) => return None,
            None => false,
        };
        if settings.model.trim().is_empty() {
            self.retry_at = Some(now + RETRY);
            return Some(LlmState::Failed("pick a model.".into()));
        }
        if let Err(e) = settings.url() {
            self.retry_at = Some(now + RETRY);
            return Some(LlmState::Failed(e));
        }
        if !refresh {
            match self.others(settings, unload_others) {
                Ok(others) if !others.is_empty() => {
                    self.retry_at = Some(now + RETRY);
                    return Some(LlmState::Blocked(others));
                }
                Ok(_) => {}
                Err(e) => {
                    self.retry_at = Some(now + RETRY);
                    return Some(LlmState::Failed(e));
                }
            }
            set(state, LlmState::Loading(settings.model.clone()));
        }
        Some(match load(settings) {
            Ok(until) => {
                self.held = Some((settings.clone(), Instant::now()));
                LlmState::Ready { model: settings.model.clone(), until }
            }
            Err(e) => {
                self.held = None;
                self.retry_at = Some(Instant::now() + RETRY);
                LlmState::Failed(e)
            }
        })
    }

    /// The other models in memory, after unloading them when asked.
    fn others(&self, settings: &LlmSettings, unload_others: bool) -> Result<Vec<String>, String> {
        let Some(names) = loaded(settings)? else { return Ok(Vec::new()) };
        let others: Vec<String> = names.into_iter().filter(|n| !same_name(n, &settings.model)).collect();
        if !unload_others {
            return Ok(others);
        }
        for other in &others {
            unload(settings, other)?;
        }
        Ok(Vec::new())
    }
}

fn set(state: &State, status: LlmState) -> bool {
    let mut shared = state.lock();
    let changed = shared.llm != status;
    shared.llm = status;
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ConfigStore;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    type Calls = Arc<Mutex<Vec<(String, Value)>>>;

    /// A server on a free port answering each `METHOD /path` with `route`; records every call.
    fn fake(route: impl Fn(&str, &Value) -> (u16, Value) + Send + 'static) -> (String, Calls) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let calls: Calls = Arc::default();
        let recorded = calls.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let target = line.split_whitespace().take(2).collect::<Vec<_>>().join(" ");
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = header.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                let (status, answer) = route(&target, &body);
                recorded.lock().unwrap().push((target, body));
                let answer = answer.to_string();
                let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len());
            }
        });
        (url, calls)
    }

    fn state(backend: Backend, url: &str) -> State {
        let state = State::new(ConfigStore { path: std::env::temp_dir().join(format!("yyy-llm-{}-unused.json", std::process::id())) });
        {
            let config = &mut state.lock().config;
            config.channel = Channel::ClaudeCode;
            config.gate.mode = Mode::Llm;
            config.gate.llm = LlmSettings { backend, url: url.into(), model: "qwen3.5:4b".into(), ..LlmSettings::default() };
        }
        state
    }

    fn targets(calls: &Calls) -> Vec<String> {
        calls.lock().unwrap().iter().map(|(t, _)| t.clone()).collect()
    }

    #[test]
    fn ollama_keeps_one_model_at_a_time() {
        let held = Arc::new(Mutex::new(vec!["llava:7b".to_string()]));
        let in_memory = held.clone();
        let (url, calls) = fake(move |target, body| match target {
            "GET /api/ps" => (200, json!({ "models": in_memory.lock().unwrap().iter().map(|n| json!({ "name": n })).collect::<Vec<_>>() })),
            "POST /api/generate" => {
                let name = body["model"].as_str().unwrap().to_string();
                let mut names = in_memory.lock().unwrap();
                names.retain(|n| *n != name);
                if body["keep_alive"] != 0 {
                    names.push(name);
                }
                (200, json!({ "done": true }))
            }
            _ => (404, json!({})),
        });
        let state = state(Backend::Ollama, &url);
        let mut keeper = Keeper::default();
        assert!(keeper.step(&state));
        assert_eq!(state.lock().llm, LlmState::Blocked(vec!["llava:7b".into()]));
        assert_eq!(targets(&calls), ["GET /api/ps"], "nothing loaded or unloaded");

        state.lock().llm_unload_others = true;
        keeper.step(&state);
        assert!(matches!(&state.lock().llm, LlmState::Ready { model, until: Some(_) } if model == "qwen3.5:4b"));
        assert_eq!(*held.lock().unwrap(), ["qwen3.5:4b"]);
        let generate = calls.lock().unwrap().iter().filter(|(t, _)| t == "POST /api/generate").map(|(_, b)| b.clone()).collect::<Vec<_>>();
        assert_eq!(generate, [json!({ "model": "llava:7b", "keep_alive": 0 }), json!({ "model": "qwen3.5:4b", "keep_alive": "60m" })]);

        let before = calls.lock().unwrap().len();
        keeper.step(&state);
        assert_eq!(calls.lock().unwrap().len(), before, "no refresh before a quarter of the keep time");

        state.lock().config.gate.mode = Mode::Regex;
        keeper.step(&state);
        assert_eq!(state.lock().llm, LlmState::Off);
        assert!(held.lock().unwrap().is_empty(), "unloaded when the mode changes");
    }

    #[test]
    fn ollama_answers_with_or_without_the_thinking_switch() {
        let (url, calls) = fake(|target, body| match target {
            "POST /api/chat" if body.get("think").is_some() => (400, json!({ "error": "\"m\" does not support thinking" })),
            "POST /api/chat" => (200, json!({ "message": { "content": r#"{"decision": "allow", "reason": "read-only"}"# } })),
            _ => (404, json!({})),
        });
        let settings = state(Backend::Ollama, &url).lock().config.gate.llm.clone();
        let answer = ask(&settings, "Bash", "ls").unwrap();
        assert_eq!((answer.decision, answer.reason.as_str()), (Decision::Allow, "read-only"));
        assert_eq!(targets(&calls), ["POST /api/chat", "POST /api/chat"]);
    }

    #[test]
    fn llama_server_router_loads_and_waits() {
        let status = Arc::new(Mutex::new("unloaded"));
        let shared = status.clone();
        let (url, calls) = fake(move |target, _| match target {
            "GET /models" => {
                let mut value = shared.lock().unwrap();
                let now = *value;
                if now == "loading" {
                    *value = "loaded";
                }
                (200, json!({ "data": [{ "id": "qwen3.5:4b", "status": { "value": now } }] }))
            }
            "POST /models/load" => {
                *shared.lock().unwrap() = "loading";
                (200, json!({ "success": true }))
            }
            "POST /v1/chat/completions" => (200, json!({ "choices": [{ "message": { "content": r#"{"decision": "ask", "reason": "deletes"}"# } }] })),
            _ => (404, json!({})),
        });
        let state = state(Backend::LlamaCpp, &url);
        Keeper::default().step(&state);
        assert_eq!(state.lock().llm, LlmState::Ready { model: "qwen3.5:4b".into(), until: None });
        assert!(targets(&calls).contains(&"POST /models/load".to_string()));
        let settings = state.lock().config.gate.llm.clone();
        assert_eq!(ask(&settings, "Bash", "rm -rf x").unwrap().decision, Decision::Ignore);
        assert_eq!(*status.lock().unwrap(), "loaded");
    }

    #[test]
    fn a_llama_server_with_one_model_keeps_it() {
        let (url, calls) = fake(|target, _| match target {
            "GET /models" => (200, json!({ "data": [{ "id": "the-model" }] })),
            _ => (404, json!({})),
        });
        let state = state(Backend::LlamaCpp, &url);
        Keeper::default().step(&state);
        assert!(state.lock().llm.is_ready());
        assert!(targets(&calls).iter().all(|t| t == "GET /models"), "nothing to load");
    }

    #[test]
    fn refuses_other_computers_and_waits_before_trying_again() {
        let state = state(Backend::Ollama, "http://192.168.1.10:11434");
        let mut keeper = Keeper::default();
        assert!(keeper.step(&state));
        assert!(matches!(&state.lock().llm, LlmState::Failed(e) if e.contains("this computer")));
        assert!(!keeper.step(&state), "no new try right away");
        state.lock().config.gate.llm.model.clear();
        state.lock().config.gate.llm.url.clear();
        keeper.step(&state);
        assert_eq!(state.lock().llm, LlmState::Failed("pick a model.".into()), "new settings, tried again");
    }

    #[test]
    fn a_stopped_server_is_a_failure() {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let state = state(Backend::Ollama, &format!("http://127.0.0.1:{port}"));
        Keeper::default().step(&state);
        assert!(matches!(&state.lock().llm, LlmState::Failed(e) if e.contains("is it running")));
    }

    #[test]
    fn names_with_and_without_a_tag() {
        assert!(same_name("qwen3.5", "qwen3.5:latest"));
        assert!(same_name("Qwen3.5:4b", "qwen3.5:4b"));
        assert!(!same_name("qwen3.5", "qwen3.5:4b"));
    }

    #[test]
    fn reads_the_size_ollama_reports() {
        let list = json!({"models": [{"name": "llava:7b", "size": 5}, {"name": "qwen3.5:latest", "size": 3_244_566_772u64}]});
        assert_eq!(ollama_size(&list, "qwen3.5"), Some(3_244_566_772));
        assert_eq!(ollama_size(&list, "other"), None);
    }
}
