//! While an output's frame is part-way out, which pending event may take the next turn
//! (hits-pass-a-blocked-front, 2026-10-20); and once a frame completes with hits due behind it,
//! which output's hit goes next (bundle-off-b-residue, backlog-turns-implementation, 2026-10-21).
use super::planner::{DueEvent, PlannedAction};
use crate::model::TrackId;
use std::collections::VecDeque;
use std::time::Instant;

/// The index of the pending event that may go now, between a sending frame's packets.
///
/// - The front goes if it's due and is a `Send` to an output that isn't sending, or an
///   `Automate` (its frame queues behind the sending one).
/// - Otherwise a later due `Send` to an output that isn't sending may pass the events before it,
///   but only if each of them is a `Send` or `Automate` held back solely because its own output
///   is sending. Lifecycle and state events (silence, stops, retirements, follow actions) are
///   barriers, and so is any earlier event on the same output or of the same voice: the planner
///   rebuilds a voice's next event as each one is consumed, so a voice keeps its order, including
///   a voice routed to several outputs.
pub(super) fn turn_index(
    pending: &VecDeque<DueEvent>,
    now: Instant,
    sending_to: impl Fn(TrackId) -> bool,
) -> Option<usize> {
    // The voices of the events passed so far; their outputs are all sending, so a hit to an
    // output that isn't can't break its per-output order by passing them.
    let mut passed_voices = Vec::new();
    for (index, event) in pending.iter().enumerate() {
        if event.at > now {
            return None;
        }
        match &event.action {
            PlannedAction::Send { output, .. } if !sending_to(*output) => {
                return (!passed_voices.contains(&event.voice)).then_some(index);
            }
            PlannedAction::Automate { .. } if index == 0 => return Some(0),
            PlannedAction::Send { output, .. } | PlannedAction::Automate { output, .. }
                if sending_to(*output) =>
            {
                passed_voices.push(event.voice);
            }
            _ => return None,
        }
    }
    None
}

/// The hits a completed frame released, and the order outputs took their turns in, while any of
/// them is pending (bundle-off-b-residue proposal revision 2).
///
/// When output X completes a frame, its pending `Send`s due by then are its backlog; X's backlog
/// stays open until none of them is pending. While any backlog is open, the completed frame
/// counts as X's turn, and the main path takes the eligible candidate of the least recently
/// served output (ties: the earliest due) instead of the front. Only backlog hits can be passed,
/// so the reordering stays inside what completed frames released. With no backlog open nothing is
/// kept, and the front goes as before.
#[derive(Debug, Default)]
pub(super) struct Backlogs {
    /// Each open backlog's output and its latest frame's completion.
    open: Vec<(TrackId, Instant)>,
    /// Outputs by their last turn, least recent first, while a backlog is open.
    served: Vec<TrackId>,
}

impl Backlogs {
    /// `output` completed a frame at `at`: open (or extend) its backlog if a `Send` of it due by
    /// then is pending; either way, while any backlog is open, the frame was its turn.
    pub(super) fn completed(&mut self, output: TrackId, at: Instant, pending: &VecDeque<DueEvent>) {
        if pending.iter().any(|e| e.at <= at && sends_to(e, output)) {
            match self.open.iter_mut().find(|(o, _)| *o == output) {
                Some((_, completed)) => *completed = at.max(*completed),
                None => self.open.push((output, at)),
            }
        }
        self.served(output);
    }

    /// A `Send` to `output` went (or its frame completed) while a backlog is open.
    pub(super) fn served(&mut self, output: TrackId) {
        if self.open.is_empty() {
            return;
        }
        self.served.retain(|&o| o != output);
        self.served.push(output);
    }

    /// Close every backlog with nothing of it left in `pending` (sent, discarded, consumed or
    /// replanned away); with none open, forget the turns too.
    pub(super) fn close_drained(&mut self, pending: &VecDeque<DueEvent>) {
        self.open.retain(|&(output, completed)| {
            pending
                .iter()
                .any(|e| e.at <= completed && sends_to(e, output))
        });
        if self.open.is_empty() {
            self.served.clear();
        }
    }

    /// Forget everything: a command cleared `pending`.
    pub(super) fn clear(&mut self) {
        self.open.clear();
        self.served.clear();
    }

    #[cfg(test)]
    pub(super) fn is_open(&self) -> bool {
        !self.open.is_empty()
    }

