//! complete-clusters-before-frames, gate 1 (proposal revision 3): tests 8–14 on the simulated
//! worker (`cluster_sim`) and the planner, for what joins, outlasts or hides a cluster.
use super::cluster_sim::*;
use super::*;
use crate::model::*;

/// The grid frame on A whose tick is `tick` after the start.
fn frame_at(sim: &Sim, tick: Duration) -> Frame {
    sim.grid_frames(A)
        .into_iter()
        .find(|f| near(f.tick, sim.start + tick))
        .unwrap_or_else(|| panic!("no frame at {tick:?}: {:?}", sim.grid_frames(A)))
}

/// Instants sent, as offsets from the transport's start.
fn sent_after(sim: &Sim, manual: bool) -> Vec<Duration> {
    sim.sent
        .iter()
        .filter(|s| s.manual == manual)
        .map(|s| s.at - sim.start)
        .collect()
}

/// A track on A with one hit at each of `ticks` (at 120 BPM a tick is ~0.52 ms) in a bar-long
/// looping clip.
fn hits_at(id: u64, ticks: &[u64]) -> Track {
    clip_track(id, A, 0.0, 3840, ticks)
}

/// Test 8: a cluster that straddles the plan's horizon. The frame waits through the
/// continuation the next plan finds, and every unconsumed clip event due by E is in `pending` at
/// every pass.
#[test]
fn a_cluster_straddling_the_horizon_is_waited_for_through_the_replan() {
    // Six hits ~0.52 ms apart from 500 ms; a 2 ms lookahead plans them in two parts.
    let project = project(
        120.0,
        &[A],
        vec![
            hits_at(1, &[960, 961, 962, 963, 964, 965]),
            ramp_track(3840),
        ],
    );
    let tick = MS(500) - US(200);
    let mut sim = Sim::new(project, MS(2), lead_for(tick)).launched();
    sim.check_visibility = true;
    sim.run_until(sim.start + MS(520));
    let hits = sent_after(&sim, false);
    let cluster: Vec<_> = hits
        .iter()
        .filter(|at| **at >= MS(500) && **at < MS(510))
        .collect();
    assert_eq!(cluster.len(), 6, "{hits:?}");
    // A plan between the first and last hit found the rest.
    let replans = sim
        .plans
        .iter()
        .filter(|(at, _)| *at > sim.start + *cluster[0] && *at < sim.start + *cluster[5])
        .count();
    assert!(replans >= 1, "{:?}", sim.plans);
    let frame = frame_at(&sim, tick);
    assert!(
        frame.start > sim.start + *cluster[5],
        "{frame:?} {cluster:?}"
    );
    assert!(frame.start < sim.start + tick + CAP);
    assert!(sim.splits().is_empty(), "{:?}", sim.splits());
    assert!(sim.overheld().is_empty());
}

/// A track on A with no clip hits, routed for manual hits `delay_ms` after they're fired.
fn manual_track(id: u64, delay_ms: f64) -> Track {
    clip_track(id, A, delay_ms, 3840, &[])
}

/// The event of `track`, to fire.
fn event_of(sim: &Sim, track: u64) -> EventRef {
    let track = sim.planner.project.track(TrackId(track)).unwrap();
    let target = track.devices[0].target().unwrap();
    EventRef {
        device: track.devices[0].id,
        key: target.events[0].key.clone(),
    }
}

/// Arm c's clips on A (hits 0.137–0.419 ms after each sixteenth) and a ramp, with manual tracks
/// at `delays`, the tick 700 µs before the 250 ms sixteenth.
fn with_manual(delays: &[f64]) -> (Sim, Duration) {
    let mut tracks: Vec<_> = CLIP_DELAYS_MS
        .iter()
        .enumerate()
        .map(|(i, &delay)| clip_track(i as u64 + 1, A, delay, 3840, &sixteenths()))
        .collect();
    tracks.push(ramp_track(3840));
    tracks.extend(
        delays
            .iter()
            .enumerate()
            .map(|(i, &delay)| manual_track(i as u64 + 20, delay)),
    );
    let tick = MS(250) - US(700);
    let sim = Sim::new(project(120.0, &[A], tracks), MS(20), lead_for(tick)).launched();
    (sim, tick)
}

