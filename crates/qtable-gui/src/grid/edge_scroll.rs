//! Bounded held-edge scrolling, independent of placement and of the model.
use std::time::{Duration, Instant};

// Qtable assumptions, not measurements of Live (tour, 2026-11-07).
const ZONE: f32 = 24.0;
const DELAY: Duration = Duration::from_millis(250);
const FRAME: Duration = Duration::from_millis(16);
const SPEED: f32 = 180.0;
const MAX_STEP_TIME: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Default)]
pub(super) struct EdgeScroll {
    held: Option<Held>,
}

#[derive(Clone, Copy)]
struct Held {
    direction: i8,
    entered: Instant,
    last: Instant,
}

pub(super) struct Step {
    pub offset: f32,
    pub next: Instant,
}

impl EdgeScroll {
    /// `at`, `start` and `end` are viewport coordinates on one axis. `offset`
    /// is the actual rendered content offset, so wheel/resize clamping wins.
    pub fn step(
        &mut self,
        now: Instant,
        at: f32,
        range: std::ops::Range<f32>,
        offset: f32,
        max: f32,
    ) -> Option<Step> {
        let zone = ZONE.min((range.end - range.start) / 3.0);
        let direction = if zone <= 0.0 || at < range.start || at > range.end {
            0
        } else if at < range.start + zone && offset.round() > 0.0 {
            -1
        } else if at > range.end - zone && offset.round() < max.round() {
            1
        } else {
            0
        };
        if direction == 0 {
            self.held = None;
            return None;
        }
        let held = self.held.get_or_insert(Held {
            direction,
            entered: now,
            last: now,
        });
        if held.direction != direction {
            *held = Held {
                direction,
                entered: now,
                last: now,
            };
        }
        let ready = held.entered + DELAY;
        if now < ready {
            return Some(Step {
                offset,
                next: ready,
            });
        }
        let elapsed = now
            .saturating_duration_since(held.last.max(ready))
            .min(MAX_STEP_TIME);
        held.last = now;
        Some(Step {
            offset: (offset + f32::from(direction) * SPEED * elapsed.as_secs_f32()).clamp(0.0, max),
            next: now + FRAME,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stationary_hold_delays_then_scrolls_to_both_clamped_endpoints() {
        let start = Instant::now();
        let mut edge = EdgeScroll::default();
        let mut offset = 0.0;
        for frame in 0..600 {
            let now = start + FRAME * frame;
            if let Some(step) = edge.step(now, 299.0, 100.0..300.0, offset, 400.0) {
                offset = step.offset;
            }
            if now <= start + DELAY {
                assert_eq!(offset, 0.0);
            }
        }
        assert_eq!(offset, 400.0);
        assert!(
            edge.step(
                start + Duration::from_secs(10),
                299.0,
                100.0..300.0,
                offset,
                400.0
            )
            .is_none()
        );
        for frame in 0..600 {
            if let Some(step) = edge.step(
                start + Duration::from_secs(11) + FRAME * frame,
                101.0,
                100.0..300.0,
                offset,
                400.0,
            ) {
                offset = step.offset;
            }
        }
        assert_eq!(offset, 0.0);
    }

    #[test]
    fn leaving_or_reversing_restarts_dwell_and_delayed_frames_are_bounded() {
        let now = Instant::now();
        let mut edge = EdgeScroll::default();
        assert!(edge.step(now, 99.0, 0.0..100.0, 50.0, 100.0).is_some());
        let step = edge
            .step(now + Duration::from_secs(5), 99.0, 0.0..100.0, 50.0, 100.0)
            .unwrap();
        assert_eq!(
            step.offset, 59.0,
            "late redraw cannot jump more than nine pixels"
        );
        assert!(edge.step(now, 101.0, 0.0..100.0, 50.0, 100.0).is_none());
        assert!(edge.step(now, 50.0, 0.0..100.0, 50.0, 100.0).is_none());
        assert_eq!(
            edge.step(now, 99.0, 0.0..100.0, 50.0, 100.0)
                .unwrap()
                .offset,
            50.0
        );
        assert_eq!(
            edge.step(now + FRAME, 1.0, 0.0..100.0, 50.0, 100.0)
                .unwrap()
                .next,
            now + FRAME + DELAY
        );
        assert!(edge.step(now, 0.0, 0.0..0.0, 50.0, 100.0).is_none());
        assert!(edge.step(now, 99.0, 0.0..100.0, 100.0, 100.4).is_none());
    }
}
