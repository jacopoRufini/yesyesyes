//! The local HTTP server the Claude Code `http` hook calls on each permission request: the hook
//! JSON in, the hook answer out. Listens on 127.0.0.1 only and refuses requests from web pages.

use crate::core::gate::{self, Decision};
use crate::core::{Channel, ClickEvent, ClickResult, requests, transcript};
use crate::state::State;
use chrono::Local;
use eframe::egui;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub const PORT: u16 = 47331;
pub const PATH: &str = "/claude/permission";
const MAX_HEADERS: u64 = 64 * 1024;
const MAX_BODY: usize = 1024 * 1024;
const WAIT: Duration = Duration::from_secs(2);
/// Event ids apart from the engine's.
static NEXT_ID: AtomicU64 = AtomicU64::new(1 << 62);

pub fn start(state: State, ctx: egui::Context) {
    if let Err(e) = start_on(state.clone(), ctx, PORT) {
        state.lock().error = Some(format!(
            "Claude Code requests cannot reach yesyesyes: port {PORT} is busy ({e}). If another program holds it, it could answer them."
        ));
    }
}

/// Returns the address it listens on (port 0 picks a free one).
fn start_on(state: State, ctx: egui::Context, port: u16) -> std::io::Result<SocketAddr> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let address = listener.local_addr()?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = serve(stream, &state, address.port());
            ctx.request_repaint();
        }
    });
    Ok(address)
}

fn serve(stream: TcpStream, state: &State, port: u16) -> std::io::Result<()> {
    stream.set_read_timeout(Some(WAIT))?;
    stream.set_write_timeout(Some(WAIT))?;
    let (status, body) = match read_request(&stream, port) {
        Ok(hook) => (200, answer(&hook, state)),
        Err(status) => (status, json!({})),
    };
    let body = body.to_string();
    let reason = match status {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Bad Request",
    };
    let head = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    (&stream).write_all((head + &body).as_bytes())
}

/// The JSON body of a `POST /claude/permission`. Refused: another path or method, a Host other
/// than this server (DNS rebinding), an Origin (a web page), a body that is too big or not JSON.
fn read_request(stream: &TcpStream, port: u16) -> Result<Value, u16> {
    let mut reader = BufReader::new(stream.take(MAX_HEADERS + MAX_BODY as u64));
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|_| 400u16)?;
    if line.split_whitespace().take(2).collect::<Vec<_>>() != ["POST", PATH] {
        return Err(404);
    }
    let (mut length, mut host_ok, mut origin) = (0usize, false, false);
    loop {
        line.clear();
        reader.read_line(&mut line).map_err(|_| 400u16)?;
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else { return Err(400) };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => length = value.parse().map_err(|_| 400u16)?,
            "host" => host_ok = value == format!("127.0.0.1:{port}") || value == format!("localhost:{port}"),
            "origin" => origin = true,
            _ => {}
        }
    }
    if !host_ok || origin {
        return Err(403);
    }
    if length > MAX_BODY {
        return Err(400);
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).map_err(|_| 400u16)?;
    serde_json::from_slice(&body).map_err(|_| 400)
}

/// The hook answer: a decision for allow and deny, nothing (the usual prompt) otherwise.
fn answer(hook: &Value, state: &State) -> Value {
    let Some(request) = requests::from_hook(hook, Local::now().fixed_offset()) else { return json!({}) };
    let (settings, active, channel) = {
        let shared = state.lock();
        (shared.config.gate.clone(), shared.config.enabled, shared.config.channel)
    };
    if channel != Channel::ClaudeCode {
        // Kept only to show the notification of this request whole.
        state.remember(request);
        return json!({});
    }
    let (decision, reason) = gate::decide(&settings, active, &request.tool, &request.text);
    let mut rule = gate::rule(&settings, active);
    if active && settings.mode == gate::Mode::Opinion
        && let Some(model) = claude_model(hook)
    {
        rule = format!("{rule} {model}");
    }
    state.record(vec![event(&request, decision, rule, chat_name(hook))]);
    state.remember(request);
    let decision = match decision {
        Decision::Allow => json!({ "behavior": "allow" }),
        Decision::Deny => json!({ "behavior": "deny", "message": format!("Denied by yesyesyes: {reason}") }),
        Decision::Ignore => return json!({}),
    };
    json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } })
}

/// The model of the Claude Code session that sent the request, from its transcript.
fn claude_model(hook: &Value) -> Option<String> {
    let path = hook.get("transcript_path")?.as_str()?;
    transcript::last_model(std::path::Path::new(path), &dirs::home_dir()?.join(".claude"))
}