/// Fire every manual track at 250.3 ms, during the cluster (the worker holds the command until
/// the cluster's hits due within 2 ms have gone).
fn fire_during_the_cluster(sim: &mut Sim, tracks: usize) {
    sim.run_until(sim.start + MS(250) + US(300));
    for track in 0..tracks as u64 {
        let event = event_of(sim, track + 20);
        sim.send(SchedulerCommand::FireEvent(TrackId(track + 20), event));
        sim.pass();
    }
}

/// Test 9a: a manual hit queued during the deferral joins the chain; the frame waits for it.
#[test]
fn a_manual_hit_joining_the_chain_is_waited_for() {
    let (mut sim, tick) = with_manual(&[0.9]);
    fire_during_the_cluster(&mut sim, 1);
    sim.run_until(sim.start + MS(260));
    let manual = sent_after(&sim, true);
    assert_eq!(manual.len(), 1);
    // Fired (arrived) at 250.3 ms, due 0.9 ms later.
    let due = sim.sent.iter().find(|s| s.manual).unwrap().due - sim.start;
    assert!(
        due >= MS(250) + US(1_200) && due < MS(250) + US(1_250),
        "{due:?}"
    );
    let frame = frame_at(&sim, tick);
    assert!(frame.start > sim.start + manual[0], "{frame:?} {manual:?}");
    assert!(sim.splits().is_empty(), "{:?}", sim.splits());
    assert!(sim.overheld().is_empty());
}

/// Test 9b: manual hits queued so that the chain ends past T + D: the frame goes at that pass,
/// splitting the cluster there (documented), and never later than T + D. The worker takes one
/// command at a time and stamps its arrival then (held-frame-commands, 2026-10-20: this test
/// first stamped all five fires at posting, which the worker doesn't), holding each fire while a
/// hit is due within 2 ms of its arrival. With ten fires at 0.8 ms delay their hits stay within
/// W of each other, and the one applied after ~252.8 ms puts the chain's end past the cap.
#[test]
fn manual_hits_chaining_past_the_cap_release_the_frame_at_that_pass() {
    let (mut sim, tick) = with_manual(&[0.8; 10]);
    fire_during_the_cluster(&mut sim, 10);
    sim.run_until(sim.start + MS(262));
    let manual = sent_after(&sim, true);
    assert_eq!(manual.len(), 10);
    let frame = frame_at(&sim, tick);
    let cap = sim.start + tick + CAP;
    // Released at the first pass after the command whose hit took the chain past the cap.
    let applied = *sim
        .applied
        .iter()
        .filter(|&&at| at < frame.start)
        .max()
        .unwrap();
    let past: Vec<_> = sim
        .sent
        .iter()
        .filter(|s| s.manual && s.due > cap)
        .collect();
    let first_past = past.iter().map(|s| s.due).min().unwrap();
    assert!(
        frame.start - applied < US(100) && first_past - frame.start < MS(1),
        "{:?} applied {:?} manual {manual:?}",
        frame.start - sim.start,
        applied - sim.start,
    );
    assert!(frame.start < cap);
    assert!(frame.profile.withheld_last.is_some_and(|at| at < CAP));
    assert!(!sim.splits().is_empty(), "the documented split");
    assert!(sim.overheld().is_empty());
}

/// Test 10a: a chain spanning ~5.7 ms from just after T: the frame goes at its first pass.
#[test]
fn a_long_cluster_lets_the_frame_go_at_its_first_pass() {
    let ticks: Vec<_> = (960..972).collect();
    let project = project(120.0, &[A], vec![hits_at(1, &ticks), ramp_track(3840)]);
    let tick = MS(500) - US(100);
    let mut sim = Sim::new(project, MS(20), lead_for(tick)).launched();
    sim.run_until(sim.start + MS(520));
    let frame = frame_at(&sim, tick);
    assert!(
        frame.start < sim.start + MS(500),
        "before the first hit: {frame:?}"
    );
    assert_eq!(frame.profile.withheld_last, None);
    assert!(sim.splits().is_empty());
}

