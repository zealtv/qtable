//! Frozen pre-refactor arithmetic is deliberately independent of Timeline.
use super::*;

fn old_end(origin: u64, length: u64, passes: u64) -> Option<BeatTime> {
    origin
        .checked_add(length.checked_mul(passes)?)
        .map(BeatTime)
}

fn old_sample(origin: u64, length: u64, end: Option<BeatTime>, ticks: f64) -> f64 {
    let ticks = end.map_or(ticks, |end| ticks.min(end.0 as f64));
    let elapsed = (ticks - origin as f64).max(0.0);
    let length = length as f64;
    if elapsed >= length && end.is_some_and(|end| ticks >= end.0 as f64) {
        let within = elapsed % length;
        if within == 0.0 { length } else { within }
    } else {
        elapsed % length
    }
}

fn old_position(
    origin: u64,
    length: u64,
    passes: u64,
    looping: bool,
    cutoff: Option<BeatTime>,
    beat: BeatTime,
) -> BeatTime {
    let beat = cutoff.map_or(beat, |end| beat.min(end));
    let elapsed = beat.0.saturating_sub(origin);
    if length == 0 {
        return BeatTime::ZERO;
    }
    if !looping && elapsed >= length.saturating_mul(passes) {
        BeatTime(length)
    } else {
        BeatTime(elapsed % length)
    }
}

fn old_cursor(clip: &Clip, order: &[usize], elapsed: u64, passes: u64) -> u64 {
    let count = order.len() as u64;
    let length = clip.length().0;
    if count == 0 || length == 0 {
        return 0;
    }
    if !clip.looping && elapsed / length >= passes {
        return count.saturating_mul(passes);
    }
    let (cycles, within) = (elapsed / length, elapsed % length);
    let before = order
        .iter()
        .take_while(|&&i| clip.events[i].position.0 < within)
        .count() as u64;
    cycles.saturating_mul(count).saturating_add(before)
}

struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        value ^ (value >> 31)
    }
}

#[test]
fn random_timelines_agree_with_the_old_arithmetic() {
    let mut random = Random(0x0005_1434_2026);
    let project = crate::scheduler::tests::project(0.0, true);
    let mut clip = project.tracks[0].clip(0).unwrap().clone();
    let order = crate::scheduler::rebase::event_order(&clip);
    for case in 0..20_000 {
        let length = 1 + random.next() % 1_000_000;
        let origin = random.next() % 10_000_000;
        let launch = origin + random.next() % (length * 3);
        let passes = 1 + random.next() % 100;
        let looping = random.next().is_multiple_of(2);
        let cutoff = random
            .next()
            .is_multiple_of(2)
            .then(|| BeatTime(origin + random.next() % (length * 102)));
        let timeline = Timeline::new(
            BeatTime(launch),
            BeatTime(origin),
            passes,
            cutoff,
            Region {
                start: BeatTime::ZERO,
                end: BeatTime(length),
                loop_start: BeatTime::ZERO,
                loop_end: BeatTime(length),
                looping,
            },
        );
        let natural = old_end(origin, length, passes);
        let end = cutoff
            .into_iter()
            .chain((!looping).then_some(natural).flatten())
            .min();
        assert_eq!(timeline.pass_end(), natural);
        assert_eq!(timeline.end(), end);
        clip.end = BeatTime(length);
        clip.loop_end = BeatTime(length);
        clip.looping = looping;
        for (index, event) in clip.events.iter_mut().enumerate() {
            event.position = BeatTime(match index % 3 {
                0 => 0,
                1 => length - 1,
                _ => random.next() % length,
            });
        }
        let order = {
            let mut order = order.clone();
            order.sort_by_key(|&i| clip.events[i].position);
            order
        };
        let beat = random.next() % (origin + length * 103);
        // Exact boundaries, pre-origin and pre-launch probes accompany each random query.
        for beat in [
            beat,
            0,
            origin,
            launch,
            origin + length,
            origin + length * passes,
            cutoff.map_or(launch, |b| b.0),
        ] {
            let at = BeatTime(beat);
            assert_eq!(
                timeline.position_at(at),
                old_position(origin, length, passes, looping, cutoff, at),
                "case {case}"
            );
            assert_eq!(timeline.pass_at(at), beat.saturating_sub(origin) / length);
            assert_eq!(
                timeline.dispatched_events(&clip, &order, at),
                old_cursor(&clip, &order, beat.saturating_sub(origin), passes)
            );
            for ticks in [beat as f64, beat as f64 + 0.25, beat as f64 + 0.999] {
                assert_eq!(
                    timeline.sample_position(ticks),
                    old_sample(origin, length, end, ticks),
                    "case {case}"
                );
                let clamped = end.map_or(ticks, |end| ticks.min(end.0 as f64));
                let elapsed = (clamped - origin as f64).max(0.0);
                let outgoing = if elapsed > 0.0 && elapsed % length as f64 == 0.0 {
                    length as f64
                } else {
                    old_sample(origin, length, end, clamped)
                };
                assert_eq!(timeline.outgoing_position(ticks), outgoing);
                let pass = ((ticks - origin as f64) / length as f64).floor().max(0.0);
                assert_eq!(timeline.fractional_pass(ticks), pass);
                assert_eq!(
                    timeline.fractional_time_at(BeatTime(length / 2), pass),
                    origin as f64 + pass * length as f64 + (length / 2) as f64
                );
            }
        }
        let position = random.next() % (length + 1); // jumps may include the endpoint
        let pass = random.next() % 103;
        assert_eq!(
            timeline.time_at(BeatTime(position), pass),
            origin
                .checked_add(pass.checked_mul(length).unwrap())
                .and_then(|b| b.checked_add(position))
                .map(BeatTime)
        );
        let first = origin + position;
        let next_pass = beat.saturating_sub(first).div_ceil(length);
        assert_eq!(
            timeline.occurrence_at_or_after(BeatTime(position), BeatTime(beat)),
            Some(BeatTime(first + next_pass * length))
        );
        assert_eq!(
            timeline.next_wrap_after(BeatTime(beat)),
            Some(BeatTime(
                origin + (beat.saturating_sub(origin) / length + 1) * length
            ))
        );
        let until = beat + random.next() % (length * 4);
        let expected = (0..110)
            .filter(|pass| {
                let at = first + pass * length;
                at >= beat && at < until
            })
            .count() as u64;
        assert_eq!(
            timeline.occurrences(BeatTime(position), BeatTime(beat), BeatTime(until)),
            expected
        );
        let elapsed = launch.saturating_sub(origin);
        assert_eq!(
            timeline.legato_origin(BeatTime(origin)),
            BeatTime(launch - if looping { elapsed % length } else { elapsed })
        );
        assert_eq!(
            timeline.linked_duration(7, 3),
            Some(if looping { length * 7 / 3 } else { length })
        );
    }
}

