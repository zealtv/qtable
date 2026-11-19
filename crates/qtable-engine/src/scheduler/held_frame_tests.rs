//! hold-commands-behind-earlier-frames (validation revision 2, 2026-10-20): a command waits while
//! an ordinary frame for a tick before its arrival is owed or part-way out, ordinary frames for
//! later ticks yield to a waiting command, and a fire neither waits for frames nor re-syncs.
//! Tests A1–A15 on the simulated worker (`cluster_sim`), every frame's values recorded; A16 is
//! gate 1 itself. Strict: no loss is allowed here.
//!
//! What they guard (held-frame-commands, 2026-10-20): every command re-syncs automation at its
//! arrival, and `Automation::sync` keeps no state before it, so a frame still owed for an earlier
//! tick would sample no automation, and an edit's slot would have no value eligible by the tick.
use super::cluster_sim::*;
use super::*;
use crate::model::*;

/// The tick the cluster rule holds: hits ~0.52 ms apart from 500 ms follow it.
const T: Duration = Duration::from_millis(500);
/// The ramp's period (one 3840-tick bar at 120 BPM): a sent ramp value × this is its sample time.
const RAMP: f32 = 2000.0;
/// At 120 BPM a clip tick is 1/1920 s.
const CLIP_TICK: f64 = 1.0 / 1920.0;

/// On A: hits at `ticks`, the 16-parameter ramp (`/parameter/p*`), 16 Every Frame volumes
/// (`/parameter/v*`, tracks 11–26) and track 41 (no clip hits, to fire). On B: track 40 (no clip
/// hits, to fire).
fn workload(ticks: &[u64]) -> Project {
    let mut tracks = vec![clip_track(1, A, 0.0, 3840, ticks)];
    tracks.push(ramp_track(3840));
    tracks.extend(every_frame_volumes());
    tracks.push(clip_track(40, B, 0.0, 3840, &[]));
    tracks.push(clip_track(41, A, 0.0, 3840, &[]));
    project(120.0, &[A, B], tracks)
}

/// Six hits from 500 ms, ~0.52 ms apart: the frame at T waits ~2.6 ms for them.
const CLUSTER: [u64; 6] = [960, 961, 962, 963, 964, 965];

fn sim_at(project: Project, tick: Duration) -> Sim {
    let mut sim = Sim::new(project, MS(20), lead_for(tick)).launched();
    sim.record_values = true;
    sim
}

fn sim(ticks: &[u64]) -> Sim {
    sim_at(workload(ticks), T)
}

/// The project as the worker holds it, destinations included.
fn current(sim: &Sim) -> Project {
    (*sim.planner.project).clone()
}

fn fire(sim: &Sim, track: u64) -> SchedulerCommand {
    let track = sim.planner.project.track(TrackId(track)).unwrap();
    let target = track.devices[0].target().unwrap();
    SchedulerCommand::FireEvent(
        track.id,
        EventRef {
            device: track.devices[0].id,
            key: target.events[0].key.clone(),
        },
    )
}

fn edit_v0(sim: &Sim, value: f64) -> SchedulerCommand {
    let mut project = current(sim);
    let track = project.tracks.iter_mut().find(|t| t.id == TrackId(11));
    track.unwrap().mixer_device.volume.value = value;
    SchedulerCommand::UpdateProject(project)
}

/// The grid frame on `output` at `tick` after the start.
fn frame(sim: &Sim, output: TrackId, tick: Duration) -> Frame {
    sim.grid_frames(output)
        .into_iter()
        .find(|f| near(f.tick, sim.start + tick))
        .unwrap_or_else(|| panic!("no frame on {output:?} at {tick:?}"))
}

/// What the grid frame on A at `tick` sent: its message count, the ramp's sample time (ms after
/// the start) and v0. None if no grid frame went for that tick.
fn at(sim: &Sim, tick: Duration) -> Option<(usize, Option<f32>, Option<f32>)> {
    let values = sim.sent_values(A, tick)?;
    let find = |address: &str| values.iter().find(|(a, _)| a == address).map(|v| v.1);
    Some((
        values.len(),
        find("/parameter/p0").map(|v| v * RAMP),
        find("/parameter/v0"),
    ))
}

