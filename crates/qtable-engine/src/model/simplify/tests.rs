use super::*;

fn bp(time: u64, value: f64, curve: f32) -> Breakpoint {
    Breakpoint {
        time: BeatTime(time),
        value,
        curve,
    }
}
fn line(points: &[(u64, f64)]) -> Vec<Breakpoint> {
    points.iter().map(|&(t, v)| bp(t, v, 0.0)).collect()
}
fn everything() -> Range<BeatTime> {
    BeatTime(0)..BeatTime(u64::MAX)
}
fn times(points: &[Breakpoint]) -> Vec<u64> {
    points.iter().map(|p| p.time.ticks()).collect()
}

#[test]
fn collinear_points_go_and_corners_stay() {
    let points = line(&[(0, 0.0), (100, 0.25), (200, 0.5), (300, 1.0), (400, 0.0)]);
    let out = simplify_points(&points, everything(), 0.0, Axis::Continuous);
    // 100 lies on the line from 0 to 200; 200 and 300 are corners.
    assert_eq!(times(&out), [0, 200, 300, 400]);
}

#[test]
fn a_point_within_the_tolerance_goes_and_one_beyond_it_stays() {
    let points = line(&[(0, 0.0), (100, 0.52), (200, 1.0), (300, 0.7), (400, 0.0)]);
    // 100 sits 0.02 off the line 0 to 200; 300 sits 0.3 off the line 200 to 400.
    let out = simplify_points(&points, everything(), 0.021, Axis::Continuous);
    assert_eq!(times(&out), [0, 200, 300, 400]);
    let out = simplify_points(&points, everything(), 0.019, Axis::Continuous);
    assert_eq!(times(&out), [0, 100, 200, 300, 400]);
}

#[test]
fn curved_segments_and_jump_pairs_survive_whole() {
    // 100 is collinear but leaves along a curve, and 200 arrives by it: both stay.
    let curved = vec![
        bp(0, 0.0, 0.0),
        bp(100, 0.25, 0.4),
        bp(200, 0.5, 0.0),
        bp(300, 0.75, 0.0),
        bp(400, 1.0, 0.0),
    ];
    let out = simplify_points(&curved, everything(), 0.5, Axis::Continuous);
    assert_eq!(times(&out), [0, 100, 200, 400]);
    assert_eq!(out[1].curve, 0.4);
    // Both members of the jump at 200 stay, though each is collinear with a neighbour.
    let jump = line(&[
        (0, 0.0),
        (100, 0.5),
        (200, 1.0),
        (200, 0.0),
        (300, 0.5),
        (400, 1.0),
    ]);
    let out = simplify_points(&jump, everything(), 0.5, Axis::Continuous);
    assert_eq!(times(&out), [0, 200, 200, 400]);
    assert_eq!(out, line(&[(0, 0.0), (200, 1.0), (200, 0.0), (400, 1.0)]));
}

#[test]
fn points_outside_the_scope_and_on_its_edges_survive() {
    let points = line(&[
        (0, 0.0),
        (100, 0.5),
        (200, 1.0),
        (300, 1.5),
        (400, 2.0),
        (500, 2.5),
        (600, 3.0),
    ]);
    // Everything is collinear. Only 300 and 400 lie strictly inside 200..500.
    let out = simplify_points(&points, BeatTime(200)..BeatTime(500), 0.0, Axis::Continuous);
    assert_eq!(times(&out), [0, 100, 200, 500, 600]);
    // An empty scope removes nothing.
    let out = simplify_points(&points, BeatTime(300)..BeatTime(300), 0.0, Axis::Continuous);
    assert_eq!(out, points);
}

#[test]
fn the_first_and_last_points_always_survive() {
    let points = line(&[(100, 0.5), (200, 0.5), (300, 0.5)]);
    let out = simplify_points(&points, everything(), 1.0, Axis::Continuous);
    assert_eq!(times(&out), [100, 300]);
    let two = line(&[(0, 0.0), (10, 1.0)]);
    assert_eq!(
        simplify_points(&two, everything(), 1.0, Axis::Continuous),
        two
    );
    assert!(simplify_points(&[], everything(), 1.0, Axis::Continuous).is_empty());
}

#[test]
fn a_choice_lane_loses_only_points_that_change_nothing() {
    let points = line(&[
        (0, 1.0),
        (100, 1.0), // repeats the held 1: goes
        (200, 2.0), // a change: stays
        (300, 2.0), // repeats: goes
        (400, 3.0), // first of a pair at once replaced: goes
        (400, 1.0), // the replacement: a change from 2, stays
        (500, 1.0), // repeats: goes
        (600, 0.0), // last: stays
    ]);
    let out = simplify_points(&points, everything(), 0.5, Axis::Discrete);
    assert_eq!(
        out.iter()
            .map(|p| (p.time.ticks(), p.value))
            .collect::<Vec<_>>(),
        [(0, 1.0), (200, 2.0), (400, 1.0), (600, 0.0)]
    );
    // Held values come out the same everywhere.
    let before = AutomationEnvelope_like(&points);
    let after = AutomationEnvelope_like(&out);
    for t in (0..=700).step_by(10) {
        assert_eq!(before(t), after(t), "at {t}");
    }
}

