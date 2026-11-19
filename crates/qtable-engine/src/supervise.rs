//! Supervision of the scheduler thread: poison-tolerant locking and panic reporting.
use std::any::Any;
use std::io;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::scheduler::EventSink;
use crate::scheduler::{PublishedState, SchedulerEvent};

/// Lock the published state, ignoring poison: it holds plain data that the next
/// publish overwrites, and a dead scheduler must not take its clients down.
pub(crate) fn lock_published(state: &Mutex<PublishedState>) -> MutexGuard<'_, PublishedState> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into())
}

/// Record that the worker panicked: set the failure flag clients read through
/// the snapshot, and queue a final error event.
pub(crate) fn report_panic(
    payload: &(dyn Any + Send),
    events: &mut EventSink,
    state: &Mutex<PublishedState>,
) {
    let message = format!("scheduler stopped: {}", panic_message(payload));
    {
        let mut state = lock_published(state);
        state.playing = false;
        state.failure = Some(message.clone());
    }
    events.send(SchedulerEvent::Error(io::Error::other(message)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use std::sync::{Arc, mpsc};
    use std::time::Instant;

    fn state() -> Arc<Mutex<PublishedState>> {
        let tempo = Tempo::new(120.0).unwrap();
        Arc::new(Mutex::new(PublishedState {
            playing: true,
            tempo,
            signature: TimeSignature::default(),
            global_quantisation: GlobalQuantisation::default(),
            pending_stops: Vec::new(),
            pending_stop_slots: Vec::new(),
            last_triggered_scene: None,
            queued_follow_scene: None,
            beat: BeatTime::ZERO,
            anchor: Instant::now(),
            tracks: Vec::new(),
            clip_positions: Vec::new(),
            lane_positions: Vec::new(),
            automation: Vec::new(),
            failure: None,
        }))
    }

    #[test]
    fn panic_while_holding_the_lock_is_reported_and_leaves_state_readable() {
        let state = state();
        let (tx, rx) = mpsc::sync_channel(4);
        let worker_state = Arc::clone(&state);
        let outcome = std::thread::spawn(move || {
            let _guard = worker_state.lock().unwrap();
            panic!("boom");
        })
        .join();
        let payload = outcome.unwrap_err();
        assert!(state.is_poisoned());
        report_panic(payload.as_ref(), &mut EventSink::new(tx), &state);
        let read = lock_published(&state);
        assert!(!read.playing);
        assert_eq!(read.failure.as_deref(), Some("scheduler stopped: boom"));
        let Ok(SchedulerEvent::Error(error)) = rx.try_recv() else {
            panic!("expected a final error event");
        };
        assert!(error.to_string().contains("scheduler stopped: boom"));
    }
}