fn sampled_at(sample: Option<f32>, tick: Duration) -> bool {
    sample.is_some_and(|ms| (ms - tick.as_secs_f32() * 1000.0).abs() < 0.01)
}

/// The frame on A at `tick` went complete: all 32 messages, every ramp parameter sampled at its
/// tick.
fn complete(sim: &Sim, tick: Duration) -> bool {
    let Some(values) = sim.sent_values(A, tick) else {
        return false;
    };
    let ramp: Vec<_> = values
        .iter()
        .filter(|(address, _)| address.starts_with("/parameter/p"))
        .collect();
    values.len() == 32
        && ramp.len() == 16
        && ramp.iter().all(|(_, v)| sampled_at(Some(v * RAMP), tick))
}

/// Posting-to-application and arrival-to-application of every logged command, for the notes.
fn latencies(sim: &Sim) -> Vec<(Duration, Duration)> {
    sim.log
        .iter()
        .map(|c| (c.applied - c.posted, c.applied - c.arrived))
        .collect()
}

/// A labelled command to send, built from the sim's state.
type Command = fn(&Sim) -> SchedulerCommand;

/// A1: each non-fire command kind arriving after a held tick waits for that frame: the frame goes
/// complete, the command applies after its last packet, and the next frame is complete.
#[test]
fn a1_a_command_after_a_held_tick_waits_for_its_frame() {
    let commands: [(&str, Command); 5] = [
        ("unchanged UpdateProject", |s| {
            SchedulerCommand::UpdateProject(current(s))
        }),
        ("SetGlobalQuantisation, unchanged", |s| {
            SchedulerCommand::SetGlobalQuantisation(s.planner.project.global_quantisation)
        }),
        ("SetFollowActionsEnabled, unchanged", |s| {
            SchedulerCommand::SetFollowActionsEnabled(s.planner.project.follow_actions_enabled)
        }),
        ("Play while playing", |_| SchedulerCommand::Play),
        ("an edit", |s| edit_v0(s, 0.5)),
    ];
    for (label, command) in commands {
        let mut sim = sim(&CLUSTER);
        sim.run_until(sim.start + T + US(500));
        let command = command(&sim);
        sim.send(command);
        sim.run_until(sim.start + MS(540));
        let held = frame(&sim, A, T);
        let applied = sim.log.last().unwrap();
        assert!(applied.applied >= held.end, "{label}: {applied:?} {held:?}");
        assert!(complete(&sim, T), "{label}: {:?}", at(&sim, T));
        assert!(
            complete(&sim, T + PERIOD),
            "{label}: {:?}",
            at(&sim, T + PERIOD)
        );
        let v0 = if label == "an edit" { 0.5 } else { 1.0 };
        assert_eq!(
            at(&sim, T).unwrap().2,
            Some(1.0),
            "{label}: pre-command at T"
        );
        assert_eq!(at(&sim, T + PERIOD).unwrap().2, Some(v0), "{label}");
        assert!(
            sim.overheld().is_empty() && sim.splits().is_empty(),
            "{label}"
        );
    }
}

/// A1 under live timing, as in the worker: after a frame the stage is dirty and shows no deadline
/// until replanned, which waits for a 1 ms gap in clip events. Hits every clip tick (~0.52 ms) from
/// 481.8 ms to 506.3 ms leave no gap: the frame for T − P goes inside them, so at T the stage is
/// dirty. A command at T + 0.5 ms is let go by `Hold`'s clip-event rule while hits still come; the
/// command path plans the stage (case (ii)) and waits for T's frame.
#[test]
fn a1_a_dirty_stage_still_holds_the_command() {
    let stream: Vec<u64> = (925..=972).collect();
    let mut sim = sim(&stream);
    sim.live_timing = true;
    sim.run_until(sim.start + T + US(500));
    sim.send(SchedulerCommand::UpdateProject(current(&sim)));
    sim.run_until(sim.start + MS(560));
    assert!(complete(&sim, T - PERIOD));
    let applied = sim.log[0].applied;
    assert!(
        applied < sim.start + MS(506),
        "let go inside the hits: {applied:?}"
    );
    assert!(applied >= frame(&sim, A, T).end);
    assert!(complete(&sim, T), "{:?}", at(&sim, T));
}

