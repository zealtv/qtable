# Releasing qtable

As of 2026-11-19, rewritten for the split between the maintainer's private development repository and the public one. Kept deliberately small: one development line, releases as tags, a stable build to run in production.

## The shape

- **main** is development. It stays green: every merge passes `./check.sh`.
- **A release** is an annotated tag `vMAJOR.MINOR.PATCH` on main, with a CHANGELOG.md entry and the workspace version in Cargo.toml set to match.
- **The public repository** receives one commit per release: the source, docs, examples and tools at that version, tagged `vMAJOR.MINOR.PATCH`. Its history is the list of releases. The development repository keeps the day-to-day history and its own tags, and isn't public.
- **The stable build** is a release build of a tag, kept in `dist/<tag>/` (not committed) in the maintainer's development checkout. `dist/current` points at the build in production use. `./qtable.sh stable` runs it; plain `./qtable.sh` runs the development tree. Anyone else builds from source with `cargo build --release` (see the README).

## Version numbers

- **Major**: a milestone (v2 was the Session View).
- **Minor**: a point release within a milestone.
- **Patch**: fixes to a release.

## What a release is

The maintainer prepares, exports and publishes a release with a private release tool kept in the development repository. It isn't part of the public source. Each step refuses rather than guess:

- **Prepare** checks that Cargo.toml, Cargo.lock and CHANGELOG.md agree on the version, that the CHANGELOG has one dated, finished section for it, and that the files going public hold nothing private. It then builds the exported tree in a fresh checkout and runs `./check.sh` there, so the tests ran on exactly what the public repository will hold. Only then does it commit "Release vX.Y.Z" and make the annotated tag, recording the tests and the tree they ran on.
- **Export** builds the public commit from the tagged tree, minus the files that stay private, under the name and address the public history carries.
- **Publish** pushes that commit and its tag to the public repository (never forced), builds the macOS zip from the exported source, and creates the GitHub release: the tag's CHANGELOG section as its notes, with the zip and a `SHA256SUMS` file.

A GitHub release carries:

- `qtable-vX.Y.Z-macos-<arch>.zip`, built by `tools/package-macos.sh`. It holds `qtable.app` (the application, with the icon from `assets/icon/qtable.icns`), the bare `qtable-gui` and `qtable`, the example sets in `examples/`, `LICENSE`, `THIRD_PARTY_NOTICES.md`, `CHANGELOG.md` and a `README.txt`. The zip is not code-signed or notarised (its `README.txt` says how to clear the quarantine flag). `<arch>` is the processor it was built on: `arm64` or `x86_64`.
- `SHA256SUMS`, one line per file.

The binaries are plain release builds: `cargo build --release`, no features. `tools/package-macos.sh` refuses a build that carries the development hooks (the GUI's capture switches, or the engine's `dev-hooks` feature).

## Building a tag for production

`tools/release.sh build vX.Y.Z` builds the tag in a temporary worktree, copies the binaries to `dist/vX.Y.Z/` and points `dist/current` at them. `tools/release.sh list` shows the builds. It builds only a tag it can trust:

- a tag the private release tool prepared, which the tool re-verifies (version, CHANGELOG section, public files and the record of the test run) before the build starts;
- or, for v3.11.0 and earlier, which were tagged before that tool, a tag with the older record of a passing `./check.sh` on its tree. `--unrecorded` accepts a tag made by hand (v3.8.0 and earlier) with a warning. A record that names the wrong tree is always refused.

`THIRD_PARTY_NOTICES.md` lists the licences of the crates the macOS binaries link. After any change to Cargo.lock, regenerate it with `tools/licenses/generate.sh` (needs `cargo install cargo-about --locked --features cli`) and commit the result; `tools/licenses/generate.sh --check` fails when the file is stale or a dependency's licence falls outside the accepted list in `tools/licenses/about.toml`.

### Testing the release scripts

`tools/test-release.sh` runs `tools/release.sh` and `tools/package-macos.sh` against throwaway fixture repositories, with a stand-in for `cargo`. It tags and publishes nothing real. Run it after changing either script. `./check.sh` does not run it. The zip cases need macOS, and the cases for tags the private tool prepared need a development checkout.

## Project files across versions

A release reads every older project format. A development build may write a newer format that the stable release can't open. Keep production sets on the stable build. Try development builds on copies, and note any format bump in CHANGELOG.md.

[file-format.md](file-format.md) states the policy: a release reads every older format, no build reads a newer one, none writes an older one, and any new stored field or enum variant bumps `FORMAT_VERSION`.