#[allow(non_snake_case)]
fn AutomationEnvelope_like(points: &[Breakpoint]) -> impl Fn(u64) -> f64 + '_ {
    move |t| {
        let envelope = crate::model::AutomationEnvelope {
            region: None,
            target: crate::model::ArgumentRef::volume(),
            points: points.to_vec(),
        };
        envelope.value_at(t as f64, Axis::Discrete).unwrap()
    }
}

#[test]
fn the_tolerance_is_the_larger_of_the_threshold_and_a_percent_of_the_range() {
    assert_eq!(simplify_tolerance(0.0, 0.0, 1.0, Axis::Continuous), 0.01);
    assert_eq!(simplify_tolerance(0.05, 0.0, 1.0, Axis::Continuous), 0.05);
    assert_eq!(simplify_tolerance(0.0, -50.0, 50.0, Axis::Continuous), 1.0);
    assert_eq!(simplify_tolerance(0.2, 0.0, 10.0, Axis::Continuous), 0.2);
    // A whole-step argument tolerates at most half a step, and so does a choice.
    assert_eq!(simplify_tolerance(0.0, 0.0, 200.0, Axis::Stepped), 0.5);
    assert_eq!(simplify_tolerance(3.0, 0.0, 8.0, Axis::Stepped), 0.5);
    assert_eq!(simplify_tolerance(0.0, 0.0, 8.0, Axis::Stepped), 0.08);
    assert_eq!(simplify_tolerance(2.0, 0.0, 3.0, Axis::Discrete), 0.5);
}

#[test]
fn simplifying_never_adds_points_and_keeps_every_value_within_the_tolerance() {
    // A noisy ramp, thinned at several tolerances.
    let points: Vec<_> = (0..=40)
        .map(|i| {
            bp(
                i * 10,
                i as f64 / 40.0 + if i % 3 == 0 { 0.01 } else { -0.005 },
                0.0,
            )
        })
        .collect();
    for tolerance in [0.0, 0.005, 0.01, 0.05, 0.5] {
        let out = simplify_points(&points, everything(), tolerance, Axis::Continuous);
        assert!(out.len() <= points.len());
        assert_eq!(out.first(), points.first());
        assert_eq!(out.last(), points.last());
        let before = crate::model::AutomationEnvelope {
            region: None,
            target: crate::model::ArgumentRef::volume(),
            points: points.clone(),
        };
        let after = crate::model::AutomationEnvelope {
            points: out,
            ..before.clone()
        };
        for t in 0..=400 {
            let (a, b) = (
                before.value_at(t as f64, Axis::Continuous).unwrap(),
                after.value_at(t as f64, Axis::Continuous).unwrap(),
            );
            assert!((a - b).abs() <= tolerance + 1e-9, "tol {tolerance} at {t}");
        }
    }
}

#[test]
fn thin_keeps_the_ends_and_drops_samples_near_the_chord() {
    let samples: Vec<(u64, f64)> = (0..=10).map(|t| (t, t as f64)).collect();
    assert_eq!(thin(&samples, 0.0), [(0, 0.0), (10, 10.0)]);
    assert_eq!(thin(&samples[..2], 0.0), samples[..2]);
    let bump = [(0, 0.0), (1, 0.4), (2, 0.0)];
    assert_eq!(thin(&bump, 0.5), [(0, 0.0), (2, 0.0)]);
    assert_eq!(thin(&bump, 0.3), bump);
}