/// A2: a command arriving before T applies first, as before; T carries its effect.
#[test]
fn a2_a_command_arriving_before_the_held_tick_applies_first() {
    let mut sim = sim(&CLUSTER);
    sim.run_until(sim.start + T - US(100));
    let edit = edit_v0(&sim, 0.5);
    sim.send(edit);
    sim.run_until(sim.start + MS(540));
    assert!(sim.log[0].applied < frame(&sim, A, T).start);
    assert!(complete(&sim, T));
    assert_eq!(at(&sim, T).unwrap().2, Some(0.5));
}

/// A3: later frames yield. An edit arriving 0.2 ms before T waits for clip hits 1.0–1.6 ms after
/// T (beyond W, so the cluster rule alone would let the frame go at T). The frame for T, a later
/// tick than the arrival, waits for it and carries the edit.
#[test]
fn a3_a_later_frame_yields_to_a_waiting_command() {
    let mut sim = sim(&[962, 963]);
    sim.run_until(sim.start + T - US(200));
    let edit = edit_v0(&sim, 0.5);
    sim.send(edit);
    sim.run_until(sim.start + MS(540));
    let applied = sim.log[0].applied;
    assert!(
        applied > sim.start + T + MS(1),
        "waited for the hits: {applied:?}"
    );
    let yielded = frame(&sim, A, T);
    assert!(yielded.start > applied, "{yielded:?}");
    assert!(complete(&sim, T));
    assert_eq!(at(&sim, T).unwrap().2, Some(0.5));
}

/// A cluster just after each of `ticks` grid ticks from T (hits 0.5–1.6 ms after each).
fn clusters_after_each_tick(ticks: u64) -> Vec<u64> {
    // At 120 BPM a grid period is exactly 32 clip ticks.
    (0..ticks)
        .flat_map(|k| [961, 962, 963].map(|t| t + 32 * k))
        .collect()
}

/// A3, later ticks don't extend the wait: with every tick held, a command after T applies after
/// T's frame and before the next tick's frame starts.
#[test]
fn a3_later_held_ticks_dont_extend_the_wait() {
    let mut sim = sim(&clusters_after_each_tick(4));
    sim.run_until(sim.start + T + US(300));
    sim.send(SchedulerCommand::UpdateProject(current(&sim)));
    sim.run_until(sim.start + T + PERIOD * 4);
    let applied = sim.log[0].applied;
    assert!(applied >= frame(&sim, A, T).end);
    assert!(applied < frame(&sim, A, T + PERIOD).start);
    for k in 0..4 {
        assert!(complete(&sim, T + PERIOD * k), "tick {k}");
    }
}

/// Tracks on `output` sending their volumes Every Frame, from track id `first`.
fn volumes(output: TrackId, first: u64, count: u64) -> impl Iterator<Item = Track> {
    (0..count).map(move |i| {
        let mut track = Track::new(TrackId(first + i), "parameter", 1);
        track.route = Route::single(output);
        let volume = &mut track.mixer_device.volume.output;
        volume.enabled = true;
        volume.address = format!("/volume/{}", first + i);
        volume.send_mode = SendMode::EveryFrame;
        track
    })
}

