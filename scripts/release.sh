#!/bin/sh
# Releases a new version: scripts/release.sh 1.2.3
# Sets the version in Cargo.toml and Cargo.lock, commits, tags v1.2.3 and pushes; the Release
# workflow (.github/workflows/release.yml) then builds macOS, Windows and Linux and publishes them.
set -e
cd "$(dirname "$0")/.."
[ -x /opt/homebrew/opt/rustup/bin/cargo ] && PATH=/opt/homebrew/opt/rustup/bin:$PATH
version=${1:?usage: scripts/release.sh X.Y.Z}
case $version in *[!0-9.]*|.*|*.|*..*) echo "not a version: $version"; exit 1;; esac
[ "$(git branch --show-current)" = main ] || { echo "release from main"; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "commit or stash your changes first"; exit 1; }
git fetch -q origin
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || { echo "main differs from origin/main: pull or push first"; exit 1; }
git rev-parse -q --verify "refs/tags/v$version" >/dev/null && { echo "tag v$version exists"; exit 1; }
cargo test --release
awk -v v="$version" '!done && /^version = "/ { print "version = \"" v "\""; done=1; next } { print }' Cargo.toml > Cargo.toml.new && mv Cargo.toml.new Cargo.toml
grep -q "^version = \"$version\"" Cargo.toml || { echo "could not set the version in Cargo.toml"; exit 1; }
cargo check --release --offline   # refreshes the version in Cargo.lock
git add Cargo.toml Cargo.lock
git diff --cached --quiet || git commit -q -m "Release $version"
git tag -a "v$version" -m "v$version"
git push origin main "v$version"
echo "Released v$version: follow the build with  gh run watch"
