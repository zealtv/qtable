use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;

use qtable_engine::engine::{Command, Engine};
use qtable_engine::file;
use qtable_engine::model::Project;

mod args;
mod parse;
mod status;
use args::*;
use parse::*;
use status::*;

/// Print the start-up banner where the person at a terminal sees it, and on stderr when
/// commands are piped in, so piped stdout carries only command output.
fn banner(interactive: bool, line: &str) {
    if interactive {
        println!("{line}");
    } else {
        eprintln!("{line}");
    }
}

/// Run until end of input or `quit`. Returns whether any command failed.
fn run(project_path: &Path) -> Result<bool, String> {
    let project = file::load(project_path).map_err(|error| error.to_string())?;
    let mut engine = Engine::new(project.clone()).map_err(|error| error.to_string())?;
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    banner(
        interactive,
        "qtable — transport stopped; launch a clip/scene or type start-transport to start; help for commands",
    );
    if let Some(notice) = project.every_frame_rate_notice() {
        banner(interactive, notice);
    }
    let mut stdout = io::stdout();
    let mut errors_seen = 0;
    let mut any_failed = false;
    loop {
        report_new_errors(&engine, &mut errors_seen);
        if interactive {
            print!("> ");
            stdout.flush().map_err(|error| error.to_string())?;
        }
        let mut line = String::new();
        if stdin
            .lock()
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        let context = engine.command_context(qtable_engine::engine::Receipt {
            received_at: std::time::Instant::now(),
            sequence: None,
        });
        let parsed = match parse_command(&line) {
            Ok(ParsedCommand::Quit) => break,
            Ok(parsed) => parsed,
            Err(error) => {
                eprintln!("{error}");
                any_failed = true;
                continue;
            }
        };
        if let Err(error) = dispatch(&mut engine, &project, context, parsed) {
            eprintln!("{error}");
            any_failed = true;
        }
    }
    Ok(any_failed)
}

/// Resolve a parsed command's selectors against the project and hand it to the engine.
fn dispatch(
    engine: &mut Engine,
    project: &Project,
    context: qtable_engine::engine::CommandContext,
    parsed: ParsedCommand,
) -> Result<(), String> {
    let command = match parsed {
        ParsedCommand::Launch(selector, slot) => Command::LaunchClip {
            track: resolve_track(&selector, &project.clip_tracks().collect::<Vec<_>>())?,
            // The engine validates the slot itself; no need to pre-check it here.
            slot: slot - 1,
        },
        ParsedCommand::Scene(selector) => Command::LaunchScene {
            scene: resolve_scene(&selector, &project.scenes)?,
        },
        ParsedCommand::StopTrack(selector) => Command::StopTrack {
            track: resolve_track(&selector, &project.clip_tracks().collect::<Vec<_>>())?,
        },
        ParsedCommand::StopGroup(selector) => Command::StopGroup {
            group: resolve_group(&selector, &project.group_tracks().collect::<Vec<_>>())?,
        },
        ParsedCommand::StopAll => Command::StopAll,
        ParsedCommand::StartTransport => Command::Play,
        ParsedCommand::StopTransport => Command::Stop,
        ParsedCommand::Tempo(bpm) => Command::SetTempo(bpm),
        ParsedCommand::Quantisation(value) => Command::SetGlobalQuantisation(value),
        ParsedCommand::Follow(value) => Command::SetFollowActionsEnabled(value),
        ParsedCommand::Status => {
            engine.poll_events();
            print_status(&engine.snapshot(), project);
            return Ok(());
        }
        ParsedCommand::Help => {
            print_help();
            return Ok(());
        }
        ParsedCommand::Quit => return Ok(()),
    };
    execute(engine, context, command)
}

fn execute(
    engine: &mut Engine,
    context: qtable_engine::engine::CommandContext,
    command: Command,
) -> Result<(), String> {
    engine
        .execute_with_context(command, context)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn main() {
    match parse_args(std::env::args().skip(1)) {
        Invocation::Version => println!("qtable {}", env!("CARGO_PKG_VERSION")),
        Invocation::Help => println!("{USAGE}"),
        Invocation::Play(path) => match run(&path) {
            Ok(false) => {}
            Ok(true) => std::process::exit(1),
            Err(error) => {
                eprintln!("qtable: {error}");
                std::process::exit(1);
            }
        },
        Invocation::Usage => {
            eprintln!("usage: qtable play <project.ron>   (qtable --help for more)");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests;
