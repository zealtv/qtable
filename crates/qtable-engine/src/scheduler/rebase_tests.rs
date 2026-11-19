use super::tests::project;
use super::*;
use crate::edit::{ProjectEdit, apply};
use crate::model::*;

const MS: fn(u64) -> Duration = Duration::from_millis;

struct Rig {
    start: Instant,
    clock: InternalClock,
    p: Planner,
}

/// Looping one-beat clip (120 BPM: 500 ms) with events /e1@0, /e0@480, /e2@480,
/// already launched at beat zero.
fn rig(looping: bool) -> Rig {
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut p = Planner::new(project(0.0, looping), MS(20)).unwrap();
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    Rig { start, clock, p }
}

impl Rig {
    /// Dispatch (consume) everything due at or before `ms` after the start.
    fn run_to(&mut self, ms: u64) {
        let at = self.start + MS(ms);
        for event in self.p.plan(&self.clock, at) {
            if event.at <= at {
                self.p.consume(&event);
            }
        }
    }

    fn edit(&mut self, ms: u64, change: impl FnOnce(&mut Project)) -> Vec<TrackId> {
        let mut project = (*self.p.project).clone();
        change(&mut project);
        self.p
            .update_project(project, &self.clock, self.start + MS(ms))
            .unwrap()
    }

    /// Sends planned from `ms` as (milliseconds after start, address).
    fn upcoming(&self, ms: u64, window: u64) -> Vec<(i64, String)> {
        let now = self.start + MS(ms);
        let mut p = self.p.clone();
        p.lookahead = MS(window);
        // A plan's horizon extends from its first event, so keep only this window.
        p.plan(&self.clock, now)
            .iter()
            .filter(|e| e.at <= now + MS(window))
            .filter(|e| matches!(e.action, PlannedAction::Send { .. }))
            .map(|e| {
                let offset = if e.at >= self.start {
                    e.at.duration_since(self.start).as_millis() as i64
                } else {
                    -(self.start.duration_since(e.at).as_millis() as i64)
                };
                (offset, address_of(e))
            })
            .collect()
    }
}

fn address_of(e: &DueEvent) -> String {
    match &e.action {
        PlannedAction::Send {
            packet: Some(OscPacket::Message(m)),
            ..
        } => m.addr.clone(),
        _ => String::new(),
    }
}

fn clip_mut(project: &mut Project, slot: usize) -> &mut Clip {
    project.tracks[0].slots[slot].clip.as_mut().unwrap()
}

#[test]
fn removing_an_event_does_not_panic_and_keeps_the_rest() {
    let mut r = rig(true);
    r.run_to(100); // /e1@0 sent
    r.edit(100, |p| clip_mut(p, 0).events.truncate(1)); // keeps /e0@480 only
    assert_eq!(
        r.upcoming(100, 700),
        [(250, "/e0".into()), (750, "/e0".into())]
    );
}

#[test]
fn removing_an_already_sent_event_does_not_skip_a_pending_one() {
    let mut r = rig(true);
    r.run_to(100); // /e1@0 sent; /e0 and /e2 pending at 250
    r.edit(100, |p| {
        clip_mut(p, 0).events.remove(1); // /e1@0 is index 1 in file order
    });
    assert_eq!(r.p.voices[0].cursor, 0);
    assert_eq!(
        r.upcoming(100, 450)[..2],
        [(250, "/e0".into()), (250, "/e2".into())]
    );
}

#[test]
fn added_events_play_ahead_of_the_playhead_and_never_replay() {
    let mut r = rig(true);
    r.run_to(600); // /e1@0, /e0 and /e2@250, /e1@500 sent
    let sent = r.p.voices[0].cursor;
    assert!(sent >= 3);
    r.edit(600, |p| {
        // 125 ms into the second cycle: 625 ms.
        let new = p.hit(
            TrackId(1),
            BeatTime(240),
            OscAddress::Absolute("/new".into()),
            vec![],
        );
        clip_mut(p, 0).events.push(new.unwrap());
        // 62 ms into the cycle: 562 ms, already past.
        let past = p.hit(
            TrackId(1),
            BeatTime(120),
            OscAddress::Absolute("/past".into()),
            vec![],
        );
        clip_mut(p, 0).events.push(past.unwrap());
    });
    let upcoming = r.upcoming(600, 500);
    assert_eq!(upcoming.first(), Some(&(625, "/new".into())));
    // /past (62 ms into the cycle) already went by this cycle: next at 1062.
    assert!(upcoming.contains(&(1062, "/past".into())), "{upcoming:?}");
    assert!(upcoming.iter().all(|(at, _)| *at >= 600), "{upcoming:?}");
}