/// The name of the Claude Code chat that sent the request, from its transcript.
fn chat_name(hook: &Value) -> Option<String> {
    let path = hook.get("transcript_path")?.as_str()?;
    transcript::chat_name(std::path::Path::new(path), &dirs::home_dir()?.join(".claude"))
}

fn event(request: &requests::Request, decision: Decision, rule: String, chat: Option<String>) -> ClickEvent {
    let result = match decision {
        Decision::Allow => ClickResult::Allowed,
        Decision::Deny => ClickResult::Denied,
        Decision::Ignore => ClickResult::Ignored,
    };
    ClickEvent {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        at: Local::now(),
        app: "Claude Code".into(),
        action: request.tool.clone(),
        result,
        texts: vec![request.text.clone()],
        request: Some(request.text.clone()),
        rule: Some(rule),
        chat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ConfigStore;
    use crate::core::gate::{GateSettings, Mode};

    /// Status and body of a raw HTTP request.
    fn send(address: SocketAddr, raw: &str) -> (u16, Value) {
        let mut stream = TcpStream::connect(address).unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        let status = reply.split_whitespace().nth(1).unwrap().parse().unwrap();
        let body = reply.split_once("\r\n\r\n").unwrap().1;
        (status, serde_json::from_str(body).unwrap())
    }

    fn post(address: SocketAddr, body: &str, extra: &str) -> (u16, Value) {
        let host = format!("127.0.0.1:{}", address.port());
        send(address, &format!("POST {PATH} HTTP/1.1\r\nHost: {host}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()))
    }

    fn hook(command: &str) -> String {
        json!({ "hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": { "command": command } }).to_string()
    }

    fn server(mode: Mode, regex: &str) -> (State, SocketAddr) {
        let state = State::new(ConfigStore { path: std::env::temp_dir().join(format!("yyy-gate-{}-unused.json", std::process::id())) });
        {
            let config = &mut state.lock().config;
            config.channel = Channel::ClaudeCode;
            config.gate = GateSettings { mode, regex: regex.into(), ignore_case: false, ..GateSettings::default() };
        }
        let address = start_on(state.clone(), egui::Context::default(), 0).unwrap();
        (state, address)
    }

    #[test]
    fn answers_the_hook_and_records_the_request() {
        let (state, address) = server(Mode::Regex, "Bash: ls( -l)?");
        let (status, allowed) = post(address, &hook("ls -l"), "");
        assert_eq!(status, 200);
        assert_eq!(allowed["hookSpecificOutput"]["decision"]["behavior"], "allow");
        assert_eq!(allowed["hookSpecificOutput"]["hookEventName"], "PermissionRequest");
        assert_eq!(post(address, &hook("ls; rm x"), ""), (200, json!({})), "asked");
        let shared = state.lock();
        assert_eq!(shared.events.len(), 2);
        assert_eq!(shared.events[1].result, ClickResult::Allowed);
        assert_eq!(shared.events[0].request.as_deref(), Some("ls; rm x"));
        assert_eq!(shared.requests.len(), 2, "kept to show their notifications whole");
    }

    #[test]
    fn with_notifications_it_only_keeps_the_request() {
        let (state, address) = server(Mode::Allow, "");
        state.lock().config.channel = Channel::Notifications;
        assert_eq!(post(address, &hook("ls"), ""), (200, json!({})));
        let shared = state.lock();
        assert!(shared.events.is_empty());
        assert_eq!(shared.requests.len(), 1);
    }

    #[test]
    fn deny_tells_claude_why() {
        let (_, address) = server(Mode::Deny, "");
        let (_, denied) = post(address, &hook("ls"), "");
        assert_eq!(denied["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert_eq!(denied["hookSpecificOutput"]["decision"]["message"], "Denied by yesyesyes: set to deny everything");
    }

    #[test]
    fn refuses_web_pages_other_hosts_and_bad_requests() {
        let (state, address) = server(Mode::Allow, "");
        assert_eq!(post(address, &hook("ls"), "Origin: https://example.com\r\n").0, 403);
        let rebound = format!("POST {PATH} HTTP/1.1\r\nHost: evil.example:{}\r\nContent-Length: 2\r\n\r\n{{}}", address.port());
        assert_eq!(send(address, &rebound).0, 403);
        let get = format!("GET {PATH} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n", address.port());
        assert_eq!(send(address, &get).0, 404);
        assert_eq!(post(address, "not json", "").0, 400);
        assert_eq!(post(address, r#"{"tool_name": "Bash"}"#, ""), (200, json!({})), "no input: asked");
        assert!(state.lock().events.is_empty());
    }
}
