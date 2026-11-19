//! Musical time ↔ content coordinates. No clocks, output delays or project mutation.
pub(super) use crate::model::region::Region;
use crate::model::{BeatTime, Clip};

impl Region {
    pub(super) fn of(clip: &Clip) -> Self {
        clip.region().normalised()
    }
}

/// A Loop-off keeps the preceding stream for delayed sends, then runs linearly to End.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Continuation {
    pub at: BeatTime,
    pub position: BeatTime,
    pub previous: Region,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Timeline {
    launch: BeatTime,
    origin: BeatTime,
    passes: u64,
    cutoff: Option<BeatTime>,
    region: Region,
    end: Option<BeatTime>,
    continuation: Option<Continuation>,
}

impl Timeline {
    pub fn new(
        launch: BeatTime,
        origin: BeatTime,
        passes: u64,
        cutoff: Option<BeatTime>,
        region: Region,
    ) -> Self {
        let mut timeline = Self {
            launch,
            origin,
            passes,
            cutoff,
            region,
            end: cutoff,
            continuation: None,
        };
        timeline.refresh_end();
        timeline
    }

    pub fn continuing(mut self, continuation: Option<Continuation>) -> Self {
        self.continuation = continuation;
        if continuation.is_some() {
            self.refresh_end();
        }
        self
    }

    fn refresh_end(&mut self) {
        self.end = self
            .cutoff
            .into_iter()
            .chain((!self.region.looping).then(|| self.pass_end()).flatten())
            .min();
    }

    /// Historical envelope drivers already know their effective end, including queued stops.
    pub fn held(launch: BeatTime, origin: BeatTime, end: Option<BeatTime>, region: Region) -> Self {
        Self {
            launch,
            origin,
            passes: 1,
            cutoff: end,
            region,
            end,
            continuation: None,
        }
    }

    fn first_length(self) -> u64 {
        (if self.region.looping {
            self.region.loop_end
        } else {
            self.region.end
        })
        .0
        .saturating_sub(self.region.start.0)
    }
    fn repeat_start(self) -> BeatTime {
        if self.region.looping {
            self.region.loop_start
        } else {
            self.region.start
        }
    }
    fn period(self) -> u64 {
        self.region.length().0
    }

    fn previous(self, c: Continuation) -> Self {
        Self::new(
            self.launch,
            self.origin,
            self.passes,
            self.cutoff,
            c.previous,
        )
    }

    /// End of the requested passes, independently of Loop and an earlier cutoff.
    pub fn pass_end(self) -> Option<BeatTime> {
        if let Some(c) = self.continuation {
            return c
                .at
                .0
                .checked_add(self.region.end.0.saturating_sub(c.position.0))
                .map(BeatTime);
        }
        self.origin
            .0
            .checked_add(self.first_length())?
            .checked_add(self.passes.checked_sub(1)?.checked_mul(self.period())?)
            .map(BeatTime)
    }
    pub fn end(self) -> Option<BeatTime> {
        self.end
    }
    pub fn elapsed(self, beat: BeatTime) -> u64 {
        beat.0.saturating_sub(self.origin.0)
    }

    pub fn pass_at(self, beat: BeatTime) -> u64 {
        let elapsed = self.elapsed(beat);
        if elapsed < self.first_length() || self.period() == 0 {
            0
        } else {
            1 + (elapsed - self.first_length()) / self.period()
        }
    }

    pub fn time_at(self, position: BeatTime, pass: u64) -> Option<BeatTime> {
        if pass == 0 {
            self.origin
                .0
                .checked_add(position.0.checked_sub(self.region.start.0)?)
                .map(BeatTime)
        } else {
            self.origin
                .0
                .checked_add(self.first_length())?
                .checked_add((pass - 1).checked_mul(self.period())?)?
                .checked_add(position.0.checked_sub(self.repeat_start().0)?)
                .map(BeatTime)
        }
    }

    pub fn fractional_pass(self, ticks: f64) -> f64 {
        let elapsed = (ticks - self.origin.0 as f64).max(0.0);
        if elapsed < self.first_length() as f64 {
            0.0
        } else {
            1.0 + ((elapsed - self.first_length() as f64) / self.period() as f64).floor()
        }
    }
    pub fn fractional_time_at(self, position: BeatTime, pass: f64) -> f64 {
        if let Some(c) = self.continuation {
            return c.at.0 as f64 + position.0 as f64 - c.position.0 as f64;
        }
        if pass == 0.0 {
            self.origin.0 as f64 + position.0 as f64 - self.region.start.0 as f64
        } else {
            self.origin.0 as f64
                + self.first_length() as f64
                + (pass - 1.0) * self.period() as f64
                + position.0 as f64
                - self.repeat_start().0 as f64
        }
    }

    /// Active content bounds of a pass, also used to clip the indexed envelope wake search.
    pub fn content_bounds(self, pass: f64) -> (BeatTime, BeatTime) {
        if let Some(c) = self.continuation {
            return (c.position, self.region.end);
        }
        (
            if pass == 0.0 {
                self.region.start
            } else {
                self.repeat_start()
            },
            if self.region.looping {
                self.region.loop_end
            } else {
                self.region.end
            },
        )
    }

    pub fn next_wrap_after(self, beat: BeatTime) -> Option<BeatTime> {
        if let Some(c) = self.continuation {
            return self
                .previous(c)
                .next_wrap_after(beat)
                .filter(|at| *at < c.at);
        }
        self.time_at(self.repeat_start(), self.pass_at(beat).checked_add(1)?)
    }

