//! Coarse waits stop before the final clip/frame spin windows.
use std::time::Duration;

/// None means poll commands without sleeping. A timed wait is capped at 1 ms: macOS timer
/// slack on a longer wait can overshoot the entire spin window, even at USER_INTERACTIVE QoS.
pub(super) fn coarse(clip: Duration, frame: Option<Duration>) -> Option<Duration> {
    let clip_spin = Duration::from_millis(6);
    let frame_spin = Duration::from_millis(1);
    if clip <= clip_spin || frame.is_some_and(|wait| wait <= frame_spin) {
        None
    } else {
        Some(
            (clip - clip_spin)
                .min(frame.map_or(frame_spin, |wait| wait - frame_spin))
                .min(Duration::from_millis(1)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_and_frame_spin_windows_include_their_boundaries() {
        let far = Duration::from_secs(1);
        for clip in [Duration::ZERO, Duration::from_millis(6)] {
            assert_eq!(coarse(clip, None), None);
            assert_eq!(coarse(clip, Some(far)), None);
        }
        for frame in [Duration::ZERO, Duration::from_millis(1)] {
            assert_eq!(coarse(far, Some(frame)), None);
        }
        let ns = Duration::from_nanos(1);
        assert_eq!(coarse(Duration::from_millis(6) + ns, None), Some(ns));
        assert_eq!(coarse(far, Some(Duration::from_millis(1) + ns)), Some(ns));
    }

    #[test]
    fn coarse_wait_preserves_both_margins_and_never_exceeds_one_millisecond() {
        let far = Duration::from_secs(1);
        let ms = Duration::from_millis(1);
        assert_eq!(coarse(far, None), Some(ms));
        assert_eq!(coarse(far, Some(far)), Some(ms));
        assert_eq!(
            coarse(Duration::from_micros(6_250), Some(far)),
            Some(ms / 4)
        );
        assert_eq!(
            coarse(far, Some(Duration::from_micros(1_500))),
            Some(ms / 2)
        );
        assert_eq!(
            coarse(
                Duration::from_micros(6_750),
                Some(Duration::from_micros(1_250))
            ),
            Some(ms / 4),
        );
    }
}
