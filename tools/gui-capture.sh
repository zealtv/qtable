#!/bin/bash
# Capture the qtable GUI window, unattended, with no clicks or key input (the machine stays in use meanwhile).
#   tools/gui-capture.sh <output.png> [--show-map-mode | --show-map-learn | --show-map-inspect-cc | --show-map-inspect-note | --show-map-inspect-encoder | --show-map-inspect-trigger | --show-map-inspect-none | --show-key-map | --show-key-learn | --show-key-inspect | --show-events-hits | --show-lanes | --show-tab-events | --show-tab-envelopes | --show-tab-overview | --show-one-lane | --show-one-lane-unlinked | --show-one-lane-volume | --show-segment-hover | --show-segment-hover-lanes | --show-no-lane | --show-device-chooser | --show-control-chooser | --show-menu-point | --show-menu-space | --show-menu-selection | --show-menu-envelope-enabled | --show-menu-envelope-disabled | --show-envelope | --show-points-gestures | --show-points-curvature | --show-points-discrete | --show-points-typed | --show-draw-switch | --show-draw-steps | --show-draw-hits | --select-first-clip | --select-first-scene | --select-first-track | --select-first-group | --select-first-output | --select-main | --show-launch | --show-hit-menu | --show-hit-copy | --show-menu | --show-menu-empty | --show-menu-track | --show-menu-group | --show-menu-output | --show-menu-main | --show-route-dropdown | --show-track-paste | --show-group-paste | --show-output-paste | --show-menu-track-paste | --show-track-duplicate | --show-group-duplicate | --show-output-duplicate | --show-undo | --show-redo | --show-shortcut-settings | --show-shortcut-settings-typing | --show-shortcut-save-rejected | --show-shortcut-save-as] [project.ron]
# Builds the debug GUI with the dev-hooks feature (release builds have none of the hooks), opens it (with the project if given), brings it to the front for ~5 s,
# captures just its window, then quits it. Run from anywhere; paths are relative to the repo root.
# --select-lone-member groups then selects the first track through ordinary startup messages.
# --show-single-track-group groups the first track through the ordinary message at startup.
# --show-hit-arguments selects the first clip and gives Detail View enough height for its hit row;
# with no project it opens crates/qtable-gui/tests/fixtures/hit-arguments.ron (seven arguments, a 900 pt window).
# --focus-path selects the first track and focuses its Volume Path through a startup task.
# Path field captures (crates/qtable-gui/tests/fixtures/paths.ron): --select-absolute-lane shows the
# absolute event lane; --show-path-info selects the first track with the Volume Path's addresses in the
# Info View; --show-lane-info selects the first clip with its selected hit's lane in the Info View.
# Status captures: --show-status-error rejects a path; --show-status-notice disables an output.
# Route captures: --show-route-dropdown uses the grouped fixture when no project is supplied;
# --show-route-no-output, --show-route-group, --show-route-fanout, --show-route-mixed show its faces.
# Paste captures (--show-track-paste, --show-group-paste, --show-output-paste) copy and paste the first
# top-level track, group or output of a fixture project at startup, with no input.
# --show-track-duplicate, --show-group-duplicate, --show-output-duplicate run ⌘D on the same selections.
# --show-undo renames the first track and presses ⌘Z; --show-redo then presses ⇧⌘Z (status bar names the step).
# --show-shortcut-<name> presses ⌘, (settings), ⌘, after typing a tempo (settings-typing), ⌘S after typing
# a tempo the engine refuses (save-rejected), or ⇧⌘S (save-as, which only opens the dialog). No plain ⌘S capture: it would overwrite the project through the ordinary key message.
# --show-menu-track-paste copies a track before opening its header menu; output-paste shows all outputs.
# QTABLE_CAPTURE_PLAY=1 starts real transport for an Every Frame fixture.
# QTABLE_CAPTURE_CONTENT=1 with QTABLE_CAPTURE_REGION=delete|paste|duplicate|sawtooth runs a region edit
# at startup; with --show-one-lane, sawtooth duplicates a 16th of the Opacity ramp three times.
# QTABLE_CAPTURE_ZOOM=<start>,<visible> zooms the shown clip's editor to that window in ticks, through the
# ordinary zoom action (try 960,1920 with --show-tab-events, --show-one-lane or --show-tab-overview).
# QTABLE_CAPTURE_ZOOM_SELECT=<start>,<end> (with QTABLE_CAPTURE_ZOOM) selects that tick range on every lane.
# QTABLE_CAPTURE_SCROLL=<points> scrolls the Events/Overview lane list down that far (clamped to its end),
# to show the ruler header staying put (try 400 with --show-tab-overview and QTABLE_CAPTURE_WINDOW=1400x600).
# QTABLE_CAPTURE_BROWSER_WIDTH=<points> opens the Browser (the mapping list too) at that width.
# QTABLE_CAPTURE_WINDOW=<width>x<height> opens the window at that size in points (short-window checks).
# QTABLE_CAPTURE_THROUGHPUT=1 holds illustrative busy/deferring counters (not measured traffic).
set -eu
cd "$(dirname "$0")/.."
out="$1"
shift
if [ "${1:-}" = "--focus-path" ]; then
    export QTABLE_CAPTURE_FOCUS_PATH=1
    export QTABLE_CAPTURE_TRACK=1
    shift
