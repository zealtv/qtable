//! grid-lead-control (2026-10-20): `spawn_with_grid_lead` puts every frame tick a whole number of
//! periods after the transport start less the lead, so gate 2 can choose the frame grid's phase.
use qtable_engine::model::*;
use qtable_engine::scheduler::{self, SchedulerCommand, SchedulerEvent};
use std::time::{Duration, Instant};

const PERIOD_NS: f64 = 1e9 / 60.0;

fn project() -> Project {
    // No receiver is needed: frames are reported as sent whether or not anything listens.
    let mut project = Project::default();
    let mut output = Track::new_output(TrackId(1001), "rx", "127.0.0.1", 9);
    output.output.as_mut().unwrap().stage.frame_rate = 60.0;
    project.outputs.push(output);
    let mut track = Track::new(TrackId(1), "parameter", 0);
    track.route = Route::single(TrackId(1001));
    let volume = &mut track.mixer_device.volume.output;
    volume.enabled = true;
    volume.send_mode = SendMode::EveryFrame;
    project.tracks.push(track);
    project
}

/// Each grid frame's offset from its nearest tick, counted from `start - lead`, in ns.
fn offsets(lead: Duration) -> Vec<f64> {
    let handle =
        scheduler::spawn_with_grid_lead(project(), Duration::from_millis(20), lead).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    handle.send(SchedulerCommand::Play).unwrap();
    let (mut start, mut dues) = (None::<Instant>, Vec::new());
    let until = Instant::now() + Duration::from_secs(5);
    while dues.len() < 20 && Instant::now() < until {
        match handle.events.recv_timeout(Duration::from_millis(100)) {
            Ok(SchedulerEvent::Playing(at)) => start = Some(at),
            Ok(SchedulerEvent::FrameSent { due, .. }) => dues.push(due),
            Ok(SchedulerEvent::Error(error)) => panic!("{error}"),
            _ => {}
        }
    }
    let anchor = start.expect("Playing") - lead;
    // The first frame is the start's own send, off the grid.
    dues.iter()
        .filter(|&&due| due > start.unwrap())
        .map(|due| {
            let ns = due.duration_since(anchor).as_nanos() as f64;
            ns - (ns / PERIOD_NS).round() * PERIOD_NS
        })
        .collect()
}

#[test]
fn grid_frames_fall_a_whole_number_of_periods_after_the_start_less_the_lead() {
    for lead in [0, 863, 8_934] {
        let offsets = offsets(Duration::from_micros(lead));
        assert!(offsets.len() >= 15, "lead {lead}: {} frames", offsets.len());
        assert!(
            offsets.iter().all(|o| o.abs() < 1_000.0),
            "lead {lead} µs: offsets {offsets:?} ns"
        );
    }
}
