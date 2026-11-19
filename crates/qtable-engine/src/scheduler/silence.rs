//! One transition check for silence notifications.
use super::events::EventSink;
use super::{DueEvent, PlannedAction, Planner, SchedulerEvent, TrackId, io};

impl Planner {
    /// Include queued launches and dummy clips, but exclude ended voices retained
    /// for positively delayed packets or a follow timer. Those already went silent.
    pub(super) fn sounding_tracks(&self) -> Vec<TrackId> {
        let mut tracks: Vec<_> = self
            .voices
            .iter()
            .filter(|voice| !voice.silent)
            .map(|voice| voice.track)
            .collect();
        tracks.sort_by_key(|track| track.0);
        tracks.dedup();
        tracks
    }

    pub(super) fn silenced_since(&self, before: Vec<TrackId>) -> Vec<TrackId> {
        before
            .into_iter()
            .filter(|track| {
                !self
                    .voices
                    .iter()
                    .any(|voice| voice.track == *track && !voice.silent)
            })
            .collect()
    }

    /// Advance a dispatched event and return each track that became silent once.
    /// Send consumption only advances a cursor or removes an already silent voice;
    /// keep the packet dispatch path free of transition snapshots/allocations.
    pub fn consume(&mut self, event: &DueEvent) -> Vec<TrackId> {
        self.consume_with_error(event).0
    }

    /// Worker consumption also surfaces failed follow launches to clients.
    pub(super) fn consume_with_error(
        &mut self,
        event: &DueEvent,
    ) -> (Vec<TrackId>, Option<io::Error>) {
        if matches!(event.action, PlannedAction::Send { .. }) {
            return (Vec::new(), self.consume_action(event).err());
        }
        let before = self.sounding_tracks();
        let error = self.consume_action(event).err();
        (self.silenced_since(before), error)
    }
}

pub(super) fn report(tracks: Vec<TrackId>, events: &mut EventSink) {
    for track in tracks {
        events.send(SchedulerEvent::TrackSilent(track));
    }
}
