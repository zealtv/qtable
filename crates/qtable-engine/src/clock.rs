//! Injectable transport clock. Beat positions use the model's exact integer ticks.

use std::time::{Duration, Instant};

use crate::model::{BeatTime, TICKS_PER_BEAT, Tempo, TimeSignature};

/// Scheduler-facing seam for internal or externally synchronised transport.
/// All mutation timestamps must be at or after the current playback anchor.
pub trait TempoClock {
    /// Starts at zero when stopped; repeated play while playing is a no-op.
    fn play(&mut self, at: Instant);
    /// Freezes at the supplied time. Returns false for a time before the anchor.
    fn stop(&mut self, at: Instant) -> bool;
    fn is_playing(&self) -> bool;
    /// Moves a stopped clock back to beat zero; a playing clock is untouched.
    fn rewind(&mut self);
    fn tempo(&self) -> Tempo;
    /// Preserves the current tick position; false leaves state unchanged.
    fn set_tempo(&mut self, tempo: Tempo, at: Instant) -> bool;
    fn time_signature(&self) -> TimeSignature;
    fn set_time_signature(&mut self, signature: TimeSignature);
    /// Stopped clocks return their frozen position regardless of the query time.
    /// Playing clocks return None before the latest anchor or on tick overflow.
    fn beat_at(&self, at: Instant) -> Option<BeatTime>;
    /// Earliest representable nanosecond at which this tick has been reached.
    /// None when stopped, before the latest anchor beat, or on overflow.
    fn instant_at(&self, beat: BeatTime) -> Option<Instant>;
}

/// Wall time is converted to ticks by flooring, so a tick is never reported
/// before it is reached. Tempo is the model's finite positive floating BPM;
/// only positions are integer ticks. Tempo changes re-anchor at the current
/// whole tick's deadline under the old tempo, retaining elapsed sub-tick wall
/// time instead of dropping it. That deadline rounds upward to a nanosecond;
/// the retained wall-time phase runs at the new tempo. Earlier tempo segments
/// are not retained. No method reads the system clock or sleeps.
#[derive(Clone, Debug)]
pub struct InternalClock {
    tempo: Tempo,
    signature: TimeSignature,
    anchor: Option<Instant>,
    position: BeatTime,
}

impl InternalClock {
    pub(crate) fn at_position(
        tempo: Tempo,
        signature: TimeSignature,
        position: BeatTime,
        at: Instant,
    ) -> Self {
        Self {
            tempo,
            signature,
            position,
            anchor: Some(at),
        }
    }
    pub fn new(tempo: Tempo, signature: TimeSignature) -> Self {
        Self {
            tempo,
            signature,
            anchor: None,
            position: BeatTime::ZERO,
        }
    }

    /// Fractional content time for frame sampling; event scheduling still uses exact ticks.
    pub(crate) fn fractional_ticks_at(&self, at: Instant) -> Option<f64> {
        let elapsed = match self.anchor {
            Some(anchor) => {
                at.checked_duration_since(anchor)?.as_secs_f64()
                    * (self.tempo.bpm() / 60.0)
                    * TICKS_PER_BEAT as f64
            }
            None => 0.0,
        };
        let ticks = self.position.0 as f64 + elapsed;
        // instant_at rounds to nanoseconds. Recover exact tick boundaries so a discrete
        // jump is not evaluated a fraction of a nanosecond before its breakpoint.
        let tolerance = (self.tempo.bpm() / 60.0) * TICKS_PER_BEAT as f64 * 1e-9;
        let ticks = if (ticks - ticks.round()).abs() <= tolerance {
            ticks.round()
        } else {
            ticks
        };
        (ticks.is_finite() && ticks < 18_446_744_073_709_551_616.0).then_some(ticks)
    }

    fn elapsed_ticks(&self, duration: Duration) -> Option<u64> {
        let ticks =
            (duration.as_secs_f64() * (self.tempo.bpm() / 60.0) * TICKS_PER_BEAT as f64).floor();
        // The exclusive upper bound avoids saturating float-to-integer casts.
        if !ticks.is_finite() || ticks >= 18_446_744_073_709_551_616.0 {
            None
        } else {
            Some(ticks as u64)
        }
    }
}

impl Default for InternalClock {
    fn default() -> Self {
        Self::new(Tempo::default(), TimeSignature::default())
    }
}

impl TempoClock for InternalClock {
    fn play(&mut self, at: Instant) {
        if self.anchor.is_none() {
            self.position = BeatTime::ZERO;
            self.anchor = Some(at);
        }
    }

