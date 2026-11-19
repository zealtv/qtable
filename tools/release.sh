#!/bin/sh
# Build a released tag for production (docs/releasing.md).
#
#   tools/release.sh build <tag>      build the tag into dist/<tag> and point dist/current at it
#   tools/release.sh list             show built releases and which is current
#
# Releases are prepared, exported and published by the maintainer's release tool, .release/release.sh,
# which is not part of the public source. This script only builds a tag locally, for ./qtable.sh stable.
#
# build refuses a tag it cannot trust:
#   - A tag that tool prepared (its message records "Tested public tree:") must pass
#     .release/release.sh verify <tag>, so the build needs a checkout that has .release/.
#   - Tags up to v3.11.0 predate that tool. They pass with the record the old tools/release.sh prepare
#     wrote ("Release check passed: ./check.sh" and "Tested tree: <tree>", the tree being the tag's own),
#     or, under --unrecorded, with no record at all (made by hand, as every tag up to v3.8.0 was), with a
#     warning. A record that names the wrong tree is always refused. Later tags need the release tool.
set -eu
cd "$(dirname "$0")/.."
root=$(pwd)

# A hook's environment would point git at another repository or index.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY \
    GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_COMMON_DIR GIT_PREFIX GIT_NAMESPACE

# The last tag made before the shared release tool; the old record and --unrecorded stop there.
last_legacy=v3.11.0
# What the old prepare wrote into the tag message, and what the shared tool's prepare writes.
legacy_record_line='Release check passed: ./check.sh'
shared_record_prefix='Tested public tree: '

die() {
    echo "release: $*" >&2
    exit 1
}

usage() {
    awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0" >&2
    exit "${1:-2}"
}

work=$(mktemp -d "${TMPDIR:-/tmp}/qtable-release.XXXXXX")
cleanup() {
    git -C "$root" worktree remove --force "$work/src" >/dev/null 2>&1 || true
    rm -rf "${work:?}"
}
trap cleanup EXIT
# dash and some shells skip the EXIT trap on a signal; exit explicitly so it runs.
trap 'exit 130' INT
trap 'exit 143' TERM

need_tag() {
    [ -n "$tag" ] || usage
    printf '%s\n' "$tag" | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' >/dev/null ||
        die "expected a version tag like v1.2.3, not '$tag'"
}

# is_legacy_tag: $tag is at or before $last_legacy.
is_legacy_tag() {
    awk -v a="${tag#v}" -v b="${last_legacy#v}" 'BEGIN {
        split(a, x, "."); split(b, y, ".")
        for (i = 1; i <= 3; i++) {
            if (x[i] + 0 < y[i] + 0) exit 0
            if (x[i] + 0 > y[i] + 0) exit 1
        }
        exit 0 }'
}

# A release binary carries no capture or test hooks (the dev-hooks feature stays off). The GUI's
# capture switches leave QTABLE_CAPTURE strings in a binary; the engine's own dev-hooks feature leaves
# none, so ask cargo (run in the tree that was built) whether the build turns it on.
no_dev_hooks() {
    for binary in "$@"; do
        if grep -aq 'QTABLE_CAPTURE' "$binary"; then
            die "$binary contains capture hooks; a release build must not enable the dev-hooks feature"
        fi
    done
    features=$(cargo tree -p qtable-gui -p qtable-cli -e normal,build,features -i qtable-engine) ||
        die "cargo tree could not report the engine's features"
    if printf '%s\n' "$features" | grep -F 'feature "dev-hooks"' >/dev/null; then
        die "this build enables the engine's dev-hooks feature; a release build must not"
    fi
}

# check_shared_tag: the release tool vouches for $tag.
check_shared_tag() {
    [ -x .release/release.sh ] ||
        die "tag $tag was prepared by the release tool, and this checkout has no .release/ to verify it with"
    if ! .release/release.sh verify "$tag" > "$work/verify" 2>&1; then
        cat "$work/verify" >&2
        die "the release tool refuses $tag (.release/release.sh verify, above)"
    fi
}

# check_legacy_tag: a tag made before the release tool, with the old record or (under --unrecorded) none.
check_legacy_tag() {
    is_legacy_tag ||
        die "tag $tag has no record from the release tool (.release/release.sh prepare); only tags up to $last_legacy may be built without one"
    tag_tree=$(git rev-parse "refs/tags/$tag^{tree}")
    ran=0
    grep -Fx "$legacy_record_line" "$work/tag-message" >/dev/null && ran=1
    recorded=$(sed -n 's/^Tested tree: \([0-9a-f]\{40,64\}\)$/\1/p' "$work/tag-message" | sed -n 1p)
    if [ "$ran" = 1 ] && [ -n "$recorded" ]; then
        [ "$recorded" = "$tag_tree" ] ||
            die "tag $tag records a check run on tree $recorded, but it points at tree $tag_tree"
        return 0
    fi
    if [ "$ran" = 1 ] || [ -n "$recorded" ]; then
        die "tag $tag carries half of the old prepare's record (the check line and the 'Tested tree:' line go together)"
    fi
    if [ "$unrecorded" = 1 ]; then
        echo "release: WARNING: tag $tag has no record of a release check (made by hand); accepted by --unrecorded" >&2
        return 0
    fi
    die "tag $tag has no record of a release check (a hand-made tag, made before the old prepare existed; --unrecorded accepts that, with a warning)"
}

# verify_tag: everything build checks before it uses $tag.
verify_tag() {
    git rev-parse -q --verify "refs/tags/$tag" >/dev/null || die "no tag $tag"
    if [ "$(git cat-file -t "refs/tags/$tag")" = tag ]; then
        git cat-file tag "refs/tags/$tag" > "$work/tag-message"
    else
        : > "$work/tag-message"
    fi
    if grep -E "^$shared_record_prefix[0-9a-f]{40,64}\$" "$work/tag-message" >/dev/null; then
        check_shared_tag
    else
        check_legacy_tag
    fi
}

cmd_build() {
    need_tag
    verify_tag
    git worktree add -q --detach "$work/src" "$tag"
    (cd "$work/src" && cargo build --quiet --release -p qtable-gui -p qtable-cli)
    (cd "$work/src" && no_dev_hooks target/release/qtable-gui target/release/qtable)
    mkdir -p "dist/$tag"
    cp "$work/src/target/release/qtable-gui" "$work/src/target/release/qtable" "dist/$tag/"
    ln -sfn "$tag" dist/current
    echo "built dist/$tag; dist/current -> $tag"
}

command=${1:-help}
[ $# -eq 0 ] || shift
tag=
unrecorded=0
while [ $# -gt 0 ]; do
    case $1 in
    --unrecorded) unrecorded=1 ;;
    -*) die "unknown option $1" ;;
    *)
        [ -z "$tag" ] || die "unexpected argument $1"
        tag=$1
        ;;
    esac
    shift
done
case $command in
prepare | verify | publish | package)
    die "'$command' moved to the maintainer's release tool: .release/release.sh $command"
    ;;
esac
[ "$unrecorded" = 0 ] || [ "$command" = build ] || die "--unrecorded applies to build only"

case $command in
build) cmd_build ;;
list)
    ls dist 2>/dev/null | grep -v '^current$' || true
    if [ -L dist/current ]; then echo "current -> $(readlink dist/current)"; fi
    ;;
help | -h | --help) usage 0 ;;
*) usage ;;
esac
