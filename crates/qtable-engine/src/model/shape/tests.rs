use super::*;
use crate::model::{ArgumentRef, AutomationEnvelope};

const LINK: Link = Link {
    before: 0.25,
    after: 0.75,
};

fn points(shape: Shape, span: u64) -> Vec<Breakpoint> {
    shape_points(shape, span, 0.0, 1.0, Axis::Continuous, LINK)
}
fn envelope(points: Vec<Breakpoint>) -> AutomationEnvelope {
    AutomationEnvelope {
        region: None,
        target: ArgumentRef::volume(),
        points,
    }
}
fn pairs(points: &[Breakpoint]) -> Vec<(u64, f64)> {
    points.iter().map(|p| (p.time.ticks(), p.value)).collect()
}

#[test]
fn the_sine_makes_one_cycle_within_a_percent_of_a_true_sine() {
    let span = 3840;
    let sine = envelope(points(Shape::Sine, span));
    let mut worst: f64 = 0.0;
    for tick in 0..=span {
        let phase = tick as f64 / span as f64;
        let truth = 0.5 + 0.5 * (std::f64::consts::TAU * phase).sin();
        let drawn = sine.value_at(tick as f64, Axis::Continuous).unwrap();
        worst = worst.max((drawn - truth).abs());
    }
    assert!(worst < 0.011, "worst error {worst} of the range");
    // One cycle: the middle, the top at a quarter, the middle, the bottom, the middle.
    for (phase, level) in [(0.0, 0.5), (0.25, 1.0), (0.5, 0.5), (0.75, 0.0), (1.0, 0.5)] {
        let at = sine
            .value_at(phase * span as f64, Axis::Continuous)
            .unwrap();
        assert!((at - level).abs() < 1e-6, "{phase}: {at}");
    }
    // It rises only once and falls only once.
    let samples: Vec<f64> = (0..=span)
        .step_by(8)
        .map(|t| sine.value_at(t as f64, Axis::Continuous).unwrap())
        .collect();
    let turns = samples
        .windows(3)
        .filter(|w| (w[1] - w[0]) * (w[2] - w[1]) < 0.0)
        .count();
    assert_eq!(turns, 2);
}

#[test]
fn every_shape_spans_the_selection_sorted_and_valid() {
    for shape in Shape::ALL {
        for span in [16, 240, 3840, 3841] {
            let points = points(shape, span);
            assert_eq!(points.first().unwrap().time, BeatTime(0), "{shape:?}");
            assert_eq!(points.last().unwrap().time, BeatTime(span), "{shape:?}");
            assert!(points.windows(2).all(|w| w[0].time <= w[1].time));
            AutomationEnvelope::validate_points(&points).unwrap();
            assert!(points.iter().all(|p| (0.0..=1.0).contains(&p.value)));
        }
    }
}

#[test]
fn the_plain_shapes_give_exactly_their_points() {
    assert_eq!(
        pairs(&points(Shape::Triangle, 3840)),
        [(0, 0.0), (1920, 1.0), (3840, 0.0)]
    );
    assert_eq!(
        pairs(&points(Shape::SawDown, 3840)),
        [(0, 1.0), (3840, 0.0)]
    );
    assert_eq!(pairs(&points(Shape::SawUp, 3840)), [(0, 0.0), (3840, 1.0)]);
    assert_eq!(
        pairs(&points(Shape::Square, 3840)),
        [(0, 1.0), (1920, 1.0), (1920, 0.0), (3840, 0.0)]
    );
    for shape in [Shape::Triangle, Shape::SawDown, Shape::SawUp, Shape::Square] {
        assert!(points(shape, 3840).iter().all(|p| p.curve == 0.0));
    }
}

#[test]
fn the_ramps_start_or_end_on_the_linked_value() {
    let ramp_in = points(Shape::RampIn, 960);
    assert_eq!(pairs(&ramp_in), [(0, 0.0), (960, LINK.after)]);
    assert_eq!(ramp_in[0].curve, -0.5);
    let ramp_out = points(Shape::RampOut, 960);
    assert_eq!(pairs(&ramp_out), [(0, LINK.before), (960, 0.0)]);
    assert_eq!(ramp_out[0].curve, 0.5);
    // Ramp In is fast at first, Ramp Out slow at first: the icons' shapes.
    let at_quarter = |points: Vec<Breakpoint>| envelope(points).value_at(240.0, Axis::Continuous);
    assert!(at_quarter(points(Shape::RampIn, 960)).unwrap() > 0.75 * 0.5);
    assert!(at_quarter(points(Shape::RampOut, 960)).unwrap() > 0.25 * 0.9);
}

#[test]
fn whole_step_arguments_get_whole_values_in_range() {
    for shape in Shape::ALL {
        let points = shape_points(
            shape,
            3841,
            0.0,
            8.0,
            Axis::Stepped,
            Link {
                before: 2.0,
                after: 7.0,
            },
        );
        assert!(!points.is_empty());
        assert!(
            points
                .iter()
                .all(|p| p.value.fract() == 0.0 && (0.0..=8.0).contains(&p.value)),
            "{shape:?}: {points:?}"
        );
    }
    // A range that does not start at zero scales the levels into it.
    let points = shape_points(Shape::Triangle, 960, 10.0, 20.0, Axis::Continuous, LINK);
    assert_eq!(pairs(&points), [(0, 10.0), (480, 20.0), (960, 10.0)]);
}

#[test]
fn a_span_under_the_minimum_yields_nothing() {
    for shape in Shape::ALL {
        assert!(points(shape, 0).is_empty());
        assert!(points(shape, MIN_SHAPE_SPAN - 1).is_empty());
        assert!(!points(shape, MIN_SHAPE_SPAN).is_empty());
    }
}

#[test]
fn shapes_have_names_and_a_fixed_order() {
    let names: Vec<_> = Shape::ALL.iter().map(|s| s.name()).collect();
    assert_eq!(
        names,
        [
            "Sine", "Triangle", "Saw Down", "Saw Up", "Square", "Ramp In", "Ramp Out"
        ]
    );
}