/// An Opacity lane from a v3.11 tour set (2026-11-18): a Draw Mode scribble, as (ticks, value,
/// curve) of each point. 46 points; values swing between 0 and 1 within a few hundredths of a beat,
/// and five same-time pairs and two curved stretches sit among them.
const SCRIBBLE: [(u64, f64, f32); 46] = [
    (240, 0.0, 0.0),
    (240, 0.5, -0.05),
    (528, 0.545892548611111, -0.2),
    (720, 0.6413845486111112, 0.0),
    (720, 0.5340057373046876, 0.0),
    (730, 0.7048390706380208, 0.0),
    (769, 0.6379987928602431, 0.0),
    (780, 0.7187279595269097, 0.0),
    (806, 0.7187279595269097, 0.0),
    (829, 0.5475040011935763, 0.0),
    (829, 0.6272566053602431, 0.0),
    (836, 0.5185326470269097, 0.0),
    (839, 0.29598490397135413, 0.0),
    (842, 1.0, 0.0),
    (842, 0.5475040011935763, 0.0),
    (855, 0.5475040011935763, 0.0),
    (890, 0.0, 0.0),
    (902, 0.6026255289713542, 0.0),
    (912, 0.18324618869357634, 0.0),
    (943, 0.9630855984157987, 0.0),
    (943, 0.2527991400824653, 0.0),
    (966, 0.08200920952690971, 0.0),
    (974, 0.4759979248046875, 0.0),
    (983, 0.6277991400824653, 0.0),
    (992, 0.7565968831380209, 0.0),
    (1007, 0.7129770914713542, 0.0),
    (1008, 0.3716142442491319, 0.0),
    (1025, 0.46688334147135413, 0.0),
    (1044, 0.0, 0.0),
    (1070, 0.0, 0.0),
    (1096, 0.3294050428602431, 0.0),
    (1149, 0.7560175193817689, 0.0),
    (1173, 0.4120873345269097, 0.0),
    (1243, 1.0, 0.0),
    (1245, 0.12031216091579866, 0.0),
    (1345, 1.0, 0.0),
    (1347, 0.35653177897135413, 0.0),
    (1349, 1.0, 0.0),
    (1356, 1.0, 0.0),
    (1488, 0.095492, -0.2),
    (1680, 0.0, 0.2),
    (2160, 0.5, 0.0),
    (2160, 0.6289178306060941, 0.0),
    (2400, 0.6289178306060941, 0.0),
    (3360, 0.5, 0.0),
    (3360, 0.0, 0.0),
];

fn scribble() -> Vec<Breakpoint> {
    SCRIBBLE.iter().map(|&(t, v, c)| bp(t, v, c)).collect()
}

#[test]
fn a_dense_zigzag_like_scribble_is_left_alone_as_in_live() {
    // Intended, and checked against Live 12 on 2026-11-18: Live's Simplify Envelope leaves a dense
    // zigzag of about 40 points untouched ("No more automation events could be removed"). Simplify
    // thins by value, within 1% of the range, and no point of this scribble lies that close to the
    // line between its neighbours, so "Nothing to simplify" is the right answer, not a bug.
    let points = scribble();
    let survivors = |tolerance: f64, scope: Range<BeatTime>| {
        simplify_points(&points, scope, tolerance, Axis::Continuous).len()
    };
    // The tolerance Simplify uses on a 0..1 argument with no Threshold (1% of the range).
    let default = simplify_tolerance(0.0, 0.0, 1.0, Axis::Continuous);
    assert_eq!(default, 0.01);
    for scope in [everything(), BeatTime(768)..BeatTime(1392)] {
        assert_eq!(survivors(default, scope), 46);
    }
    // Only a tolerance far above 1% thins it, and then slowly.
    assert_eq!(survivors(0.1, everything()), 41);
    assert_eq!(survivors(0.25, everything()), 33);
}

#[test]
fn a_dense_but_smooth_stroke_thins_to_far_fewer_points_within_the_tolerance() {
    // What Draw Mode writes for a slow hand: 49 samples of one sine cycle across 4 beats, on a
    // 0..1 argument. Unlike the scribble, neighbouring samples sit a hair off a straight line.
    let points: Vec<_> = (0..=48)
        .map(|i| {
            let phase = i as f64 / 48.0 * std::f64::consts::TAU;
            bp(i * 80, 0.5 + 0.5 * phase.sin(), 0.0)
        })
        .collect();
    let tolerance = simplify_tolerance(0.0, 0.0, 1.0, Axis::Continuous);
    let out = simplify_points(&points, everything(), tolerance, Axis::Continuous);
    assert!(
        out.len() <= points.len() / 2,
        "{} of {}",
        out.len(),
        points.len()
    );
    assert!(out.len() >= 4, "the crest and the trough stay");
    assert_eq!(out.first(), points.first());
    assert_eq!(out.last(), points.last());
    let (before, after) = (
        crate::model::AutomationEnvelope {
            region: None,
            target: crate::model::ArgumentRef::volume(),
            points: points.clone(),
        },
        crate::model::AutomationEnvelope {
            region: None,
            target: crate::model::ArgumentRef::volume(),
            points: out,
        },
    );
    for t in 0..=3840 {
        let (a, b) = (
            before.value_at(t as f64, Axis::Continuous).unwrap(),
            after.value_at(t as f64, Axis::Continuous).unwrap(),
        );
        assert!((a - b).abs() <= tolerance + 1e-9, "at tick {t}");
    }
}
