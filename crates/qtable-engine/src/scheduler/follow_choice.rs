//! The follow action a playing clip has chosen, drawn when the clip launches.
//!
//! Live 12.1 (observed over AbletonOSC, 2026-10-11): right after A launched, Live already
//! reported B as triggered, so the destination is chosen at launch and shown until the timer
//! fires.
//! `QueuedFollow` keeps the weighted A/B pick plus one raw random number. The
//! destination is resolved from the current project on each use, so edits to
//! the slot layout or scenes keep it consistent; edits to the action settings
//! draw again (`Planner::update_project`).
use super::*;
use crate::model::FollowActions;

// Status notes moved with the rules from follow.rs; IDs refer to the Live test plan.
// C01: observed in Live 12, 2026-10-11, live-observations check 1.
const PREVIOUS_WRAPS: bool = true;
const SINGLETON_RESTARTS: bool = true; // C07, C08: documented, Live 12 manual Any/Other include a singleton.
const EMPTY_JUMP_STOP_BUTTON: bool = true; // C10, C11: guessed, not yet observed.

#[derive(Clone, Copy, Debug)]
pub(super) struct QueuedFollow {
    pub(super) action: FollowAction,
    /// Raw draw used by Any and Other, scaled to however many slots then qualify.
    pub(super) pick: u64,
}

/// What a follow timer does with a queued choice in the current project.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FollowOutcome {
    Nothing,
    Stop,
    Launch(usize),
}

/// Maps a raw draw onto `0..bound` (multiply-shift; bias below `bound / 2^64`).
pub(super) fn scale(pick: u64, bound: usize) -> usize {
    ((u128::from(pick) * bound as u128) >> 64) as usize
}

impl Planner {
    pub(super) fn draw_follow(&mut self, actions: &FollowActions) -> QueuedFollow {
        QueuedFollow {
            action: self.choose(actions),
            pick: self.next_u64(),
        }
    }

    /// The slot the queued choice would launch now, if any.
    pub(super) fn queued_follow_slot(&self, voice: &Voice) -> Option<usize> {
        match self.follow_outcome(voice.track, voice.slot, voice.queued?) {
            FollowOutcome::Launch(slot) => Some(slot),
            _ => None,
        }
    }

    pub(super) fn follow_outcome(
        &self,
        track: TrackId,
        source: usize,
        queued: QueuedFollow,
    ) -> FollowOutcome {
        let Some(data) = self.project.track(track) else {
            return FollowOutcome::Nothing;
        };
        let slots = &data.slots;
        let first = (0..source)
            .rev()
            .find(|&i| slots[i].clip.is_none())
            .map_or(0, |i| i + 1);
        let last = (source + 1..slots.len())
            .find(|&i| slots[i].clip.is_none())
            .map_or(slots.len() - 1, |i| i - 1);
        let count = last - first + 1;
        let destination = match queued.action {
            FollowAction::NoAction => return FollowOutcome::Nothing,
            // Live 12 manual: Stop overrides loop/region settings.
            FollowAction::Stop => return FollowOutcome::Stop,
            FollowAction::PlayAgain => source,
            FollowAction::Previous => {
                if source == first {
                    if PREVIOUS_WRAPS { last } else { first }
                } else {
                    source - 1
                }
            }
            FollowAction::Next => {
                if source == last {
                    first
                } else {
                    source + 1
                }
            }
            FollowAction::First => first,
            FollowAction::Last => last,
            FollowAction::Any => {
                if count == 1 && !SINGLETON_RESTARTS {
                    return FollowOutcome::Nothing;
                }
                first + scale(queued.pick, count)
            }
            FollowAction::Other => {
                if count == 1 {
                    if !SINGLETON_RESTARTS {
                        return FollowOutcome::Nothing;
                    }
                    source
                } else {
                    let candidate = first + scale(queued.pick, count - 1);
                    candidate + usize::from(candidate >= source)
                }
            }
            FollowAction::Jump(scene) => {
                match self.project.scenes.iter().position(|s| s.id == scene) {
                    Some(row) => row,
                    None => return FollowOutcome::Nothing,
                }
            }
        };
        match slots.get(destination) {
            Some(slot) if slot.clip.is_some() => FollowOutcome::Launch(destination),
            Some(slot) if EMPTY_JUMP_STOP_BUTTON && slot.has_stop_button => FollowOutcome::Stop,
            _ => FollowOutcome::Nothing,
        }
    }
}
