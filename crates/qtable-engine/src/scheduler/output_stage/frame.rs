use std::time::{Duration, Instant};

pub(crate) struct FrameGrid {
    anchor: Instant,
    rate: f64,
}

impl FrameGrid {
    pub(crate) fn new(anchor: Instant, rate: f64) -> Self {
        assert!(rate.is_finite() && (1.0..=60.0).contains(&rate));
        Self { anchor, rate }
    }

    pub(crate) fn rate(&self) -> f64 {
        self.rate
    }

    pub(crate) fn period(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.rate)
    }

    /// First grid tick strictly after `after`, skipping missed ticks.
    pub(crate) fn next_tick(&self, after: Instant) -> Option<Instant> {
        let Some(elapsed) = after.checked_duration_since(self.anchor) else {
            return Some(self.anchor);
        };
        let mut index = (elapsed.as_secs_f64() * self.rate).floor() as u64;
        loop {
            // Round once from the anchor, rather than accumulate a rounded period.
            let offset = Duration::try_from_secs_f64(index as f64 / self.rate).ok()?;
            let tick = self.anchor.checked_add(offset)?;
            if tick > after {
                return Some(tick);
            }
            index = index.checked_add(1)?;
        }
    }

    /// First grid tick at or after `at`.
    pub(crate) fn tick_at_or_after(&self, at: Instant) -> Option<Instant> {
        match at.checked_sub(Duration::from_nanos(1)) {
            Some(before) => self.next_tick(before),
            None => self.next_tick(at),
        }
    }

    /// Latest grid tick at or before `at`; None before the anchor.
    pub(crate) fn latest_tick(&self, at: Instant) -> Option<Instant> {
        let elapsed = at.checked_duration_since(self.anchor)?;
        // One above the estimate, in case rounding put it a tick low.
        let mut index = (elapsed.as_secs_f64() * self.rate).floor() as u64 + 1;
        loop {
            let offset = Duration::try_from_secs_f64(index as f64 / self.rate).ok()?;
            let tick = self.anchor.checked_add(offset)?;
            if tick <= at {
                return Some(tick);
            }
            index = index.checked_sub(1)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outputs_share_the_anchor_and_grid() {
        let anchor = Instant::now();
        let a = FrameGrid::new(anchor, 30.0);
        let b = FrameGrid::new(anchor, 30.0);
        assert_eq!(a.period(), Duration::from_secs_f64(1.0 / 30.0));
        assert_eq!(a.next_tick(anchor - Duration::from_nanos(1)), Some(anchor));
        for millis in [0, 12, 33, 100, 1234, 120_000] {
            let after = anchor + Duration::from_millis(millis);
            assert_eq!(a.next_tick(after), b.next_tick(after));
            assert!(a.next_tick(after).unwrap() > after);
        }
    }

    #[test]
    fn missed_ticks_are_skipped_without_restarting_the_phase() {
        let anchor = Instant::now();
        let grid = FrameGrid::new(anchor, 30.0);
        let after = anchor + Duration::from_millis(245);
        let next = anchor + Duration::from_secs_f64(8.0 / 30.0);
        assert_eq!(grid.next_tick(after), Some(next));
        assert_eq!(
            grid.next_tick(next),
            Some(anchor + Duration::from_secs_f64(9.0 / 30.0))
        );
        assert_eq!(grid.next_tick(next - Duration::from_nanos(1)), Some(next));
    }

    #[test]
    fn thirty_and_sixty_hertz_ticks_coincide_without_drift() {
        let anchor = Instant::now();
        let slow = FrameGrid::new(anchor, 30.0);
        let fast = FrameGrid::new(anchor, 60.0);
        for index in [1, 2, 3, 30, 301, 3600, 108_000] {
            let tick = anchor + Duration::from_secs_f64(index as f64 / 30.0);
            let before = tick - Duration::from_nanos(1);
            assert_eq!(slow.next_tick(before), Some(tick));
            assert_eq!(fast.next_tick(before), Some(tick));
        }
    }

    #[test]
    fn ticks_at_or_around_an_instant() {
        let anchor = Instant::now();
        let grid = FrameGrid::new(anchor, 30.0);
        let third = anchor + Duration::from_secs_f64(3.0 / 30.0);
        assert_eq!(grid.tick_at_or_after(third), Some(third));
        assert_eq!(
            grid.tick_at_or_after(third + Duration::from_nanos(1)),
            grid.next_tick(third)
        );
        assert_eq!(grid.latest_tick(third), Some(third));
        assert_eq!(
            grid.latest_tick(grid.next_tick(third).unwrap() - Duration::from_nanos(1)),
            Some(third)
        );
        assert_eq!(grid.latest_tick(anchor), Some(anchor));
        assert_eq!(grid.latest_tick(anchor - Duration::from_nanos(1)), None);
    }

    #[test]
    fn fractional_rates_keep_their_phase() {
        let anchor = Instant::now();
        let grid = FrameGrid::new(anchor, 29.97);
        let after = anchor + Duration::from_secs(100);
        assert_eq!(
            grid.next_tick(after),
            Some(anchor + Duration::from_secs_f64(2998.0 / 29.97))
        );
    }
}
