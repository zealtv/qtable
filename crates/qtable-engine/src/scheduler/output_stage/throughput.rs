//! Per-output sliding one-second windows. Readers may delay publication, never a send.
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::model::TrackId;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Counts {
    pub messages: u64,
    pub bytes: u64,
    /// Parameter values overwritten before a frame sent them.
    pub merged: u64,
    /// Datagrams sent; below `messages` when Bundle packs several into one.
    pub packets: u64,
    /// Changes left waiting for room in the Budget, counted once per frame they waited.
    pub deferred: u64,
    /// Repeats the Budget had no room for.
    pub skipped: u64,
    /// Messages too big for any bundle, sent plain.
    pub oversize: u64,
    pub frames_late: u64,
}

/// Counts in the preceding second, plus the current Budget wait state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Throughput {
    pub messages: u64,
    pub packets: u64,
    pub bytes: u64,
    pub merged: u64,
    pub deferred: u64,
    pub skipped: u64,
    pub oversize: u64,
    pub frames_late: u64,
    pub deferring: bool,
}

impl Counts {
    fn since(self, previous: Self) -> Self {
        Self {
            messages: self.messages.saturating_sub(previous.messages),
            packets: self.packets.saturating_sub(previous.packets),
            bytes: self.bytes.saturating_sub(previous.bytes),
            merged: self.merged.saturating_sub(previous.merged),
            deferred: self.deferred.saturating_sub(previous.deferred),
            skipped: self.skipped.saturating_sub(previous.skipped),
            oversize: self.oversize.saturating_sub(previous.oversize),
            frames_late: self.frames_late.saturating_sub(previous.frames_late),
        }
    }
}

#[derive(Debug, Default)]
struct Window {
    previous: Counts,
    samples: VecDeque<(Instant, Counts)>,
    deferring: bool,
}

impl Window {
    fn record(&mut self, counts: Counts, deferring: bool, now: Instant) {
        let delta = counts.since(self.previous);
        self.previous = counts;
        self.deferring = deferring;
        while self
            .samples
            .front()
            .is_some_and(|(at, _)| now.saturating_duration_since(*at) >= Duration::from_secs(1))
        {
            self.samples.pop_front();
        }
        if delta != Counts::default() {
            self.samples.push_back((now, delta));
        }
    }

    fn read(&self, now: Instant) -> Throughput {
        let mut value = Throughput {
            deferring: self.deferring,
            ..Throughput::default()
        };
        for (_, counts) in self
            .samples
            .iter()
            .filter(|(at, _)| now.saturating_duration_since(*at) < Duration::from_secs(1))
        {
            value.messages += counts.messages;
            value.packets += counts.packets;
            value.bytes += counts.bytes;
            value.merged += counts.merged;
            value.deferred += counts.deferred;
            value.skipped += counts.skipped;
            value.oversize += counts.oversize;
            value.frames_late += counts.frames_late;
        }
        value
    }
}

#[derive(Debug, Default)]
pub(crate) struct Published {
    windows: Mutex<Vec<(TrackId, Window)>>,
}

impl Published {
    pub fn record(
        &self,
        outputs: impl Iterator<Item = (TrackId, Counts, bool)>,
        now: Instant,
    ) -> bool {
        let Ok(mut windows) = self.windows.try_lock() else {
            return false;
        };
        let mut present = Vec::new();
        for (id, counts, deferring) in outputs {
            present.push(id);
            let index = windows
                .iter()
                .position(|(held, _)| *held == id)
                .unwrap_or_else(|| {
                    windows.push((id, Window::default()));
                    windows.len() - 1
                });
            windows[index].1.record(counts, deferring, now);
        }
        windows.retain(|(id, _)| present.contains(id));
        true
    }

    pub fn read(&self, now: Instant) -> Vec<(TrackId, Throughput)> {
        self.windows
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(id, window)| (*id, window.read(now)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_slide_independently_and_idle_reads_expire_every_counter() {
        let shared = Published::default();
        let now = Instant::now();
        let all = Counts {
            messages: 8,
            packets: 2,
            bytes: 128,
            merged: 4,
            deferred: 3,
            skipped: 2,
            oversize: 1,
            frames_late: 1,
        };
        assert!(
            shared.record(
                [
                    (TrackId(1), all, true),
                    (TrackId(2), Counts::default(), false)
                ]
                .into_iter(),
                now
            )
        );
        let read = shared.read(now);
        assert_eq!(
            read[0].1,
            Throughput {
                messages: 8,
                packets: 2,
                bytes: 128,
                merged: 4,
                deferred: 3,
                skipped: 2,
                oversize: 1,
                frames_late: 1,
                deferring: true
            }
        );
        assert_eq!(read[1].1, Throughput::default());
        let later = now + Duration::from_millis(500);
        let more = Counts {
            messages: 10,
            ..all
        };
        shared.record([(TrackId(1), more, false)].into_iter(), later);
        assert_eq!(shared.read(later)[0].1.messages, 10);
        assert!(!shared.read(later)[0].1.deferring);
        assert_eq!(
            shared.read(now + Duration::from_secs(1)),
            vec![(
                TrackId(1),
                Throughput {
                    messages: 2,
                    ..Throughput::default()
                }
            )]
        );
        assert_eq!(
            shared.read(later + Duration::from_secs(1)),
            vec![(TrackId(1), Throughput::default())]
        );
        shared.record(std::iter::empty(), later);
        assert!(shared.read(later).is_empty());
    }

    #[test]
    fn a_reader_never_blocks_publication_and_counters_catch_up_after_contention() {
        let shared = Published::default();
        let now = Instant::now();
        let outputs = || {
            [(
                TrackId(1),
                Counts {
                    messages: 3,
                    ..Counts::default()
                },
                false,
            )]
            .into_iter()
        };
        let guard = shared.windows.lock().unwrap();
        assert!(!shared.record(outputs(), now));
        drop(guard);
        assert!(shared.record(outputs(), now));
        assert_eq!(shared.read(now)[0].1.messages, 3);
        shared.record(outputs(), now);
        assert_eq!(shared.read(now)[0].1.messages, 3);
    }
}
