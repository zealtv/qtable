//! Backlog turns (validation revision 2, 2026-10-21): once an
//! output completes a frame with hits due behind it (its backlog), the frame counts as its turn,
//! and the main path serves the least recently served output among the eligible candidates.
//! D1–D5, D7–D9 and D11 run the simulated worker (`cluster_sim`); D6, D10 and the rest of D11 are
//! `turns::Backlogs` unit tests. The sim's costs are synthetic: these check the mechanism, not
//! timing.
//!
//! What they guard: after A's exact jump, A's overdue hits
//! went first because they were due earlier, and B's hit due at the jump's completion waited
//! behind all of them (Bundle Off), or behind one after the jump's indivisible datagram (On).
use super::cluster_sim::*;
use super::*;
use crate::model::*;
use std::collections::HashMap;

const C: TrackId = TrackId(1003);
/// One 3840-tick bar at 120 BPM: the ramp wraps, an exact jump on A, at each bar.
const BAR: Duration = Duration::from_secs(2);
/// A Bundle Off frame packet as the 2026-10-20 harness runs measured it, about 25–30 µs.
const OFF_PACKET: Duration = Duration::from_micros(30);

/// The ramp and four hits on A at each bar (c's delays), and a hit at each bar on `others` (output,
/// delay ms). The grid sits 8 ms before each bar, clear of the clusters.
fn workload(others: &[(TrackId, f64)], bundle: bool) -> Project {
    let mut tracks: Vec<_> = CLIP_DELAYS_MS
        .iter()
        .enumerate()
        .map(|(i, &delay)| clip_track(i as u64 + 1, A, delay, 3840, &[0]))
        .collect();
    tracks.extend(
        others
            .iter()
            .enumerate()
            .map(|(i, &(output, delay))| clip_track(i as u64 + 5, output, delay, 3840, &[0])),
    );
    tracks.push(ramp_track(3840));
    let mut outputs = vec![A, B];
    if others.iter().any(|&(output, _)| output == C) {
        outputs.push(C);
    }
    let mut project = project(120.0, &outputs, tracks);
    for output in &mut project.outputs {
        output.output.as_mut().unwrap().stage.bundle = bundle;
    }
    project
}

/// The sim through the second bar's cluster, resumable as in the worker, each frame packet costing
/// `packet`.
fn run(project: Project, packet: Duration) -> Sim {
    let mut sim = Sim::new(project, MS(20), MS(8)).launched();
    sim.resumable = true;
    sim.frame_packet = packet;
    let bar = sim.start + BAR;
    sim.run_until(bar + MS(3));
    sim
}

/// The exact jump at the second bar on A.
fn jump(sim: &Sim) -> Frame {
    let bar = sim.start + BAR;
    *sim.frames
        .iter()
        .find(|f| f.output == A && f.start_send && near(f.tick, bar))
        .expect("the jump at the bar")
}

/// The clip sends due from `from`, in the order they went: (output, due, sent), offsets from `from`.
fn sends(sim: &Sim, from: Instant) -> Vec<(TrackId, Duration, Duration)> {
    let mut sends: Vec<_> = sim
        .sent
        .iter()
        .filter(|s| !s.manual && s.due >= from && s.due < from + MS(2))
        .map(|s| (s.output, s.due - from, s.at - from))
        .collect();
    sends.sort_by_key(|&(_, _, at)| at);
    sends
}

