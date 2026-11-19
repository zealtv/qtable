//! GUI acknowledgement of accepted manual event commands, independent of the input client.
use qtable_engine::model::{EventRef, TrackId};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const FLASH: Duration = Duration::from_millis(150);

#[derive(Default)]
pub(crate) struct State {
    until: HashMap<(TrackId, EventRef), Instant>,
}

impl State {
    pub(crate) fn fired(&mut self, track: TrackId, event: EventRef, now: Instant) {
        self.until.retain(|_, until| *until > now);
        self.until.insert((track, event), now + FLASH);
    }

    pub(crate) fn lit(&self, track: TrackId, event: &EventRef, now: Instant) -> bool {
        self.until
            .get(&(track, event.clone()))
            .is_some_and(|until| *until > now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtable_engine::model::{DeviceId, EntryKey};

    #[test]
    fn flashes_are_scoped_expire_and_retrigger_without_waiting_for_a_release() {
        let mut state = State::default();
        let event = EventRef {
            device: DeviceId(1),
            key: EntryKey("ping".into()),
        };
        let now = Instant::now();
        state.fired(TrackId(1), event.clone(), now);
        assert!(state.lit(TrackId(1), &event, now));
        assert!(!state.lit(TrackId(2), &event, now));
        assert!(!state.lit(TrackId(1), &event, now + FLASH));
        state.fired(TrackId(1), event.clone(), now + FLASH / 2);
        assert!(state.lit(TrackId(1), &event, now + FLASH));
        assert!(!state.lit(TrackId(1), &event, now + FLASH * 2));
        state.fired(TrackId(2), event, now + FLASH * 2);
        assert_eq!(state.until.len(), 1);
    }
}
