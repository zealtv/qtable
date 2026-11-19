//! Exact discontinuity deadlines and arithmetic catch-up.
use super::*;

impl Timeline {
    /// Occurrences include endpoints for envelope jumps; hits use the stricter event ranges below.
    pub fn occurrence_at_or_after(self, position: BeatTime, from: BeatTime) -> Option<BeatTime> {
        if let Some(c) = self.continuation {
            let prior = self
                .previous(c)
                .occurrence_at_or_after(position, from)
                .filter(|at| *at < c.at);
            let linear = position
                .0
                .checked_sub(c.position.0)
                .and_then(|offset| c.at.0.checked_add(offset))
                .map(BeatTime)
                .filter(|at| *at >= from && position <= self.region.end);
            return prior.into_iter().chain(linear).min();
        }
        let endpoint = if self.region.looping {
            self.region.loop_end
        } else {
            self.region.end
        };
        let first = (position >= self.region.start && position <= endpoint)
            .then(|| self.time_at(position, 0))
            .flatten()
            .filter(|at| *at >= from);
        let repeat = if position >= self.repeat_start() && position <= endpoint && self.period() > 0
        {
            self.time_at(position, 1).and_then(|at| {
                let pass = from.0.saturating_sub(at.0).div_ceil(self.period());
                at.0.checked_add(pass.checked_mul(self.period())?)
                    .map(BeatTime)
            })
        } else {
            None
        };
        first.into_iter().chain(repeat).min()
    }

    /// A point at Position can be crossed in the intro, but subsequent arrivals are wrap jumps.
    pub fn jump_at_or_after(self, position: BeatTime, from: BeatTime) -> Option<BeatTime> {
        if let Some(c) = self.continuation {
            let prior = self
                .previous(c)
                .jump_at_or_after(position, from)
                .filter(|at| *at < c.at);
            let linear = position
                .0
                .checked_sub(c.position.0)
                .and_then(|offset| c.at.0.checked_add(offset))
                .map(BeatTime)
                .filter(|at| *at >= from && position <= self.region.end);
            return prior.into_iter().chain(linear).min();
        }
        if self.region.looping && position >= self.region.loop_end {
            return None;
        }
        if !self.region.looping {
            return self
                .time_at(position, 0)
                .filter(|at| *at >= from && position <= self.region.end);
        }
        if position == self.region.loop_start {
            return self.time_at(position, 0).filter(|at| *at >= from);
        }
        self.occurrence_at_or_after(position, from)
    }

    pub fn jump_occurrences(self, position: BeatTime, from: BeatTime, until: BeatTime) -> u64 {
        if let Some(c) = self.continuation {
            let prior = self
                .previous(c)
                .jump_occurrences(position, from, until.min(c.at));
            let linear = position
                .0
                .checked_sub(c.position.0)
                .and_then(|offset| c.at.0.checked_add(offset))
                .is_some_and(|at| at >= from.0 && at < until.0 && position <= self.region.end);
            return prior + u64::from(linear);
        }
        if self.region.looping && position >= self.region.loop_end {
            return 0;
        }
        if !self.region.looping {
            return u64::from(
                self.time_at(position, 0)
                    .is_some_and(|at| at >= from && at < until && position <= self.region.end),
            );
        }
        if position == self.region.loop_start {
            return u64::from(
                self.time_at(position, 0)
                    .is_some_and(|at| at >= from && at < until),
            );
        }
        self.occurrences(position, from, until)
    }

    /// Count occurrences without enumerating missed loops.
    pub fn occurrences(self, position: BeatTime, from: BeatTime, until: BeatTime) -> u64 {
        if until <= from {
            return 0;
        }
        if let Some(c) = self.continuation {
            let prior = self
                .previous(c)
                .occurrences(position, from, until.min(c.at));
            let linear = position
                .0
                .checked_sub(c.position.0)
                .and_then(|offset| c.at.0.checked_add(offset))
                .is_some_and(|at| at >= from.0 && at < until.0 && position <= self.region.end);
            return prior.saturating_add(u64::from(linear));
        }
        let endpoint = if self.region.looping {
            self.region.loop_end
        } else {
            self.region.end
        };
        if position > endpoint {
            return 0;
        }
        let first = self
            .time_at(position, 0)
            .is_some_and(|at| at >= from && at < until);
        let repeats = self.time_at(position, 1).map_or(0, |first| {
            if self.period() == 0 || until <= first {
                return 0;
            }
            let low = from.0.saturating_sub(first.0).div_ceil(self.period());
            let high = (until.0 - 1 - first.0) / self.period();
            if high >= low { high - low + 1 } else { 0 }
        });
        u64::from(first).saturating_add(repeats)
    }
    pub fn wraps(self, from: BeatTime, until: BeatTime) -> u64 {
        if let Some(c) = self.continuation {
            return self.previous(c).wraps(from, until.min(c.at));
        }
        if until <= from || self.period() == 0 {
            return 0;
        }
        let Some(first) = self.time_at(self.repeat_start(), 1) else {
            return 0;
        };
        if until <= first {
            return 0;
        }
        let low = from.0.saturating_sub(first.0).div_ceil(self.period());
        let high = (until.0 - 1 - first.0) / self.period();
        if high >= low { high - low + 1 } else { 0 }
    }
}
