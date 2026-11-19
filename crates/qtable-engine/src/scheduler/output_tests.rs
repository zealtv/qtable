//! Fan-out timing is independent per output, with one musical voice and follow timer.
use super::*;
use crate::model::*;

fn rig(latencies: [f64; 2], looping: bool) -> (Instant, InternalClock, Planner) {
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut project = super::tests::project(20.0, looping);
    let track = &mut project.tracks[0];
    let mut clip = track.slots[0].clip.take().unwrap();
    for event in &mut clip.events {
        let path = OscAddress::Relative(format!("/{}", event.position.0));
        *event = track.hit(event.position, path, vec![], || DeviceId(1));
    }
    track.slots[0].clip = Some(clip);
    project.outputs[0] = project.outputs[0].clone().with_path("/slow");
    project.outputs[0].delay_ms = latencies[0];
    let mut fast = project.outputs[0].clone();
    fast.id = TrackId(202);
    fast = fast.with_path("/fast");
    fast.delay_ms = latencies[1];
    project.outputs.push(fast);
    project.tracks[0].route = Route::Outputs(vec![TrackId(202), TrackId(201)]);
    let mut planner = Planner::new(project, Duration::from_secs(2)).unwrap();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    (start, clock, planner)
}

fn sends(events: &[DueEvent]) -> Vec<(TrackId, Instant, String)> {
    events
        .iter()
        .filter_map(|e| match &e.action {
            PlannedAction::Send {
                output,
                packet: Some(OscPacket::Message(message)),
            } => Some((*output, e.at, message.addr.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn fanout_crosses_event_boundaries_and_consumes_each_output_once() {
    let (start, clock, mut p) = rig([600.0, -100.0], false);
    let events = p.plan(&clock, start);
    let packets = sends(&events);
    assert_eq!(
        packets.iter().map(|e| e.0).collect::<Vec<_>>(),
        [
            TrackId(202),
            TrackId(202),
            TrackId(202),
            TrackId(201),
            TrackId(201),
            TrackId(201)
        ]
    );
    assert_eq!(
        packets[0],
        (
            TrackId(202),
            start - Duration::from_millis(80),
            "/fast/0".into()
        )
    );
    assert_eq!(
        packets[3],
        (
            TrackId(201),
            start + Duration::from_millis(620),
            "/slow/0".into()
        )
    );
    assert_eq!(packets[5].1, start + Duration::from_millis(870));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e.action, PlannedAction::TrackSilent(_)))
            .count(),
        1
    );
    assert_eq!(p.voices.len(), 1);
    for e in &events {
        p.consume(e);
    }
    assert!(p.plan(&clock, start).is_empty());
}

#[test]
fn route_order_breaks_equal_deadlines_and_absolute_addresses_bypass_prefixes() {
    let (start, clock, mut p) = rig([0.0, 0.0], false);
    let mut project = (*p.project).clone();
    let hit = project
        .hit(
            TrackId(1),
            BeatTime::ZERO,
            OscAddress::Absolute("/absolute".into()),
            vec![],
        )
        .unwrap();
    project.tracks[0].slots[0].clip.as_mut().unwrap().events[1].definition = hit.definition;
    p.update_project(project, &clock, start).unwrap();
    let packets = sends(&p.plan(&clock, start));
    assert_eq!(packets[0].0, TrackId(202));
    assert_eq!(packets[1].0, TrackId(201));
    assert_eq!(packets[0].2, "/absolute");
    assert_eq!(packets[1].2, "/absolute");
}

#[test]
fn latency_sets_lookahead_and_tempo_preserves_each_elapsed_send_deadline() {
    let (start, mut clock, mut p) = rig([600.0, 100.0], false);
    p.set_tempo(
        &mut clock,
        Tempo::new(60.0).unwrap(),
        start + Duration::from_millis(50),
    );
    let packets = sends(&p.plan(&clock, start + Duration::from_millis(50)));
    assert!(packets.contains(&(
        TrackId(202),
        start + Duration::from_millis(120),
        "/fast/0".into()
    )));
    assert!(packets.contains(&(
        TrackId(201),
        start + Duration::from_millis(620),
        "/slow/0".into()
    )));
    let mut project = (*p.project).clone();
    project.tracks[0].delay_ms = -200.0;
    project.outputs[1].delay_ms = -800.0;
    let p = Planner::new(project.clone(), Duration::from_millis(12)).unwrap();
    assert_eq!(p.lookahead(), Duration::from_secs(1));
    project.outputs[1].delay_ms = -800.01;
    assert!(project.validate().is_err());
}

#[test]
fn disabled_outputs_and_empty_routes_keep_musical_playback_without_sends() {
    let (start, clock, mut p) = rig([0.0, 0.0], true);
    let mut project = (*p.project).clone();
    project.outputs[1].mixer.active = false;
    p.update_project(project.clone(), &clock, start).unwrap();
    let packets = sends(&p.plan(&clock, start));
    assert!(packets.iter().all(|e| e.0 == TrackId(201)));
    project.tracks[0].route = Route::empty();
    p.update_project(project.clone(), &clock, start).unwrap();
    assert!(p.plan(&clock, start).is_empty());
    assert_eq!(p.voices.len(), 1);
    project.tracks[0].route = Route::single(TrackId(201));
    p.update_project(project, &clock, start + Duration::from_secs(10))
        .unwrap();
    assert!(
        sends(&p.plan(&clock, start + Duration::from_secs(10)))
            .iter()
            .all(|e| e.1 >= start + Duration::from_secs(10))
    );
}

#[test]
fn editing_one_output_preserves_other_outputs_elapsed_tempo_deadlines() {
    let (start, mut clock, mut p) = rig([600.0, -100.0], true);
    for event in p.plan(&clock, start) {
        if event.at <= start {
            p.consume(&event);
        }
    }
    p.set_tempo(
        &mut clock,
        Tempo::new(60.0).unwrap(),
        start + Duration::from_millis(50),
    );
    let mut project = (*p.project).clone();
    project.outputs[1].delay_ms = -50.0;
    p.update_project(project, &clock, start + Duration::from_millis(100))
        .unwrap();
    assert!(
        sends(&p.plan(&clock, start + Duration::from_millis(100))).contains(&(
            TrackId(201),
            start + Duration::from_millis(620),
            "/slow/0".into()
        ))
    );
}

#[test]
fn fanout_catchup_skips_backlog_per_output_and_reports_packet_count() {
    let (start, clock, mut p) = rig([600.0, -100.0], true);
    let now = start + Duration::from_secs(10);
    let bound = Duration::from_millis(50);
    let expected = sends(&p.plan(&clock, now))
        .iter()
        .filter(|(_, at, _)| *at < now - bound)
        .count();
    assert_eq!(p.skip_stale_sends(&clock, now, bound), expected as u64);
    let remaining = sends(&p.plan(&clock, now));
    assert!(!remaining.is_empty());
    assert!(remaining.iter().all(|(_, at, _)| *at >= now - bound));
}

#[test]
fn main_global_delay_shifts_its_destination_once_and_unset_main_is_skipped() {
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut project = super::tests::project(20.0, false);
    project.main.delay_ms = 40.0;
    project.outputs[0].delay_ms = 10.0;
    let main = project.main.id;
    project.tracks[0].route = Route::Outputs(vec![main, TrackId(201)]);
    let mut planner = Planner::new(project.clone(), Duration::from_secs(2)).unwrap();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let packets = sends(&planner.plan(&clock, start));
    assert_eq!(packets[0].0, main);
    assert_eq!(packets[0].1, start + Duration::from_millis(60));
    assert_eq!(packets[1].0, TrackId(201));
    assert_eq!(packets[1].1, start + Duration::from_millis(70));
    assert_eq!(planner.tail_offset_ms(TrackId(1)), 70.0);
    project.main.output = None;
    let mut planner = Planner::new(project, Duration::from_secs(2)).unwrap();
    planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let packets = sends(&planner.plan(&clock, start));
    assert_eq!(packets.len(), 3);
    assert!(packets.iter().all(|p| p.0 == TrackId(201)));
}
