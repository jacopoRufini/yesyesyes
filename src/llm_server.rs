//! Starts the LLM server on this computer when it is not running: Ollama (its app, or `ollama
//! serve`) or `llama-server` in router mode. A server started here is stopped when yesyesyes quits.

use crate::core::clicklog;
use crate::core::llm::{Backend, LlmSettings};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

static STARTED: Mutex<Option<Child>> = Mutex::new(None);

/// Folders searched besides `PATH`: an app opened from Finder has a short `PATH`.
const BIN_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];
const OLLAMA_APP: &str = "/Applications/Ollama.app";

#[derive(Debug, PartialEq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Stopped when yesyesyes quits; the Ollama app is left to itself.
    pub own: bool,
}

pub fn name(backend: Backend) -> &'static str {
    match backend {
        Backend::Ollama => "Ollama",
        Backend::LlamaCpp => "llama-server",
    }
}

/// Host and port of `http://host:port`, the host without brackets.
pub fn host_port(url: &str) -> (String, u16) {
    let rest = url.strip_prefix("http://").unwrap_or(url);
    let authority = rest.split('/').next().unwrap_or("");
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.contains(']') => (host, port.parse().unwrap_or(80)),
        _ => (authority, 80),
    };
    (host.trim_start_matches('[').trim_end_matches(']').to_string(), port)
}

/// How the server of `settings` is started; `find` locates a program.
pub fn plan(settings: &LlmSettings, find: impl Fn(&str) -> Option<PathBuf>, ollama_app: bool) -> Result<Launch, String> {
    let url = settings.url()?;
    let (host, port) = host_port(&url);
    match settings.backend {
        Backend::Ollama if ollama_app && url == settings.backend.default_url() => {
            Ok(Launch { program: "/usr/bin/open".into(), args: vec!["-a".into(), OLLAMA_APP.into()], env: vec![], own: false })
        }
        Backend::Ollama => {
            let program = find("ollama").ok_or("ollama is not installed (brew install ollama, or the app from ollama.com).")?;
            let address = if host.contains(':') { format!("[{host}]:{port}") } else { format!("{host}:{port}") };
            Ok(Launch { program, args: vec!["serve".into()], env: vec![("OLLAMA_HOST".into(), address)], own: true })
        }
        Backend::LlamaCpp => {
            let program = find("llama-server").ok_or("llama-server is not installed (brew install llama.cpp).")?;
            let mut args = vec!["--host".into(), host, "--port".into(), port.to_string(), "--models-max".into(), "1".into()];
            let dir = settings.models_dir.trim();
            if !dir.is_empty() {
                args.extend(["--models-dir".into(), dir.to_string()]);
            }
            Ok(Launch { program, args, env: vec![], own: true })
        }
    }
}

fn find(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).chain(BIN_DIRS.iter().map(PathBuf::from)).map(|dir| dir.join(program)).find(|file| file.is_file())
}

/// Starts the server; its output goes to `<name>.log` next to the click log.
pub fn start(settings: &LlmSettings) -> Result<(), String> {
    let launch = plan(settings, find, cfg!(target_os = "macos") && Path::new(OLLAMA_APP).exists())?;
    let mut command = Command::new(&launch.program);
    command.args(&launch.args).envs(launch.env.iter().map(|(k, v)| (k, v))).stdin(Stdio::null());
    let log = clicklog::default_path().with_file_name(format!("{}.log", name(settings.backend)));
    match std::fs::File::create(&log).and_then(|f| Ok((f.try_clone()?, f))) {
        Ok((out, err)) => command.stdout(out).stderr(err),
        Err(_) => command.stdout(Stdio::null()).stderr(Stdio::null()),
    };
    let child = command.spawn().map_err(|e| format!("cannot start {}: {e}", launch.program.display()))?;
    if launch.own {
        let mut started = STARTED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut old) = started.replace(child) {
            let _ = old.kill();
            let _ = old.wait();
        }
    }
    Ok(())
}

/// Stops the server started by yesyesyes, if any.
pub fn stop() {
    if let Some(mut child) = STARTED.lock().unwrap_or_else(|e| e.into_inner()).take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(program: &str) -> Option<PathBuf> {
        Some(PathBuf::from("/bin").join(program))
    }

    fn settings(backend: Backend, url: &str) -> LlmSettings {
        LlmSettings { backend, url: url.into(), ..LlmSettings::default() }
    }

    #[test]
    fn reads_host_and_port() {
        assert_eq!(host_port("http://127.0.0.1:8091"), ("127.0.0.1".into(), 8091));
        assert_eq!(host_port("http://[::1]:11434/x"), ("::1".into(), 11434));
        assert_eq!(host_port("http://localhost"), ("localhost".into(), 80));
    }

    #[test]
    fn ollama_opens_its_app_on_the_usual_address() {
        let launch = plan(&settings(Backend::Ollama, ""), installed, true).unwrap();
        assert_eq!(launch.args, ["-a", OLLAMA_APP]);
        assert!(!launch.own);
        let launch = plan(&settings(Backend::Ollama, "http://127.0.0.1:9000"), installed, true).unwrap();
        assert_eq!((launch.program, launch.args), (PathBuf::from("/bin/ollama"), vec!["serve".to_string()]));
        assert_eq!(launch.env, [("OLLAMA_HOST".to_string(), "127.0.0.1:9000".to_string())]);
        assert!(launch.own);
    }

    #[test]
    fn llama_server_in_router_mode_with_one_model() {
        let mut chosen = settings(Backend::LlamaCpp, "http://127.0.0.1:8091");
        let launch = plan(&chosen, installed, false).unwrap();
        assert_eq!(launch.args, ["--host", "127.0.0.1", "--port", "8091", "--models-max", "1"]);
        chosen.models_dir = "/models".into();
        assert!(plan(&chosen, installed, false).unwrap().args.ends_with(&["--models-dir".to_string(), "/models".to_string()]));
    }

    #[test]
    fn refuses_what_is_missing_or_not_local() {
        assert!(plan(&settings(Backend::LlamaCpp, ""), |_| None, false).unwrap_err().contains("brew install llama.cpp"));
        assert!(plan(&settings(Backend::Ollama, "http://example.com:11434"), installed, false).is_err());
    }
}