/// Test 10b: a chain that began before T and runs past T + D (~10 ms from 500 ms): the frame
/// goes at its first pass, splitting it (documented).
#[test]
fn a_long_cluster_begun_before_the_tick_is_split_at_the_first_pass() {
    let ticks: Vec<_> = (960..980).collect();
    let project = project(120.0, &[A], vec![hits_at(1, &ticks), ramp_track(3840)]);
    let tick = MS(502);
    let mut sim = Sim::new(project, MS(20), lead_for(tick)).launched();
    sim.run_until(sim.start + MS(520));
    let frame = frame_at(&sim, tick);
    assert!(frame.start < sim.start + tick + US(100), "{frame:?}");
    assert_eq!(frame.profile.withheld_last, None);
    // The split falls at T, between the hits before it and those after; `splits` doesn't count a
    // cluster's part sent before T.
    let hits = sent_after(&sim, false);
    let at = frame.start - sim.start;
    assert!(hits.iter().any(|h| *h >= MS(500) && *h < at));
    assert!(hits.iter().any(|h| *h > at && *h < MS(510)));
}

/// Test 11: hits at most ~0.18 ms apart for 2 s, with 16 ramping parameters. Every tick's frame
/// goes, none held back at or past its cap.
#[test]
fn a_continuous_stream_of_hits_never_starves_the_frames() {
    // Three tracks hitting every tick (~0.52 ms) of a 96-tick loop, 0.17 ms apart.
    let every: Vec<_> = (0..96).collect();
    let tracks = vec![
        clip_track(1, A, 0.0, 96, &every),
        clip_track(2, A, 0.17, 96, &every),
        clip_track(3, A, 0.35, 96, &every),
        ramp_track(3840),
    ];
    let mut sim = Sim::new(project(120.0, &[A], tracks), MS(20), US(300)).launched();
    sim.run_until(sim.start + Duration::from_secs(2));
    let frames: Vec<_> = sim
        .grid_frames(A)
        .into_iter()
        .filter(|f| f.tick < sim.start + Duration::from_secs(2) - PERIOD)
        .collect();
    assert!(frames.len() >= 118, "{} frames", frames.len());
    for pair in frames.windows(2) {
        let step = pair[1].tick - pair[0].tick;
        assert!(step < PERIOD + US(1), "a tick skipped: {pair:?}");
    }
    for frame in &frames {
        assert!(
            frame.profile.withheld_last.is_none_or(|at| at < CAP),
            "{frame:?}"
        );
        assert_eq!(frame.start - frame.tick, frame.profile.released);
        assert!(frame.profile.released < CAP, "{frame:?}");
    }
    assert!(sim.overheld().is_empty());
    assert!(sim.sent.len() > 10_000, "{}", sim.sent.len());
}

/// Test 12: an edit applied while a frame waits for a cluster: `pending` clears and replans, and
/// the decision resumes from the new `pending`, still capped. It arrives 100 µs before the tick,
/// and the worker holds it until the hits due within 2 ms of its arrival have gone. (One arriving
/// after the tick costs the tick its automated values: `held_frame_tests`.)
#[test]
fn a_command_during_the_deferral_replans_and_the_decision_resumes() {
    // Six hits ~0.52 ms apart from 500 ms.
    let mut tracks = vec![hits_at(1, &[960, 961, 962, 963, 964, 965])];
    tracks.extend(every_frame_volumes());
    let project = project(120.0, &[A], tracks);
    let tick = MS(500);
    let mut sim = Sim::new(project, MS(20), lead_for(tick)).launched();
    sim.run_until(sim.start + tick - US(100));
    // The worker's own project: a clone of `project` would move A's destination back off the
    // sim's receiver (held-frame-commands, 2026-10-20).
    let unchanged = (*sim.planner.project).clone();
    sim.send(SchedulerCommand::UpdateProject(unchanged));
    sim.run_until(sim.start + MS(520));
    let applied = *sim.applied.last().unwrap();
    assert!(
        applied > sim.start + tick,
        "held into the deferral: {applied:?}"
    );
    let hits = sent_after(&sim, false);
    let last = *hits.iter().filter(|at| **at < MS(510)).max().unwrap();
    assert!(applied < sim.start + last, "applied mid-cluster");
    assert!(sim.plans.iter().any(|(at, _)| *at >= applied), "replanned");
    let frame = frame_at(&sim, tick);
    assert!(frame.start > sim.start + last, "{frame:?}");
    assert!(frame.start < sim.start + tick + CAP);
    assert!(sim.splits().is_empty(), "{:?}", sim.splits());
    assert!(sim.overheld().is_empty());
}