    pub fn position_at(self, beat: BeatTime) -> BeatTime {
        let beat = self.cutoff.map_or(beat, |end| beat.min(end));
        if let Some(c) = self.continuation {
            if beat < c.at {
                return self.previous(c).position_at(beat);
            }
            return BeatTime(
                c.position
                    .0
                    .saturating_add(beat.0 - c.at.0)
                    .min(self.region.end.0.max(c.position.0)),
            );
        }
        if self.period() == 0 {
            return self.region.start;
        }
        let elapsed = self.elapsed(beat);
        if !self.region.looping && elapsed >= self.period().saturating_mul(self.passes) {
            self.region.end
        } else if elapsed < self.first_length() {
            BeatTime(self.region.start.0.saturating_add(elapsed))
        } else {
            BeatTime(self.repeat_start().0 + (elapsed - self.first_length()) % self.period())
        }
    }

    /// How far through the current pass the clip has played, from 0.0 to 1.0.
    /// The first pass runs from Start to the loop end (or End with Loop off), so an
    /// intro before the loop brace counts as its own pass; later passes run over the
    /// loop brace. Independent of where the brace sits.
    pub fn pass_progress(self, beat: BeatTime) -> f64 {
        let beat = self.cutoff.map_or(beat, |end| beat.min(end));
        if let Some(c) = self.continuation {
            if beat < c.at {
                return self.previous(c).pass_progress(beat);
            }
            let length = self.region.end.0.saturating_sub(c.position.0);
            let played = beat.0 - c.at.0;
            return fraction(played, length);
        }
        let (first, period) = (self.first_length(), self.period());
        if period == 0 {
            return 0.0;
        }
        let elapsed = self.elapsed(beat);
        if !self.region.looping && elapsed >= period.saturating_mul(self.passes) {
            1.0
        } else if elapsed < first {
            fraction(elapsed, first)
        } else {
            fraction((elapsed - first) % period, period)
        }
    }

    pub fn sample_position(self, ticks: f64) -> f64 {
        let ticks = self.end.map_or(ticks, |end| ticks.min(end.0 as f64));
        if let Some(c) = self.continuation {
            if ticks < c.at.0 as f64 {
                return self.previous(c).sample_position(ticks);
            }
            return (c.position.0 as f64 + ticks - c.at.0 as f64)
                .min(self.region.end.0.max(c.position.0) as f64);
        }
        let elapsed = (ticks - self.origin.0 as f64).max(0.0);
        let first = self.first_length() as f64;
        if elapsed < first {
            return self.region.start.0 as f64 + elapsed;
        }
        let within = if self.region.start == self.repeat_start() {
            elapsed % self.period() as f64
        } else {
            (elapsed - first) % self.period() as f64
        };
        if self.end.is_some_and(|end| ticks >= end.0 as f64) && within == 0.0 {
            if self.region.looping {
                self.region.loop_end.0 as f64
            } else {
                self.region.end.0 as f64
            }
        } else {
            self.repeat_start().0 as f64 + within
        }
    }
    pub fn outgoing_position(self, ticks: f64) -> f64 {
        let ticks = self.end.map_or(ticks, |end| ticks.min(end.0 as f64));
        if self.continuation.is_some() {
            return self.sample_position(ticks);
        }
        let elapsed = (ticks - self.origin.0 as f64).max(0.0);
        let within = if self.region.start == self.repeat_start() {
            elapsed % self.period() as f64
        } else {
            (elapsed - self.first_length() as f64) % self.period() as f64
        };
        if elapsed > 0.0 && elapsed >= self.first_length() as f64 && within == 0.0 {
            if self.region.looping {
                self.region.loop_end.0 as f64
            } else {
                self.region.end.0 as f64
            }
        } else {
            self.sample_position(ticks)
        }
    }

    /// Keep elapsed take-over time; discard only whole destination loops after its first pass.
    pub fn legato_origin(self, source_origin: BeatTime) -> BeatTime {
        let elapsed = self.launch.0.saturating_sub(source_origin.0);
        let elapsed = if self.region.looping && elapsed >= self.first_length() {
            self.first_length() + (elapsed - self.first_length()) % self.period()
        } else {
            elapsed
        };
        // For the default region an exact wrap can use launch itself (the old origin rule).
        let elapsed = if self.region.start == self.region.loop_start && self.region.looping {
            elapsed % self.period()
        } else {
            elapsed
        };
        BeatTime(self.launch.0 - elapsed)
    }
    pub fn linked_duration(self, numerator: u64, denominator: u64) -> Option<u64> {
        if self.continuation.is_some() {
            return Some(self.pass_end()?.0.saturating_sub(self.launch.0));
        }
        if !self.region.looping {
            return Some(self.region.end.0 - self.region.start.0);
        }
        let first = self.first_length();
        if numerator <= denominator {
            first.checked_mul(numerator).map(|v| v / denominator)
        } else {
            first.checked_add(self.period().checked_mul(numerator - denominator)? / denominator)
        }
    }
}

mod events;
mod occurrences;

/// `part` of `whole`, held to 0..=1; an empty `whole` counts as complete.
fn fraction(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        1.0
    } else {
        (part as f64 / whole as f64).min(1.0)
    }
}

#[cfg(test)]
mod tests;