    fn stop(&mut self, at: Instant) -> bool {
        let Some(position) = self.beat_at(at) else {
            return false;
        };
        self.position = position;
        self.anchor = None;
        true
    }

    fn is_playing(&self) -> bool {
        self.anchor.is_some()
    }

    fn rewind(&mut self) {
        if self.anchor.is_none() {
            self.position = BeatTime::ZERO;
        }
    }
    fn tempo(&self) -> Tempo {
        self.tempo
    }

    fn set_tempo(&mut self, tempo: Tempo, at: Instant) -> bool {
        let Some(position) = self.beat_at(at) else {
            return false;
        };
        if self.is_playing() && tempo != self.tempo {
            let Some(anchor) = self.instant_at(position) else {
                return false;
            };
            self.position = position;
            self.anchor = Some(anchor);
        }
        self.tempo = tempo;
        true
    }

    fn time_signature(&self) -> TimeSignature {
        self.signature
    }
    fn set_time_signature(&mut self, signature: TimeSignature) {
        self.signature = signature;
    }

    fn beat_at(&self, at: Instant) -> Option<BeatTime> {
        let Some(anchor) = self.anchor else {
            return Some(self.position);
        };
        let ticks = self.elapsed_ticks(at.checked_duration_since(anchor)?)?;
        self.position.ticks().checked_add(ticks).map(BeatTime)
    }