#[test]
fn shrinking_a_long_clip_does_not_burst_overdue_sends() {
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut proj = project(0.0, true);
    clip_mut(&mut proj, 0).end = BeatTime(3840);
    clip_mut(&mut proj, 0).loop_end = BeatTime(3840); // 2 s
    let mut p = Planner::new(proj, MS(20)).unwrap();
    p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    let mut r = Rig { start, clock, p };
    r.run_to(10_000);
    r.edit(10_000, |p| {
        let clip = clip_mut(p, 0);
        clip.end = BeatTime(960);
        clip.loop_end = BeatTime(960);
    });
    let now = r.start + MS(10_000);
    let overdue =
        r.p.plan(&r.clock, now)
            .iter()
            .filter(|e| e.at + MS(1) < now)
            .count();
    assert_eq!(overdue, 0);
}

#[test]
fn growing_a_clip_resumes_inside_the_current_cycle() {
    let mut r = rig(true);
    r.run_to(700); // second cycle: /e1@500 sent
    r.edit(700, |p| {
        let clip = clip_mut(p, 0);
        clip.end = BeatTime(1920);
        clip.loop_end = BeatTime(1920);
    }); // 1 s loop
    // All three events of the 1 s clip's first cycle are behind us at 700 ms.
    let next = r.upcoming(700, 400);
    assert_eq!(next, [(1000, "/e1".into())]);
}

#[test]
fn non_looping_clip_turned_looping_continues_and_vice_versa() {
    let mut r = rig(false);
    r.run_to(600); // all three sent, voice silent at 500
    assert!(r.p.voices.iter().all(|v| v.silent) || r.p.voices.is_empty());
    let mut r = rig(false);
    r.run_to(100);
    r.edit(100, |p| clip_mut(p, 0).looping = true);
    assert_eq!(r.p.voices[0].cursor, 1);
    r.run_to(1200);
    assert!(!r.p.voices.is_empty() && !r.p.voices[0].silent);
    let mut r = rig(true);
    r.run_to(1200);
    r.edit(1200, |p| clip_mut(p, 0).looping = false);
    // Live 12 check 4 (2026-10-11): pass 3 finishes before silence.
    assert_eq!(r.p.voices[0].cursor, 7);
    assert_eq!(
        r.upcoming(1200, 400),
        [(1250, "/e0".into()), (1250, "/e2".into())]
    );
}

#[test]
fn unrelated_edits_leave_cursor_and_deadlines_alone() {
    let mut r = rig(true);
    r.run_to(100);
    let cursor = r.p.voices[0].cursor;
    r.edit(100, |p| clip_mut(p, 0).name = "renamed".into());
    assert_eq!(r.p.voices[0].cursor, cursor);
}

#[test]
fn voice_follows_its_clip_when_rows_move() {
    let mut r = rig(true);
    r.run_to(100);
    let id = r.p.voices[0].id;
    let lost = r.edit(100, |p| {
        apply(
            p,
            ProjectEdit::AddScene {
                at: 0,
                name: "new".into(),
            },
        )
        .unwrap();
    });
    assert!(lost.is_empty());
    assert_eq!((r.p.voices[0].id, r.p.voices[0].slot), (id, 1));
    assert!(!r.upcoming(100, 450).is_empty());
    r.edit(100, |p| {
        apply(p, ProjectEdit::MoveScene { from: 1, to: 2 }).unwrap();
    });
    assert_eq!(r.p.voices[0].slot, 2);
    r.edit(100, |p| {
        apply(
            p,
            ProjectEdit::MoveClip {
                from: (TrackId(1), 2),
                to: (TrackId(1), 0),
            },
        )
        .unwrap();
    });
    assert_eq!(r.p.voices[0].slot, 0);
}

#[test]
fn deleted_or_departed_clips_end_their_voice_and_report_the_track() {
    let mut r = rig(true);
    r.run_to(100);
    let lost = r.edit(100, |p| {
        apply(
            p,
            ProjectEdit::DeleteClip {
                track: TrackId(1),
                slot: 0,
            },
        )
        .unwrap();
    });
    assert_eq!(lost, [TrackId(1)]);
    assert!(r.p.voices.is_empty());
    assert!(r.p.plan(&r.clock, r.start + MS(100)).is_empty());
}

