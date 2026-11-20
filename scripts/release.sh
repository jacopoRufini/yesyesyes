#!/bin/sh
# Releases a new version: scripts/release.sh 1.2.3
# main is protected (changes need a pull request with passing CI), so this: sets the version in
# Cargo.toml and Cargo.lock on a release/vX.Y.Z branch, opens a pull request, waits for CI, squash
# merges it, then tags the merge commit v1.2.3 and pushes the tag. The Release workflow
# (.github/workflows/release.yml) then builds macOS, Windows and Linux and publishes them.
set -e
cd "$(dirname "$0")/.."
[ -x /opt/homebrew/opt/rustup/bin/cargo ] && PATH=/opt/homebrew/opt/rustup/bin:$PATH
version=${1:?usage: scripts/release.sh X.Y.Z}
case $version in *[!0-9.]*|.*|*.|*..*) echo "not a version: $version"; exit 1;; esac
branch=release/v$version
[ "$(git branch --show-current)" = main ] || { echo "release from main"; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "commit or stash your changes first"; exit 1; }
git fetch -q origin
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || { echo "main differs from origin/main: pull first"; exit 1; }
git rev-parse -q --verify "refs/tags/v$version" >/dev/null && { echo "tag v$version exists"; exit 1; }
grep -q "^## $version" CHANGELOG.md || { echo "add a '## $version' section to CHANGELOG.md first (and merge it)"; exit 1; }
cargo test --release

git switch -q -c "$branch"
awk -v v="$version" '!done && /^version = "/ { print "version = \"" v "\""; done=1; next } { print }' Cargo.toml > Cargo.toml.new && mv Cargo.toml.new Cargo.toml
grep -q "^version = \"$version\"" Cargo.toml || { echo "could not set the version in Cargo.toml"; git switch -q main; exit 1; }
cargo check --release --offline   # refreshes the version in Cargo.lock
git add Cargo.toml Cargo.lock
git commit -q -m "Release $version"
git push -q -u origin "$branch"
gh pr create --base main --head "$branch" --title "Release $version" --body "Version $version. Merging tags v$version and publishes the binaries."

# CI starts a few seconds after the push
sleep 15
gh pr checks "$branch" --watch --fail-fast
gh pr merge "$branch" --squash --delete-branch

git switch -q main
git pull -q --ff-only origin main
[ "$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)" = "$version" ] || { echo "merged main is not at $version"; exit 1; }
git tag -a "v$version" -m "v$version"
git push origin "v$version"
echo "Released v$version: follow the build with  gh run watch"
