//! Wake a frame only for a changing segment or the next envelope boundary.
use super::*;

impl Automation {
    pub fn next_frame(
        &self,
        target: &ParameterRef,
        from: Instant,
        delay_ms: f64,
    ) -> Option<Instant> {
        let musical = delay::shift(from, -delay_ms)?;
        let boundary = self.history.iter().find(|s| s.at > musical).map(|s| s.at);
        let change = self.state(musical).and_then(|state| {
            if !state.clock.is_playing() {
                return None;
            }
            let ticks = state.clock.fractional_ticks_at(musical)?;
            state
                .parameters
                .get(target)?
                .fields
                .iter()
                .filter_map(|key| state.fields.get(key))
                .flat_map(|field| {
                    field.drivers.iter().filter(|driver| {
                        field
                            .overridden
                            .as_ref()
                            .is_none_or(|o| !o.voices.contains(&driver.voice))
                    })
                })
                .filter_map(|driver| driver.next_change(ticks))
                .filter_map(|ticks| {
                    if ticks <= state.clock.fractional_ticks_at(musical)? {
                        Some(musical)
                    } else {
                        state.clock.instant_at(BeatTime(ticks.ceil() as u64))
                    }
                })
                .min()
        });
        boundary
            .into_iter()
            .chain(change)
            .min()
            .and_then(|at| delay::shift(at, delay_ms))
    }
}

/// Keep flat stretches out of the wake search, including an entirely flat envelope.
pub(super) fn changing_segments(points: &[Breakpoint]) -> Arc<[usize]> {
    points
        .windows(2)
        .enumerate()
        .filter_map(|(i, pair)| (pair[0].value != pair[1].value).then_some(i))
        .collect()
}

#[cfg(test)]
thread_local! {
    static SEARCH_COST: std::cell::Cell<[usize; 2]> = const { std::cell::Cell::new([0; 2]) };
}
#[cfg(test)]
impl Automation {
    pub fn take_wake_search_cost() -> [usize; 2] {
        SEARCH_COST.with(|cost| cost.replace([0; 2]))
    }
}