#[test]
fn pending_triggers_follow_row_moves_too() {
    let mut r = rig(true);
    r.p.launch_at(TrackId(1), 1, BeatTime(1920)).unwrap();
    r.edit(0, |p| {
        apply(
            p,
            ProjectEdit::AddScene {
                at: 0,
                name: "new".into(),
            },
        )
        .unwrap();
    });
    let mut slots: Vec<_> = r.p.voices.iter().map(|v| v.slot).collect();
    slots.sort();
    assert_eq!(slots, [1, 2]);
}

#[test]
fn loop_off_finishes_current_pass_and_reports_silence_once() {
    // Live 12 check 4 (2026-10-11): applies in pass 1 and later passes.
    for pass in [0, 2] {
        let mut r = rig(true);
        let edit_at = pass * 500 + 200;
        r.run_to(edit_at);
        assert!(
            r.edit(edit_at, |p| clip_mut(p, 0).looping = false)
                .is_empty()
        );
        let remaining_at = pass * 500 + 250;
        assert_eq!(
            r.upcoming(edit_at, 1000),
            [
                (remaining_at as i64, "/e0".into()),
                (remaining_at as i64, "/e2".into())
            ]
        );
        r.run_to(remaining_at);
        let end = r.start + MS((pass + 1) * 500);
        let events = r.p.plan(&r.clock, end);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].at, end);
        assert!(matches!(
            events[0].action,
            PlannedAction::TrackSilent(TrackId(1))
        ));
        assert_eq!(r.p.consume(&events[0]), [TrackId(1)]);
        assert!(r.p.plan(&r.clock, end + MS(1000)).is_empty());
    }
}

#[test]
fn loop_reenabled_before_pass_end_keeps_playing() {
    // Live 12 check 4 (2026-10-11): cancelling Loop-off preserves the pass.
    let mut r = rig(true);
    r.run_to(1200);
    r.edit(1200, |p| clip_mut(p, 0).looping = false);
    r.run_to(1250);
    assert!(r.edit(1300, |p| clip_mut(p, 0).looping = true).is_empty());
    assert_eq!(r.upcoming(1300, 400), [(1500, "/e1".into())]);
    r.run_to(1600);
    assert!(!r.p.voices[0].silent);
}

#[test]
fn loop_off_keeps_follow_time_anchored_to_launch() {
    // Live 12 check 4 (2026-10-11): finishing a pass does not relaunch it.
    let mut r = rig(true);
    r.edit(0, |p| {
        p.follow_actions_enabled = true;
        let follow = &mut clip_mut(p, 0).follow_actions;
        follow.actions.enabled = true;
        follow.linked = false;
        follow.time = BeatTime(3840);
    });
    r.p.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
    r.run_to(1200);
    r.edit(1200, |p| clip_mut(p, 0).looping = false);
    assert_eq!(r.p.voices[0].launch, BeatTime::ZERO);
    assert_eq!(r.p.follow_beat(&r.p.voices[0]), Some(BeatTime(3840)));
}

#[test]
fn loop_off_uses_musical_pass_end_and_retains_delayed_packets() {
    // Live 12 check 4 (2026-10-11): track delay shifts sends, not pass end.
    for delay in [-200.0, 400.0] {
        let mut r = rig(true);
        r.edit(0, |p| p.tracks[0].delay_ms = delay);
        r.run_to(1200);
        r.edit(1200, |p| clip_mut(p, 0).looping = false);
        if delay > 0.0 {
            assert_eq!(
                r.upcoming(1200, 1000),
                [
                    (1400, "/e1".into()),
                    (1650, "/e0".into()),
                    (1650, "/e2".into())
                ]
            );
        } else {
            assert!(r.upcoming(1200, 1000).is_empty());
        }
        r.run_to(1400);
        let end = r.start + MS(1500);
        let event =
            r.p.plan(&r.clock, end)
                .into_iter()
                .find(|e| e.at <= end)
                .unwrap();
        assert!(matches!(
            event.action,
            PlannedAction::TrackSilent(TrackId(1))
        ));
        assert_eq!(event.at, end);
        assert_eq!(r.p.consume(&event), [TrackId(1)]);
        if delay > 0.0 {
            assert!(!r.p.voices.is_empty());
            r.run_to(1650);
        }
        assert!(r.p.voices.is_empty());
    }
}
