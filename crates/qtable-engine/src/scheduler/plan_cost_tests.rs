//! Cost probe for `plan()` on a realistic project.

use super::tests::project;
use super::*;
use crate::model::*;
use std::time::Instant;

/// 8 tracks x 16 clips x 32 events, a large project for the plan-cost check.
fn large_project() -> Project {
    let mut base = project(0.0, true);
    let template = base.tracks[0].slots[0].clip.clone().unwrap();
    let mut next_clip = 1;
    base.tracks = (1..=8)
        .map(|t| {
            let mut track =
                Track::new(TrackId(t), format!("track {t}"), 16).with_path(format!("/track{t}"));
            track.group = Some(TrackId(100));
            track.route = Route::Group;
            let events: Vec<Event> = (0..32)
                .map(|i| {
                    let path = OscAddress::Relative(format!("/event/{i}"));
                    let arguments = vec![OscArgument::Int(i as i32)];
                    track.hit(BeatTime(i * 100), path, arguments, || DeviceId(t))
                })
                .collect();
            let target = track.devices[0].target_mut().unwrap();
            target.path = Path::new("/target").unwrap();
            for slot in &mut track.slots {
                let mut clip = template.clone();
                clip.id = ClipId(next_clip);
                next_clip += 1;
                clip.end = BeatTime(3840);
                clip.loop_end = BeatTime(3840);
                clip.events = events.clone();
                slot.clip = Some(clip);
            }
            track
        })
        .collect();
    let mut group = Track::new_group(TrackId(100), "group", 16).with_path("/group");
    group.route = Route::single(TrackId(201));
    base.tracks.insert(0, group);
    base.outputs[0].path = Path::new("/out").unwrap();
    base.scenes = (1..=16)
        .map(|i| Scene {
            id: SceneId(i),
            name: "scene".into(),
            follow_actions: SceneFollowActions::default(),
        })
        .collect();
    base
}

/// Median and maximum of `samples`, sorted in place.
fn median_max(samples: &mut [f64]) -> (f64, f64) {
    samples.sort_by(f64::total_cmp);
    (samples[samples.len() / 2], samples[samples.len() - 1])
}

#[test]
fn plan_cost_probe_composes_relative_target_and_group_paths() {
    let project = large_project();
    let tracks: Vec<_> = project.tracks.iter().filter(|t| !t.is_group()).collect();
    assert_eq!(tracks.len(), 8);
    assert!(tracks.iter().all(|t| t.slots.len() == 16));
    assert!(tracks.iter().all(|t| {
        t.slots
            .iter()
            .all(|s| s.clip.as_ref().is_some_and(|c| c.events.len() == 32))
    }));
    let mut planner = Planner::new(project, Duration::from_millis(12)).unwrap();
    for track in 1..=8 {
        planner
            .launch_at(TrackId(track), 0, BeatTime::ZERO)
            .unwrap();
    }
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let planned = planner.plan(&clock, start);
    assert_eq!(planned.len(), 8);
    for (index, event) in planned.iter().enumerate() {
        let PlannedAction::Send {
            packet: Some(OscPacket::Message(message)),
            ..
        } = &event.action
        else {
            panic!("expected a packet from each voice");
        };
        assert_eq!(
            message.addr,
            format!("/out/group/track{}/target/event/0", index + 1)
        );
    }
}

