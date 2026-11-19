#!/bin/sh
# Tests for tools/release.sh (build, list) and tools/package-macos.sh on throwaway fixture repositories:
# which tags build accepts (the old record, --unrecorded, a tag the maintainer's release tool prepared),
# the development-hook checks, and the macOS zip. Nothing here touches this repository, its tags or its
# origin. ./check.sh does not run this script.
#
#   tools/test-release.sh
#
# cargo is a stub on a fixture's PATH. The zip cases need macOS (the zip holds macOS builds), and the
# shared-tool cases need .release/ (a development checkout), so those are skipped elsewhere.
set -u
here=$(cd "$(dirname "$0")/.." && pwd)
T=$(mktemp -d "${TMPDIR:-/tmp}/qtable-release-test.XXXXXX")
trap 'rm -rf "${T:?}"' EXIT

# A hook's or the user's git configuration must not leak into the fixtures.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES \
    GIT_COMMON_DIR GIT_PREFIX GIT_NAMESPACE
GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
GIT_AUTHOR_NAME=Tester GIT_AUTHOR_EMAIL=tester@example.invalid
GIT_COMMITTER_NAME=Tester GIT_COMMITTER_EMAIL=tester@example.invalid
export GIT_CONFIG_GLOBAL GIT_CONFIG_NOSYSTEM GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL

STUB_LOG=$T/stub.log
export STUB_LOG
mkdir "$T/stubs"
cat > "$T/stubs/cargo" <<'EOF'
#!/bin/sh
echo "cargo $*" >> "$STUB_LOG"
if [ "$1" = tree ]; then
    echo 'qtable-engine v3.9.0 (/stub)'
    echo '`-- qtable-engine feature "default"'
    [ -z "${STUB_ENGINE_HOOKS:-}" ] || echo '`-- qtable-engine feature "dev-hooks"'
    exit 0
fi
dir=${CARGO_TARGET_DIR:-target}/release
mkdir -p "$dir"
echo binary > "$dir/qtable-gui"
echo binary > "$dir/qtable"
chmod +x "$dir/qtable-gui" "$dir/qtable"
[ -z "${STUB_HOOKS:-}" ] || echo QTABLE_CAPTURE_CLIP >> "$dir/qtable-gui"
# STUB_HANG=<file>: write this process's id there, then wait, as a long build does.
if [ -n "${STUB_HANG:-}" ]; then
    echo $$ > "$STUB_HANG"
    exec sleep 60
fi
EOF
chmod +x "$T/stubs/cargo"
PATH=$T/stubs:$PATH
export PATH

passed=0
failed=0
n=0
ok() { passed=$((passed + 1)); }
bad() {
    failed=$((failed + 1))
    echo "FAIL: $*"
    sed 's/^/    | /' "$T/out" 2>/dev/null | head -20
}

# new_fixture: a development repository $F on main at "v3.8.0 as released by hand", with a bare origin.
new_fixture() {
    n=$((n + 1))
    F=$T/f$n
    O=$T/o$n.git
    rm -f "$STUB_LOG"
    mkdir -p "$F/tools" "$F/crates/qtable-engine/src" "$F/examples"
    git init -q -b main "$F"
    cp "$here/tools/release.sh" "$here/tools/package-macos.sh" "$F/tools/"
    echo '#!/bin/sh' > "$F/check.sh"
    chmod +x "$F/check.sh" "$F/tools/release.sh" "$F/tools/package-macos.sh"
    cat > "$F/Cargo.toml" <<'EOF'
[workspace]
members = ["crates/qtable-engine"]

[workspace.package]
version = "3.8.0"
edition = "2024"

[workspace.dependencies]
rosc = "=0.11.4"
EOF
    cat > "$F/Cargo.lock" <<'EOF'
version = 4

[[package]]
name = "rosc"
version = "0.11.4"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "qtable-engine"
version = "3.8.0"
dependencies = [
 "rosc",
]
EOF
    cat > "$F/CHANGELOG.md" <<'EOF'
# Changelog

## v3.8.0 (2026-11-12) — format 11

Point release: Undo.

- **Undo and redo.** One history per set.
EOF
    echo 'pub const FORMAT_VERSION: u32 = 11;' > "$F/crates/qtable-engine/src/file.rs"
    echo readme > "$F/README.md"
    echo licence > "$F/LICENSE"
    echo notices > "$F/THIRD_PARTY_NOTICES.md"
    echo '(example: 1)' > "$F/examples/one.ron"
    echo '(example: 2)' > "$F/examples/two.ron"
    mkdir -p "$F/assets/icon"
    echo icns > "$F/assets/icon/qtable.icns"
    (
        cd "$F" || exit 1
        git add -A
        git commit -q -m "v3.8.0 sources"
        git tag -a v3.8.0 -m "qtable v3.8.0"
        git init -q --bare -b main "$O"
        git remote add origin "$O"
        git push -q origin main v3.8.0
    )
}