/// A4: staggered outputs. A at 60 Hz holds its frame for 516.67 ms (hits to +3.65 ms); B at 50 Hz
/// has a tick at 520 ms, after the command's arrival (+0.5 ms) and inside the hold. The command
/// waits for A's frame only; B's frame starts only after the command applies.
#[test]
fn a4_staggered_outputs_wait_for_earlier_frames_only() {
    let tick = MS(516) + US(666) + NS(667);
    let mut project = workload(&[993, 994, 995, 996, 997, 998, 999]);
    project.tracks.extend(volumes(B, 60, 4));
    project.outputs[1].output.as_mut().unwrap().stage.frame_rate = 50.0;
    let mut sim = sim_at(project, MS(500));
    sim.run_until(sim.start + tick + US(500));
    sim.send(SchedulerCommand::UpdateProject(current(&sim)));
    sim.run_until(sim.start + MS(560));
    let applied = sim.log[0].applied;
    let a = frame(&sim, A, tick);
    let b = frame(&sim, B, MS(520));
    assert!(a.end <= applied, "A {a:?} applied {applied:?}");
    assert!(b.start > applied, "B {b:?} applied {applied:?}");
    assert!(sim.sent_values(B, MS(520)).is_some_and(|v| v.len() == 4));
    assert!(complete(&sim, tick));
}

/// A5: long transmissions. Frames go a packet per pass; A carries 200 more volumes (~3.6 ms of
/// packets) and B has hits due part-way through A's frame, beyond W after its tick. B's hits go between A's packets; a
/// command posted during the frame is taken after its last packet; the frame arrives whole.
#[test]
fn a5_a_long_frame_goes_whole_before_the_command() {
    let mut project = workload(&[]);
    project.tracks.extend(volumes(A, 100, 200));
    // 1.56–2.6 ms after T: beyond W, so A's frame isn't held for them.
    project
        .tracks
        .push(clip_track(42, B, 0.0, 3840, &[963, 964, 965]));
    let mut sim = Sim::new(project, MS(20), lead_for(T)).launched();
    sim.record_values = true;
    sim.resumable = true;
    sim.run_until(sim.start + T + US(200));
    sim.send(SchedulerCommand::UpdateProject(current(&sim)));
    sim.run_until(sim.start + MS(540));
    let long = frame(&sim, A, T);
    assert!(long.end - long.start > MS(3), "{long:?}");
    let between = sim
        .sent
        .iter()
        .filter(|s| s.at > long.start && s.at < long.end)
        .count();
    assert_eq!(between, 3, "B's hits between A's packets");
    let command = sim.log[0];
    assert!(command.arrived >= long.end && command.applied >= long.end);
    assert_eq!(sim.sent_values(A, T).unwrap().len(), 232);
}

/// A6: queued commands, an edit, a Stop and a fire posted together during one hold, apply in
/// order, each waiting only for frames before its own arrival, and no later frame starts while a
/// non-fire one waits.
#[test]
fn a6_queued_commands_apply_in_order() {
    let mut sim = sim(&CLUSTER);
    sim.run_until(sim.start + T + US(300));
    let (edit, fire) = (edit_v0(&sim, 0.5), fire(&sim, 41));
    sim.send(edit);
    sim.send(SchedulerCommand::Stop);
    sim.send(fire);
    sim.run_until(sim.start + MS(540));
    assert_eq!(sim.log.len(), 3);
    assert_eq!(
        sim.log.iter().map(|c| c.fire).collect::<Vec<_>>(),
        [false, false, true]
    );
    for pair in sim.log.windows(2) {
        assert!(pair[1].arrived >= pair[0].applied, "FIFO: {pair:?}");
    }
    for command in sim.log.iter().filter(|c| !c.fire) {
        for f in sim
            .frames
            .iter()
            .filter(|f| !f.start_send && f.tick >= command.arrived)
        {
            assert!(
                f.start > command.applied,
                "{f:?} started while {command:?} waited"
            );
        }
    }
    assert!(complete(&sim, T));
    eprintln!("A6 posting / arrival to application: {:?}", latencies(&sim));
}