/// The jump's completion after the bar, calibrated with the hits on other outputs in `others`
/// (none of them due after it, so it doesn't depend on a hit placed after it).
fn completion(others: &[(TrackId, f64)], bundle: bool, packet: Duration) -> Duration {
    let sim = run(workload(others, bundle), packet);
    jump(&sim).end - (sim.start + BAR)
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// The rule as an oracle over the recorded timeline (validation's checker): replays frame
/// completions and main-path sends in time order, keeping the open backlogs and the served order,
/// and reports every send that wasn't the least recently served output's eligible candidate.
fn violations(sim: &Sim, from: Instant, to: Instant) -> Vec<String> {
    let sends: Vec<_> = sim
        .sent
        .iter()
        .filter(|s| !s.manual && s.at >= from && s.at < to)
        .copied()
        .collect();
    let frames: Vec<_> = sim
        .frames
        .iter()
        .filter(|f| f.end >= from && f.end < to)
        .copied()
        .collect();
    let in_flight = |t: Instant| frames.iter().any(|f| f.start <= t && t < f.end);
    let mut timeline: Vec<(Instant, Option<Frame>, Option<Sent>)> = frames
        .iter()
        .map(|f| (f.end, Some(*f), None))
        .chain(sends.iter().map(|s| (s.at, None, Some(*s))))
        .collect();
    timeline.sort_by_key(|&(at, frame, _)| (at, frame.is_none()));
    let unsent = |t: Instant| -> Vec<Sent> {
        let mut v: Vec<_> = sends.iter().filter(|s| s.at >= t).copied().collect();
        v.sort_by_key(|s| s.due);
        v
    };
    let mut open: HashMap<TrackId, Instant> = HashMap::new();
    let mut served: Vec<TrackId> = Vec::new();
    let mut out = Vec::new();
    for (t, frame, sent) in timeline {
        if let Some(frame) = frame {
            if unsent(t)
                .iter()
                .any(|s| s.output == frame.output && s.due <= t)
            {
                open.insert(frame.output, t);
            }
            if !open.is_empty() {
                served.retain(|&o| o != frame.output);
                served.push(frame.output);
            }
            continue;
        }
        let sent = sent.unwrap();
        if !open.is_empty() && !in_flight(t) {
            let due: Vec<_> = unsent(t).into_iter().filter(|s| s.due <= t).collect();
            let backlog = |s: &Sent| open.get(&s.output).is_some_and(|&c| s.due <= c);
            let mut candidates: Vec<Sent> = Vec::new();
            let mut passed: Vec<TrackId> = Vec::new();
            for (i, s) in due.iter().enumerate() {
                if i == 0
                    || !passed.contains(&s.output)
                        && !candidates.iter().any(|c| c.output == s.output)
                {
                    candidates.push(*s);
                }
                if !backlog(s) {
                    break;
                }
                passed.push(s.output);
            }
            let rank = |o: TrackId| served.iter().position(|&s| s == o).map_or(-1, |p| p as i64);
            if let Some(expected) = candidates.iter().min_by_key(|c| (rank(c.output), c.due))
                && expected.output != sent.output
            {
                out.push(format!(
                    "at {:.3} ms {:?} went; {:?} (due {:.3}) was the least recently served candidate",
                    ms(t - from),
                    sent.output,
                    expected.output,
                    ms(expected.due - from)
                ));
            }
            served.retain(|&o| o != sent.output);
            served.push(sent.output);
        }
        open.retain(|&output, &mut c| {
            sends
                .iter()
                .any(|s| s.output == output && s.due <= c && s.at > t)
        });
        if open.is_empty() {
            served.clear();
        }
    }
    out
}

/// Off, D1 and D2: B's probe hit placed `offset` after the jump's calibrated completion (negative:
/// before it, inside the last packet).
fn off_case(offset_us: i64) -> (Sim, Duration) {
    let others = [(B, 0.2), (B, 0.3)];
    let c = completion(&others, false, OFF_PACKET);
    let probe = ms(c) + offset_us as f64 / 1000.0;
    let mut with = others.to_vec();
    with.push((B, probe));
    let sim = run(workload(&with, false), OFF_PACKET);
    let bar = sim.start + BAR;
    assert_eq!(
        jump(&sim).end - bar,
        c,
        "the probe moved the jump's completion"
    );
    (sim, c)
}

/// `off_case` with the order before backlog-turns-implementation, for A's comparison.
fn off_case_without_turns(offset_us: i64) -> (Sim, Duration) {
    let others = [(B, 0.2), (B, 0.3)];
    let c = completion(&others, false, OFF_PACKET);
    let mut with = others.to_vec();
    with.push((B, ms(c) + offset_us as f64 / 1000.0));
    let mut sim = Sim::new(workload(&with, false), MS(20), MS(8)).launched();
    sim.resumable = true;
    sim.frame_packet = OFF_PACKET;
    sim.backlog_turns = false;
    sim.run_until(sim.start + BAR + MS(3));
    (sim, c)
}

fn report(name: &str, sim: &Sim) {
    let bar = sim.start + BAR;
    for (output, due, at) in sends(sim, bar) {
        eprintln!(
            "{name}: {output:?} due {:.3} sent {:.3} late {:.3} ms",
            ms(due),
            ms(at),
            ms(at - due)
        );
    }
}

/// The sends of `output` after `c` that were due by it: its backlog, in the order they went.
fn backlog(sim: &Sim, output: TrackId, c: Duration) -> Vec<(Duration, Duration)> {
    let bar = sim.start + BAR;
    sends(sim, bar)
        .into_iter()
        .filter(|&(o, due, at)| o == output && due <= c && at >= c)
        .map(|(_, due, at)| (due, at))
        .collect()
}

#[test]
fn d1_off_a_hit_due_just_after_the_frame_waits_behind_one_backlog_send_at_most() {
    let (sim, c) = off_case(5);
    report("d1", &sim);
    let bar = sim.start + BAR;
    let a = backlog(&sim, A, c);
    assert_eq!(a.len(), 4, "A's four hits are its backlog: {a:?}");
    let probe = sends(&sim, bar)
        .into_iter()
        .find(|&(o, due, _)| o == B && due > c)
        .unwrap();
    let before = a.iter().filter(|&&(_, at)| at < probe.2).count();
    assert!(
        before <= 1,
        "B's hit at {:.3} ms went after {before} of A's backlog",
        ms(probe.2)
    );
    assert!(a.windows(2).all(|w| w[0].0 < w[1].0), "A's order: {a:?}");
    // A's cost: each backlog hit later by at most the one B send taken before it.
    let (old, _) = off_case_without_turns(5);
    let before: Vec<_> = backlog(&old, A, c);
    for (now, then) in a.iter().zip(&before) {
        assert!(
            now.1 <= then.1 + US(25),
            "A's hit due {:.3}: {:.3} ms, before {:.3}",
            ms(now.0),
            ms(now.1),
            ms(then.1)
        );
    }
    let v = violations(&sim, bar, bar + MS(2));
    assert!(v.is_empty(), "{v:#?}");
}

#[test]
fn d2_off_a_hit_due_inside_the_last_packet_goes_first_after_the_frame() {
    let (sim, c) = off_case(-10);
    report("d2", &sim);
    let bar = sim.start + BAR;
    let after: Vec<_> = sends(&sim, bar)
        .into_iter()
        .filter(|&(_, _, at)| at >= c)
        .collect();
    assert_eq!(after[0].0, B, "first send after the frame: {after:?}");
    assert!(backlog(&sim, A, c).windows(2).all(|w| w[0].0 < w[1].0));
    let v = violations(&sim, bar, bar + MS(2));
    assert!(v.is_empty(), "{v:#?}");
}

/// On: the jump goes as one datagram costing `datagram`; B's hit at 0.2 ms and A's at 0.137 ms are
/// both due when it completes.
fn on_case(datagram: Duration) -> (Sim, Duration) {
    let others = [(B, 0.2), (B, 0.3), (B, 0.4), (B, 0.5)];
    let sim = run(workload(&others, true), datagram);
    let c = jump(&sim).end - (sim.start + BAR);
    (sim, c)
}

#[test]
fn d3_on_the_hit_due_during_the_datagram_goes_next_after_it() {
    let (sim, c) = on_case(US(250));
    report("d3", &sim);
    let bar = sim.start + BAR;
    assert!(
        c > US(200) && c < US(300),
        "the datagram completes at {:.3} ms",
        ms(c)
    );
    let after: Vec<_> = sends(&sim, bar)
        .into_iter()
        .filter(|&(_, _, at)| at >= c)
        .collect();
    assert_eq!(after[0].0, B, "first send after the datagram: {after:?}");
    let (_, due, at) = after[0];
    assert!(
        at - due >= c - due,
        "B still waited for the indivisible datagram"
    );
    let v = violations(&sim, bar, bar + MS(2));
    assert!(v.is_empty(), "{v:#?}");
}

#[test]
fn d4_on_a_datagram_completing_over_100_us_late_still_makes_b_miss() {
    let (sim, c) = on_case(US(320));
    report("d4", &sim);
    let bar = sim.start + BAR;
    let after: Vec<_> = sends(&sim, bar)
        .into_iter()
        .filter(|&(_, _, at)| at >= c)
        .collect();
    let (output, due, at) = after[0];
    assert_eq!(output, B);
    assert!(
        at - due > US(100),
        "B's hit {:.3} ms late: the part the fix can't cover",
        ms(at - due)
    );
}

#[test]
fn d5_three_outputs_take_turns_among_eligible_candidates() {
    let base = [(B, 0.2), (B, 0.3)];
    let c = completion(&base, false, OFF_PACKET);
    let mut others = base.to_vec();
    others.push((B, ms(c) - 0.010));
    others.push((C, ms(c) - 0.008));
    let sim = run(workload(&others, false), OFF_PACKET);
    report("d5", &sim);
    let bar = sim.start + BAR;
    assert_eq!(jump(&sim).end - bar, c);
    let order: Vec<_> = sends(&sim, bar)
        .into_iter()
        .filter(|&(_, _, at)| at >= c)
        .map(|(o, _, _)| o)
        .take(4)
        .collect();
    assert_eq!(order, [B, C, A, A], "after the frame");
    let v = violations(&sim, bar, bar + MS(2));
    assert!(v.is_empty(), "{v:#?}");
}

#[test]
fn d7_without_a_completed_frame_the_front_goes_first() {
    // Hits on A and B at the bar's second beat (0.5 s): no jump there, and the grid's frames at
    // 492 and 508.7 ms are clear of the cluster.
    let mut tracks = vec![
        clip_track(1, A, 0.137, 3840, &[960]),
        clip_track(2, B, 0.1, 3840, &[960]),
        clip_track(3, A, 0.251, 3840, &[960]),
        clip_track(4, B, 0.2, 3840, &[960]),
    ];
    tracks.push(ramp_track(3840));
    let sim = run(project(120.0, &[A, B], tracks), OFF_PACKET);
    let beat = sim.start + MS(500);
    assert!(
        !sim.frames
            .iter()
            .any(|f| f.end > beat && f.end < beat + MS(1)),
        "no frame completes in the cluster"
    );
    let sent = sends(&sim, beat);
    assert_eq!(sent.len(), 4);
    let mut by_due = sent.clone();
    by_due.sort_by_key(|&(_, due, _)| due);
    assert_eq!(sent, by_due, "due order");
}

#[test]
fn d8_one_output_keeps_todays_order() {
    // g: the ramp and A's four hits, nothing on other outputs; with and without backlog turns.
    let order = |turns: bool| {
        let mut sim = Sim::new(workload(&[], false), MS(20), MS(8)).launched();
        sim.resumable = true;
        sim.frame_packet = OFF_PACKET;
        sim.backlog_turns = turns;
        sim.run_until(sim.start + BAR + MS(3));
        let start = sim.start;
        sim.sent
            .iter()
            .map(|s| (s.output, s.due - start, s.at - start))
            .collect::<Vec<_>>()
    };
    assert_eq!(order(true), order(false));
}

/// D1's case with `command` applied just after A's first backlog hit, mid-backlog.
fn command_mid_backlog(command: impl FnOnce(&Sim) -> SchedulerCommand) -> Sim {
    let (probe, c) = {
        let (sim, c) = off_case(5);
        let bar = sim.start + BAR;
        let probe = sends(&sim, bar)
            .into_iter()
            .find(|&(o, due, _)| o == B && due > c)
            .unwrap()
            .1;
        (probe, c)
    };
    let mut with = vec![(B, 0.2), (B, 0.3)];
    with.push((B, ms(probe)));
    let mut sim = Sim::new(workload(&with, false), MS(20), MS(8)).launched();
    sim.resumable = true;
    sim.frame_packet = OFF_PACKET;
    let bar = sim.start + BAR;
    // The pass after the jump's last packet stamps its completion and takes A's first hit.
    sim.run_until(bar + c + US(3));
    assert!(
        sim.backlogs.is_open(),
        "A's backlog is open when the command comes"
    );
    let command = command(&sim);
    sim.command(command);
    assert!(!sim.backlogs.is_open(), "the command cleared it");
    sim.run_until(bar + MS(3));
    sim
}

#[test]
fn d11_a_command_mid_backlog_leaves_no_state_behind() {
    let sim = command_mid_backlog(|_| SchedulerCommand::SetFollowActionsEnabled(true));
    let bar = sim.start + BAR;
    let after: Vec<_> = sends(&sim, bar)
        .into_iter()
        .filter(|&(_, _, at)| bar + at > sim.applied[sim.applied.len() - 1])
        .collect();
    let mut by_due = after.clone();
    by_due.sort_by_key(|&(_, due, _)| due);
    assert_eq!(after, by_due, "front-first after the command");
    assert!(!sim.backlogs.is_open());
}

#[test]
fn d11_deleting_b_mid_backlog_leaves_no_state_behind() {
    let sim = command_mid_backlog(|sim| {
        let mut project = (*sim.planner.project).clone();
        project.outputs.retain(|o| o.id != B);
        // B's tracks are 5–7 (`workload`).
        project.tracks.retain(|t| !(5..=7).contains(&t.id.0));
        SchedulerCommand::UpdateProject(project)
    });
    let bar = sim.start + BAR;
    let applied = sim.applied[sim.applied.len() - 1];
    let after: Vec<_> = sends(&sim, bar)
        .into_iter()
        .filter(|&(_, _, at)| bar + at > applied)
        .collect();
    assert!(after.iter().all(|&(o, _, _)| o == A), "{after:?}");
    let mut by_due = after.clone();
    by_due.sort_by_key(|&(_, due, _)| due);
    assert_eq!(after, by_due);
    assert!(!sim.backlogs.is_open());
}

#[test]
fn d9_the_transport_starts_values_precede_their_hits() {
    // The transport's start sends its values on A as a start send, with A's hits due behind it: a
    // backlog at the launch.
    let others = [(B, 0.2), (B, 0.3)];
    let sim = run(workload(&others, false), OFF_PACKET);
    let start = sim.start;
    let values = sim
        .frames
        .iter()
        .find(|f| f.output == A && f.start_send && near(f.tick, start))
        .unwrap();
    for (output, due, at) in sends(&sim, start) {
        if output == A {
            assert!(
                start + at >= values.end,
                "A's hit due {:.3} went before its values",
                ms(due)
            );
        }
    }
    let v = violations(&sim, start, start + MS(2));
    assert!(v.is_empty(), "{v:#?}");
}

#[test]
fn d12_turns_taken_on_the_turn_path_count_when_backlog_selection_resumes() {
    // A and B jump at the bar; A's frame goes first (stage order), so A completes, with its hits
    // due behind it (its backlog), while B's frame is still part-way out. Voice 3, routed to A and
    // C, has its C copy 10 µs after its A copy. A synthetic `RetireVoice` (an unknown voice: a
    // no-op when consumed) at 0.21 ms is the test's seam: `turn_index` never passes it, so A's
    // last backlog hit (0.3 ms) and C's next (0.6 ms) stay pending until B completes.
    let mut tracks = vec![
        clip_track(1, A, 0.137, 3840, &[0]),
        clip_track(2, A, 0.3, 3840, &[0]),
        clip_track(4, C, 0.59, 3840, &[0]),
        ramp_track(3840),
        ramp_track_on(B, 600, 3840),
    ];
    let mut shared = clip_track(3, A, 0.19, 3840, &[0]);
    shared.route = Route::Outputs(vec![A, C]);
    tracks.push(shared);
    let mut project = project(120.0, &[A, B, C], tracks);
    project.outputs[2].delay_ms = 0.01;
    let mut sim = Sim::new(project, MS(20), MS(8)).launched();
    sim.resumable = true;
    sim.frame_packet = OFF_PACKET;
    let bar = sim.start + BAR;
    sim.run_until(bar - MS(1));
    let at = bar + US(210);
    assert!(
        sim.pending.iter().any(|e| e.at > at),
        "the bar's events are planned"
    );
    let index = sim.pending.iter().position(|e| e.at > at).unwrap();
    sim.pending.insert(
        index,
        DueEvent {
            at,
            action: PlannedAction::RetireVoice,
            voice: u64::MAX,
            cursor: 0,
        },
    );
    sim.run_until(bar + MS(3));
    report("d12", &sim);

    let frame = |output| {
        *sim.frames
            .iter()
            .find(|f| f.output == output && f.start_send && near(f.tick, bar))
            .unwrap()
    };
    let (a, b) = (frame(A), frame(B));
    assert!(
        a.end < b.end,
        "A completes while B's frame is still part-way out"
    );
    let sent = sends(&sim, bar);
    let at = |output, due: u64| {
        sent.iter()
            .find(|&&(o, d, _)| o == output && d.abs_diff(US(due)) < US(2))
            .map(|&(_, _, at)| bar + at)
            .unwrap_or_else(|| panic!("{output:?} due {due} µs: {sent:?}"))
    };
    // While B's frame is out, `turn_index` takes A's backlog head and voice 3 on A, then on C, in
    // due order, and stops at the barrier.
    let turn_path = [at(A, 137), at(A, 190), at(C, 200)];
    assert!(
        turn_path.iter().all(|&t| t > a.end && t < b.end),
        "{sent:?}"
    );
    assert!(turn_path.windows(2).all(|w| w[0] < w[1]));
    // After B completes, backlog selection resumes: A's last turn-path send came before C's, so A
    // is the least recently served and its remaining backlog hit goes before C's hit. Without the
    // turn path's history, C (never served) would go first.
    let (a2, c2) = (at(A, 300), at(C, 600));
    assert!(a2 > b.end && c2 > b.end);
    assert!(a2 < c2, "A's backlog hit before C's: {sent:?}");
}