/// Median microseconds per idle `plan()` call, printed for reference.
#[test]
fn idle_plan_cost_on_128_clips() {
    let planner = Planner::new(large_project(), Duration::from_millis(12)).unwrap();
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut samples: Vec<f64> = (0..400)
        .map(|_| {
            let t = Instant::now();
            let planned = planner.plan(&clock, start);
            let us = t.elapsed().as_secs_f64() * 1e6;
            assert!(planned.is_empty());
            us
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    let (median, max) = (samples[samples.len() / 2], samples[samples.len() - 1]);
    println!("plan_cost idle 128x32: median {median:.1} us, max {max:.1} us");
    if !cfg!(debug_assertions) {
        assert!(median < 50.0, "idle plan() median {median:.1} us");
    }
}

/// Median microseconds per `plan()` while one clip plays on each of the 8 tracks.
#[test]
fn playing_plan_cost_on_128_clips() {
    let mut planner = Planner::new(large_project(), Duration::from_millis(12)).unwrap();
    for track in 1..=8 {
        planner.launch_at(TrackId(track), 0, BeatTime(0)).unwrap();
    }
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut samples: Vec<f64> = (0..400)
        .map(|_| {
            let t = Instant::now();
            let planned = planner.plan(&clock, start);
            let us = t.elapsed().as_secs_f64() * 1e6;
            assert!(planned.len() >= 8, "planned {}", planned.len());
            us
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    let (median, max) = (samples[samples.len() / 2], samples[samples.len() - 1]);
    println!("plan_cost playing 8 voices: median {median:.1} us, max {max:.1} us");
    if !cfg!(debug_assertions) {
        assert!(median < 250.0, "playing plan() median {median:.1} us");
    }
}

/// Same shape with every track at the most negative legal delay: the lookahead
/// grows to 1 s, so the window holds about 8 x 8 events.
#[test]
fn negative_delay_plan_cost_on_128_clips() {
    let mut project = large_project();
    for track in project.tracks.iter_mut().filter(|t| !t.is_group()) {
        track.delay_ms = -1000.0;
    }
    let mut planner = Planner::new(project, Duration::from_millis(12)).unwrap();
    for track in 1..=8 {
        planner.launch_at(TrackId(track), 0, BeatTime(0)).unwrap();
    }
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut planned_len = 0;
    let mut samples: Vec<f64> = (0..100)
        .map(|_| {
            let t = Instant::now();
            let planned = planner.plan(&clock, start);
            let us = t.elapsed().as_secs_f64() * 1e6;
            planned_len = planned.len();
            us
        })
        .collect();
    let (median, max) = median_max(&mut samples);
    println!(
        "plan_cost -1000 ms, 8 voices ({planned_len} events): median {median:.1} us, max {max:.1} us"
    );
    assert!(planned_len >= 8 * 8, "planned {planned_len}");
}

/// A 60 s stall with 8 voices looping 32-event clips leaves about 7700 missed
/// sends. Time the catch-up the way the worker does it: skip, then plan.
#[test]
fn backlog_plan_cost_on_128_clips() {
    let mut base = Planner::new(large_project(), Duration::from_millis(12)).unwrap();
    for track in 1..=8 {
        base.launch_at(TrackId(track), 0, BeatTime(0)).unwrap();
    }
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let now = start + Duration::from_secs(60);
    let (mut planned_len, mut skipped) = (0, 0);
    let mut samples: Vec<f64> = (0..20)
        .map(|_| {
            let mut planner = base.clone();
            let t = Instant::now();
            skipped = planner.skip_stale_sends(&clock, now, Duration::from_millis(50));
            let planned = planner.plan(&clock, now);
            let us = t.elapsed().as_secs_f64() * 1e6;
            planned_len = planned.len();
            us
        })
        .collect();
    let (median, max) = median_max(&mut samples);
    println!(
        "plan_cost backlog 60 s, 8 voices (skipped {skipped}, planned {planned_len}): median {median:.1} us, max {max:.1} us"
    );
    assert!(skipped > 7000, "skipped {skipped}");
    assert!(planned_len < 20, "planned {planned_len}");
}

/// The worker runs the skip before every plan, so it must cost little when
/// nothing is stale.
#[test]
fn skip_cost_with_nothing_stale() {
    let mut planner = Planner::new(large_project(), Duration::from_millis(12)).unwrap();
    for track in 1..=8 {
        planner.launch_at(TrackId(track), 0, BeatTime(0)).unwrap();
    }
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let mut samples: Vec<f64> = (0..400)
        .map(|_| {
            let t = Instant::now();
            let skipped = planner.skip_stale_sends(&clock, start, Duration::from_millis(50));
            let us = t.elapsed().as_secs_f64() * 1e6;
            assert_eq!(skipped, 0);
            us
        })
        .collect();
    let (median, max) = median_max(&mut samples);
    println!("plan_cost skip, nothing stale: median {median:.1} us, max {max:.1} us");
}

/// One looping 32-event clip fanned out to `outputs` outputs.
fn fanout_project(outputs: u64) -> Project {
    let mut project = large_project();
    project
        .tracks
        .retain(|t| t.is_group() || t.id == TrackId(1));
    project.outputs = (1..=outputs)
        .map(|i| {
            let (host, port) = {
                let d = project.outputs[0].destination().unwrap();
                (d.host.clone(), d.port)
            };
            Track::new_output(TrackId(200 + i), format!("output {i}"), host, port)
                .with_path(format!("/out{i}"))
        })
        .collect();
    project.track_mut(TrackId(100)).unwrap().route =
        Route::Outputs((1..=outputs).map(|i| TrackId(200 + i)).collect());
    project
}

/// Median microseconds per planned send, with a window of about 1 s, as the
/// number of outputs on one track grows. Printed for reference.
#[test]
fn fanout_plan_cost_scales_with_outputs() {
    let mut per_send = Vec::new();
    for outputs in [1, 8, 32] {
        let mut planner =
            Planner::new(fanout_project(outputs), Duration::from_millis(1000)).unwrap();
        planner.launch_at(TrackId(1), 0, BeatTime(0)).unwrap();
        let start = Instant::now();
        let mut clock = InternalClock::default();
        clock.play(start);
        let mut planned_len = 0;
        let mut samples: Vec<f64> = (0..50)
            .map(|_| {
                let t = Instant::now();
                let planned = planner.plan(&clock, start);
                let us = t.elapsed().as_secs_f64() * 1e6;
                planned_len = planned.len();
                us / planned.len().max(1) as f64
            })
            .collect();
        let (median, max) = median_max(&mut samples);
        println!(
            "plan_cost fanout {outputs} outputs ({planned_len} sends): median {median:.2} us/send, max {max:.2} us/send"
        );
        assert!(planned_len >= outputs as usize, "planned {planned_len}");
        per_send.push(median);
    }
    if !cfg!(debug_assertions) {
        // Linear in outputs would keep this near 1; quadratic gave about 4x at 8 outputs.
        assert!(
            per_send[2] < per_send[0] * 8.0,
            "per-send cost {per_send:?}"
        );
    }
}

/// Deterministic frame-wake planning cost: 256 drivers, one output, dense Draw Mode data.
/// No wall-clock measurement (the worker shares the machine with another build).
#[test]
fn dense_envelope_frame_wake_plan_cost() {
    use super::automation::Automation;
    for count in [256, 4096] {
        for shape in ["ramp", "flat", "long-flat"] {
            let mut project = project(0.0, true);
            project.outputs[0].id = TrackId(1000);
            let mut template = project.tracks[0].clone();
            template.route = Route::single(TrackId(1000));
            template.devices.clear();
            template.slots[1].clip = None;
            project.tracks = (1..=256)
                .map(|id| {
                    let mut track = template.clone();
                    track.id = TrackId(id);
                    let clip = track.slots[0].clip.as_mut().unwrap();
                    clip.id = ClipId(id);
                    clip.events.clear();
                    clip.end = BeatTime(count * 2);
                    clip.loop_end = BeatTime(count * 2);
                    clip.lanes = vec![Lane::Envelope(AutomationEnvelope {
                        region: None,
                        target: ArgumentRef::volume(),
                        points: (0..count)
                            .map(|i| Breakpoint {
                                time: BeatTime(i * 2),
                                value: match shape {
                                    "flat" => 0.0,
                                    "long-flat" => f64::from(i == count - 1),
                                    _ => (i % 2) as f64,
                                },
                                curve: 0.0,
                            })
                            .collect(),
                    })];
                    track
                })
                .collect();
            let mut planner = Planner::new(project, Duration::ZERO).unwrap();
            for id in 1..=256 {
                planner.launch_at(TrackId(id), 0, BeatTime::ZERO).unwrap();
            }
            let now = Instant::now();
            let mut clock = InternalClock::default();
            clock.play(now);
            let mut automation = Automation::default();
            automation.sync(&planner, &clock, now);
            let at = clock.instant_at(BeatTime(count)).unwrap();
            Automation::take_wake_search_cost();
            for id in 1..=256 {
                let target = ParameterRef {
                    track: TrackId(id),
                    device: DeviceRef::Mixer,
                    key: ParameterKey("volume".into()),
                };
                let wake = automation.next_frame(&target, at, 0.0);
                if shape == "flat" {
                    assert_eq!(wake, None);
                } else if shape == "ramp" {
                    assert_eq!(wake, Some(at));
                } else {
                    assert_eq!(wake, clock.instant_at(BeatTime((count - 2) * 2)));
                }
            }
            let [comparisons, pairs] = Automation::take_wake_search_cost();
            let old_visits = 256 * 2 * (count - 1);
            println!(
                "plan_cost frame wake 256 drivers x {count} points ({shape}), one output: old {old_visits} pair visits; indexed {comparisons} search comparisons + {pairs} candidate pairs"
            );
            let log_bound = (count as usize).ilog2() as usize + 1;
            assert!(comparisons <= 256 * 2 * log_bound);
            assert!(pairs <= 256 * 2);
            if shape == "flat" {
                assert_eq!([comparisons, pairs], [0, 0]);
            }
        }
    }
}

// Count only the calling test thread, so concurrent tests cannot contaminate the probe.
struct CountingAllocator;
thread_local! {
    static ALLOCATIONS: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
fn count_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(n) = count.get() {
            count.set(Some(n + 1));
        }
    });
}
#[global_allocator]
static TEST_ALLOCATOR: CountingAllocator = CountingAllocator;
unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        count_allocation();
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        count_allocation();
        unsafe { std::alloc::System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        count_allocation();
        unsafe { std::alloc::System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}
pub(super) fn allocations(f: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ALLOCATIONS.with(|count| count.set(None));
        }
    }
    ALLOCATIONS.with(|count| assert!(count.replace(Some(0)).is_none()));
    let _reset = Reset;
    f();
    ALLOCATIONS.with(|count| count.get().unwrap())
}