impl Driver {
    fn next_change(&self, ticks: f64) -> Option<f64> {
        if self.end.is_some_and(|end| ticks > end.0 as f64) {
            return None;
        }
        let launch = self.launch.0 as f64;
        if ticks <= launch {
            return Some(launch);
        }
        let region = self.region();
        let timeline = self.timeline();
        let cycle = timeline.fractional_pass(ticks);
        let mut next: Option<f64> = None;
        let mut offer = |at: f64| {
            if at >= ticks && self.end.is_none_or(|end| at <= end.0 as f64) {
                next = Some(next.map_or(at, |old| old.min(at)));
            }
        };
        let points = &self.envelope().points;
        for pass in [cycle, cycle + 1.0] {
            let (start, end) = timeline.content_bounds(pass);
            let base = timeline.fractional_time_at(start, pass);
            if region.looping
                && pass > 0.0
                && self
                    .envelope()
                    .value_at(region.loop_start.0 as f64, self.axis)
                    != self
                        .envelope()
                        .value_at(region.loop_end.0 as f64, self.axis)
            {
                offer(base);
            }
            // The first changing pair whose endpoint has not passed is sufficient:
            // all later pairs start at or after it. A ramp excludes its right endpoint;
            // a discrete change or same-time jump includes it, as the original scan did.
            let right = self.changes.partition_point(|&i| {
                #[cfg(test)]
                SEARCH_COST.with(|cost| {
                    let [search, pairs] = cost.get();
                    cost.set([search + 1, pairs]);
                });
                let left = timeline.fractional_time_at(points[i].time.max(start), pass);
                let right = timeline.fractional_time_at(points[i + 1].time, pass);
                if self.axis == Axis::Discrete || left == right {
                    right < ticks
                } else {
                    right <= ticks
                }
            });
            if let Some(&i) = self.changes.get(right) {
                #[cfg(test)]
                SEARCH_COST.with(|cost| {
                    let [search, pairs] = cost.get();
                    cost.set([search, pairs + 1]);
                });
                let left = timeline.fractional_time_at(points[i].time.max(start), pass);
                let right = timeline.fractional_time_at(points[i + 1].time, pass);
                if points[i].time <= end && points[i + 1].time >= start {
                    if self.axis == Axis::Discrete || points[i].time == points[i + 1].time {
                        if points[i + 1].time <= end {
                            offer(right);
                        }
                    } else if points[i].time < end {
                        let end_tick = timeline.fractional_time_at(end, pass);
                        if ticks < right.min(end_tick) {
                            offer(left.max(ticks));
                        }
                    }
                }
            }
        }
        // A final frame reaches an endpoint even if the preceding frame was before it.
        if let Some(end) = self.end {
            offer(end.0 as f64);
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Frozen pre-M6 scan: oracle for preserving wake deadlines.
    impl Driver {
        fn linear_next_change(&self, ticks: f64) -> Option<f64> {
            if self.end.is_some_and(|end| ticks > end.0 as f64) {
                return None;
            }
            let launch = self.launch.0 as f64;
            if ticks <= launch {
                return Some(launch);
            }
            let length = self.clip().length().0 as f64;
            let cycle = ((ticks - self.origin.0 as f64) / length).floor().max(0.0);
            let mut next: Option<f64> = None;
            let mut offer = |at: f64| {
                if at >= ticks && self.end.is_none_or(|end| at <= end.0 as f64) {
                    next = Some(next.map_or(at, |old| old.min(at)));
                }
            };
            let points = &self.envelope().points;
            for pass in [cycle, cycle + 1.0] {
                let base = self.origin.0 as f64 + pass * length;
                if self.clip().looping && points.first()?.value != points.last()?.value {
                    offer(base);
                }
                for pair in points.windows(2) {
                    if pair[0].value == pair[1].value {
                        continue;
                    }
                    let left = base + pair[0].time.0 as f64;
                    let right = base + pair[1].time.0 as f64;
                    if self.axis == Axis::Discrete || left == right {
                        offer(right);
                    } else if ticks < right {
                        offer(left.max(ticks));
                    }
                }
            }
            // A final frame reaches an endpoint even if the preceding frame was before it.
            if let Some(end) = self.end {
                offer(end.0 as f64);
            }
            next
        }
    }

    #[test]
    fn indexed_wakes_match_the_original_scan_at_boundaries_and_flat_stretches() {
        // All combinations include flat pairs, ramps, reversals and duplicate-time jumps.
        for times in [
            &[0][..],
            &[10][..],
            &[0, 40][..],
            &[0, 10, 20, 30][..],
            &[10, 20, 20, 30][..],
            &[0, 0, 30, 40][..],
            &[0, 20, 40, 40][..],
        ] {
            for values in 0..3_u32.pow(times.len() as u32) {
                let points: Vec<_> = times
                    .iter()
                    .enumerate()
                    .map(|(i, &time)| Breakpoint {
                        time: BeatTime(time),
                        value: ((values / 3_u32.pow(i as u32)) % 3) as f64,
                        curve: 0.5,
                    })
                    .collect();
                for looping in [false, true] {
                    let mut project = super::super::super::tests::project(0.0, looping);
                    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
                    clip.end = BeatTime(40);
                    clip.loop_end = BeatTime(40);
                    clip.lanes = vec![Lane::Envelope(AutomationEnvelope {
                        region: None,
                        target: ArgumentRef::volume(),
                        points: points.clone(),
                    })];
                    let project = Arc::new(project);
                    for axis in [Axis::Continuous, Axis::Stepped, Axis::Discrete] {
                        for end in [None, Some(BeatTime(47)), Some(BeatTime(85))] {
                            let driver = Driver {
                                voice: 1,
                                project: Arc::clone(&project),
                                track: 0,
                                slot: 0,
                                lane: 0,
                                launch: BeatTime(9),
                                origin: BeatTime(5),
                                continuation: None,
                                end,
                                axis,
                                changes: changing_segments(&points),
                            };
                            for half_tick in 0..260 {
                                let ticks = half_tick as f64 / 2.0;
                                assert_eq!(
                                    driver.next_change(ticks),
                                    driver.linear_next_change(ticks),
                                    "times={times:?} values={values} looping={looping} axis={axis:?} end={end:?} ticks={ticks}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