# bump <version>: set the version in Cargo.toml and Cargo.lock.
bump() {
    for f in Cargo.toml Cargo.lock; do
        sed "s/^version = \"3.8.0\"\$/version = \"$1\"/" "$F/$f" > "$F/$f.new"
        mv "$F/$f.new" "$F/$f"
    done
}

# add_section <text>: put a CHANGELOG section above the v3.8.0 one.
add_section() {
    { sed -n 1,2p "$F/CHANGELOG.md"; printf '%s\n\n' "$1"; sed 1,2d "$F/CHANGELOG.md"; } > "$F/CHANGELOG.md.new"
    mv "$F/CHANGELOG.md.new" "$F/CHANGELOG.md"
}

good_section='## v3.9.0 (2026-11-20) — format 11

Point release: Shortcuts. Fixes drafts of names staying on screen.

- **Shortcuts.** Cmd-S saves.'

# ready: edits for a v3.9.0 release, uncommitted.
ready() {
    new_fixture
    bump 3.9.0
    add_section "$good_section"
}

# legacy_tagged [<record>]: v3.9.0 committed and tagged as the old tools/release.sh prepare did, or with
# the message given.
legacy_tagged() {
    ready
    git -C "$F" commit -q -am "Release v3.9.0"
    git -C "$F" tag -a v3.9.0 -m "${1-qtable v3.9.0

Release check passed: ./check.sh
Tested tree: $(git -C "$F" rev-parse 'HEAD^{tree}')}"
}

# install_release_tool: put the maintainer's release tool and a config for it into $F, committed.
install_release_tool() {
    mkdir -p "$F/.release"
    cp "$here/.release/release.sh" "$F/.release/release.sh"
    chmod +x "$F/.release/release.sh"
    cat > "$F/.release/config" <<'EOF'
name = qtable
public = /nonexistent/public.git
author = Tester <tester@users.noreply.example.invalid>
tests = true
version = cargo
scan_ignore = .release
EOF
    cat > "$F/.release/manifest" <<'EOF'
private .release
public CHANGELOG.md
public Cargo.lock
public Cargo.toml
public LICENSE
public README.md
public THIRD_PARTY_NOTICES.md
public assets
public check.sh
public crates
public examples
public tools
EOF
    git -C "$F" add -A
    git -C "$F" commit -q -m "Install the release tool"
    git -C "$F" push -q origin main
}

# shared_ready: v3.9.0 prepared by the release tool in a fixture that has it.
shared_ready() {
    new_fixture
    install_release_tool
    bump 3.9.0
    add_section "$good_section"
    (cd "$F" && .release/release.sh prepare v3.9.0) > "$T/out" 2>&1
    rc=$?
}

# run_in_f <args...>: run in $F, keeping output and exit code.
run_in_f() {
    (cd "$F" && "$@") > "$T/out" 2>&1
    rc=$?
}

# refuses <description> <message fragment> <args...>: the command fails with that message.
refuses() {
    desc=$1 frag=$2
    shift 2
    run_in_f "$@"
    if [ "$rc" != 0 ] && grep -F -- "$frag" "$T/out" >/dev/null; then ok; else bad "$desc (rc=$rc, wanted '$frag')"; fi
}

succeeds() {
    desc=$1
    shift
    run_in_f "$@"
    if [ "$rc" = 0 ]; then ok; else bad "$desc (rc=$rc)"; fi
}

assert() {
    desc=$1
    shift
    if "$@" >/dev/null 2>&1; then ok; else bad "$desc"; fi
}

called() { grep -F -- "$1" "$STUB_LOG" >/dev/null 2>&1; }
not_called() { ! called "$1"; }

echo "build: tags made before the release tool"

