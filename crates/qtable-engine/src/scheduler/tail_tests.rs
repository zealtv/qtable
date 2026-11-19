//! Regression coverage for the 2026-10-11 positive-offset review probes.
use super::*;
use crate::model::*;

fn rig(delay: f64, latency: f64) -> (Instant, InternalClock, Planner) {
    let mut project = super::tests::project(delay, true);
    project.outputs[0].delay_ms = latency;
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut planner = Planner::new(project, Duration::from_secs(5)).unwrap();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    (start, clock, planner)
}

fn sends(events: &[DueEvent], voice: u64) -> Vec<&DueEvent> {
    events
        .iter()
        .filter(|e| e.voice == voice && matches!(e.action, PlannedAction::Send { .. }))
        .collect()
}

#[test]
fn replacement_preserves_all_twelve_sends_and_output_order() {
    for (delay, latency) in [(600.0, 0.0), (0.0, 600.0), (200.0, 400.0)] {
        let (start, mut clock, mut planner) = rig(delay, latency);
        planner
            .launch(
                TrackId(1),
                1,
                &mut clock,
                start + Duration::from_millis(100),
            )
            .unwrap();
        let events = planner.plan(&clock, start);
        let old = sends(&events, 1);
        assert_eq!(old.len(), 12);
        assert_eq!(old.last().unwrap().at, start + Duration::from_millis(2350));
        assert!(old.last().unwrap().at < sends(&events, 2)[0].at);
        for event in events {
            assert!(planner.consume(&event).is_empty());
        }
        assert!(planner.voices.iter().all(|v| v.id != 1));
    }
}

#[test]
fn quantised_stop_reports_musical_silence_then_drains_tail_once() {
    for (delay, latency) in [(600.0, 0.0), (0.0, 600.0), (200.0, 400.0)] {
        let (start, clock, mut planner) = rig(delay, latency);
        planner
            .request_stop(TrackId(1), &clock, start + Duration::from_millis(100))
            .unwrap();
        let events = planner.plan(&clock, start);
        assert_eq!(sends(&events, 1).len(), 12);
        let mut silence = Vec::new();
        for event in events {
            for track in planner.consume(&event) {
                silence.push((track, event.at));
                assert!(planner.sounding_tracks().is_empty());
            }
        }
        assert_eq!(silence, [(TrackId(1), start + Duration::from_secs(2))]);
        assert!(planner.voices.is_empty());
        assert!(
            planner
                .plan(&clock, start + Duration::from_secs(3))
                .is_empty()
        );
    }
}

#[test]
fn transport_stop_freezes_tail_deadlines_and_restart_keeps_them() {
    let (start, mut clock, mut planner) = rig(0.0, 600.0);
    let stop = start + Duration::from_secs(2);
    // Dispatch what actually left before Stop, rather than speculative future events.
    for event in planner
        .plan(&clock, start)
        .into_iter()
        .filter(|e| e.at < stop)
    {
        planner.consume(&event);
    }
    planner.stop_transport(&clock, stop);
    clock.stop(stop);
    assert!(planner.sounding_tracks().is_empty());
    let tail = planner.plan(&clock, stop);
    assert_eq!(sends(&tail, 1).len(), 3);
    assert_eq!(sends(&tail, 1)[0].at, start + Duration::from_millis(2100));
    assert_eq!(sends(&tail, 1)[2].at, start + Duration::from_millis(2350));
    clock.rewind();
    // A stopped-transport launch resets the musical clock to zero.
    planner
        .launch(TrackId(1), 1, &mut clock, stop + Duration::from_millis(50))
        .unwrap();
    let events = planner.plan(&clock, stop);
    assert_eq!(sends(&events, 1).len(), 3);
    assert!(sends(&events, 1).last().unwrap().at < sends(&events, 2)[0].at);
    for event in events {
        planner.consume(&event);
    }
    assert!(planner.voices.iter().all(|v| v.id != 1));
}

#[test]
fn mixed_outputs_each_keep_their_shifted_stream() {
    let (start, mut clock, mut planner) = rig(200.0, 400.0);
    let mut project = (*planner.project).clone();
    let mut early = project.outputs[0].clone();
    early.id = TrackId(202);
    early.delay_ms = -400.0;
    project.outputs.push(early);
    project.tracks[0].route = Route::Outputs(vec![TrackId(201), TrackId(202)]);
    // Configure before launch so the early output includes the first overdue send.
    planner = Planner::new(project, Duration::from_secs(5)).unwrap();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    planner
        .launch(
            TrackId(1),
            1,
            &mut clock,
            start + Duration::from_millis(100),
        )
        .unwrap();
    let events = planner.plan(&clock, start);
    for output_id in [TrackId(201), TrackId(202)] {
        let routed: Vec<_> = events
            .iter()
            .filter(
                |e| matches!(&e.action, PlannedAction::Send { output, .. } if *output == output_id),
            )
            .collect();
        assert_eq!(routed.iter().filter(|e| e.voice == 1).count(), 12);
        let last_old = routed.iter().rfind(|e| e.voice == 1).unwrap();
        let first_new = routed.iter().find(|e| e.voice == 2).unwrap();
        assert!(last_old.at < first_new.at);
    }
}

#[test]
fn tempo_change_after_cutoff_keeps_tail_and_retirement_deadlines() {
    let (start, mut clock, mut planner) = rig(0.0, 600.0);
    planner
        .request_stop(TrackId(1), &clock, start + Duration::from_millis(100))
        .unwrap();
    let change = start + Duration::from_millis(2050);
    for event in planner
        .plan(&clock, start)
        .into_iter()
        .filter(|e| e.at <= change)
    {
        planner.consume(&event);
    }
    assert!(planner.set_tempo(&mut clock, Tempo::new(60.0).unwrap(), change));
    let events = planner.plan(&clock, change);
    assert_eq!(sends(&events, 1).len(), 3);
    assert_eq!(
        events.last().unwrap().at,
        start + Duration::from_millis(2600)
    );
    for event in events {
        assert!(planner.consume(&event).is_empty());
    }
    assert!(planner.voices.is_empty());
}

#[test]
fn relaunch_during_a_stopped_tail_keeps_its_pending_sends() {
    let (start, mut clock, mut planner) = rig(0.0, 600.0);
    planner
        .request_stop(TrackId(1), &clock, start + Duration::from_millis(100))
        .unwrap();
    let now = start + Duration::from_millis(2050);
    for event in planner
        .plan(&clock, start)
        .into_iter()
        .filter(|e| e.at <= now)
    {
        planner.consume(&event);
    }
    planner
        .set_global_quantisation(GlobalQuantisation::None)
        .unwrap();
    planner.launch(TrackId(1), 1, &mut clock, now).unwrap();
    let events = planner.plan(&clock, now);
    assert_eq!(sends(&events, 1).len(), 3);
    assert!(sends(&events, 1).last().unwrap().at < sends(&events, 2)[0].at);
}