    fn instant_at(&self, beat: BeatTime) -> Option<Instant> {
        let anchor = self.anchor?;
        let ticks = beat.ticks().checked_sub(self.position.ticks())?;
        if ticks == 0 {
            return Some(anchor);
        }
        // Binary search uses the same forward mapping: floating rounding cannot
        // make the returned deadline precede its tick. It also handles tiny BPM
        // and platform Instant limits without panics or unchecked casts.
        let reached = |nanos: u128| -> bool {
            let duration = Duration::new(
                (nanos / 1_000_000_000) as u64,
                (nanos % 1_000_000_000) as u32,
            );
            self.elapsed_ticks(duration)
                .is_none_or(|value| value >= ticks)
        };
        let mut low = 0;
        let mut high = Duration::MAX.as_nanos();
        if !reached(high) {
            return None;
        }
        while low < high {
            let mid = low + (high - low) / 2;
            if reached(mid) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        let duration = Duration::new((low / 1_000_000_000) as u64, (low % 1_000_000_000) as u32);
        let at = anchor.checked_add(duration)?;
        // Some enormous tempos skip ticks even within one nanosecond.
        (self.beat_at(at)? == beat).then_some(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_and_round_trip_at_120_and_90() {
        let start = Instant::now();
        for (bpm, seconds, beats) in [(120.0, 2, 4), (90.0, 2, 3)] {
            let mut clock = InternalClock::new(Tempo::new(bpm).unwrap(), TimeSignature::default());
            clock.play(start);
            let beat = BeatTime::from_beats(beats).unwrap();
            assert_eq!(
                clock.beat_at(start + Duration::from_secs(seconds)),
                Some(beat)
            );
            assert_eq!(
                clock.instant_at(beat),
                Some(start + Duration::from_secs(seconds))
            );
            for ticks in [0, 1, 959, 960, 12345, 1000000] {
                let beat = BeatTime(ticks);
                let at = clock.instant_at(beat).unwrap();
                assert_eq!(clock.beat_at(at), Some(beat));
                if ticks > 0 {
                    assert!(clock.beat_at(at - Duration::from_nanos(1)).unwrap() < beat);
                }
            }
        }
    }

    #[test]
    fn tempo_change_is_continuous() {
        let start = Instant::now();
        let mut clock = InternalClock::default();
        clock.play(start);
        let change = start + Duration::from_millis(1234);
        let before = clock.beat_at(change).unwrap();
        let tick_instant = clock.instant_at(before).unwrap();
        assert!(clock.set_tempo(Tempo::new(90.0).unwrap(), change));
        assert_eq!(clock.beat_at(change), Some(before));
        assert_eq!(clock.instant_at(before), Some(tick_instant));
        assert_eq!(clock.instant_at(BeatTime(before.ticks() - 1)), None);
        assert_eq!(
            clock.beat_at(change + Duration::from_secs(2)),
            Some(BeatTime(before.ticks() + 2880))
        );
        assert!(!clock.set_tempo(Tempo::default(), start));
        assert_eq!(clock.tempo().bpm(), 90.0);
    }

    #[test]
    fn repeated_tempo_changes_retain_phase() {
        let start = Instant::now();
        let mut clock = InternalClock::default();
        clock.play(start);
        let mut elapsed_ns = 0_u64;
        // Integer segment integral: ticks = ns * BPM * 960 / 60e9.
        let mut weighted_ns = 0_u128;
        let mut bpm = 120_u64;
        for index in 0..1000_u64 {
            let segment_ns = 1_000_003 + (index * 7919 % 3_000_017);
            elapsed_ns += segment_ns;
            weighted_ns += u128::from(segment_ns) * u128::from(bpm);
            bpm = if bpm == 120 { 121 } else { 120 };
            assert!(clock.set_tempo(
                Tempo::new(bpm as f64).unwrap(),
                start + Duration::from_nanos(elapsed_ns),
            ));
        }
        let tail_ns = 987_654_321_u64;
        weighted_ns += u128::from(tail_ns) * u128::from(bpm);
        let expected = (weighted_ns * u128::from(TICKS_PER_BEAT) / 60_000_000_000) as u64;
        let actual = clock
            .beat_at(start + Duration::from_nanos(elapsed_ns + tail_ns))
            .unwrap()
            .ticks();
        assert!(actual.abs_diff(expected) <= 1, "{actual} versus {expected}");
    }

    #[test]
    fn stop_freezes_and_play_restarts() {
        let start = Instant::now();
        let mut clock = InternalClock::default();
        assert!(!clock.is_playing());
        assert_eq!(clock.beat_at(start), Some(BeatTime::ZERO));
        assert_eq!(clock.instant_at(BeatTime::ZERO), None);
        clock.play(start);
        clock.play(start + Duration::from_secs(1));
        assert_eq!(
            clock.beat_at(start + Duration::from_secs(1)),
            Some(BeatTime(1920))
        );
        assert_eq!(clock.beat_at(start - Duration::from_nanos(1)), None);
        assert!(!clock.stop(start - Duration::from_nanos(1)));
        assert!(clock.stop(start + Duration::from_secs(1)));
        assert_eq!(
            clock.beat_at(start + Duration::from_secs(10)),
            Some(BeatTime(1920))
        );
        assert_eq!(clock.instant_at(BeatTime(1920)), None);
        assert!(clock.set_tempo(Tempo::new(90.0).unwrap(), start));
        clock.play(start + Duration::from_secs(10));
        assert_eq!(
            clock.beat_at(start + Duration::from_secs(10)),
            Some(BeatTime::ZERO)
        );
    }

    #[test]
    fn rewind_resets_a_stopped_clock_but_not_a_playing_one() {
        let start = Instant::now();
        let mut clock = InternalClock::default();
        clock.play(start);
        clock.rewind();
        assert!(clock.is_playing());
        assert_eq!(
            clock.beat_at(start + Duration::from_secs(1)),
            Some(BeatTime(1920))
        );
        assert!(clock.stop(start + Duration::from_secs(1)));
        assert_eq!(clock.beat_at(start), Some(BeatTime(1920)));
        clock.rewind();
        assert_eq!(clock.beat_at(start), Some(BeatTime::ZERO));
        assert!(!clock.is_playing());
    }

    #[test]
    fn signature_helpers_and_changes_do_not_change_position() {
        let start = Instant::now();
        let mut clock = InternalClock::default();
        assert_eq!(clock.time_signature().bar_length(), BeatTime(3840));
        clock.play(start);
        clock.set_time_signature(TimeSignature::new(6, 8).unwrap());
        assert_eq!(clock.time_signature().bar_length(), BeatTime(2880));
        assert_eq!(
            clock.beat_at(start + Duration::from_secs(1)),
            Some(BeatTime(1920))
        );
    }

    #[test]
    fn extreme_tempos_fail_without_panicking() {
        let start = Instant::now();
        let mut clock = InternalClock::new(Tempo::new(f64::MAX).unwrap(), TimeSignature::default());
        clock.play(start);
        assert_eq!(clock.beat_at(start), Some(BeatTime::ZERO));
        assert_eq!(clock.beat_at(start + Duration::from_secs(1)), None);
        assert_eq!(clock.instant_at(BeatTime(1)), None);
        assert!(clock.set_tempo(Tempo::new(f64::MIN_POSITIVE).unwrap(), start));
        assert_eq!(clock.instant_at(BeatTime(1)), None);
    }
}
