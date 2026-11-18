#!/bin/sh
# Builds build/yesyesyes.app (menu bar app, no Dock icon) and signs it with "yesyesyes dev"
# when that certificate exists, ad hoc otherwise.
set -e
cd "$(dirname "$0")/.."
# Prefer rustup (Homebrew installs it keg-only): rust-toolchain.toml then picks a recent compiler.
[ -x /opt/homebrew/opt/rustup/bin/cargo ] && PATH=/opt/homebrew/opt/rustup/bin:$PATH
cargo build --release
app=build/yesyesyes.app
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp target/release/yesyesyes "$app/Contents/MacOS/yesyesyes"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>dev.yesyesyes.app</string>
  <key>CFBundleName</key><string>yesyesyes</string>
  <key>CFBundleExecutable</key><string>yesyesyes</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSUIElement</key><true/>
</dict></plist>
PLIST
# A stable identity keeps the Accessibility permission across builds; ad hoc loses it every time.
identity="yesyesyes dev"
if security find-certificate -c "$identity" >/dev/null 2>&1; then
  codesign --force --sign "$identity" "$app"
  echo "Built $app (signed with \"$identity\")"
else
  codesign --force --sign - "$app"
  echo "Built $app (signed ad hoc: allow Accessibility again; see README to keep it across builds)"
fi
