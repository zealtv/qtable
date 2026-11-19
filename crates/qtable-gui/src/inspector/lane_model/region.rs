//! Static source-to-launch projection for unlinked overview curves.
use super::*;

impl EnvelopeRow {
    /// Static launch-time projection for the read-only overview, on the clip's content axis.
    /// It holds at End for a one-shot and includes both sides of each wrap/jump.
    /// The editor's own lane view continues to draw editable source coordinates.
    pub(crate) fn overview_samples(
        &self,
        start: BeatTime,
        length: BeatTime,
        pixels: f32,
    ) -> Vec<(f64, f64)> {
        let Some(region) = self.region.map(|r| r.normalised()) else {
            return self.samples(length, length.0 as f64 / f64::from(pixels).max(1.0));
        };
        let end = length.0.saturating_sub(start.0) as f64;
        let limit = if region.looping {
            region.loop_end.0
        } else {
            region.end.0
        };
        let first = limit.saturating_sub(region.start.0) as f64;
        let period = (region.loop_end.0 - region.loop_start.0) as f64;
        let mut times = vec![0.0, end];
        let step = (end / f64::from(pixels).max(1.0)).max(1.0);
        let mut tick = step;
        while tick < end {
            times.push(tick);
            tick += step;
        }
        let mut boundaries = vec![first];
        for point in &self.points {
            let t = point.time.0;
            if t >= region.start.0 && t <= limit {
                boundaries.push((t - region.start.0) as f64);
            }
        }
        if region.looping {
            // At subpixel loop lengths the pixel sampling is the useful representation;
            // avoid work proportional to an arbitrarily long editor span.
            let passes = ((end - first).max(0.0) / period)
                .ceil()
                .min(f64::from(pixels).max(1.0)) as usize;
            for pass in 0..=passes {
                let base = first + pass as f64 * period;
                boundaries.push(base);
                for point in &self.points {
                    if point.time >= region.loop_start && point.time < region.loop_end {
                        boundaries.push(base + (point.time.0 - region.loop_start.0) as f64);
                    }
                }
            }
        }
        for t in boundaries.into_iter().filter(|t| *t >= 0.0 && *t <= end) {
            if t > 0.0 {
                times.push(t - 0.001);
            }
            times.push(t);
        }
        times.sort_by(f64::total_cmp);
        times.dedup();
        let envelope = qtable_engine::model::AutomationEnvelope {
            region: None,
            target: self.target.clone(),
            points: self.points.clone(),
        };
        let axis = if self.axis == Axis::Stepped {
            Axis::Continuous
        } else {
            self.axis
        };
        times
            .into_iter()
            .filter_map(|elapsed| {
                let content = if elapsed < first {
                    region.start.0 as f64 + elapsed
                } else if region.looping {
                    region.loop_start.0 as f64 + (elapsed - first) % period
                } else {
                    region.end.0 as f64
                };
                envelope
                    .value_at(content, axis)
                    .map(|value| (start.0 as f64 + elapsed, value))
            })
            .collect()
    }

    /// Where the overview draws the lane's own content `position`: its first pass from the
    /// clip's Start, or for a looped intro's skipped part, its first repeat.
    pub(crate) fn overview_time(&self, start: BeatTime, position: BeatTime) -> BeatTime {
        let Some(region) = self.region.map(|r| r.normalised()) else {
            return position;
        };
        let limit = if region.looping {
            region.loop_end
        } else {
            region.end
        };
        let elapsed = if position >= region.start {
            position.0.min(limit.0) - region.start.0
        } else if region.looping && position >= region.loop_start {
            limit.0 - region.start.0 + (position.0 - region.loop_start.0)
        } else {
            0
        };
        BeatTime(start.0 + elapsed)
    }

    pub(crate) fn overview_polyline(&self, start: BeatTime, lane: &PointLane) -> Path {
        let samples = self.overview_samples(start, lane.length(), lane.plot.width);
        Path::new(|path| {
            for (index, (ticks, value)) in samples.into_iter().enumerate() {
                let at = Point::new(lane.time.x_at(ticks), lane.y(value));
                if index == 0 {
                    path.move_to(at);
                } else {
                    path.line_to(at);
                }
            }
        })
    }
}