#[test]
fn checked_and_saturating_arithmetic_keep_their_original_limits() {
    for length in [1, 960, u64::MAX / 2, u64::MAX] {
        for origin in [0, 1, u64::MAX / 2, u64::MAX] {
            for passes in [1, 2, u64::MAX] {
                for looping in [false, true] {
                    let timeline = Timeline::new(
                        BeatTime(origin),
                        BeatTime(origin),
                        passes,
                        Some(BeatTime(u64::MAX - 1)),
                        Region {
                            start: BeatTime::ZERO,
                            end: BeatTime(length),
                            loop_start: BeatTime::ZERO,
                            loop_end: BeatTime(length),
                            looping,
                        },
                    );
                    assert_eq!(timeline.pass_end(), old_end(origin, length, passes));
                    for position in [0, 1, length] {
                        assert_eq!(
                            timeline.time_at(BeatTime(position), passes),
                            length
                                .checked_mul(passes)
                                .and_then(|duration| origin.checked_add(duration))
                                .and_then(|b| b.checked_add(position))
                                .map(BeatTime)
                        );
                        let from = u64::MAX;
                        let expected = origin
                            .checked_add(position)
                            .and_then(|first| {
                                let pass = from.saturating_sub(first).div_ceil(length);
                                first.checked_add(pass.checked_mul(length)?)
                            })
                            .map(BeatTime);
                        assert_eq!(
                            timeline.occurrence_at_or_after(BeatTime(position), BeatTime(from)),
                            expected
                        );
                    }
                    for beat in [0, origin, u64::MAX - 1, u64::MAX] {
                        assert_eq!(
                            timeline.position_at(BeatTime(beat)),
                            old_position(
                                origin,
                                length,
                                passes,
                                looping,
                                timeline.cutoff,
                                BeatTime(beat)
                            )
                        );
                        assert_eq!(
                            timeline.sample_position(beat as f64),
                            old_sample(origin, length, timeline.end(), beat as f64)
                        );
                    }
                }
            }
        }
    }
    // PublishedTrack historically tolerates an empty region although stored clips cannot have one.
    let empty = Timeline::new(
        BeatTime::ZERO,
        BeatTime::ZERO,
        1,
        None,
        Region {
            start: BeatTime::ZERO,
            end: BeatTime::ZERO,
            loop_start: BeatTime::ZERO,
            loop_end: BeatTime::ZERO,
            looping: true,
        },
    );
    assert_eq!(empty.position_at(BeatTime(100)), BeatTime::ZERO);
}