elif [ "${1:-}" = "--show-status-error" ]; then
    export QTABLE_CAPTURE_PATHS=error
    shift
elif [ "${1:-}" = "--show-status-notice" ]; then
    export QTABLE_CAPTURE_STATUS=notice
    shift
elif [ "${1:-}" = "--select-absolute-lane" ]; then
    export QTABLE_CAPTURE_CLIP=absolute-lane
    shift
elif [ "${1:-}" = "--show-path-info" ]; then
    export QTABLE_CAPTURE_INFO=volume-path
    export QTABLE_CAPTURE_TRACK=1
    set -- --show-info "${@:2}"
elif [ "${1:-}" = "--show-lane-info" ]; then
    export QTABLE_CAPTURE_CLIP=1
    export QTABLE_CAPTURE_INFO=lane
    set -- --show-info "${@:2}"
elif [ "${1:-}" = "--show-hit-arguments" ]; then
    export QTABLE_CAPTURE_CLIP=1
    export QTABLE_CAPTURE_HIT_ARGUMENTS=1
    shift
    [ $# -gt 0 ] || set -- crates/qtable-gui/tests/fixtures/hit-arguments.ron
elif case "${1:-}" in --show-lane-traits-range | --show-lane-traits-drag | --show-lane-traits-menu | --show-lane-traits-event-menu | --show-lane-traits-go-to-device | --show-lane-traits-parameter-menu | --show-lane-traits-volume-menu | --show-lane-traits-go-to-parameter | --show-lane-traits-paste | --show-lane-traits-reenable) true ;; *) false ;; esac; then
    export QTABLE_CAPTURE_CLIP=1
    export QTABLE_CAPTURE_LANES=1
    export QTABLE_CAPTURE_LANE_TRAITS="${1#--show-lane-traits-}"
    shift
    [ $# -gt 0 ] || set -- crates/qtable-gui/tests/fixtures/lanes.ron
elif [ "${1:-}" = "--show-lanes" ]; then
    # The clip editor's All lanes view on crates/qtable-gui/tests/fixtures/lanes.ron: an event lane,
    # then envelope lanes for Float, Int, Toggle and Enum arguments and Mixer Volume, playing, with
    # Volume overridden by a manual change (grey dot). With QTABLE_CAPTURE_TRACK=1 set as well, the track's
    # device chain shows instead, with the automation LEDs on the Target face and Mixer.
    export QTABLE_CAPTURE_CLIP=1
    export QTABLE_CAPTURE_LANES=1
    shift
    [ $# -gt 0 ] || set -- crates/qtable-gui/tests/fixtures/lanes.ron
elif case "${1:-}" in --show-draw-switch | --show-draw-steps | --show-draw-hits) true ;; *) false ;; esac; then
    # Draw Mode on the lanes fixture (draw-mode): the transport switch lit, steps painted across the
    # first envelope lane, or hits painted across the first event lane, each a real stroke committed
    # as one ReplaceRange.
    export QTABLE_CAPTURE_CLIP=1
    export QTABLE_CAPTURE_LANES=1
    export QTABLE_CAPTURE_DRAW="${1#--show-draw-}"
    shift
    [ $# -gt 0 ] || set -- crates/qtable-gui/tests/fixtures/lanes.ron
elif case "${1:-}" in --show-points-gestures | --show-points-curvature | --show-points-discrete | --show-points-typed) true ;; *) false ;; esac; then
    # The same clip after scripted point gestures (point-editing): each leaves a point selected,
    # with the strip's Position, Value and Curve. A drag in flight needs the pointer, so isn't shown.
    export QTABLE_CAPTURE_CLIP=1
    export QTABLE_CAPTURE_LANES=1
    export QTABLE_CAPTURE_POINTS="${1#--show-points-}"
    shift
    [ $# -gt 0 ] || set -- crates/qtable-gui/tests/fixtures/lanes.ron
elif [ "${1:-}" = "--show-events-hits" ]; then
    # Events with its hits unselected; --show-tab-events keeps the startup-selected hit.
    export QTABLE_CAPTURE_CLIP=1
    export QTABLE_CAPTURE_LANES=1
    export QTABLE_CAPTURE_VIEW=events-hits
    shift
    [ $# -gt 0 ] || set -- crates/qtable-gui/tests/fixtures/lanes.ron
elif case "${1:-}" in --show-tab-events | --show-tab-envelopes | --show-tab-overview) true ;; *) false ;; esac; then
    # The clip editor's Events | Envelopes | Overview tabs on the lanes fixture (clip-view-tabs), playing as
    # --show-lanes does, with each tab pressed through its own handler. --show-tab-envelopes opens the
    # tab without a remembered pick, so it shows the clip's first envelope.
    export QTABLE_CAPTURE_CLIP=1
    export QTABLE_CAPTURE_LANES=1
    export QTABLE_CAPTURE_VIEW="tab-${1#--show-tab-}"
    shift
    [ $# -gt 0 ] || set -- crates/qtable-gui/tests/fixtures/lanes.ron
elif case "${1:-}" in --show-one-lane | --show-one-lane-unlinked | --show-one-lane-volume | --show-no-lane | --show-device-chooser | --show-control-chooser | --show-menu-point | --show-menu-space | --show-menu-selection) true ;; *) false ;; esac; then
    # The clip editor's Envelopes tab on the lanes fixture (lane-chooser-and-one-lane-view), playing as
    # --show-lanes does: --show-one-lane chooses BopOS Opacity with its second point selected and the
    # clip's other envelopes as ghosts; --show-one-lane-volume chooses Mixer Volume (overridden, grey);
    # --show-no-lane chooses Position x, which has no lane ("Click to start an envelope");
    # --show-one-lane-unlinked shows Opacity with its own region;
    # --show-device-chooser and --show-control-chooser open each chooser's list (one overlay slot, so
    # one at a time); --show-menu-point, --show-menu-space and --show-menu-selection open the content
    # menu on Opacity's second point, on empty lane space, or inside a time selection.
    export QTABLE_CAPTURE_CLIP=1
    export QTABLE_CAPTURE_LANES=1
    case "$1" in
        --show-one-lane) export QTABLE_CAPTURE_VIEW=opacity ;;
        --show-one-lane-unlinked) export QTABLE_CAPTURE_VIEW=opacity-unlinked ;;
        --show-one-lane-volume) export QTABLE_CAPTURE_VIEW=volume ;;
        --show-no-lane) export QTABLE_CAPTURE_VIEW=no-lane ;;
        --show-device-chooser) export QTABLE_CAPTURE_VIEW=device-chooser ;;
        --show-control-chooser) export QTABLE_CAPTURE_VIEW=control-chooser ;;
        --show-menu-point) export QTABLE_CAPTURE_VIEW=menu-point ;;
        --show-menu-space) export QTABLE_CAPTURE_VIEW=menu-space ;;
        --show-menu-selection) export QTABLE_CAPTURE_VIEW=menu-selection ;;
    esac
    shift
    [ $# -gt 0 ] || set -- crates/qtable-gui/tests/fixtures/lanes.ron
elif case "${1:-}" in --show-menu-envelope-enabled | --show-menu-envelope-disabled | --show-envelope) true ;; *) false ;; esac; then
    # Show Envelope (show-envelope) on the lanes fixture, playing as --show-lanes does, with the Target
    # face showing: --show-menu-envelope-enabled opens the Opacity control's menu with the clip selected;
    # --show-menu-envelope-disabled with no selection while the clip plays (greyed out); --show-envelope
    # runs the entry from the Track tab and shows the one-lane view it opens.
    export QTABLE_CAPTURE_CLIP=1
    export QTABLE_CAPTURE_LANES=1
    case "$1" in
        --show-menu-envelope-enabled) export QTABLE_CAPTURE_VIEW=menu-enabled QTABLE_CAPTURE_TRACK=1 ;;
        --show-menu-envelope-disabled) export QTABLE_CAPTURE_VIEW=menu-disabled QTABLE_CAPTURE_TRACK=1 ;;
        --show-envelope) export QTABLE_CAPTURE_VIEW=show-envelope ;;
    esac
    shift
    [ $# -gt 0 ] || set -- crates/qtable-gui/tests/fixtures/lanes.ron
elif case "${1:-}" in --show-segment-hover | --show-segment-hover-lanes) true ;; *) false ;; esac; then
    # A pointer three quarters of the band below an envelope segment (segment-hover-band), placed by the canvases
    # themselves with no input: --show-segment-hover in the one-lane view as --show-one-lane does,
    # --show-segment-hover-lanes in All lanes on the first linked envelope row.
    export QTABLE_CAPTURE_CLIP=1
    export QTABLE_CAPTURE_LANES=1
    export QTABLE_CAPTURE_HOVER=segment
    [ "$1" = "--show-segment-hover" ] && export QTABLE_CAPTURE_VIEW=opacity
    shift
    [ $# -gt 0 ] || set -- crates/qtable-gui/tests/fixtures/lanes.ron
elif [ "${1:-}" = "--select-first-clip" ]; then
    export QTABLE_CAPTURE_CLIP=1
    shift
elif [ "${1:-}" = "--show-hit-menu" ]; then
    export QTABLE_CAPTURE_CLIP=hit-menu
    shift
elif [ "${1:-}" = "--show-hit-copy" ]; then
    export QTABLE_CAPTURE_CLIP=hit-copy
    shift
elif [ "${1:-}" = "--select-first-scene" ]; then
    export QTABLE_CAPTURE_SCENE=1
    shift
elif [ "${1:-}" = "--select-first-track" ]; then
    export QTABLE_CAPTURE_TRACK=1
    shift
elif [ "${1:-}" = "--select-lone-member" ]; then
    export QTABLE_CAPTURE_TRACK=single-member
    shift
elif [ "${1:-}" = "--show-single-track-group" ]; then
    export QTABLE_CAPTURE_TRACK=single-group
    shift
elif [ "${1:-}" = "--select-first-group" ]; then
    export QTABLE_CAPTURE_TRACK=group
    shift
elif [ "${1:-}" = "--select-first-output" ]; then
    export QTABLE_CAPTURE_OUTPUT=1
    shift
elif case "${1:-}" in --show-route-no-output | --show-route-group | --show-route-fanout | --show-route-mixed) true ;; *) false ;; esac; then
    export QTABLE_CAPTURE_CLIP=route-dropdown
    export QTABLE_CAPTURE_ROUTE_STATE="${1#--show-route-}"
    shift
