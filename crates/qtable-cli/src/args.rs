//! The command line itself: `--version`, `--help` and `play <project.ron>`.
use std::path::PathBuf;

pub(super) const USAGE: &str = "\
qtable: a headless OSC sequencer that plays a project made in the qtable GUI

Usage:
  qtable play <project.ron>   Load a project and drive it with commands typed on standard
                              input (type help once it runs)
  qtable --version, -V        Print the version
  qtable --help, -h           Print this help

The command words and the status text may change before the CLI reaches 1.0.
Exit status: 0 on a clean run, 1 if the project would not load or any command failed,
2 for a bad command line.";

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Invocation {
    Version,
    Help,
    Play(PathBuf),
    Usage,
}

pub(super) fn parse_args(args: impl IntoIterator<Item = String>) -> Invocation {
    let args: Vec<String> = args.into_iter().collect();
    let flag = |arg: &str, short: &str, long: &str| arg == short || arg == long;
    match args.as_slice() {
        [one] if flag(one, "-V", "--version") => Invocation::Version,
        [one] if flag(one, "-h", "--help") => Invocation::Help,
        [play, one] if play == "play" && flag(one, "-h", "--help") => Invocation::Help,
        [play, path] if play == "play" && !path.starts_with('-') => {
            Invocation::Play(PathBuf::from(path))
        }
        _ => Invocation::Usage,
    }
}
