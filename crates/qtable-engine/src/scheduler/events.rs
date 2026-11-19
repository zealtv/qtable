//! The scheduler's outgoing event queue, which counts what it has to drop.
use std::sync::mpsc::{SyncSender, TrySendError};

use super::SchedulerEvent;

/// Wraps the bounded event channel. The realtime thread must never block on a
/// slow client, so a full queue drops events; the sink counts them and reports
/// the total in-band as soon as the queue has room again.
pub(crate) struct EventSink {
    queue: SyncSender<SchedulerEvent>,
    missed: u64,
}

impl EventSink {
    pub(crate) fn new(queue: SyncSender<SchedulerEvent>) -> Self {
        Self { queue, missed: 0 }
    }

    /// A second sink on the same queue with its own count, for the panic path.
    pub(crate) fn share(&self) -> Self {
        Self::new(self.queue.clone())
    }

    pub(crate) fn send(&mut self, event: SchedulerEvent) {
        if self.missed != 0 {
            let count = self.missed;
            if self
                .queue
                .try_send(SchedulerEvent::EventsDropped { count })
                .is_ok()
            {
                self.missed = 0;
            }
        }
        // A disconnected queue means the engine is shutting down: nothing to count.
        if let Err(TrySendError::Full(_)) = self.queue.try_send(event) {
            self.missed = self.missed.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn overflow_is_counted_and_reported_once_the_queue_drains() {
        let (tx, rx) = mpsc::sync_channel(2);
        let mut sink = EventSink::new(tx);
        for _ in 0..5 {
            sink.send(SchedulerEvent::TrackSilent(crate::model::TrackId(1)));
        }
        assert_eq!(sink.missed, 3);
        while rx.try_recv().is_ok() {}
        sink.send(SchedulerEvent::TrackSilent(crate::model::TrackId(2)));
        assert!(matches!(
            rx.try_recv(),
            Ok(SchedulerEvent::EventsDropped { count: 3 })
        ));
        assert!(matches!(rx.try_recv(), Ok(SchedulerEvent::TrackSilent(_))));
        assert_eq!(sink.missed, 0);
    }
}
