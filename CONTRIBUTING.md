# Contributing

Thanks for helping. Bug reports, ideas and pull requests are welcome.

## Before a pull request

- For anything bigger than a small fix, open an issue first so we agree on the direction.
- Keep a change to one topic; explain why in the description.
- Add a test when you change rules, matching or the gate (`src/core` is tested without a UI).
- yesyesyes answers permission requests for people: when in doubt, a change should ask the user
  rather than allow.

## Build and check

See [Build from source](README.md#build-from-source) and [Develop](README.md#develop):

```sh
cargo test
cargo clippy --all-targets -- -D warnings
python3 -m unittest discover -s scripts
```

CI runs the same on macOS, Windows and Linux for every pull request. If you cannot try your change
on another OS, say so in the pull request.

## Releases

Maintainers run `scripts/release.sh X.Y.Z`: it opens a version-bump pull request, merges it once CI passes, tags it, and the Release
workflow publishes the binaries. Add the change to [CHANGELOG.md](CHANGELOG.md) before.

## Conduct

Be kind and assume good faith. Harassment is not tolerated.