    fn in_backlog(&self, event: &DueEvent) -> bool {
        self.open
            .iter()
            .any(|&(output, completed)| event.at <= completed && sends_to(event, output))
    }

    /// The index of the event the main path takes now, or `None` for the front (as with no
    /// backlog open). Candidates: the front, and each later due `Send` whose every predecessor is
    /// a due backlog `Send` of another output and of another voice. Each output's first reachable
    /// `Send` is its only candidate; lifecycle and state events, exact jumps, hits outside a
    /// backlog and events not yet due end the search.
    pub(super) fn index(&self, pending: &VecDeque<DueEvent>, now: Instant) -> Option<usize> {
        if self.open.is_empty() {
            return None;
        }
        let mut candidates: Vec<(usize, TrackId)> = Vec::new();
        let mut passed: Vec<(TrackId, u64)> = Vec::new();
        for (index, event) in pending.iter().enumerate() {
            if event.at > now {
                break;
            }
            let PlannedAction::Send { output, .. } = event.action else {
                break;
            };
            let blocked = passed
                .iter()
                .any(|&(o, voice)| o == output || voice == event.voice);
            if !blocked && !candidates.iter().any(|&(_, o)| o == output) {
                candidates.push((index, output));
            }
            if !self.in_backlog(event) {
                break;
            }
            passed.push((output, event.voice));
        }
        let rank = |output: TrackId| self.served.iter().position(|&o| o == output);
        // Never served ranks first (None < Some); the earliest due, the lowest index, breaks ties.
        candidates
            .into_iter()
            .min_by_key(|&(index, output)| (rank(output), index))
            .map(|(index, _)| index)
            .filter(|&index| index > 0)
    }
}

