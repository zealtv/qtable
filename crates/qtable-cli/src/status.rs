//! Human-readable transport status and scheduler errors.
use crate::parse::quantisation_label;
use qtable_engine::engine::{Engine, Snapshot};
use qtable_engine::model::{Project, TICKS_PER_BEAT};

pub(super) fn print_help() {
    println!(
        "Commands (case-insensitive; aliases in brackets):
  launch <track> <slot> (l)         Launch a clip, or press the slot's stop button if empty
  scene <scene>                     Launch a scene
  stop <track> (s)                  Stop one track
  stopgroup <group> (sg)            Stop every track in a group
  stop-all (stopall, sa)            Stop all clips
  start-transport (play)            Start the transport
  stop-transport (halt)             Stop the transport; a second one rewinds to 1.1.1
  tempo <bpm>                       Set the tempo
  quantisation <value> (quant)      Set global launch quantisation: none, 1bar, 2bars, 4bars,
                                    8bars, 1/2 1/4 1/8 1/16 1/32, with T for triplets
  follow-actions on|off (follow)    Turn follow actions on or off
  status                            Show transport, tracks and routes (a blank line does too)
  help (h, ?)                       Show this help
  quit (q, exit)                    Leave; end of input does the same
Tracks, groups and scenes go by name or 1-based position; names match without regard to case.
Slots start at 1."
    );
}

pub(super) fn print_status(snapshot: &Snapshot, project: &Project) {
    let bar_ticks = project.time_signature.bar_length().ticks();
    let bar = snapshot.current_beat.ticks() / bar_ticks + 1;
    let in_bar_ticks = snapshot.current_beat.ticks() % bar_ticks;
    let beat = in_bar_ticks as f64
        / (TICKS_PER_BEAT as f64 * 4.0 / f64::from(project.time_signature.denominator()))
        + 1.0;
    println!(
        "Transport: {} | {:.2} BPM | {}.{:.2} | quant {}",
        if snapshot.playing {
            "playing"
        } else {
            "stopped"
        },
        snapshot.tempo_bpm,
        bar,
        beat,
        quantisation_label(snapshot.global_quantisation)
    );
    if let Some(failure) = &snapshot.status.scheduler_failure {
        println!("Scheduler FAILED: {failure}");
    }
    if let Some(error) = &snapshot.status.last_error {
        println!(
            "Scheduler errors: {} (last: {error})",
            snapshot.status.error_count
        );
    }
    if let Some(warning) = &snapshot.status.last_warning {
        println!("Output warning: {warning}");
    }
    for (id, throughput) in &snapshot.output_throughput {
        if let Some(output) = project.destination(*id) {
            let name = project
                .outputs
                .iter()
                .position(|track| track.id == *id)
                .map_or_else(
                    || output.name.clone(),
                    |position| format!("{} {}", output_letter(position), output.name),
                );
            println!("{}", throughput_line(&name, throughput));
        }
    }
    for item in &snapshot.unresolved_outputs {
        let name = project
            .outputs
            .iter()
            .find(|d| d.id == item.id)
            .map_or("?", |d| d.name.as_str());
        println!("Output {name} unresolved: {}", item.reason);
    }
    if let Some(id) = snapshot.last_triggered_scene
        && let Some((row, scene)) = project.scenes.iter().enumerate().find(|(_, s)| s.id == id)
    {
        println!("Last triggered scene: {} (row {})", scene.name, row + 1);
    }
    for state in &snapshot.tracks {
        let Some(track) = project.track(state.id) else {
            continue;
        };
        let mut activity = if let Some(slot) = state.playing_slot {
            let clip = project.clip(track.id, slot);
            let name = clip.map_or("?", |clip| clip.name.as_str());
            format!(
                "{name} (slot {}, {:.2} beats)",
                slot + 1,
                state.play_position_beats.unwrap_or(0.0)
            )
        } else {
            "—".to_owned()
        };
        if let Some(triggered) = &state.triggered {
            let name = project
                .clip(track.id, triggered.slot)
                .map_or("?", |clip| clip.name.as_str());
            activity.push_str(&format!(
                " | triggered: {name} (slot {}, at beat {:.3})",
                triggered.slot + 1,
                triggered.launch_beat.ticks() as f64 / TICKS_PER_BEAT as f64
            ));
        }
        if let Some((_, beat)) = snapshot
            .pending_stops
            .iter()
            .find(|(id, _)| *id == track.id)
        {
            activity.push_str(&format!(
                " | stopping at beat {:.3}",
                beat.ticks() as f64 / TICKS_PER_BEAT as f64
            ));
        }
        let route = project
            .routed_outputs(track.id)
            .map(|o| {
                format!(
                    "{}{}",
                    o.name,
                    if o.mixer.active { "" } else { " (disabled)" }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let route = if route.is_empty() {
            "empty (sends nothing)"
        } else {
            &route
        };
        println!("{}: {} | route: {}", track.name, activity, route);
    }
}

/// Print scheduler errors that arrived since the last call.
pub(super) fn report_new_errors(engine: &Engine, seen: &mut u64) {
    engine.poll_events();
    let status = engine.snapshot().status;
    let new = status.error_count.saturating_sub(*seen) as usize;
    let skipped = new.saturating_sub(status.recent_errors.len());
    if skipped > 0 {
        eprintln!("scheduler: {skipped} earlier errors not shown");
    }
    let from = status.recent_errors.len().saturating_sub(new);
    for error in &status.recent_errors[from..] {
        eprintln!("scheduler: {error}");
    }
    *seen = status.error_count;
}

fn output_letter(mut position: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push((b'A' + (position % 26) as u8) as char);
        if position < 26 {
            break;
        }
        position = position / 26 - 1;
    }
    letters.into_iter().rev().collect()
}

fn throughput_line(name: &str, value: &qtable_engine::scheduler::Throughput) -> String {
    format!(
        "{name}  {} msg/s  {} pkt/s  {:.1} kB/s  merged {}/s  deferred {}/s  skipped {}/s  oversize {}/s  frame late {}/s{}",
        value.messages,
        value.packets,
        value.bytes as f64 / 1000.0,
        value.merged,
        value.deferred,
        value.skipped,
        value.oversize,
        value.frames_late,
        if value.deferring { " (deferring)" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtable_engine::scheduler::Throughput;

    #[test]
    fn output_status_includes_rates_and_budget_wait_state() {
        assert_eq!(output_letter(0), "A");
        assert_eq!(output_letter(26), "AA");
        let value = Throughput {
            messages: 600,
            packets: 30,
            bytes: 21400,
            merged: 1204,
            deferred: 112,
            deferring: true,
            ..Throughput::default()
        };
        assert_eq!(
            throughput_line("B bopOS", &value),
            "B bopOS  600 msg/s  30 pkt/s  21.4 kB/s  merged 1204/s  deferred 112/s  skipped 0/s  oversize 0/s  frame late 0/s (deferring)"
        );
        assert!(!throughput_line("Main", &Throughput::default()).contains("deferring"));
    }
}
