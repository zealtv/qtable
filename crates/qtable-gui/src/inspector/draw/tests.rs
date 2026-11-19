use super::*;
use crate::inspector::point_gesture::Plot;
use qtable_engine::model::ArgumentRef;

const NONE: Keys = Keys {
    alt: false,
    shift: false,
    command: false,
};
const SHIFT: Keys = Keys {
    shift: true,
    ..NONE
};
const COMMAND: Keys = Keys {
    command: true,
    ..NONE
};

fn target() -> ArgumentRef {
    ArgumentRef::volume()
}
fn p(time: u64, value: f64) -> Breakpoint {
    Breakpoint {
        time: BeatTime(time),
        value,
        curve: 0.0,
    }
}
fn pairs(points: &[Breakpoint]) -> Vec<(u64, f64)> {
    points
        .iter()
        .map(|p| (p.time.ticks(), (p.value * 1e6).round() / 1e6))
        .collect()
}
/// A 3840-tick clip over 384 px (10 ticks a pixel, a sixteenth is 24 px) and 0..1 over 100 px,
/// 1 at the top.
fn lane_on(axis: Axis, (min, max): (f64, f64), grid: u64, points: &[Breakpoint]) -> Lane {
    Lane::new(
        Plot {
            left: 0.0,
            width: 384.0,
            top: 0.0,
            height: 100.0,
        },
        (BeatTime(3840), axis, min, max, grid),
        &target(),
        points,
    )
}
fn float(points: &[Breakpoint]) -> Lane {
    lane_on(Axis::Continuous, (0.0, 1.0), 240, points)
}
/// The pointer at `tick` and `value`.
fn at(lane: &Lane, tick: u64, value: f64) -> Point {
    Point::new(lane.x(BeatTime(tick)), lane.y(value))
}

#[test]
fn b_toggles_and_a_long_hold_is_momentary() {
    let t0 = Instant::now();
    let mut mode = Mode::default();
    mode.key_down(t0);
    assert!(mode.on);
    mode.key_up(t0 + Duration::from_millis(100));
    assert!(mode.on, "a tap leaves Draw Mode toggled on");
    mode.key_down(t0 + Duration::from_secs(1));
    assert!(!mode.on);
    mode.key_up(t0 + Duration::from_secs(1) + Duration::from_millis(100));
    assert!(!mode.on, "a tap toggles off and stays");
    mode.key_down(t0 + Duration::from_secs(2));
    assert!(mode.on);
    mode.key_up(t0 + Duration::from_secs(3));
    assert!(!mode.on, "holding B toggles only while held");
    mode.toggle();
    assert!(mode.on);
    mode.key_up(t0 + Duration::from_secs(9));
    assert!(mode.on, "a release with no press changes nothing");
}