fn sends_to(event: &DueEvent, output: TrackId) -> bool {
    matches!(event.action, PlannedAction::Send { output: o, .. } if o == output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BeatTime, TimeSignature};
    use std::time::Duration;

    const A: TrackId = TrackId(1001);
    const B: TrackId = TrackId(1002);

    fn send(at: Instant, output: TrackId, voice: u64) -> DueEvent {
        DueEvent {
            at,
            action: PlannedAction::Send {
                output,
                packet: None,
            },
            voice,
            cursor: 0,
        }
    }

    fn event(at: Instant, action: PlannedAction, voice: u64) -> DueEvent {
        DueEvent {
            at,
            action,
            voice,
            cursor: 0,
        }
    }

    fn automate(at: Instant, output: TrackId, voice: u64) -> DueEvent {
        event(
            at,
            PlannedAction::Automate {
                output,
                fields: vec![],
            },
            voice,
        )
    }

    /// A is sending; B isn't.
    fn turn(pending: Vec<DueEvent>, now: Instant) -> Option<usize> {
        turn_index(&pending.into(), now, |output| output == A)
    }

    #[test]
    fn the_front_goes_when_it_can() {
        let now = Instant::now();
        assert_eq!(turn(vec![send(now, B, 1), send(now, A, 2)], now), Some(0));
        assert_eq!(
            turn(vec![automate(now, A, 1), send(now, B, 2)], now),
            Some(0)
        );
    }

    #[test]
    fn a_due_hit_on_another_output_passes_hits_blocked_by_their_sending_output() {
        let now = Instant::now();
        let pending = vec![send(now, A, 1), send(now, A, 2), send(now, B, 3)];
        assert_eq!(turn(pending, now), Some(2));
        // Also past an exact jump queued on the sending output.
        let pending = vec![send(now, A, 1), automate(now, A, 2), send(now, B, 3)];
        assert_eq!(turn(pending, now), Some(2));
    }

    #[test]
    fn nothing_passes_before_it_is_due() {
        let now = Instant::now();
        let later = now + Duration::from_micros(50);
        assert_eq!(turn(vec![send(now, A, 1), send(later, B, 2)], now), None);
    }

    #[test]
    fn lifecycle_and_state_events_are_barriers() {
        let now = Instant::now();
        let signature = TimeSignature::default();
        let barriers = [
            PlannedAction::TrackSilent(TrackId(1)),
            PlannedAction::StopTrack(TrackId(1)),
            PlannedAction::RetireVoice,
            PlannedAction::Follow {
                beat: BeatTime::ZERO,
                launch_beat: BeatTime::ZERO,
                signature,
            },
            PlannedAction::SceneFollow {
                scene: crate::model::SceneId(1),
                beat: BeatTime::ZERO,
                launch_beat: BeatTime::ZERO,
                tempo: crate::model::Tempo::new(120.0).unwrap(),
                signature,
            },
        ];
        for barrier in barriers {
            let pending = vec![send(now, A, 1), event(now, barrier, 2), send(now, B, 3)];
            assert_eq!(turn(pending, now), None);
        }
    }

    #[test]
    fn a_voice_keeps_its_order_across_outputs() {
        let now = Instant::now();
        // Voice 1 routed to A and B: its B hit doesn't pass its own blocked A hit.
        assert_eq!(turn(vec![send(now, A, 1), send(now, B, 1)], now), None);
        // A later B hit of another voice doesn't pass the held B hit either.
        let pending = vec![send(now, A, 1), send(now, B, 1), send(now, B, 2)];
        assert_eq!(turn(pending, now), None);
    }

    #[test]
    fn per_output_order_holds() {
        let now = Instant::now();
        // The first of two B hits goes first.
        let pending = vec![send(now, A, 1), send(now, B, 2), send(now, B, 3)];
        assert_eq!(turn(pending, now), Some(1));
    }

    const C: TrackId = TrackId(1003);

    /// A backlog of A's hits due by `completed`, opened there.
    fn backlogs(pending: &VecDeque<DueEvent>, completed: Instant) -> Backlogs {
        let mut backlogs = Backlogs::default();
        backlogs.completed(A, completed, pending);
        backlogs
    }

    #[test]
    fn d10_a_completed_frame_is_its_outputs_turn() {
        let t = Instant::now();
        let us = |n| t + Duration::from_micros(n);
        let pending: VecDeque<_> = vec![
            send(us(137), A, 1),
            send(us(251), A, 2),
            send(us(500), B, 3),
        ]
        .into();
        let b = backlogs(&pending, us(495));
        // B's hit isn't due yet: A's backlog head goes.
        assert_eq!(b.index(&pending, us(496)), None);
        // Once it's due, it passes A's backlog: A's frame was A's turn.
        assert_eq!(b.index(&pending, us(500)), Some(2));
    }

    #[test]
    fn d10_nothing_opens_without_hits_due_behind_the_frame() {
        let t = Instant::now();
        let us = |n| t + Duration::from_micros(n);
        let pending: VecDeque<_> = vec![send(us(600), A, 1), send(us(600), B, 2)].into();
        let b = backlogs(&pending, us(500));
        assert!(!b.is_open());
        assert_eq!(b.index(&pending, us(600)), None);
    }

    #[test]
    fn d10_overlapping_backlogs_alternate_and_close_when_drained() {
        let t = Instant::now();
        let us = |n| t + Duration::from_micros(n);
        let mut pending: VecDeque<_> = vec![
            send(us(100), A, 1),
            send(us(110), B, 2),
            send(us(120), A, 3),
            send(us(130), B, 4),
        ]
        .into();
        let mut b = backlogs(&pending, us(200));
        // B completes a frame with its hits due behind it: both open, B served last.
        b.completed(B, us(210), &pending);
        assert_eq!(b.open.len(), 2);
        // A's frame came first, so A is least recently served: its head (the front) goes.
        assert_eq!(b.index(&pending, us(215)), None);
        pending.pop_front();
        b.served(A);
        b.close_drained(&pending);
        // Now B; its head is the front.
        assert_eq!(b.index(&pending, us(220)), None);
        pending.pop_front();
        b.served(B);
        // A again (index 0), then B.
        assert_eq!(b.index(&pending, us(225)), None);
        pending.pop_front();
        b.served(A);
        b.close_drained(&pending);
        assert_eq!(b.open, vec![(B, us(210))], "A's backlog drained");
        pending.pop_front();
        b.close_drained(&pending);
        assert!(
            !b.is_open() && b.served.is_empty(),
            "nothing outlives the backlogs"
        );
    }

    #[test]
    fn d10_a_second_frame_moves_the_backlogs_completion() {
        let t = Instant::now();
        let us = |n| t + Duration::from_micros(n);
        let pending: VecDeque<_> = vec![send(us(100), A, 1), send(us(300), A, 2)].into();
        let mut b = backlogs(&pending, us(200));
        b.completed(A, us(350), &pending);
        assert_eq!(b.open, vec![(A, us(350))]);
    }

    #[test]
    fn d5_only_backlog_hits_can_be_passed() {
        let t = Instant::now();
        let us = |n| t + Duration::from_micros(n);
        // A's backlog, then B's hit (not a backlog), then C's: C can't pass B.
        let pending: VecDeque<_> = vec![
            send(us(137), A, 1),
            send(us(251), A, 2),
            send(us(540), B, 3),
            send(us(542), C, 4),
        ]
        .into();
        let mut b = backlogs(&pending, us(550));
        assert_eq!(b.index(&pending, us(552)), Some(2));
        // B went; C passes A's backlog now, and C was never served.
        let mut pending = pending;
        pending.remove(2);
        b.served(B);
        assert_eq!(b.index(&pending, us(570)), Some(2));
    }

    #[test]
    fn d6_barriers_stop_a_backlog_pass() {
        let t = Instant::now();
        let us = |n| t + Duration::from_micros(n);
        let signature = TimeSignature::default();
        let barriers = [
            PlannedAction::TrackSilent(TrackId(1)),
            PlannedAction::StopTrack(TrackId(1)),
            PlannedAction::RetireVoice,
            PlannedAction::Follow {
                beat: BeatTime::ZERO,
                launch_beat: BeatTime::ZERO,
                signature,
            },
            PlannedAction::SceneFollow {
                scene: crate::model::SceneId(1),
                beat: BeatTime::ZERO,
                launch_beat: BeatTime::ZERO,
                tempo: crate::model::Tempo::new(120.0).unwrap(),
                signature,
            },
            PlannedAction::Automate {
                output: A,
                fields: vec![],
            },
        ];
        for barrier in barriers {
            let pending: VecDeque<_> = vec![
                send(us(100), A, 1),
                event(us(150), barrier, 2),
                send(us(500), B, 3),
            ]
            .into();
            let b = backlogs(&pending, us(495));
            assert_eq!(b.index(&pending, us(500)), None);
        }
    }

    #[test]
    fn d6_a_voice_and_an_output_keep_their_order() {
        let t = Instant::now();
        let us = |n| t + Duration::from_micros(n);
        // Voice 1 routed to A and B: its B hit doesn't pass its own A backlog hit.
        let pending: VecDeque<_> = vec![send(us(100), A, 1), send(us(500), B, 1)].into();
        assert_eq!(backlogs(&pending, us(495)).index(&pending, us(500)), None);
        // B's second hit is never a candidate before its first.
        let pending: VecDeque<_> = vec![
            send(us(100), A, 1),
            send(us(500), B, 2),
            send(us(501), B, 3),
        ]
        .into();
        assert_eq!(
            backlogs(&pending, us(495)).index(&pending, us(501)),
            Some(1)
        );
        // A hit outside the backlog ends the search: C's hit can't pass B's.
        let pending: VecDeque<_> = vec![
            send(us(100), A, 1),
            send(us(500), B, 2),
            send(us(501), C, 3),
        ]
        .into();
        let mut b = backlogs(&pending, us(495));
        b.served(B);
        assert_eq!(
            b.index(&pending, us(501)),
            None,
            "A, least recently served, goes"
        );
    }

    #[test]
    fn d11_stale_state_goes_with_the_backlogs() {
        let t = Instant::now();
        let us = |n| t + Duration::from_micros(n);
        let pending: VecDeque<_> = vec![send(us(100), A, 1), send(us(500), B, 2)].into();
        // (a) A command clears `pending` and everything here.
        let mut b = backlogs(&pending, us(495));
        b.clear();
        assert!(!b.is_open() && b.served.is_empty());
        assert_eq!(b.index(&pending, us(500)), None);
        // (c) A's backlog hit discarded as late: the backlog closes, and the turns go with it.
        let mut b = backlogs(&pending, us(495));
        let discarded: VecDeque<_> = vec![send(us(500), B, 2)].into();
        b.close_drained(&discarded);
        assert!(!b.is_open() && b.served.is_empty());
        // (b) Later, with nothing open, nothing is reordered.
        let later: VecDeque<_> = vec![send(us(900), A, 3), send(us(905), B, 4)].into();
        assert_eq!(b.index(&later, us(910)), None);
        // A replan returning the same events keeps the backlog open.
        let mut b = backlogs(&pending, us(495));
        let replanned: VecDeque<_> = pending.iter().cloned().collect();
        b.close_drained(&replanned);
        assert!(b.is_open());
    }
}