/// A7: a fire behind an already-held edit doesn't overtake it; its hit's due time comes from its
/// own arrival.
#[test]
fn a7_a_fire_behind_a_held_edit_waits() {
    let mut sim = sim(&CLUSTER);
    sim.run_until(sim.start + T + US(300));
    let (edit, fire) = (edit_v0(&sim, 0.5), fire(&sim, 41));
    sim.send(edit);
    sim.pass();
    sim.send(fire);
    sim.run_until(sim.start + MS(540));
    let (edit, fire) = (sim.log[0], sim.log[1]);
    assert!(!edit.fire && fire.fire);
    assert!(fire.arrived >= edit.applied, "{edit:?} {fire:?}");
    let hit = sim.sent.iter().find(|s| s.manual).unwrap();
    assert_eq!(hit.due, fire.arrived);
    eprintln!("A7 posting / arrival to application: {:?}", latencies(&sim));
}

/// A8: Stop after a held tick: the frame goes complete with pre-Stop values, then Stop; no later
/// frame carries running values.
#[test]
fn a8_stop_after_a_held_tick() {
    let mut sim = sim(&CLUSTER);
    sim.run_until(sim.start + T + US(500));
    sim.send(SchedulerCommand::Stop);
    sim.run_until(sim.start + MS(560));
    let stop = sim.log[0];
    assert!(stop.applied >= frame(&sim, A, T).end);
    assert!(complete(&sim, T));
    for (f, values) in sim.frames.iter().zip(&sim.values) {
        if f.output == A && f.tick > stop.arrived {
            assert!(
                !values.iter().any(|(a, _)| a.starts_with("/parameter/v")),
                "{f:?}"
            );
        }
    }
}

/// A9: a destination change after a held tick: the frame goes complete (all 32 messages, all 16
/// ramp slots sampled) before the change applies, so at the old destination; frames follow at the
/// new one.
#[test]
fn a9_a_destination_change_after_a_held_tick() {
    let mut sim = sim(&CLUSTER);
    sim.record_values = false;
    sim.run_until(sim.start + T + US(500));
    let mut moved = current(&sim);
    moved.outputs[0].destination_mut().unwrap().port = 9;
    sim.send(SchedulerCommand::UpdateProject(moved));
    sim.run_until(sim.start + MS(600));
    let held = frame(&sim, A, T);
    assert!(sim.log[0].applied >= held.end);
    assert_eq!(held.profile.sampled, 16, "{held:?}");
    let messages: usize = sim
        .frames
        .iter()
        .filter(|f| f.output == A && near(f.tick, sim.start + T))
        .count();
    assert_eq!(messages, 1);
    assert!(
        sim.grid_frames(A)
            .iter()
            .any(|f| f.tick > sim.start + T + PERIOD),
        "frames follow at the new destination"
    );
}

/// A10: edits every 2 ms over 10 held ticks: every tick goes complete, with v0 as of the last edit
/// arriving before it.
#[test]
fn a10_repeated_edits_over_held_ticks() {
    let mut sim = sim(&clusters_after_each_tick(10));
    let mut value = 0.0;
    sim.run_until(sim.start + T + US(300));
    let mut values = Vec::new();
    while sim.now < sim.start + T + PERIOD * 9 + MS(2) {
        value = ((value + 0.01) * 100.0_f64).round() / 100.0 % 1.0;
        values.push(value);
        let edit = edit_v0(&sim, value);
        sim.send(edit);
        let next = sim.now + MS(2);
        sim.run_until(next);
    }
    sim.run_until(sim.start + T + PERIOD * 12);
    for k in 0..12 {
        let tick = T + PERIOD * k;
        assert!(complete(&sim, tick), "tick {k}: {:?}", at(&sim, tick));
        let before = sim
            .log
            .iter()
            .zip(&values)
            .rfind(|(c, _)| c.arrived < sim.start + tick)
            .map_or(1.0, |(_, v)| *v as f32);
        assert_eq!(at(&sim, tick).unwrap().2, Some(before), "tick {k}");
    }
    let (posting, arrival): (Vec<_>, Vec<_>) = latencies(&sim).into_iter().unzip();
    eprintln!(
        "A10 max posting / arrival to application: {:?} / {:?}",
        posting.iter().max(),
        arrival.iter().max()
    );
}