elif case "${1:-}" in --show-menu | --show-menu-empty | --show-menu-track | --show-menu-group | --show-menu-output | --show-menu-main | --show-route-dropdown) true ;; *) false ;; esac; then
    export QTABLE_CAPTURE_CLIP="${1#--show-}"
    shift
elif [ "${1:-}" = "--show-track-paste" ]; then
    export QTABLE_CAPTURE_TRACK=paste
    shift
elif [ "${1:-}" = "--show-group-paste" ]; then
    export QTABLE_CAPTURE_TRACK=group-paste
    shift
elif [ "${1:-}" = "--show-track-duplicate" ]; then
    export QTABLE_CAPTURE_TRACK=duplicate
    shift
elif [ "${1:-}" = "--show-group-duplicate" ]; then
    export QTABLE_CAPTURE_TRACK=group-duplicate
    shift
elif [ "${1:-}" = "--show-output-duplicate" ]; then
    export QTABLE_CAPTURE_OUTPUT=duplicate
    export QTABLE_CAPTURE_ALL_OUTPUTS=1
    shift
elif [ "${1:-}" = "--show-menu-track-paste" ]; then
    export QTABLE_CAPTURE_TRACK=menu-paste
    shift
elif [ "${1:-}" = "--show-output-paste" ]; then
    export QTABLE_CAPTURE_OUTPUT=paste
    export QTABLE_CAPTURE_ALL_OUTPUTS=1
    shift
