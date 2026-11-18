# yesyesyes

Menu bar app that presses buttons on macOS notifications for you, only for the apps and
button labels you list.

## Build and run

```sh
scripts/build-app.sh
open build/yesyesyes.app
```

Grant it Accessibility (System Settings → Privacy & Security → Accessibility). The app is
signed ad hoc, so macOS asks again after every rebuild: remove it from the list and add it back.

## Use

Menu bar icon → **Rules & Log…**: add a rule with the app name (as it appears in the banner)
and the exact button label (e.g. Claude → Allow once). Every rule needs both. **Pause** stops
all clicking; **Launch at Login** starts it with your Mac. Recent clicks are listed in the
same window (app and button only, never the notification text).

Rules are saved in `~/Library/Application Support/yesyesyes/config.json`.

A rule on a destructive button (Merge, Delete…) presses it with no confirmation.

Outputs are suggestions: review the code before relying on it.

## Develop

```sh
swift run selftest
```