#[test]
fn hits_remain_half_open_and_stopped_sampling_holds_the_outgoing_endpoint() {
    let region = Region {
        start: BeatTime::ZERO,
        end: BeatTime(40),
        loop_start: BeatTime::ZERO,
        loop_end: BeatTime(40),
        looping: true,
    };
    let running = Timeline::new(BeatTime(9), BeatTime(5), 1, None, region);
    let stopped = Timeline::new(BeatTime(9), BeatTime(5), 1, Some(BeatTime(85)), region);
    assert_eq!(running.sample_position(85.0), 0.0);
    assert_eq!(stopped.sample_position(85.0), 40.0);
    assert_eq!(stopped.sample_position(100.0), 40.0);
    assert_eq!(running.outgoing_position(85.0), 40.0);
    assert_eq!(stopped.position_at(BeatTime(85)), BeatTime::ZERO);
    assert_eq!(
        running.occurrences(BeatTime::ZERO, BeatTime(5), BeatTime(85)),
        2
    );
    assert_eq!(
        running.occurrence_at_or_after(BeatTime::ZERO, BeatTime(85)),
        Some(BeatTime(85))
    );
}

#[test]
fn marker_hit_cursors_and_catch_up_match_a_tick_by_tick_oracle() {
    let mut random = Random(0x2026_1025_0001);
    let project = crate::scheduler::tests::project(0.0, true);
    for case in 0..500 {
        let mut clip = project.clip(crate::model::TrackId(1), 0).unwrap().clone();
        clip.start = BeatTime(random.next() % 20);
        clip.end = BeatTime(clip.start.0 + 1 + random.next() % 20);
        clip.loop_start = BeatTime(random.next() % 20);
        clip.loop_end = BeatTime(clip.loop_start.0 + 1 + random.next() % 20);
        clip.looping = case % 2 == 0;
        let r = clip.region().normalised();
        clip.start = r.start;
        // Include duplicate-time hits and retained content on both sides of the markers.
        let event = clip.events[0].clone();
        clip.events = (0..12)
            .map(|_| crate::model::Event {
                position: BeatTime(random.next() % 40),
                ..event.clone()
            })
            .collect();
        let order = crate::scheduler::rebase::event_order(&clip);
        let launch = BeatTime(random.next() % 100);
        let running = Timeline::new(launch, launch, 1, None, r);
        for off_at in [None, Some(40)] {
            let position = |elapsed: u64| {
                if r.looping {
                    let first = r.loop_end.0 - r.start.0;
                    Some(if elapsed < first {
                        r.start.0 + elapsed
                    } else {
                        r.loop_start.0 + (elapsed - first) % (r.loop_end.0 - r.loop_start.0)
                    })
                } else {
                    (elapsed < r.end.0 - r.start.0).then_some(r.start.0 + elapsed)
                }
            };
            let continuation = off_at.filter(|_| r.looping).map(|at| Continuation {
                at: BeatTime(launch.0 + at),
                position: BeatTime(position(at).unwrap()),
                previous: r,
            });
            let timeline = if continuation.is_some() {
                Timeline::new(
                    launch,
                    launch,
                    1,
                    None,
                    Region {
                        looping: false,
                        ..r
                    },
                )
                .continuing(continuation)
            } else {
                running
            };
            let mut expected = Vec::new();
            for elapsed in 0..160 {
                assert_eq!(
                    timeline.dispatched_events(&clip, &order, BeatTime(launch.0 + elapsed)),
                    expected.len() as u64,
                    "case={case} off={off_at:?} elapsed={elapsed} r={r:?}"
                );
                let pos = if let Some(c) = continuation.filter(|c| launch.0 + elapsed >= c.at.0) {
                    let pos = c.position.0 + launch.0 + elapsed - c.at.0;
                    (pos < r.end.0).then_some(pos)
                } else {
                    position(elapsed)
                };
                for &index in &order {
                    if pos == Some(clip.events[index].position.0) {
                        expected.push((index, BeatTime(launch.0 + elapsed)));
                    }
                }
            }
            for (cursor, event) in expected.into_iter().enumerate() {
                assert_eq!(
                    timeline.event_at(&clip, &order, cursor as u64),
                    Some(event),
                    "case={case} off={off_at:?} r={r:?}"
                );
            }
        }
    }
}

