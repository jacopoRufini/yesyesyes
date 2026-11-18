# yesyesyes

Tray app that presses buttons on notifications for you, only for the apps and button labels
you list. Runs on macOS, Windows and Linux (GNOME).

## Build and run

Needs Rust 1.95 or later, best through rustup (`brew install rustup` on macOS; the build script finds it).

| OS | Build | Run |
|---|---|---|
| macOS | `scripts/build-macos-app.sh` | `open build/yesyesyes.app` |
| Windows | `cargo build --release` | `target\release\yesyesyes.exe` |
| Linux | `cargo build --release` | `target/release/yesyesyes` |

### Permissions

- **macOS**: allow yesyesyes in System Settings → Privacy & Security → Accessibility.
  To keep the permission across rebuilds, create a signing certificate once: Keychain Access →
  Keychain Access menu → Certificate Assistant → Create a Certificate…, name `yesyesyes dev`,
  identity type Self-Signed Root, certificate type Code Signing. The build script signs with it
  when it exists; without it every rebuild needs the permission again (remove the app from the
  list and add it back).
- **Windows**: nothing to allow.
- **Linux (GNOME)**: turn on accessibility, then log out and back in:
  `gsettings set org.gnome.desktop.interface toolkit-accessibility true`.
  The tray icon needs the AppIndicator extension (on by default on Ubuntu); without it the
  window stays open and closing it quits the app.

## Use

The tray icon is a Y in a ring; crossed out when paused. Its menu has **Pause** / **Start** and
**Configure…**, which opens the window with the rules, the recent clicks and **Launch at login**. While paused nothing is pressed, but matching
notifications still show in the recent clicks (and the log file) as *ignored*.
Each recent click shows the start of the notification text (e.g. the command Claude asked to
run); click it to see the whole text. The text stays in memory only, for the last 50 clicks, and
is gone when you quit, unless you turn on **Save the log to a file** (off by default).

The log file has one JSON line per click (time, app, button, result, notification text) and is
readable by you only. By default it is `~/Library/Logs/yesyesyes/clicks.jsonl` (macOS, also
shown in Console), `~/.local/state/yesyesyes/clicks.jsonl` (Linux) or
`%LOCALAPPDATA%\yesyesyes\clicks.jsonl` (Windows); type another path in the window to change it.
The text may contain commands, paths or secrets: check it before sharing.

A rule is the app name (as it appears in the notification) and the exact button label, e.g.
app Claude, button Allow once. Every rule needs both.

To find the exact names, show a notification and run `yesyesyes --dump` while it is on screen
(on macOS: `build/yesyesyes.app/Contents/MacOS/yesyesyes --dump`). It prints what the app sees,
notification text included, to your terminal only.

Rules are saved in `config.json` in `~/Library/Application Support/yesyesyes` (macOS),
`%APPDATA%\yesyesyes` (Windows) or `~/.config/yesyesyes` (Linux).

A rule on a destructive button (Merge, Delete…) presses it with no confirmation.

Outputs are suggestions: review the code before relying on it.

## Develop

```sh
cargo test
cargo check --target x86_64-pc-windows-msvc      # needs rustup and the target
cargo check --target x86_64-unknown-linux-gnu
```

`src/core` holds the rules, matching and engine (tested with a fake tree); `src/backend` has one
file per OS: macOS Accessibility API, Windows UI Automation, Linux AT-SPI over D-Bus.