new_fixture
refuses "a tag made by hand" "no record of a release check" tools/release.sh build v3.8.0
assert "a refused build: cargo never ran" not_called "cargo "
succeeds "--unrecorded accepts it" tools/release.sh build v3.8.0 --unrecorded
assert "--unrecorded warns" grep -F "WARNING" "$T/out"
assert "build: cargo ran" called "cargo build --quiet --release -p qtable-gui -p qtable-cli"
assert "build: binaries in dist/v3.8.0" test -f "$F/dist/v3.8.0/qtable-gui" -a -f "$F/dist/v3.8.0/qtable"
assert "build: dist/current -> v3.8.0" test "$(readlink "$F/dist/current")" = v3.8.0
refuses "an unknown tag" "no tag v9.9.9" tools/release.sh build v9.9.9
refuses "a tag without the v" "expected a version tag" tools/release.sh build 3.8.0
refuses "a short tag" "expected a version tag" tools/release.sh build v3.8

legacy_tagged
succeeds "the old prepare's record" tools/release.sh build v3.9.0
assert "the old record: no warning" sh -c "! grep -F WARNING '$T/out'"
assert "the old record: dist/current -> v3.9.0" test "$(readlink "$F/dist/current")" = v3.9.0

ready; git -C "$F" commit -q -am "Release v3.9.0"; git -C "$F" tag v3.9.0
refuses "a lightweight tag" "no record" tools/release.sh build v3.9.0

legacy_tagged "qtable v3.9.0

Release check passed: ./check.sh
Tested tree: 0000000000000000000000000000000000000000"
refuses "a record naming another tree" "records a check run on tree" tools/release.sh build v3.9.0
refuses "...even under --unrecorded" "records a check run on tree" tools/release.sh build v3.9.0 --unrecorded

legacy_tagged "qtable v3.9.0

Release check passed: ./check.sh"
refuses "half a record" "half of the old prepare's record" tools/release.sh build v3.9.0

legacy_tagged "qtable v3.12.0"
git -C "$F" tag -a v3.12.0 -m "qtable v3.12.0

Release check passed: ./check.sh
Tested tree: $(git -C "$F" rev-parse 'HEAD^{tree}')"
refuses "the old record after v3.11.0" "only tags up to v3.11.0" tools/release.sh build v3.12.0
refuses "...and --unrecorded after v3.11.0" "only tags up to v3.11.0" tools/release.sh build v3.12.0 --unrecorded
git -C "$F" tag -a v3.11.0 -m "qtable v3.11.0"
succeeds "--unrecorded at v3.11.0 itself" tools/release.sh build v3.11.0 --unrecorded

echo "build: tags the release tool prepared"

if [ -x "$here/.release/release.sh" ]; then
    shared_ready
    if [ "$rc" = 0 ]; then ok; else bad "the release tool's prepare in the fixture (rc=$rc)"; fi
    assert "the tag records the tested public tree" sh -c "git -C '$F' cat-file tag v3.9.0 | grep -E '^Tested public tree: [0-9a-f]{40,64}\$'"
    rm -f "$STUB_LOG"
    succeeds "build accepts it" tools/release.sh build v3.9.0
    assert "shared tag: cargo ran" called "cargo build --quiet --release -p qtable-gui -p qtable-cli"
    assert "shared tag: dist/current -> v3.9.0" test "$(readlink "$F/dist/current")" = v3.9.0
    assert "shared tag: no warning" sh -c "! grep -F WARNING '$T/out'"
    succeeds "build accepts it under --unrecorded too" tools/release.sh build v3.9.0 --unrecorded

    # A shared-record tag the tool didn't make: its record names no tree this commit exports.
    shared_ready
    git -C "$F" tag -a v3.9.1 -m "qtable v3.9.1

Release tests passed on the exported tree: true
Tested public tree: 0000000000000000000000000000000000000000"
    rm -f "$STUB_LOG"
    refuses "a shared-record tag naming another tree" "the release tool refuses v3.9.1" tools/release.sh build v3.9.1
    refuses "...even under --unrecorded" "the release tool refuses v3.9.1" tools/release.sh build v3.9.1 --unrecorded
    assert "refused: cargo never ran" not_called "cargo "

    # The tag's record names the tree that was tested. A tag re-made over a later commit, with the same
    # message, names a tree that is no longer the one it exports.
    shared_ready
    message=$(git -C "$F" cat-file tag v3.9.0 | sed 1,5d)
    echo more >> "$F/README.md"
    git -C "$F" commit -q -am "Change a public file after the tests"
    GIT_COMMITTER_DATE='2001-01-01T00:00:00Z' git -C "$F" tag -f -a v3.9.0 -m "$message" > /dev/null 2>&1
    refuses "a record that names an earlier tree" "the release tool refuses v3.9.0" tools/release.sh build v3.9.0

    # Without .release/ in the checkout there is nothing to verify it with.
    shared_ready
    rm -rf "${F:?}/.release"
    refuses "a shared-tool tag in a checkout without .release/" "no .release/ to verify it with" tools/release.sh build v3.9.0
