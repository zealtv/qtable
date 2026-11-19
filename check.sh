#!/bin/sh
set -eu

cd "$(dirname "$0")"
cargo fmt --check
# The development configuration: the tests (which turn on the engine's dev-hooks feature) and
# the GUI with its capture hooks, so the hooked code is linted and tested.
cargo clippy --workspace --all-targets --features qtable-gui/dev-hooks -- -D warnings
cargo test --workspace --features qtable-gui/dev-hooks
# The release configuration: no dev-hooks anywhere, so what ships stays warning-free.
cargo clippy --workspace -- -D warnings
# Run the release configuration too: build the CLI as it ships and drive it for a second. The tests
# above always link the engine with dev-hooks (their dev-dependencies turn it on), so this is the
# only step that plays through the engine without it.
cargo build --release -p qtable-cli
if cargo tree -p qtable-cli -e normal,features -i qtable-engine | grep -q dev-hooks; then
    echo "check: the shipped CLI's engine has dev-hooks on" >&2
    exit 1
fi
# `set -e` fails the check if qtable exits non-zero (a project that won't load, a command that fails).
shipped_out=$( (printf 'quantisation none\nstart-transport\nlaunch 1 1\n'; sleep 1; printf 'status\nstop-all\nstop-transport\n') |
    target/release/qtable play examples/generic-receiver.ron)
printf '%s\n' "$shipped_out"
for expected in 'Transport: playing' 'Music: Pulse (slot 1'; do
    if ! printf '%s\n' "$shipped_out" | grep -qF "$expected"; then
        echo "check: shipped CLI run printed no '$expected'" >&2
        exit 1
    fi
done
cargo test --release -p qtable-engine --lib plan_cost -- --nocapture --test-threads=1
