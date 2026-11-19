//! Shared scheduler lookups, state transitions and follow timing/choice rules.
use super::{BeatTime, FollowAction, Instant, Planner, TempoClock, TrackId};
use crate::model::FollowActions;

impl Planner {
    /// Cancel waiting triggers, retaining playing voices and undoing their cutoffs.
    pub(super) fn cancel_triggers(&mut self, track: TrackId, beat: BeatTime) {
        self.voices.retain(|voice| {
            voice.track != track
                || (voice.cutoff.is_some_and(|end| end <= beat))
                || (voice.silent && voice.cutoff.is_some())
                || (voice.launch <= beat
                    && voice.cutoff.is_none_or(|end| end > beat)
                    && !voice.silent)
        });
        for voice in self.voices.iter_mut().filter(|voice| {
            voice.track == track && !voice.silent && voice.cutoff.is_none_or(|end| end > beat)
        }) {
            voice.cutoff = None;
            voice.cutoff_deadline = None;
        }
    }

    /// Live 12 manual (2026-10-10 capture), checklist T09/S12: zero weights
    /// mean No Action. Otherwise consume the same single seeded A/B draw for
    /// clips and scenes, even when one of the two weights is zero.
    pub(super) fn choose(&mut self, actions: &FollowActions) -> FollowAction {
        let total = u64::from(actions.chance_a) + u64::from(actions.chance_b);
        if total == 0 {
            FollowAction::NoAction
        } else if self.random_below(total) < u64::from(actions.chance_a) {
            actions.action_a
        } else {
            actions.action_b
        }
    }
}

/// State transitions in discarded tempo segments are due immediately once
/// their musical position has been reached. Future unmappable beats stay absent.
pub(super) fn due_at(clock: &impl TempoClock, beat: BeatTime, now: Instant) -> Option<Instant> {
    clock
        .instant_at(beat)
        .or_else(|| (clock.beat_at(now)? >= beat).then_some(now))
}
