//! Coalesce snapshot changes and retry publication without waiting on clients.
use std::sync::{Mutex, TryLockError};
use std::time::Instant;

use super::{InternalClock, Planner, PublishedState};

#[derive(Default)]
pub(super) struct Publisher {
    dirty: bool,
    /// A client command changed the state: publish even ahead of due sends.
    urgent: bool,
}

impl Publisher {
    /// Musical-state change from the playback loop. Publication waits for
    /// due sends so follow actions' batches coalesce.
    pub(super) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Change a client just commanded. A client that sees the first packet
    /// of a launch must already see the launch (race seen 2026-10-11), so
    /// this publishes before the sends it made due; they follow
    /// microseconds later.
    pub(super) fn mark_commanded(&mut self) {
        self.dirty = true;
        self.urgent = true;
    }

    /// Called after planning, so follow actions' newly created due sends also
    /// drain before publication. A busy client leaves the latest state dirty;
    /// retry even when no further commands or musical state events arrive.
    pub(super) fn retry(
        &mut self,
        state: &Mutex<PublishedState>,
        planner: &Planner,
        clock: &InternalClock,
        now: Instant,
        next: Option<Instant>,
    ) {
        if !self.dirty || (!self.urgent && next.is_some_and(|at| at <= now)) {
            return;
        }
        let mut state = match state.try_lock() {
            Ok(state) => state,
            // Match the engine reader's poison tolerance: this is plain data,
            // overwritten by the next successful publication.
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
            Err(TryLockError::WouldBlock) => return,
        };
        state.publish(planner, clock, now);
        self.dirty = false;
        self.urgent = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::TempoClock;
    use crate::model::{BeatTime, GlobalQuantisation, TrackId};

    fn setup() -> (Instant, InternalClock, Planner, Mutex<PublishedState>) {
        let project = super::super::tests::project(0.0, true);
        let now = Instant::now();
        let clock = InternalClock::new(project.tempo, project.time_signature);
        let state = Mutex::new(PublishedState {
            playing: false,
            tempo: project.tempo,
            signature: project.time_signature,
            global_quantisation: project.global_quantisation,
            pending_stops: vec![],
            pending_stop_slots: Vec::new(),
            last_triggered_scene: None,
            queued_follow_scene: None,
            beat: BeatTime::ZERO,
            anchor: now,
            tracks: vec![],
            clip_positions: vec![],
            lane_positions: vec![],
            automation: vec![],
            failure: None,
        });
        let planner = Planner::new(project, std::time::Duration::ZERO).unwrap();
        (now, clock, planner, state)
    }

    #[test]
    fn defers_due_batch_and_coalesces_changes_into_one_publication() {
        let (now, mut clock, mut planner, state) = setup();
        let mut publisher = Publisher::default();
        planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
        publisher.mark_dirty();
        publisher.retry(&state, &planner, &clock, now, Some(now));
        assert!(!state.lock().unwrap().playing);
        planner.stop_all();
        planner
            .set_global_quantisation(GlobalQuantisation::None)
            .unwrap();
        publisher.mark_dirty();
        publisher.retry(&state, &planner, &clock, now, None);
        let read = state.lock().unwrap();
        assert!(read.playing);
        assert!(read.tracks.is_empty());
        assert_eq!(read.global_quantisation, GlobalQuantisation::None);
        drop(read);
        // A clean publisher doesn't rebuild/re-anchor on every idle pass.
        publisher.retry(
            &state,
            &planner,
            &clock,
            now + std::time::Duration::from_secs(1),
            None,
        );
        assert_eq!(state.lock().unwrap().anchor, now);
    }

    #[test]
    fn commanded_change_publishes_ahead_of_due_sends() {
        let (now, mut clock, mut planner, state) = setup();
        let mut publisher = Publisher::default();
        planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
        publisher.mark_commanded();
        publisher.retry(&state, &planner, &clock, now, Some(now));
        let read = state.lock().unwrap();
        assert!(read.playing);
        assert_eq!(read.tracks.len(), 1);
        drop(read);
        // Urgency ends with the publication: later loop-made changes wait again.
        let later = now + std::time::Duration::from_secs(1);
        publisher.mark_dirty();
        publisher.retry(&state, &planner, &clock, later, Some(later));
        assert_eq!(state.lock().unwrap().anchor, now);
    }

    #[test]
    fn busy_lock_retains_dirty_state_and_retries_latest_changes() {
        let (now, mut clock, mut planner, state) = setup();
        let mut publisher = Publisher::default();
        let read = state.lock().unwrap();
        clock.play(now);
        publisher.mark_dirty();
        // This same-thread locked mutex would deadlock with blocking publication.
        publisher.retry(&state, &planner, &clock, now, None);
        assert!(!read.playing);
        planner.launch_at(TrackId(1), 1, BeatTime::ZERO).unwrap();
        publisher.mark_dirty();
        publisher.retry(&state, &planner, &clock, now, None);
        drop(read);
        publisher.retry(&state, &planner, &clock, now, None);
        let read = state.lock().unwrap();
        assert!(read.playing);
        assert_eq!(read.tracks.len(), 1);
        assert_eq!(read.tracks[0].slot, 1);
    }

    #[test]
    fn poisoned_lock_remains_publishable() {
        let (now, mut clock, planner, state) = setup();
        let _ = std::panic::catch_unwind(|| {
            let _read = state.lock().unwrap();
            panic!("poison snapshot");
        });
        assert!(state.is_poisoned());
        clock.play(now);
        let mut publisher = Publisher::default();
        publisher.mark_dirty();
        publisher.retry(&state, &planner, &clock, now, None);
        assert!(crate::supervise::lock_published(&state).playing);
    }
}