/// A11: a fire at the head after a held tick applies without the frame wait, joins the chain, and
/// keeps the tick's automation. Its hit (no delay) goes once `Hold`'s clip-event condition lets
/// the fire apply; the lateness is reported against the previous code in the notes.
#[test]
fn a11_a_fire_at_the_head_doesnt_wait_for_frames() {
    let mut sim = sim(&CLUSTER);
    sim.run_until(sim.start + T + US(300));
    let fire = fire(&sim, 41);
    sim.send(fire);
    sim.run_until(sim.start + MS(540));
    let held = frame(&sim, A, T);
    assert!(sim.log[0].applied < held.start, "{:?} {held:?}", sim.log[0]);
    let hit = sim.sent.iter().find(|s| s.manual).unwrap();
    assert!(held.start > hit.at, "the frame waited for the fire's hit");
    assert!(complete(&sim, T));
    eprintln!("A11 manual hit lateness {:?}", hit.at - hit.due);
}

/// `workload` with a second scene: track 50 on A (a hit at 0) and the ramp track hold clips there;
/// every other track an empty slot.
fn with_second_scene(ticks: &[u64], quantisation: GlobalQuantisation) -> Project {
    let mut project = workload(ticks);
    project.global_quantisation = quantisation;
    project.scenes.push(Scene {
        id: SceneId(2),
        name: "two".into(),
        follow_actions: SceneFollowActions::default(),
    });
    let mut launched = clip_track(50, A, 0.0, 3840, &[0]);
    launched.slots.insert(0, launched.slots[0].clone());
    launched.slots[0].clip = None;
    for track in &mut project.tracks {
        let mut second = track.slots[0].clone();
        if track.id == TrackId(500) {
            let clip = second.clip.as_mut().unwrap();
            clip.id = ClipId(501);
        } else {
            second.clip = None;
        }
        track.slots.push(second);
    }
    project.tracks.push(launched);
    project
}

/// The lateness of the first hit a launch planned at `launch` (track 50's), and of the frame
/// carrying the relaunched ramp's exact jump there, if one went as its own frame.
fn launch_timing(sim: &Sim, launch: Instant) -> (Duration, Option<Duration>) {
    let hit = sim
        .sent
        .iter()
        .filter(|s| !s.manual && near(s.due, launch))
        .map(|s| s.at - s.due)
        .max()
        .unwrap_or_else(|| panic!("nothing launched at {:?}", launch - sim.start));
    let jump = sim
        .frames
        .iter()
        .find(|f| f.output == A && f.profile.jump.is_some() && near(f.tick, launch))
        .map(|f| f.start - launch);
    (hit, jump)
}

/// A12: an immediate launch (no quantisation) arriving after a held tick. It launches on the clip
/// tick at or before its arrival, as before; its hits (and the ramp's relaunch) go once it
/// applies, after the held frame. Launch values exist only when a launch starts the transport
/// (gate 1's test 14), so there are none here. Lateness reported against the previous code.
#[test]
fn a12_an_immediate_launch_after_a_held_tick() {
    let mut sim = sim_at(with_second_scene(&CLUSTER, GlobalQuantisation::None), T);
    sim.run_until(sim.start + T + US(500));
    sim.send(SchedulerCommand::LaunchScene(SceneId(2)));
    sim.run_until(sim.start + MS(540));
    let launch = sim.log[0];
    assert!(launch.applied >= frame(&sim, A, T).end);
    assert!(complete(&sim, T));
    // At 120 BPM a clip tick is ~0.52 ms: the launch lands on 960, at T.
    let (hit, jump) = launch_timing(&sim, sim.start + T);
    let wait = launch.applied - launch.arrived;
    assert!(hit <= wait + MS(1), "hit {hit:?}, wait {wait:?}");
    eprintln!("A12 wait {wait:?}; launched hit late {hit:?}; jump frame late {jump:?}");
}