fn region(start: u64, end: u64, loop_start: u64, loop_end: u64, looping: bool) -> Region {
    Region {
        start: BeatTime(start),
        end: BeatTime(end),
        loop_start: BeatTime(loop_start),
        loop_end: BeatTime(loop_end),
        looping,
    }
}

#[test]
fn pass_progress_follows_a_loop_brace_that_does_not_start_at_zero() {
    // Start and brace both at 400..800: content positions 400..800 map onto 0..1.
    let timeline = Timeline::new(
        BeatTime(100),
        BeatTime(100),
        1,
        None,
        region(400, 800, 400, 800, true),
    );
    for (beat, expected) in [(100, 0.0), (200, 0.25), (300, 0.5), (400, 0.75), (500, 0.0)] {
        assert_eq!(
            timeline.pass_progress(BeatTime(beat)),
            expected,
            "beat {beat}"
        );
    }
    // The content position the grid used to divide by the loop length sits above it.
    assert_eq!(timeline.position_at(BeatTime(300)), BeatTime(600));
}

#[test]
fn pass_progress_counts_an_intro_as_its_own_pass() {
    // Start 0 with the brace at 400..800: one pass over 0..800, then passes over 400..800.
    let timeline = Timeline::new(
        BeatTime::ZERO,
        BeatTime::ZERO,
        1,
        None,
        region(0, 800, 400, 800, true),
    );
    assert_eq!(timeline.pass_progress(BeatTime(0)), 0.0);
    assert_eq!(timeline.pass_progress(BeatTime(200)), 0.25);
    assert_eq!(timeline.pass_progress(BeatTime(400)), 0.5);
    assert_eq!(timeline.pass_progress(BeatTime(799)), 799.0 / 800.0);
    // The loop begins: progress restarts over the brace alone.
    assert_eq!(timeline.pass_progress(BeatTime(800)), 0.0);
    assert_eq!(timeline.pass_progress(BeatTime(1000)), 0.5);
    assert_eq!(timeline.pass_progress(BeatTime(1200)), 0.0);
    // A Start inside the brace plays on to the loop end first.
    let inside = Timeline::new(
        BeatTime::ZERO,
        BeatTime::ZERO,
        1,
        None,
        region(600, 800, 400, 800, true),
    );
    assert_eq!(inside.pass_progress(BeatTime(100)), 0.5);
    assert_eq!(inside.pass_progress(BeatTime(200)), 0.0);
    assert_eq!(inside.pass_progress(BeatTime(300)), 0.25);
}

#[test]
fn pass_progress_runs_to_the_end_with_loop_off_and_after_a_stop_cutoff() {
    let once = Timeline::new(
        BeatTime::ZERO,
        BeatTime::ZERO,
        1,
        None,
        region(400, 800, 0, 100, false),
    );
    assert_eq!(once.pass_progress(BeatTime(100)), 0.25);
    assert_eq!(once.pass_progress(BeatTime(400)), 1.0);
    assert_eq!(once.pass_progress(BeatTime(9_000)), 1.0);
    let cut = Timeline::new(
        BeatTime::ZERO,
        BeatTime::ZERO,
        1,
        Some(BeatTime(300)),
        region(400, 800, 400, 800, true),
    );
    assert_eq!(cut.pass_progress(BeatTime(900)), 0.75);
}

#[test]
fn pass_progress_after_loop_off_runs_from_the_take_over_to_end() {
    let previous = region(400, 800, 400, 800, true);
    let linear = region(400, 800, 400, 800, false);
    // Loop was switched off at beat 200, at content position 600: 200 of 400 left to End.
    let timeline = Timeline::new(BeatTime::ZERO, BeatTime::ZERO, 1, None, linear).continuing(Some(
        Continuation {
            at: BeatTime(200),
            position: BeatTime(600),
            previous,
        },
    ));
    assert_eq!(timeline.pass_progress(BeatTime(100)), 0.25);
    assert_eq!(timeline.pass_progress(BeatTime(200)), 0.0);
    assert_eq!(timeline.pass_progress(BeatTime(300)), 0.5);
    assert_eq!(timeline.pass_progress(BeatTime(400)), 1.0);
}