else
    echo "skipped: the shared-tool cases need .release/ (a development checkout)"
fi

echo "build: development hooks"

legacy_tagged
STUB_HOOKS=1; export STUB_HOOKS
refuses "build: a binary with capture hooks" "contains capture hooks" tools/release.sh build v3.9.0
unset STUB_HOOKS
STUB_ENGINE_HOOKS=1; export STUB_ENGINE_HOOKS
refuses "build: the engine's dev-hooks feature alone" "enables the engine's dev-hooks feature" tools/release.sh build v3.9.0
assert "build refused: dist/current untouched" test ! -e "$F/dist/current"
unset STUB_ENGINE_HOOKS

echo "build: the old commands"

new_fixture
for cmd in prepare verify publish package; do
    refuses "$cmd has moved" "moved to the maintainer's release tool" tools/release.sh "$cmd" v3.9.0
done

echo "list"

new_fixture
succeeds "list with no builds" tools/release.sh list
assert "list prints nothing" test ! -s "$T/out"
run_in_f tools/release.sh build v3.8.0 --unrecorded
succeeds "list after a build" tools/release.sh list
assert "list shows the build and current" sh -c "grep -qx v3.8.0 '$T/out' && grep -qx 'current -> v3.8.0' '$T/out'"

echo "build: interrupted"

# interrupted <shell> <signal> <exit status>: signal a running build and check it leaves no worktree
# or temporary directory behind. A terminal's Ctrl-C reaches the build's children too, so the stub
# cargo is stopped as well; the script's own signal goes first, which is what an untrapped dash
# would die of without cleaning up. perl starts the script because a background job of a
# non-interactive shell ignores SIGINT, and an ignored signal cannot be trapped.
interrupted() {
    shell=$1 sig=$2 want=$3
    legacy_tagged
    rm -rf "${T:?}"/tmp "${T:?}"/hang
    mkdir "$T/tmp"
    (cd "$F" && STUB_HANG=$T/hang TMPDIR=$T/tmp exec perl -e '$SIG{INT} = "DEFAULT"; exec @ARGV' \
        "$shell" tools/release.sh build v3.9.0 > "$T/out" 2>&1) &
    script=$!
    waited=0
    while [ ! -s "$T/hang" ] && [ "$waited" -lt 200 ]; do sleep 0.1; waited=$((waited + 1)); done
    if [ ! -s "$T/hang" ]; then
        kill -KILL "$script" 2>/dev/null
        wait "$script" 2>/dev/null
        bad "build interrupted ($shell, $sig): the build never started"
        return
    fi
    kill -s "$sig" "$script"
    kill -KILL "$(cat "$T/hang")" 2>/dev/null
    wait "$script"
    rc=$?
    assert "build interrupted ($shell, $sig): exit status $want" test "$rc" = "$want"
    assert "build interrupted ($shell, $sig): no worktree left" test "$(git -C "$F" worktree list | wc -l | tr -d ' ')" = 1
    assert "build interrupted ($shell, $sig): no temporary directory left" test -z "$(ls -A "$T/tmp")"
    assert "build interrupted ($shell, $sig): dist/current untouched" test ! -e "$F/dist/current"
}

for shell in sh dash; do
    command -v "$shell" >/dev/null 2>&1 || { echo "skipped: $shell not installed"; continue; }
    interrupted "$shell" INT 130
    interrupted "$shell" TERM 143
done

echo "package-macos.sh"