/// A13: a launch quantised to a sixteenth whose boundary falls 2.5 ms after the arrival, during
/// the held frame's wait: the boundary is kept; lateness reported against the previous code.
#[test]
fn a13_a_quantised_launch_whose_boundary_falls_in_the_wait() {
    let sixteenth = GlobalQuantisation::Value(QuantisationValue::Note(NoteValue::Sixteenth));
    // A tick 3 ms before the 625 ms sixteenth, with hits from 0.5 ms after it to just before.
    let tick = MS(622);
    let first = ((0.622 + 0.0005) / CLIP_TICK).ceil() as u64;
    let hits: Vec<u64> = (first..first + 4).collect();
    let mut sim = sim_at(with_second_scene(&hits, sixteenth), tick);
    sim.run_until(sim.start + tick + US(500));
    sim.send(SchedulerCommand::LaunchScene(SceneId(2)));
    sim.run_until(sim.start + MS(680));
    let boundary = sim.start + MS(625);
    assert_eq!(
        sim.log[0].admission,
        Some(commands::admission::AdmissionOutcome::Admitted)
    );
    assert!(sim.log[0].arrived < boundary && sim.log[0].applied > boundary);
    assert!(
        sim.planner
            .voices
            .iter()
            .any(|v| v.track == TrackId(50) && v.launch == BeatTime(1200))
    );
    let (hit, jump) = launch_timing(&sim, boundary);
    eprintln!(
        "A13 applied {:?} after the boundary; launched hit late {hit:?}; jump frame late {jump:?}",
        sim.log[0].applied.saturating_duration_since(boundary)
    );
}

/// A value batch waits behind the earlier ordinary frame and due hits. The launch
/// is still in the channel when B passes, then recovers it using P and the FIFO floor.
#[test]
fn receive_time_launch_queued_behind_held_values_recovers_boundary() {
    let tick = T - US(500);
    let sixteenth = GlobalQuantisation::Value(QuantisationValue::Note(NoteValue::Sixteenth));
    let mut sim = sim_at(with_second_scene(&[960], sixteenth), tick);
    sim.run_until(sim.start + tick + US(100));
    let mut project = current(&sim);
    project[TrackId(11)].mixer_device.volume.value = 0.5;
    let key = ArgumentRef::volume().on(TrackId(11));
    sim.send(SchedulerCommand::UpdateParameters(
        crate::engine::ValueEdit::for_test(project, vec![key]),
    ));
    sim.pass();
    assert!(sim.log.is_empty(), "the value batch must actually be held");
    sim.run_until(sim.start + T - US(100));
    sim.send(SchedulerCommand::Launch(TrackId(50), 1));
    sim.run_until(sim.start + T + MS(40));
    let (value, launch) = (sim.log[0], sim.log[1]);
    assert!(value.applied > sim.start + T, "{value:?}");
    assert!(launch.arrived >= value.applied && launch.arrived > sim.start + T);
    assert!(launch.arrived <= sim.start + T + MS(2), "{launch:?}");
    assert_eq!(
        launch.admission,
        Some(commands::admission::AdmissionOutcome::Admitted)
    );
    assert_eq!(launch.effective, launch.posted.max(value.effective));
    assert!(launch.effective < sim.start + T);
    assert!(
        sim.planner
            .voices
            .iter()
            .any(|v| v.track == TrackId(50) && v.launch == BeatTime(960))
    );
    assert!(value.applied >= frame(&sim, A, tick).end);
    assert!(
        complete(&sim, tick),
        "the earlier frame retained all sampled automation"
    );
    assert_eq!(at(&sim, tick).unwrap().2, Some(1.0));
    assert_eq!(at(&sim, tick + PERIOD).unwrap().2, Some(0.5));
}

