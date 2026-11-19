//! Sequential hit cursors across the intro, repeats and a live Loop-off.
use super::*;

impl Timeline {
    fn range(
        self,
        clip: &Clip,
        order: &[usize],
        start: BeatTime,
        end: BeatTime,
    ) -> std::ops::Range<usize> {
        let low = order.partition_point(|&i| clip.events[i].position < start);
        let high = order.partition_point(|&i| clip.events[i].position < end);
        low..high.max(low)
    }
    fn first_range(self, clip: &Clip, order: &[usize]) -> std::ops::Range<usize> {
        self.range(
            clip,
            order,
            self.region.start,
            if self.region.looping {
                self.region.loop_end
            } else {
                self.region.end
            },
        )
    }
    fn repeat_range(self, clip: &Clip, order: &[usize]) -> std::ops::Range<usize> {
        self.range(
            clip,
            order,
            self.repeat_start(),
            if self.region.looping {
                self.region.loop_end
            } else {
                self.region.end
            },
        )
    }

    /// Sequential cursor → event index and musical deadline, including a distinct intro pass.
    pub fn event_at(self, clip: &Clip, order: &[usize], cursor: u64) -> Option<(usize, BeatTime)> {
        if let Some(c) = self.continuation {
            let old = self.previous(c);
            let prefix = old.dispatched_events(clip, order, c.at);
            if cursor < prefix {
                return old.event_at(clip, order, cursor);
            }
            let range = self.range(clip, order, c.position, self.region.end);
            let index = *order.get(
                range
                    .start
                    .checked_add(usize::try_from(cursor - prefix).ok()?)?,
            )?;
            if clip.events[index].position >= self.region.end {
                return None;
            }
            return Some((
                index,
                BeatTime(
                    c.at.0
                        .checked_add(clip.events[index].position.0 - c.position.0)?,
                ),
            ));
        }
        let first = self.first_range(clip, order);
        let (offset, pass, range) = if cursor < first.len() as u64 {
            (cursor, 0, first)
        } else {
            let repeat = self.repeat_range(clip, order);
            let count = repeat.len() as u64;
            let remaining = cursor - first.len() as u64;
            let pass = remaining.checked_div(count)?.checked_add(1)?;
            if !self.region.looping && pass >= self.passes {
                return None;
            }
            (remaining % count, pass, repeat)
        };
        let index = order[range.start + offset as usize];
        Some((index, self.time_at(clip.events[index].position, pass)?))
    }
    pub fn event_limit(self, clip: &Clip, order: &[usize]) -> u64 {
        if let Some(c) = self.continuation {
            return self
                .previous(c)
                .dispatched_events(clip, order, c.at)
                .saturating_add(self.range(clip, order, c.position, self.region.end).len() as u64);
        }
        (self.first_range(clip, order).len() as u64).saturating_add(
            (self.repeat_range(clip, order).len() as u64)
                .saturating_mul(self.passes.saturating_sub(1)),
        )
    }
    /// Hits strictly before `beat` have gone; boundary hits remain pending.
    pub fn dispatched_events(self, clip: &Clip, order: &[usize], beat: BeatTime) -> u64 {
        if let Some(c) = self.continuation {
            let old = self.previous(c);
            if beat < c.at {
                return old.dispatched_events(clip, order, beat);
            }
            let position = c
                .position
                .0
                .saturating_add(beat.0 - c.at.0)
                .min(self.region.end.0);
            return old.dispatched_events(clip, order, c.at).saturating_add(
                self.range(clip, order, c.position, BeatTime(position))
                    .len() as u64,
            );
        }
        let pass = self.pass_at(beat);
        if !self.region.looping && pass >= self.passes {
            return self.event_limit(clip, order);
        }
        let first = self.first_range(clip, order);
        let repeat = self.repeat_range(clip, order);
        let elapsed = self.elapsed(beat);
        let position = if pass == 0 {
            self.region.start.0.saturating_add(elapsed)
        } else {
            self.repeat_start().0 + (elapsed - self.first_length()) % self.period()
        };
        let range = if pass == 0 {
            first.clone()
        } else {
            repeat.clone()
        };
        let before = order[range].partition_point(|&i| clip.events[i].position.0 < position) as u64;
        if pass == 0 {
            before
        } else {
            (first.len() as u64)
                .saturating_add((pass - 1).saturating_mul(repeat.len() as u64))
                .saturating_add(before)
        }
    }
}
