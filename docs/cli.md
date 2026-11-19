# qtable on the command line

`qtable` is a headless client of the engine: it loads a project, plays it and sends its OSC, and
you drive it by typing commands. It doesn't edit or save projects; make and change sets in the GUI.

**Unstable.** The command words, the `status` text and the exit statuses may change before the
command line reaches 1.0. Scripts written against them today may need updating; the words below
are the ones to expect to keep, and the short forms and old spellings listed beside them stay
accepted for now.

## Starting it

From a release zip, run the bare `qtable` in the unzipped folder, which also holds the examples:

```sh
./qtable play examples/generic-receiver.ron
```

From a clone of the repository, build first (`cargo build --release`, see the README), then run
the binary directly:

```sh
target/release/qtable play examples/generic-receiver.ron
```

Or let cargo build and run it, rebuilding if the sources changed:

```sh
cargo run --release -p qtable-cli -- play examples/generic-receiver.ron
```

`./qtable.sh cli examples/generic-receiver.ron` does the same, and
`./qtable.sh fast cli examples/generic-receiver.ron` runs your last release build in
`target/release/` without building. The forms are:

```text
qtable play <project.ron>   load a project and take commands on standard input
qtable --version, -V        print the version and exit
qtable --help, -h           print the usage and exit
```

Anything else prints a usage line and exits with status 2.

It starts with the transport stopped:

```text
qtable — transport stopped; launch a clip/scene or type start-transport to start; help for commands
> status
Transport: stopped | 120.00 BPM | 1.1.00 | quant 1bar
Music: — | route: main
Lighting cue: — | route: lights
>
```

Each track line shows what it's playing (`—` for nothing) and where it routes. Errors from the
engine, such as a send that failed, print as they arrive.

## Commands

Commands are case-insensitive. A blank line shows the status.

| Command | Also accepted | What it does |
| --- | --- | --- |
| `start-transport` | `play` | Start the transport. |
| `stop-transport` | `halt` | Stop the transport; a second one rewinds to 1.1.1. |
| `launch <track> <slot>` | `l` | Launch the clip in that slot, or press the slot's stop button if it's empty. Slots start at 1. |
| `scene <scene>` | | Launch a scene. Scenes start at 1. |
| `stop <track>` | `s` | Stop one track. A bare `stop` is a usage error; it doesn't stop the transport. |
| `stopgroup <group>` | `sg` | Stop every track in a group. |
| `stop-all` | `stopall`, `sa` | Stop all clips. |
| `tempo <bpm>` | | Set the tempo. |
| `quantisation <value>` | `quant` | Set global launch quantisation: `none`, `1bar`, `2bars`, `4bars`, `8bars`, or a note value `1/2`, `1/4`, `1/8`, `1/16`, `1/32`, with `T` for triplets (`1/8T`). |
| `follow-actions on\|off` | `follow` | Turn follow actions on or off. |
| `status` | | Show transport, tracks and routes. |
| `help` | `h`, `?` | List the commands. |
| `quit` | `q`, `exit` | Leave. End of input (Ctrl-D) does the same. |

The transport words are `start-transport` and `stop-transport`, not `play` and `stop`, on purpose:
`play` is also the `qtable play` subcommand, and `stop <track>` already means a track. They follow
the names qtable will use for OSC remote control, which it doesn't serve yet: `/qtable/transport/start`
and `/qtable/transport/stop`, and for the other settings `transport/stop_all` (`stop-all`),
`transport/quantisation/value` (`quantisation`) and `transport/follow_actions/value`
(`follow-actions`). Sending those addresses to qtable does nothing today. The CLI says `launch` and
`scene` where the addresses say `fire`.

### Naming tracks, groups and scenes

A track, group or scene can be given by its name or by its 1-based position: `launch 2 1`,
`launch lighting cue 3`, `scene Verse`. The rules, as they stand:

- A name made only of digits reads as a position, so a track named `2` can't be picked by name.
- The last word of `launch` is always the slot; everything before it names the track. Names may
  contain spaces: `launch lighting cue 3` launches slot 3 of `lighting cue`.
- Names match without regard to case (`MUSIC` finds `Music`).
- Duplicate names are legal in a project. The first match wins, in project order; use the
  position to reach a later one.
- `stop` and `launch` look at the clip tracks only, and `stopgroup` at the group tracks only,
  so positions count within that kind of track, not across the whole set.

A selector by id may come later without changing these.

## Watching the OSC

The examples send to UDP ports 9000 and 9001 on localhost. To see what arrives on port 9000, run
one of these in another terminal:

```sh
python3 tools/osc-monitor.py 9000   # in this repo; prints time and gap per message
oscdump 9000                        # with liblo installed
```

## Scripting it

The CLI reads commands from standard input, so a pipe works:

```sh
printf 'launch 1 1\nstatus\n' | ./qtable play examples/generic-receiver.ron
```

When standard input isn't a terminal:

- There is no `> ` prompt, and the start-up banner goes to standard error, so standard output
  carries only what commands print (`status` and `help`).
- Errors go to standard error, one line each.

**Exit status.** `0` after a run in which every command worked; `1` if the project didn't load or
if any command failed by the end of input or `quit` (an unknown command, a bad argument, a track
or scene that doesn't exist, or an error the engine returned for the command); `2` for a bad
command line. A mistyped command at a terminal sets it too. Errors the scheduler reports between
commands, such as a UDP send that failed, print as they arrive but don't change the exit status.

It quits at the end of input, so a pipe that ends at once stops playback at once. For a longer run,
keep the input open, for example with `(printf 'scene 1\n'; sleep 60) | …`.
