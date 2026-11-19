//! Recent send times per output and per track, for activity meters. The scheduler
//! thread records with `try_lock`, so a reader holding the lock costs a missed blink,
//! never a late send.
use crate::model::TrackId;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long an output or track keeps reporting as active after its last packet.
pub const ACTIVE_WINDOW: Duration = Duration::from_millis(150);
/// An estimate of Live's indicator, 2026-10-12: about a third of a second.
pub const FADE_WINDOW: Duration = Duration::from_millis(330);

#[derive(Debug, Default)]
pub struct Levels {
    pub outputs: Vec<(TrackId, f32)>,
    pub tracks: Vec<(TrackId, f32)>,
}

fn levels<K: Copy>(list: &[(K, Instant)], now: Instant) -> Vec<(K, f32)> {
    list.iter()
        .filter_map(|(id, at)| {
            let age = now.saturating_duration_since(*at);
            (age < FADE_WINDOW).then(|| (*id, 1.0 - age.as_secs_f32() / FADE_WINDOW.as_secs_f32()))
        })
        .collect()
}

#[derive(Debug, Default)]
struct LastSent {
    outputs: Vec<(TrackId, Instant)>,
    tracks: Vec<(TrackId, Instant)>,
}

#[derive(Debug, Default)]
pub struct Activity {
    last_sent: Mutex<LastSent>,
}

fn stamp<K: PartialEq>(list: &mut Vec<(K, Instant)>, key: K, at: Instant) {
    match list.iter_mut().find(|(id, _)| *id == key) {
        Some(entry) => entry.1 = at,
        None => list.push((key, at)),
    }
}

fn recent<K: Copy>(list: &[(K, Instant)], now: Instant) -> Vec<K> {
    list.iter()
        .filter(|(_, at)| now.saturating_duration_since(*at) <= ACTIVE_WINDOW)
        .map(|(id, _)| *id)
        .collect()
}

impl Activity {
    /// One packet left for `output`, played by `track` when its voice is still known.
    pub(crate) fn record(&self, output: TrackId, track: Option<TrackId>, at: Instant) {
        let Ok(mut last) = self.last_sent.try_lock() else {
            return;
        };
        stamp(&mut last.outputs, output, at);
        if let Some(track) = track {
            stamp(&mut last.tracks, track, at);
        }
    }

    /// Outputs that sent a packet within `ACTIVE_WINDOW` before `now`.
    pub fn active(&self, now: Instant) -> Vec<TrackId> {
        recent(&self.lock().outputs, now)
    }

    /// Tracks that sent a packet within `ACTIVE_WINDOW` before `now`.
    pub fn active_tracks(&self, now: Instant) -> Vec<TrackId> {
        recent(&self.lock().tracks, now)
    }

    /// Linear send brightness, sampled once for tracks and outputs.
    pub fn levels(&self, now: Instant) -> Levels {
        let last = self.lock();
        Levels {
            outputs: levels(&last.outputs, now),
            tracks: levels(&last.tracks, now),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LastSent> {
        self.last_sent.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outputs_stay_active_for_the_window_only() {
        let activity = Activity::default();
        let start = Instant::now();
        activity.record(TrackId(201), Some(TrackId(3)), start);
        assert_eq!(activity.active(start), vec![TrackId(201)]);
        assert_eq!(activity.active_tracks(start), vec![TrackId(3)]);
        assert!(
            activity
                .active(start + ACTIVE_WINDOW + Duration::from_millis(1))
                .is_empty()
        );
        assert!(
            activity
                .active_tracks(start + ACTIVE_WINDOW + Duration::from_millis(1))
                .is_empty()
        );
    }
}

#[cfg(test)]
mod fade_tests {
    use super::*;
    #[test]
    fn a_send_fades_to_dark_in_330_ms_and_a_new_send_relights_it() {
        let activity = Activity::default();
        let start = Instant::now();
        let track = TrackId(3);
        let output = TrackId(201);
        activity.record(output, Some(track), start);
        let full = activity.levels(start);
        assert_eq!(full.outputs, vec![(output, 1.0)]);
        assert_eq!(full.tracks, vec![(track, 1.0)]);
        let half = activity.levels(start + Duration::from_millis(165));
        assert!((half.tracks[0].1 - 0.5).abs() < 0.001);
        assert_eq!(half.outputs[0].1, half.tracks[0].1);
        assert!(activity.levels(start + FADE_WINDOW).tracks.is_empty());
        assert!(activity.levels(start + FADE_WINDOW).outputs.is_empty());
        activity.record(output, Some(track), start + Duration::from_millis(200));
        assert_eq!(
            activity.levels(start + Duration::from_millis(200)).tracks,
            vec![(track, 1.0)]
        );
        assert!(activity.levels(start + Duration::from_millis(529)).tracks[0].1 > 0.0);
        assert!(
            activity
                .levels(start + Duration::from_millis(530))
                .tracks
                .is_empty()
        );
    }
}
