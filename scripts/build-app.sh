#!/bin/sh
# Builds build/yesyesyes.app (menu bar app, no Dock icon) and signs it ad hoc.
set -e
cd "$(dirname "$0")/.."
swift build -c release
app=build/yesyesyes.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$(swift build -c release --show-bin-path)/yesyesyes" "$app/Contents/MacOS/yesyesyes"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>dev.yesyesyes.app</string>
  <key>CFBundleName</key><string>yesyesyes</string>
  <key>CFBundleExecutable</key><string>yesyesyes</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSUIElement</key><true/>
</dict></plist>
PLIST
codesign --force --sign - "$app"
echo "Built $app"
