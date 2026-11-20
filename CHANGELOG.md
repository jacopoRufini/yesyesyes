# Changelog

## 1.0.0

First release: tray app for macOS, Windows and Linux that answers Claude's permission requests.

- Claude Code requests through a local HTTP hook: ignore, allow or deny everything, regex,
  wildcard, local LLM (Ollama or llama.cpp) and Claude's own opinion.
- Notifications: presses chosen buttons (for example Claude Desktop's **Allow once**) for the apps
  and labels you list.
- Recent requests with the chat name, the option that answered and an optional log file.
- **Turn off after** inactivity, so it is not left answering by mistake.