if [ "$(uname -s)" = Darwin ]; then
    new_fixture
    succeeds "package a tree" tools/package-macos.sh "$T/zips"
    zip=$(ls "$T"/zips/qtable-v3.8.0-macos-*.zip 2>/dev/null)
    assert "the zip is named for the version and arch" test -n "$zip"
    assert "only the zip is written to the output directory" test "$(ls "$T/zips" | wc -l | tr -d ' ')" = 1
    assert "cargo built the release binaries" called "cargo build --quiet --release -p qtable-gui -p qtable-cli"
    zip_list=$(unzip -Z1 "$zip")
    for entry in /LICENSE /THIRD_PARTY_NOTICES.md /CHANGELOG.md /README.txt /qtable-gui /qtable \
        /qtable.app/Contents/MacOS/qtable-gui /qtable.app/Contents/Resources/qtable.icns \
        /qtable.app/Contents/Info.plist /examples/one.ron /examples/two.ron; do
        assert "zip carries $entry" sh -c "printf '%s\n' '$zip_list' | grep -q '$entry\$'"
    done
    unzip -p "$zip" '*/qtable.app/Contents/Info.plist' > "$T/Info.plist"
    assert "app's Info.plist names the icon and the version" sh -c "grep -q '<string>qtable</string>' '$T/Info.plist' && grep -q '<key>CFBundleIconFile</key>' '$T/Info.plist' && grep -q '<string>3.8.0</string>' '$T/Info.plist'"
    unzip -p "$zip" '*/README.txt' > "$T/README.txt"
    assert "README.txt names the tag, the format and the examples" sh -c "grep -q 'qtable v3.8.0' '$T/README.txt' && grep -q 'format 11' '$T/README.txt' && grep -q 'examples/' '$T/README.txt'"

    succeeds "package again over the same directory" tools/package-macos.sh "$T/zips"
    assert "package again: still one file" test "$(ls "$T/zips" | wc -l | tr -d ' ')" = 1

    RELEASE_VERSION=3.99.0; export RELEASE_VERSION
    succeeds "package under RELEASE_VERSION" tools/package-macos.sh "$T/zips2/nested"
    unset RELEASE_VERSION
    assert "RELEASE_VERSION names the zip" sh -c "ls '$T'/zips2/nested/qtable-v3.99.0-macos-*.zip"
    refuses "package with no out directory" "usage" tools/package-macos.sh

    rm -rf "${T:?}/cache" "${F:?}/target"
    CARGO_TARGET_DIR=$T/cache; export CARGO_TARGET_DIR
    succeeds "package with CARGO_TARGET_DIR" tools/package-macos.sh "$T/zips3"
    unset CARGO_TARGET_DIR
    assert "CARGO_TARGET_DIR: the binaries came from the cache" test -x "$T/cache/release/qtable-gui"
    assert "CARGO_TARGET_DIR: nothing built inside the checkout" test ! -e "$F/target"

    new_fixture
    STUB_HOOKS=1; export STUB_HOOKS
    refuses "package: a binary with capture hooks" "contains capture hooks" tools/package-macos.sh "$T/zips4"
    unset STUB_HOOKS
    STUB_ENGINE_HOOKS=1; export STUB_ENGINE_HOOKS
    refuses "package: the engine's dev-hooks feature alone" "enables the engine's dev-hooks feature" tools/package-macos.sh "$T/zips4"
    unset STUB_ENGINE_HOOKS
    assert "a refused package writes no zip" test ! -e "$T/zips4"

    new_fixture; rm -f "$F/assets/icon/qtable.icns"
    refuses "package: no icon" "no assets/icon/qtable.icns" tools/package-macos.sh "$T/zips5"
    assert "no icon: cargo never ran" not_called "cargo "
    new_fixture; rm -f "$F/LICENSE"
    refuses "package: no LICENSE" "no LICENSE" tools/package-macos.sh "$T/zips5"
    assert "no LICENSE: cargo never ran" not_called "cargo "
    new_fixture; rm -rf "${F:?}/examples"
    refuses "package: no examples" "no example sets" tools/package-macos.sh "$T/zips5"
    assert "no examples: cargo never ran" not_called "cargo "
    new_fixture; sed 's/^version = "3.8.0"$/version = "x"/' "$F/Cargo.toml" > "$F/c.new"; mv "$F/c.new" "$F/Cargo.toml"
    refuses "package: no version" "no version found" tools/package-macos.sh "$T/zips5"
    assert "no version: cargo never ran" not_called "cargo "
    assert "refusals: no zip written" test ! -e "$T/zips5"
else
    echo "skipped: package-macos.sh needs macOS"
fi

echo
echo "$passed passed, $failed failed"
[ "$failed" = 0 ]