/// An admitted command retains D-based frame priority even when its E precedes
/// an owed tick. Conversely a frame after D yields through the launch's own hold.
#[test]
fn receive_time_launch_finishes_earlier_frames_and_yields_later_frames() {
    let sixteenth = GlobalQuantisation::Value(QuantisationValue::Note(NoteValue::Sixteenth));
    for (ticks, offset, earlier) in [
        (CLUSTER.to_vec(), US(500), true),
        (vec![962, 963], Duration::ZERO, false),
    ] {
        let mut sim = sim_at(with_second_scene(&ticks, sixteenth), T);
        let posted = if earlier {
            sim.start + T + offset
        } else {
            sim.start + T - US(200)
        };
        sim.run_until(posted);
        let receipt = sim.start + T - US(1000);
        sim.send_received(SchedulerCommand::Launch(TrackId(50), 1), receipt);
        sim.run_until(sim.start + MS(540));
        let launch = sim.log[0];
        assert_eq!(
            launch.admission,
            Some(commands::admission::AdmissionOutcome::Admitted)
        );
        assert_eq!(launch.effective, receipt);
        let ordinary = frame(&sim, A, T);
        if earlier {
            assert!(launch.applied >= ordinary.end);
        } else {
            assert!(ordinary.start > launch.applied);
        }
        assert!(complete(&sim, T), "{earlier}: {:?}", at(&sim, T));
        assert!(
            sim.planner
                .voices
                .iter()
                .any(|v| v.track == TrackId(50) && v.launch == BeatTime(960))
        );
    }
}

/// A14: clip hits keep their timing while a command waits and later frames yield (A3's and A4's
/// cases), and an exact jump at a held tick keeps gate 1's bounds.
#[test]
fn a14_hits_and_jumps_keep_their_timing() {
    let mut sim = sim(&[962, 963]);
    sim.run_until(sim.start + T - US(200));
    let edit = edit_v0(&sim, 0.5);
    sim.send(edit);
    sim.run_until(sim.start + MS(540));
    // From 400 ms: the transport start's hit waits behind its start values, as before.
    let late = sim
        .sent
        .iter()
        .filter(|s| s.due > sim.start + MS(400))
        .map(|s| s.at - s.due)
        .max()
        .unwrap();
    assert!(late < US(100), "{late:?}");
    // The ramp's wrap at 2 s is an exact jump on the grid tick there; hits follow it.
    let wrap = MS(2000);
    let mut sim = sim_at(workload(&[1, 2, 3]), wrap);
    sim.run_until(sim.start + wrap + US(300));
    sim.send(SchedulerCommand::UpdateProject(current(&sim)));
    sim.run_until(sim.start + wrap + MS(40));
    let jump = sim
        .frames
        .iter()
        .find(|f| f.profile.jump.is_some() && near(f.tick, sim.start + wrap))
        .unwrap();
    assert!(
        jump.start - jump.tick <= US(250) && jump.end - jump.tick <= US(500),
        "{jump:?}"
    );
    // The sim records the jump itself as sent once its frame has gone; the hits follow it.
    let late = sim
        .sent
        .iter()
        .filter(|s| s.due > sim.start + wrap)
        .map(|s| s.at - s.due)
        .max()
        .unwrap();
    assert!(late < US(100), "{late:?}");
}

/// A15: a command posted while an exact jump's packets are going is taken after the last of them
/// (existing), and then still waits for the earlier ordinary frame the cluster holds.
#[test]
fn a15_an_exact_jump_in_flight_delays_the_command() {
    let wrap = MS(2000);
    let mut project = workload(&[1, 2, 3]);
    project.tracks.extend(volumes(A, 100, 100));
    let mut sim = Sim::new(project, MS(20), lead_for(wrap)).launched();
    sim.record_values = true;
    sim.resumable = true;
    sim.run_until(sim.start + wrap + US(50));
    sim.send(SchedulerCommand::UpdateProject(current(&sim)));
    sim.run_until(sim.start + wrap + MS(40));
    let jump = sim
        .frames
        .iter()
        .find(|f| f.profile.jump.is_some() && near(f.tick, sim.start + wrap))
        .unwrap();
    let command = sim.log[0];
    assert!(command.arrived >= jump.end, "{jump:?} {command:?}");
    let ordinary = frame(&sim, A, wrap);
    assert!(command.applied >= ordinary.end, "{ordinary:?} {command:?}");
}