#[test]
fn a_drag_paints_one_flat_step_per_cell_at_the_pointer_value() {
    let lane = float(&[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, NONE);
    // Three cells (0..240, 240..480, 480..720), the second at a different height.
    stroke.push(&lane, at(&lane, 10, 0.5), NONE);
    stroke.push(&lane, at(&lane, 300, 0.8), NONE);
    stroke.push(&lane, at(&lane, 500, 0.8), NONE);
    let r = stroke.replacement().unwrap();
    assert_eq!(r.range, BeatTime(0)..BeatTime(721));
    assert_eq!(
        pairs(&r.points),
        vec![(0, 0.5), (240, 0.5), (240, 0.8), (720, 0.8)],
        "equal neighbours merge; unequal ones meet as two points at one time"
    );
}

#[test]
fn a_fast_pointer_still_paints_every_cell_it_crossed() {
    let lane = float(&[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, NONE);
    stroke.push(&lane, at(&lane, 10, 0.0), NONE);
    stroke.push(&lane, at(&lane, 970, 1.0), NONE);
    let r = stroke.replacement().unwrap();
    // Cells 0..4 take 0, 0.25, 0.5, 0.75, 1 along the path: five distinct steps.
    assert_eq!(
        pairs(&r.points),
        vec![
            (0, 0.0),
            (240, 0.0),
            (240, 0.25),
            (480, 0.25),
            (480, 0.5),
            (720, 0.5),
            (720, 0.75),
            (960, 0.75),
            (960, 1.0),
            (1200, 1.0),
        ]
    );
}

#[test]
fn a_backwards_drag_paints_the_same_cells() {
    let lane = float(&[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, NONE);
    stroke.push(&lane, at(&lane, 700, 0.5), NONE);
    stroke.push(&lane, at(&lane, 10, 0.5), NONE);
    let r = stroke.replacement().unwrap();
    assert_eq!(r.range, BeatTime(0)..BeatTime(721));
    assert_eq!(pairs(&r.points), vec![(0, 0.5), (720, 0.5)]);
}

#[test]
fn a_click_paints_its_one_cell() {
    let lane = float(&[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, NONE);
    stroke.push(&lane, at(&lane, 250, 0.3), NONE);
    let r = stroke.replacement().unwrap();
    assert_eq!(r.range, BeatTime(240)..BeatTime(481));
    assert_eq!(pairs(&r.points), vec![(240, 0.3), (480, 0.3)]);
}

#[test]
fn the_last_cell_stops_at_the_clip_end() {
    let lane = float(&[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, NONE);
    stroke.push(&lane, at(&lane, 3840, 0.5), NONE);
    let r = stroke.replacement().unwrap();
    assert_eq!(r.range, BeatTime(3600)..BeatTime(3841));
    assert_eq!(pairs(&r.points), vec![(3600, 0.5), (3840, 0.5)]);
}

#[test]
fn painting_replaces_inside_keeps_edges_and_restores_a_point_at_the_end() {
    let existing = [
        p(0, 0.1),
        p(240, 0.2),
        p(300, 0.9),
        p(480, 0.4),
        p(700, 0.6),
    ];
    let lane = float(&existing);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, NONE);
    stroke.push(&lane, at(&lane, 250, 0.5), NONE);
    let r = stroke.replacement().unwrap();
    // The point at 480 would sit one tick inside the widened range; it comes back after the step.
    assert_eq!(pairs(&r.points), vec![(240, 0.5), (480, 0.5), (480, 0.4)]);
    let after = replaced(&existing, &r);
    assert_eq!(
        pairs(&after),
        vec![
            (0, 0.1),
            (240, 0.2),
            (240, 0.5),
            (480, 0.5),
            (480, 0.4),
            (700, 0.6)
        ],
        "300 went; the edge point at 240 stayed before the new step"
    );
    assert_eq!(pairs(&stroke.preview()), pairs(&after));
}

#[test]
fn repainting_jump_boundaries_commits_the_new_step() {
    use qtable_engine::{
        edit::{LaneContent, LaneEdit, LaneKey, PointReplacement, ProjectEdit},
        engine::{Command, Engine},
        model::TrackId,
    };
    // The first case is review probe 1b; the second also repaints an end jump pair.
    for end_jump in [false, true] {
        let mut existing = vec![p(0, 0.2), p(240, 0.2), p(240, 0.6), p(480, 0.6)];
        if end_jump {
            existing.push(p(480, 0.4));
        }
        let lane = float(&existing);
        let mut stroke = EnvelopeStroke::new(&lane, 0.0, NONE);
        stroke.push(&lane, at(&lane, 250, 0.9), NONE);
        let r = stroke.replacement().unwrap();
        assert_eq!(r.range, BeatTime(240)..BeatTime(481));
        let mut project = crate::inspector::lane_model::tests::mixed_project();
        let envelope = project.tracks[0].slots[0]
            .clip
            .as_mut()
            .unwrap()
            .lanes
            .iter_mut()
            .find_map(|l| match l {
                qtable_engine::model::Lane::Envelope(e) if e.target == target() => Some(e),
                _ => None,
            })
            .unwrap();
        envelope.points = existing;
        let mut engine = Engine::new(project).unwrap();
        engine
            .execute(Command::Edit(ProjectEdit::EditLane {
                track: TrackId(1),
                slot: 0,
                edit: LaneEdit::ReplaceRange {
                    range: r.range,
                    lanes: vec![(LaneKey::Envelope(target()), LaneContent::Points(r.points))],
                    point_policy: PointReplacement::KeepEdges,
                },
            }))
            .unwrap();
        assert_eq!(engine.project_revision(), 1);
        let envelope = engine
            .project()
            .clip(TrackId(1), 0)
            .unwrap()
            .lanes
            .iter()
            .filter_map(|l| l.envelope())
            .find(|e| e.target == target())
            .unwrap();
        assert_eq!(
            pairs(&envelope.points),
            vec![
                (0, 0.2),
                (240, 0.2),
                (240, 0.9),
                (480, 0.9),
                (480, if end_jump { 0.4 } else { 0.6 })
            ]
        );
        assert_eq!(pairs(&stroke.preview()), pairs(&envelope.points));
    }
}

#[test]
fn shift_paints_a_tenth_of_the_change_from_where_it_went_down() {
    let lane = float(&[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, NONE);
    stroke.push(&lane, at(&lane, 10, 0.5), NONE);
    // Shift down, then 20 px (0.2) lower: a tenth of that, 0.02, from 0.5.
    stroke.push(&lane, at(&lane, 10, 0.5), SHIFT);
    stroke.push(
        &lane,
        Point::new(lane.x(BeatTime(10)), lane.y(0.5) + 20.0),
        SHIFT,
    );
    let r = stroke.replacement().unwrap();
    assert_eq!(pairs(&r.points), vec![(0, 0.48), (240, 0.48)]);
}

#[test]
fn discrete_steps_snap_to_an_option() {
    let lane = lane_on(Axis::Discrete, (0.0, 3.0), 240, &[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, NONE);
    stroke.push(&lane, at(&lane, 10, 1.4), NONE);
    stroke.push(&lane, at(&lane, 300, 2.6), NONE);
    let r = stroke.replacement().unwrap();
    assert_eq!(
        pairs(&r.points),
        vec![(0, 1.0), (240, 1.0), (240, 3.0), (480, 3.0)]
    );
}

#[test]
fn command_draws_freehand_and_thins_the_path() {
    let lane = float(&[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, COMMAND);
    // A straight ramp sampled at every other pixel: hundreds of samples, two points.
    for x in (0..=300).step_by(2) {
        stroke.push(
            &lane,
            Point::new(x as f32, lane.y(x as f64 / 300.0)),
            COMMAND,
        );
    }
    let r = stroke.replacement().unwrap();
    assert_eq!(r.points.len(), 2, "{:?}", pairs(&r.points));
    assert_eq!(r.range.start, BeatTime(0));
    assert_eq!(r.points[0].time, BeatTime(0));
    assert_eq!(r.points[1].time, BeatTime(3000));
    assert_eq!(r.range.end, BeatTime(3001));
}

#[test]
fn freehand_keeps_a_corner_the_threshold_does_not_cover() {
    let lane = float(&[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.05, COMMAND);
    // Up to the peak at tick 1000, then back down, with a bump of 0.03 that the threshold hides.
    for x in 0..=200 {
        let t = x as f64 * 10.0;
        let v = if t <= 1000.0 {
            t / 1000.0
        } else {
            2.0 - t / 1000.0
        };
        let bump = if x % 2 == 0 { 0.03 } else { 0.0 };
        let v = (v + bump).clamp(0.0, 1.0);
        stroke.push(&lane, at(&lane, t as u64, v), COMMAND);
    }
    let r = stroke.replacement().unwrap();
    assert!(r.points.len() <= 4, "{:?}", pairs(&r.points));
    assert!(r.points.iter().any(|p| p.time.ticks().abs_diff(1000) <= 20));
}

#[test]
fn a_zero_threshold_still_thins_to_a_pixel() {
    let lane = float(&[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, COMMAND);
    // Noise of a tenth of a pixel on a flat line.
    for x in 0..=300 {
        let wobble = if x % 2 == 0 { 0.001 } else { 0.0 };
        stroke.push(&lane, at(&lane, x * 10, 0.5 + wobble), COMMAND);
    }
    assert_eq!(stroke.replacement().unwrap().points.len(), 2);
}

#[test]
fn grid_off_draws_freehand_too() {
    let lane = lane_on(Axis::Continuous, (0.0, 1.0), 1, &[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, NONE);
    stroke.push(&lane, at(&lane, 100, 0.2), NONE);
    stroke.push(&lane, at(&lane, 400, 0.8), NONE);
    let r = stroke.replacement().unwrap();
    assert_eq!(r.range, BeatTime(100)..BeatTime(401));
    assert_eq!(pairs(&r.points), vec![(100, 0.2), (400, 0.8)]);
}

#[test]
fn freehand_on_a_discrete_argument_holds_levels() {
    let lane = lane_on(Axis::Discrete, (0.0, 3.0), 1, &[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, COMMAND);
    for (t, v) in [(0, 1.0), (100, 1.2), (200, 1.0), (300, 2.9), (400, 3.0)] {
        stroke.push(&lane, at(&lane, t, v), COMMAND);
    }
    let r = stroke.replacement().unwrap();
    assert_eq!(pairs(&r.points), vec![(0, 1.0), (300, 3.0), (400, 3.0)]);
}

#[test]
fn freehand_going_backwards_keeps_the_last_value_at_a_tick() {
    let lane = float(&[]);
    let mut stroke = EnvelopeStroke::new(&lane, 0.0, COMMAND);
    stroke.push(&lane, at(&lane, 500, 0.2), COMMAND);
    stroke.push(&lane, at(&lane, 100, 0.4), COMMAND);
    stroke.push(&lane, at(&lane, 500, 0.9), COMMAND);
    let r = stroke.replacement().unwrap();
    assert_eq!(pairs(&r.points), vec![(100, 0.4), (500, 0.9)]);
}

#[test]
fn an_unpainted_stroke_replaces_nothing() {
    let lane = float(&[p(0, 0.3)]);
    let stroke = EnvelopeStroke::new(&lane, 0.0, NONE);
    assert_eq!(stroke.replacement(), None);
    assert_eq!(pairs(&stroke.preview()), vec![(0, 0.3)]);
}

fn hits(grid: u64, existing: &[u64]) -> HitStroke {
    HitStroke::new(grid, BeatTime(3840), existing.iter().map(|&t| BeatTime(t)))
}

#[test]
fn hits_go_at_the_start_of_each_cell_crossed() {
    let mut stroke = hits(240, &[]);
    stroke.push(250.0);
    stroke.push(800.0);
    assert_eq!(
        stroke.positions(),
        vec![BeatTime(240), BeatTime(480), BeatTime(720)]
    );
    assert_eq!(stroke.span(), Some(BeatTime(240)..BeatTime(960)));
}

#[test]
fn cells_that_already_hold_a_hit_are_skipped() {
    let mut stroke = hits(240, &[500]);
    stroke.push(0.0);
    stroke.push(700.0);
    assert_eq!(
        stroke.positions(),
        vec![BeatTime(0), BeatTime(240)],
        "the cell holding the hit at 500 gets none"
    );
}

#[test]
fn hits_never_land_past_the_clip() {
    let mut stroke = hits(240, &[]);
    stroke.push(3840.0);
    stroke.push(9999.0);
    assert_eq!(stroke.positions(), vec![BeatTime(3600)]);
    assert_eq!(stroke.span(), Some(BeatTime(3600)..BeatTime(3840)));
}

#[test]
fn grid_off_places_hits_a_sixteenth_apart() {
    let mut stroke = hits(1, &[]);
    stroke.push(10.0);
    stroke.push(500.0);
    assert_eq!(
        stroke.positions(),
        vec![BeatTime(0), BeatTime(240), BeatTime(480)]
    );
}

#[test]
fn a_click_on_a_hit_erases_it_and_a_drag_from_it_paints() {
    let press = Point::new(10.0, 10.0);
    let mut click = Painting::hits(hits(240, &[250]), 250.0, Some(3), press);
    click.push(None, Point::new(11.0, 10.0), 255.0, NONE);
    assert_eq!(click.finish(), Some(Outcome::Erase(3)));

    let mut drag = Painting::hits(hits(240, &[250]), 250.0, Some(3), press);
    drag.push(None, Point::new(60.0, 10.0), 800.0, NONE);
    assert_eq!(
        drag.finish(),
        Some(Outcome::Hits {
            span: BeatTime(240)..BeatTime(960),
            positions: vec![BeatTime(480), BeatTime(720)],
        }),
        "the cell of the pressed hit is skipped"
    );
}

#[test]
fn a_click_on_an_empty_cell_places_one_hit() {
    let mut click = Painting::hits(hits(240, &[]), 250.0, None, Point::ORIGIN);
    assert_eq!(
        click.clone().finish(),
        Some(Outcome::Hits {
            span: BeatTime(240)..BeatTime(480),
            positions: vec![BeatTime(240)],
        })
    );
    click.push(None, Point::ORIGIN, 250.0, NONE);
}

#[test]
fn a_stroke_over_only_occupied_cells_commits_nothing() {
    let click = Painting::hits(hits(240, &[250]), 250.0, None, Point::ORIGIN);
    assert_eq!(click.finish(), None);
}
