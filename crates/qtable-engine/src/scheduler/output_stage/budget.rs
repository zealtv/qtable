use std::cmp::Reverse;
use std::time::Instant;

pub(crate) struct TokenBucket {
    rate: f64,
    frame_rate: f64,
    capacity: f64,
    tokens: f64,
    updated_at: Instant,
}

impl TokenBucket {
    pub(crate) fn new(messages_per_second: u32, frame_rate: f64, now: Instant) -> Self {
        assert!(messages_per_second > 0);
        assert!(frame_rate.is_finite() && (1.0..=60.0).contains(&frame_rate));
        let rate = f64::from(messages_per_second);
        // A fractional capacity below one could never admit a message.
        let capacity = (2.0 * rate / frame_rate).max(1.0);
        Self {
            rate,
            frame_rate,
            capacity,
            tokens: capacity,
            updated_at: now,
        }
    }

    /// The Frame Rate that sized the capacity.
    pub(crate) fn frame_rate(&self) -> f64 {
        self.frame_rate
    }

    pub(crate) fn refill(&mut self, now: Instant) {
        let Some(elapsed) = now.checked_duration_since(self.updated_at) else {
            return;
        };
        self.tokens = (self.tokens + elapsed.as_secs_f64() * self.rate).min(self.capacity);
        self.updated_at = now;
    }

    /// Events are sent even in debt; each takes a token, with debt capped at the capacity.
    pub(crate) fn event(&mut self, now: Instant) {
        self.refill(now);
        self.tokens = (self.tokens - 1.0).max(-self.capacity);
    }