/// A lifecycle event, as planned.
fn lifecycle(at: Instant) -> DueEvent {
    DueEvent {
        at,
        action: PlannedAction::TrackSilent(TrackId(99)),
        voice: 0,
        cursor: 0,
    }
}

/// Test 12b: `pending` holds only a lifecycle event, and E < now + W. The frame waits and a
/// visibility replan follows; a clip hit it finds within W is waited for.
#[test]
fn a_lifecycle_only_queue_without_visibility_waits_and_replans() {
    // A hit 0.5 ms after the tick at 250 ms.
    let tick = MS(250) - US(500);
    let project = project(120.0, &[A], vec![hits_at(1, &[480]), ramp_track(3840)]);
    let mut sim = Sim::new(project, MS(20), lead_for(tick)).launched();
    sim.run_until(sim.start + tick - US(50));
    // As if the last plan had seen only to 1 ms before the tick, and left a lifecycle event.
    sim.pending = VecDeque::from([lifecycle(sim.start + tick + MS(3))]);
    sim.horizon = sim.start + tick - MS(1);
    let plans = sim.plans.len();
    sim.now = sim.start + tick;
    sim.pass();
    assert!(
        sim.grid_frames(A).iter().all(|f| f.tick < sim.start + tick),
        "held back"
    );
    assert!(near(sim.withheld.last().unwrap().0, sim.start + tick));
    sim.pass();
    assert_eq!(sim.plans.len(), plans + 1);
    assert!(sim.plans[plans].1, "a visibility replan");
    sim.run_until(sim.start + MS(260));
    let frame = frame_at(&sim, tick);
    let hit = sent_after(&sim, false)
        .into_iter()
        .find(|at| *at >= MS(250))
        .unwrap();
    assert!(frame.start > sim.start + hit, "{frame:?} {hit:?}");
}

/// Test 12b, with nothing found: the frame goes once the replan shows nothing due within W.
#[test]
fn a_lifecycle_only_queue_goes_after_a_replan_finds_nothing() {
    let tick = MS(250) - US(500);
    let project = project(120.0, &[A], vec![hits_at(1, &[960]), ramp_track(3840)]);
    let mut sim = Sim::new(project, MS(20), lead_for(tick)).launched();
    sim.run_until(sim.start + tick - US(50));
    sim.pending = VecDeque::from([lifecycle(sim.start + tick + MS(3))]);
    sim.horizon = sim.start + tick - MS(1);
    sim.now = sim.start + tick;
    sim.pass();
    sim.pass();
    sim.pass();
    let frame = frame_at(&sim, tick);
    assert!(sim.plans.iter().any(|(_, visibility)| *visibility));
    assert!(frame.start < sim.start + tick + MS(1), "{frame:?}");
    assert!(frame.profile.withheld_last.is_some());
}