elif case "${1:-}" in --show-map-mode | --show-map-learn | --show-map-inspect-* | --show-key-map | --show-key-learn | --show-key-inspect) true ;; *) false ;; esac; then
    # MIDI Map Mode (midi-map-mode): --show-map-mode turns it on with a spread of mappings (the list in
    # the Browser's place, Volume waiting to learn); --show-map-learn turns it on with nothing mapped.
    # The mapping inspector's cases (mapping-inspector): --show-map-inspect-cc (absolute CC on a ranged
    # Float, Value Scaling), -note (a note on a switch, Gate), -encoder (relative CC at 250 % on an
    # event's Int argument), -trigger (a note on a Bang), -none (nothing selected), and
    # --show-key-inspect (a key on a Bang in Key Map Mode).
    # All open examples/grid-demo.ron unless a project is given; add --show-info to see Info View help.
    case "$1" in
        --show-key-map) export QTABLE_CAPTURE_MAP=key-list ;;
        --show-key-learn) export QTABLE_CAPTURE_MAP=key-learn ;;
        --show-key-inspect) export QTABLE_CAPTURE_MAP=key-inspect ;;
        *) export QTABLE_CAPTURE_MAP="${1#--show-map-}" ;;
    esac
    [ "$QTABLE_CAPTURE_MAP" = "mode" ] && export QTABLE_CAPTURE_MAP=list
    export QTABLE_CAPTURE_CLIP=1
    shift
    [ $# -gt 0 ] || set -- examples/grid-demo.ron
elif [ "${1:-}" = "--select-main" ]; then
    export QTABLE_CAPTURE_MAIN=1
    shift
elif [ "${1:-}" = "--show-undo" ] || [ "${1:-}" = "--show-redo" ]; then
    export QTABLE_CAPTURE_UNDO="${1#--show-}"
    shift
elif [[ "${1:-}" == --show-shortcut-* ]]; then
    export QTABLE_CAPTURE_SHORTCUT="${1#--show-shortcut-}"
    shift
elif [ "${1:-}" = "--show-launch" ]; then
    export QTABLE_CAPTURE_CLIP=launch
    shift
fi
mkdir -p "$(dirname "$out")"
# The capture hooks and the layout switches exist only with the dev-hooks feature.
cargo build -q -p qtable-gui --features dev-hooks
"${CARGO_TARGET_DIR:-target}/debug/qtable-gui" "$@" &
pid=$!
trap 'kill $pid 2>/dev/null || true' EXIT
swiftfile=$(mktemp -t qtable-wid).swift
cat > "$swiftfile" <<'SWIFT'
import Foundation
import CoreGraphics
let pid = Int(CommandLine.arguments[1])!
let l = CGWindowListCopyWindowInfo(.optionAll, kCGNullWindowID) as? [[String: Any]] ?? []
for w in l { if let p = w["kCGWindowOwnerPID"] as? Int, p == pid, let layer = w["kCGWindowLayer"] as? Int, layer == 0, (w["kCGWindowIsOnscreen"] as? Bool) == true, let name = w["kCGWindowName"] as? String, !name.isEmpty { print(w["kCGWindowNumber"]!); break } }
SWIFT
# New binaries can take longer than three seconds to open their first window.
# Wait for the capture surface rather than attempting activation before it exists.
wid=""
for attempt in {1..30}; do
    wid=$(swift "$swiftfile" "$pid" | tail -1)
    [ -n "$wid" ] && break
    sleep 2
done
[ -n "$wid" ] || { echo "No qtable window appeared for capture" >&2; exit 1; }
osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $pid) to true" || true
sleep 2
rm -f "$swiftfile"
screencapture -x -o -l"$wid" "$out"
echo "saved $out (window $wid)"
