# Environment variables

qtable reads four environment variables. Each is optional and changes only the behaviour named here.

| Variable | Effect |
| --- | --- |
| `QTABLE_SETTINGS_PATH` | The GUI reads and writes its per-machine preferences (MIDI ports and the like) at this path instead of the default `config.ron` in the platform's configuration folder, so a test run leaves real preferences alone. |
| `QTABLE_MIDI_DEBUG` | When set to anything, the GUI prints each MIDI input message, timestamped, to stderr. |
| `QTABLE_DISABLE_REALTIME` | When set to anything, the scheduler thread skips the macOS realtime time-constraint policy and keeps only its high-priority quality of service; for A/B timing comparisons and troubleshooting. Other systems already run without it. |
| `QTABLE_SCHEDULER_POLICY_DIAGNOSTICS` | When set to anything, the scheduler prints once to stderr which thread policy it ended up with (macOS). |

## Development hooks

The `dev-hooks` cargo feature, off by default, adds what only development needs: the GUI's
screenshot switches and test fixtures (the `QTABLE_CAPTURE_*` variables and layout options that
`tools/gui-capture.sh` uses) and the engine's timing-measurement entry points. Release builds and
`tools/release.sh` leave it out, so a release binary reads none of those variables. `./check.sh` and
`tools/gui-capture.sh` turn it on.
