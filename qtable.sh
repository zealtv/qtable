#!/bin/sh
# Start qtable. `./qtable.sh help` (or -h, --help) lists what it can do; any
# call it can't read prints why, then the same help, and exits 2.
#
# Builds in release mode: the debug GUI uses a full CPU core at idle.
set -eu

usage() {
    cat <<'EOF'
qtable.sh: start qtable, the OSC sequencer modelled on Ableton Live's Session View.

Usage:
  ./qtable.sh [stable|fast] [project.ron]                     open the GUI
  ./qtable.sh [stable|fast] cli <project.ron>                 play a project headless in the terminal
  ./qtable.sh help                                            this help (also -h, --h, -help, --help)

Which build:
  (default)  the development tree; cargo rebuilds in release mode first if the sources changed
  stable     the stable release in dist/current, the build for production sets; never builds
  fast       your last local release build in target/release; never builds

GUI:
  With no project the GUI opens an empty one. Relative project paths are read from the
  repository root.

Command line (cli):
  Loads one project, plays it and sends its OSC, starting with the transport stopped. Type
  commands at its prompt; "help" there lists them. It doesn't edit or save projects.

Examples:
  ./qtable.sh examples/grid-demo.ron
  ./qtable.sh stable examples/grid-demo.ron
  ./qtable.sh cli examples/generic-receiver.ron
  printf 'launch 1 1\nstatus\n' | ./qtable.sh stable cli examples/generic-receiver.ron

More: README.md, docs/cli.md, docs/releasing.md.
EOF
}

malformed() {
    printf 'qtable.sh: %s\n\n' "$1" >&2
    usage >&2
    exit 2
}

for arg in "$@"; do
    case "$arg" in
        help | -h | --h | -help | --help | -\?) usage; exit 0 ;;
    esac
done

cd "$(dirname "$0")"

build=development
case "${1:-}" in
    stable | fast) build=$1; shift ;;
esac

if [ "${1:-}" = "cli" ]; then
    shift
    [ $# -eq 1 ] || malformed "cli takes exactly one project file"
    [ -f "$1" ] || malformed "no such project file: $1"
    case $build in
        stable)
            [ -x dist/current/qtable ] || { echo "no stable build: run tools/release.sh build <tag>" >&2; exit 1; }
            exec dist/current/qtable play "$1" ;;
        fast)
            [ -x target/release/qtable ] || { echo "no CLI build: run cargo build --release -p qtable-cli" >&2; exit 1; }
            exec target/release/qtable play "$1" ;;
        *)
            exec cargo run --quiet --release -p qtable-cli -- play "$1" ;;
    esac
fi

# The GUI takes at most one project. (The layout switches for screenshots exist only in builds
# with the dev-hooks feature; tools/gui-capture.sh uses them.)
project=
for arg in "$@"; do
    case "$arg" in
        -*) malformed "unknown option: $arg" ;;
        *)
            [ -z "$project" ] || malformed "more than one project: $project, $arg"
            [ -f "$arg" ] || malformed "unknown command or no such project file: $arg"
            project=$arg ;;
    esac
done

case $build in
    stable)
        [ -x dist/current/qtable-gui ] || { echo "no stable build: run tools/release.sh build <tag>" >&2; exit 1; }
        exec dist/current/qtable-gui "$@" ;;
    fast)
        [ -x target/release/qtable-gui ] || { echo "no GUI build: run cargo build --release -p qtable-gui" >&2; exit 1; }
        exec target/release/qtable-gui "$@" ;;
    *)
        exec cargo run --quiet --release -p qtable-gui -- "$@" ;;
esac