    fn take(&mut self) -> bool {
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Class {
    Change,
    Repeat,
}

/// The caller supplies a stable (parameter, output) ordering key and retains
/// each slot's accumulator across frames, including skipped repeats.
pub(crate) struct Candidate<K> {
    pub(crate) key: K,
    pub(crate) class: Class,
    pub(crate) accumulator: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    pub(crate) chosen: Vec<usize>,
    pub(crate) deferred: Vec<usize>,
    pub(crate) skipped: Vec<usize>,
}

/// Select one frame in priority order. Refill the bucket at the frame time first.
/// Returned indices refer to `candidates`; only chosen accumulators reset.
pub(crate) fn select<K: Ord>(
    candidates: &mut [Candidate<K>],
    mut tokens: Option<&mut TokenBucket>,
) -> Selection {
    for candidate in candidates.iter_mut() {
        candidate.accumulator = candidate.accumulator.saturating_add(1);
    }
    let mut order: Vec<_> = (0..candidates.len()).collect();
    order.sort_by_key(|&index| {
        let candidate = &candidates[index];
        (
            candidate.class,
            Reverse(candidate.accumulator),
            &candidate.key,
        )
    });
    let mut selection = Selection {
        chosen: Vec::new(),
        deferred: Vec::new(),
        skipped: Vec::new(),
    };
    for index in order {
        let candidate = &mut candidates[index];
        if tokens.as_mut().is_none_or(|bucket| bucket.take()) {
            selection.chosen.push(index);
            candidate.accumulator = 0;
        } else {
            match candidate.class {
                Class::Change => selection.deferred.push(index),
                Class::Repeat => selection.skipped.push(index),
            }
        }
    }
    selection
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn candidate(key: u32, class: Class, accumulator: u64) -> Candidate<u32> {
        Candidate {
            key,
            class,
            accumulator,
        }
    }

    #[test]
    fn events_create_debt_and_parameters_wait_for_recovery() {
        let now = Instant::now();
        let mut bucket = TokenBucket::new(30, 30.0, now);
        for _ in 0..4 {
            bucket.event(now);
        }
        assert_eq!(bucket.tokens, -2.0);
        let mut candidates = [candidate(0, Class::Change, 0)];
        bucket.refill(now + Duration::from_millis(34));
        assert_eq!(select(&mut candidates, Some(&mut bucket)).deferred, [0]);
        bucket.refill(now + Duration::from_millis(100));
        assert_eq!(select(&mut candidates, Some(&mut bucket)).chosen, [0]);
    }

    #[test]
    fn a_long_event_burst_does_not_extend_parameter_recovery() {
        let now = Instant::now();
        for (budget, recovery) in [(30, 4.0 / 30.0), (10, 0.2)] {
            let mut bucket = TokenBucket::new(budget, 30.0, now);
            for _ in 0..1000 {
                bucket.event(now);
            }
            assert_eq!(bucket.tokens, -bucket.capacity);
            let mut candidates = [candidate(0, Class::Change, 0)];
            bucket.refill(now + Duration::from_secs_f64(recovery));
            assert_eq!(select(&mut candidates, Some(&mut bucket)).chosen, [0]);
        }
    }

    #[test]
    fn changes_precede_repeats_then_age_then_stable_key() {
        let mut candidates = [
            candidate(1, Class::Repeat, 100),
            candidate(2, Class::Change, 0),
            candidate(3, Class::Change, 4),
            candidate(0, Class::Change, 0),
        ];
        let mut bucket = TokenBucket::new(30, 30.0, Instant::now());
        let selection = select(&mut candidates, Some(&mut bucket));
        assert_eq!(selection.chosen, [2, 3]);
        assert_eq!(selection.deferred, [1]);
        assert_eq!(selection.skipped, [0]);
        assert_eq!(candidates[0].accumulator, 101);
        assert_eq!(candidates[1].accumulator, 1);
        assert_eq!(candidates[2].accumulator, 0);
    }

    #[test]
    fn ten_busy_slots_each_send_within_four_frames() {
        let now = Instant::now();
        let mut bucket = TokenBucket::new(90, 60.0, now);
        let mut candidates: Vec<_> = (0..10)
            .map(|key| candidate(key, Class::Change, 0))
            .collect();
        let mut seen = [false; 10];
        for frame in 0..4 {
            bucket.refill(now + Duration::from_millis(frame * 34));
            let selected = select(&mut candidates, Some(&mut bucket));
            assert_eq!(selected.chosen.len(), 3);
            for index in selected.chosen {
                seen[index] = true;
            }
        }
        assert!(seen.into_iter().all(|sent| sent));
    }

    #[test]
    fn stable_order_does_not_depend_on_input_order() {
        for keys in [[3, 1, 2, 0], [0, 2, 1, 3], [2, 3, 0, 1]] {
            let mut candidates = keys.map(|key| candidate(key, Class::Change, 0));
            let selection = select(&mut candidates, None);
            let chosen: Vec<_> = selection
                .chosen
                .iter()
                .map(|&i| candidates[i].key)
                .collect();
            assert_eq!(chosen, [0, 1, 2, 3]);
            assert!(selection.deferred.is_empty());
            assert!(selection.skipped.is_empty());
        }
    }

    #[test]
    fn refill_caps_bursts_and_ignores_backwards_time() {
        let now = Instant::now();
        let mut bucket = TokenBucket::new(100, 20.0, now);
        bucket.event(now);
        bucket.refill(now - Duration::from_secs(1));
        assert_eq!(bucket.tokens, 9.0);
        bucket.refill(now + Duration::from_secs(100));
        assert_eq!(bucket.tokens, 10.0);
        let mut candidates: Vec<_> = (0..20)
            .map(|key| candidate(key, Class::Repeat, 0))
            .collect();
        let selected = select(&mut candidates, Some(&mut bucket));
        assert_eq!(selected.chosen.len(), 10);
        assert_eq!(selected.skipped.len(), 10);
    }

    #[test]
    fn low_budget_can_eventually_send_one_message() {
        let now = Instant::now();
        let mut bucket = TokenBucket::new(10, 60.0, now);
        assert!(bucket.take());
        bucket.refill(now + Duration::from_millis(50));
        assert!(!bucket.take());
        bucket.refill(now + Duration::from_millis(100));
        assert!(bucket.take());
    }
}
