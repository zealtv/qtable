#!/bin/sh
# Focused check for scheduler timing work: command/frame interaction, the cluster rule, output
# stages, turns and ordering, plus the real-time worker tests for them. For iteration only;
# ./check.sh stays the full gate (run once on the final code state before tying a code change).
set -eu

cd "$(dirname "$0")"
# Development checkout only: every top-level entry must be classified for release.
if [ -x .release/test-manifest.sh ]; then .release/test-manifest.sh; fi
cargo fmt --check
cargo test -p qtable-engine --lib -- \
    scheduler::automation \
    scheduler::plan_cost_tests::dense_envelope_frame_wake_plan_cost \
    scheduler::cluster \
    scheduler::held_frame \
    scheduler::commands \
    scheduler::command_queue \
    scheduler::jumps \
    scheduler::output_stage \
    scheduler::turns \
    scheduler::backlog_turn \
    scheduler::oneshots \
    scheduler::wait \
    scheduler::stopping \
    scheduler::transport \
    scheduler::valve_tests
cargo test -p qtable-engine \
    --test cluster_frames \
    --test envelope_playback \
    --test grid_lead \
    --test held_frame_commands \
    --test fire_event \
    --test cross_output_turns \
    --test bundle_flush \
    --test launch_quantisation \
    --test receive_time_admission \
    --test launch_from_stopped \
    --test transport_stop \
    --test scheduler_marks \
    --test parameter_updates \
    --test timing