/// Test 12c: replacing a non-empty `pending` with a fresh plan keeps every unconsumed event up to
/// the old horizon exactly once, in order, lifecycle events included.
#[test]
fn a_visibility_replan_neither_loses_nor_duplicates_events() {
    // A non-looping one-beat clip (hits at 0 and twice at 480 ticks, then silence), and a looping
    // copy on track 2, launched together.
    let mut p = super::tests::project(0.0, false);
    let mut looping = super::tests::copy_track(&p.tracks[0], TrackId(2));
    for slot in &mut looping.slots {
        let clip = slot.clip.as_mut().unwrap();
        clip.id.0 += 10;
        clip.looping = true;
    }
    p.tracks.push(looping);
    let start = Instant::now();
    let mut clock = InternalClock::new(p.tempo, p.time_signature);
    let mut planner = Planner::new(p, MS(600)).unwrap();
    planner.launch_scene(SceneId(1), &mut clock, start).unwrap();
    let key = |e: &DueEvent| (e.at, e.voice, e.cursor, format!("{:?}", e.action));
    for consumed in 0..4 {
        for later in [MS(0), MS(100), MS(260), MS(510)] {
            let mut copy = planner.clone();
            let (first, horizon) = copy.plan_until(&clock, start);
            assert!(
                first
                    .iter()
                    .any(|e| !matches!(e.action, PlannedAction::Send { .. })),
                "a lifecycle event is planned: {first:?}"
            );
            for event in &first[..consumed] {
                copy.consume(event);
            }
            let (fresh, _) = copy.plan_until(&clock, start + later);
            let kept: Vec<_> = first[consumed..].iter().map(key).collect();
            let replanned: Vec<_> = fresh.iter().filter(|e| e.at <= horizon).map(key).collect();
            assert_eq!(
                replanned, kept,
                "consumed {consumed}, replanned at {later:?}"
            );
        }
    }
}

/// Test 13: an exact jump at the tick, inside a cluster, still goes at its deadline and before
/// its tied hit; the grid frame's own part waits for the cluster.
#[test]
fn an_exact_jump_inside_a_cluster_goes_at_its_deadline_before_its_tied_hit() {
    // The ramp wraps every beat: an exact jump at 500 ms, tied with a hit (delay 0) and followed
    // by hits 0.137 and 0.251 ms later.
    let tracks = vec![
        clip_track(1, A, 0.0, 3840, &sixteenths()),
        clip_track(2, A, 0.137, 3840, &sixteenths()),
        clip_track(3, A, 0.251, 3840, &sixteenths()),
        ramp_track(960),
    ];
    let jump = MS(500);
    let mut sim = Sim::new(project(120.0, &[A], tracks), MS(20), lead_for(jump)).launched();
    sim.run_until(sim.start + MS(520));
    let frame = sim
        .frames
        .iter()
        .find(|f| f.profile.jump.is_some() && near(f.tick, sim.start + jump))
        .copied()
        .unwrap_or_else(|| panic!("no jump at 500 ms: {:?}", sim.frames));
    // At its deadline, within the simulated worker's 2 µs pass.
    assert!(
        within(frame.start, sim.start + jump, US(3)),
        "at its deadline: {frame:?}"
    );
    let tied = sim
        .sent
        .iter()
        .find(|s| near(s.due, sim.start + jump))
        .unwrap_or_else(|| panic!("no tied hit after the jump: {:?}", sim.sent));
    assert!(tied.at > frame.start);
    assert!(sim.splits().is_empty(), "{:?}", sim.splits());
}

/// Test 14: transport start values at the launch still win their tie with the launch's hits.
#[test]
fn launch_values_still_win_their_tie_with_the_launchs_hits() {
    let mut project = Arm::C.project(120.0);
    project.tracks[0].delay_ms = 0.0;
    let mut sim = Sim::new(project, MS(20), US(5_000)).launched();
    sim.run_until(sim.start + MS(5));
    // The worker plans after the launch command, then dispatches: both go ~80 µs late.
    let start_values = sim
        .frames
        .iter()
        .find(|f| f.tick == sim.start)
        .copied()
        .expect("transport start values");
    let first = sim.sent.iter().min_by_key(|s| s.at).unwrap();
    assert_eq!(first.due, sim.start, "the launch's tied hit");
    assert!(first.at > start_values.start, "{first:?} {start_values:?}");
}
