use super::*;

const NONE: Keys = Keys {
    alt: false,
    shift: false,
    command: false,
};
const ALT: Keys = Keys { alt: true, ..NONE };
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
fn times(points: &[Breakpoint]) -> Vec<(u64, f64)> {
    points
        .iter()
        .map(|p| (p.time.ticks(), (p.value * 1e6).round() / 1e6))
        .collect()
}
/// A 3840-tick clip over 384 px (10 ticks a pixel, a sixteenth is 24 px) and values 0..1 over
/// 40 px, 1 at the top.
fn lane(axis: Axis, (min, max): (f64, f64), points: &[Breakpoint]) -> Lane {
    Lane::new(
        Plot {
            left: 0.0,
            width: 384.0,
            top: 0.0,
            height: 40.0,
        },
        (BeatTime(3840), axis, min, max, 240),
        &target(),
        points,
    )
}
fn float(points: &[Breakpoint]) -> Lane {
    lane(Axis::Continuous, (0.0, 1.0), points)
}
fn at(lane: &Lane, time: u64, value: f64) -> Point {
    Point::new(lane.x(BeatTime(time)), lane.y(value))
}
fn nudge(point: Point, dx: f32, dy: f32) -> Point {
    Point::new(point.x + dx, point.y + dy)
}
/// Press, move through `via`, release at the last of them.
fn gesture(lane: &Lane, from: Point, keys: Keys, via: &[Point]) -> Option<Commit> {
    selected_gesture(lane, from, keys, via, None)
}
/// The same with point `selected` already selected on the lane.
fn selected_gesture(
    lane: &Lane,
    from: Point,
    keys: Keys,
    via: &[Point],
    selected: Option<usize>,
) -> Option<Commit> {
    let mut g = Gesture::press(lane, from, keys, selected)?;
    for &to in via {
        g.drag(lane, to, keys);
    }
    g.release(lane, via.last().copied().unwrap_or(from), keys)
}
fn value_at(points: &[Breakpoint], ticks: f64) -> f64 {
    AutomationEnvelope {
        region: None,
        target: target(),
        points: points.to_vec(),
    }
    .value_at(ticks, Axis::Continuous)
    .unwrap()
}

#[test]
fn click_versus_drag_is_decided_on_release() {
    let lane = float(&[p(480, 0.2), p(960, 0.8), p(1920, 0.4)]);
    let point = at(&lane, 960, 0.8);
    // Under the threshold, a click on the selected point deletes it, even after a wobble.
    let commit = selected_gesture(&lane, point, NONE, &[], Some(1)).unwrap();
    assert_eq!(times(&commit.points), [(480, 0.2), (1920, 0.4)]);
    assert_eq!(commit.select, None);
    let commit = selected_gesture(&lane, point, NONE, &[nudge(point, 2.0, 1.0)], Some(1)).unwrap();
    assert_eq!(commit.points.len(), 2);
    // Over it, the press is a drag: it moves and never deletes.
    let commit = gesture(&lane, point, NONE, &[nudge(point, 24.0, 0.0)]).unwrap();
    assert_eq!(
        times(&commit.points),
        [(480, 0.2), (1200, 0.8), (1920, 0.4)]
    );
    assert_eq!(commit.select, Some(1));
    // Dragging out and back to the start is still a drag: nothing to commit, nothing deleted.
    let out_and_back = [nudge(point, 30.0, 0.0), point];
    assert_eq!(gesture(&lane, point, NONE, &out_and_back), None);
}

#[test]
fn a_click_selects_a_point_and_a_second_click_on_it_deletes_it() {
    // Choice A (2026-10-26): the fields need a click that only selects.
    let lane = float(&[p(480, 0.2), p(960, 0.8), p(1920, 0.4)]);
    let point = at(&lane, 960, 0.8);
    for selected in [None, Some(0), Some(2)] {
        let commit = selected_gesture(&lane, point, NONE, &[], selected).unwrap();
        assert_eq!(commit.points, lane.points(), "a select never edits");
        assert_eq!(commit.select, Some(1));
    }
    let commit = selected_gesture(&lane, point, NONE, &[], Some(1)).unwrap();
    assert_eq!(times(&commit.points), [(480, 0.2), (1920, 0.4)]);
    // A drag moves and selects whether or not the point was selected; it never deletes.
    for selected in [None, Some(1)] {
        let commit =
            selected_gesture(&lane, point, NONE, &[nudge(point, 24.0, 0.0)], selected).unwrap();
        assert_eq!(times(&commit.points)[1], (1200, 0.8));
        assert_eq!(commit.select, Some(1));
    }
    // A line click still adds and selects the new point.
    let line = Point::new(lane.x(BeatTime(1440)), lane.y(0.6));
    let commit = selected_gesture(&lane, line, NONE, &[], Some(1)).unwrap();
    assert_eq!(commit.points.len(), 4);
    assert_eq!(commit.select, Some(2));
}

#[test]
fn a_press_does_not_preview_until_it_drags() {
    let lane = float(&[p(0, 0.0), p(960, 1.0)]);
    let point = at(&lane, 960, 1.0);
    let mut g = Gesture::press(&lane, point, NONE, None).unwrap();
    assert!(g.preview().is_none());
    g.drag(&lane, nudge(point, 1.0, 1.0), NONE);
    assert!(g.preview().is_none());
    g.drag(&lane, nudge(point, 0.0, 20.0), NONE);
    assert_eq!(times(g.preview().unwrap())[1], (960, 0.5));
    assert_eq!(g.readout(&lane), Some(Readout::Value(0.5)));
}

#[test]
fn clicking_a_line_adds_a_point_on_the_grid_at_the_lines_value() {
    let lane = float(&[p(0, 0.0), p(3840, 1.0)]);
    // x = 100 px is tick 1000, which snaps to 960; the line there reads 0.25.
    let press = Point::new(100.0, lane.y(1000.0 / 3840.0));
    let commit = gesture(&lane, press, NONE, &[]).unwrap();
    assert_eq!(times(&commit.points), [(0, 0.0), (960, 0.25), (3840, 1.0)]);
    assert_eq!(commit.select, Some(1));
    // ⌘ leaves the grid.
    let commit = gesture(&lane, press, COMMAND, &[]).unwrap();
    assert_eq!(times(&commit.points)[1].0, 1000);
    // The line before the first point and after the last also takes a click.
    let lane = float(&[p(960, 0.5), p(1920, 0.5)]);
    let early = at(&lane, 240, 0.5);
    let commit = gesture(&lane, early, NONE, &[]).unwrap();
    assert_eq!(times(&commit.points)[0], (240, 0.5));
}

#[test]
fn empty_space_does_not_edit_and_the_current_value_line_can_start_an_envelope() {
    let lane = float(&[p(0, 0.0), p(3840, 0.0)]);
    assert!(Gesture::press(&lane, Point::new(100.0, 10.0), NONE, None).is_none());
    let lane = float(&[]).with_default(Some(0.5));
    assert!(Gesture::press(&lane, Point::new(96.0, 10.0), NONE, None).is_none());
    let commit = gesture(&lane, Point::new(96.0, 20.0), NONE, &[]).unwrap();
    assert_eq!(times(&commit.points), [(960, 0.5)]);
}

#[test]
fn a_line_press_previews_a_new_point_and_the_same_press_drags_it() {
    let lane = float(&[p(0, 0.5), p(3840, 0.5)]);
    let start = at(&lane, 960, 0.5);
    let mut g = Gesture::press(&lane, start, NONE, None).unwrap();
    assert_eq!(
        times(g.preview().unwrap()),
        [(0, 0.5), (960, 0.5), (3840, 0.5)]
    );
    g.drag(&lane, nudge(start, 24.0, -8.0), NONE);
    let commit = g.release(&lane, nudge(start, 24.0, -8.0), NONE).unwrap();
    assert_eq!(times(&commit.points), [(0, 0.5), (1200, 0.7), (3840, 0.5)]);
    assert_eq!(commit.select, Some(1));
}

#[test]
fn dragging_a_segment_moves_both_points_vertically_and_stops_at_the_range() {
    let lane = float(&[p(0, 0.2), p(960, 0.6), p(1920, 0.6)]);
    let middle = Point::new(lane.x(BeatTime(480)), lane.y(0.4) + 3.0);
    let commit = gesture(&lane, middle, NONE, &[nudge(middle, 0.0, -10.0)]).unwrap();
    assert_eq!(times(&commit.points), [(0, 0.45), (960, 0.85), (1920, 0.6)]);
    assert_eq!(commit.select, None);
    // 40 px up asks for +1.0, but the higher end may only reach 1.0 and the shape keeps.
    let commit = gesture(&lane, middle, NONE, &[nudge(middle, 0.0, -40.0)]).unwrap();
    let values: Vec<f64> = commit.points.iter().map(|p| p.value).collect();
    assert!((values[0] - 0.6).abs() < 1e-9 && (values[1] - 1.0).abs() < 1e-9);
    // A drag sideways moves nothing.
    assert_eq!(
        gesture(&lane, middle, NONE, &[nudge(middle, 30.0, 0.0)]),
        None
    );
}

#[test]
fn the_segment_band_is_live_s_width_on_a_tall_lane_and_a_quarter_of_a_short_one() {
    let points = [p(0, 0.5), p(960, 0.5), p(1920, 0.5)];
    let tall = Lane::new(
        Plot {
            left: 0.0,
            width: 384.0,
            top: 0.0,
            height: 200.0,
        },
        (BeatTime(3840), Axis::Continuous, 0.0, 1.0, 240),
        &target(),
        &points,
    );
    let line = at(&tall, 480, 0.5);
    assert_eq!(tall.segment_reach(), 16.0);
    assert_eq!(tall.hit(nudge(line, 0.0, 1.0)), Hit::Segment(0));
    for dy in [3.0, 14.0, 16.0, -16.0] {
        assert_eq!(tall.hit(nudge(line, 0.0, dy)), Hit::Segment(0), "{dy}");
    }
    assert_eq!(tall.hit(nudge(line, 0.0, 18.0)), Hit::Empty);
    // A drag from deep in the band moves both points; the on-line strip still adds.
    let commit = gesture(
        &tall,
        nudge(line, 0.0, 14.0),
        NONE,
        &[nudge(line, 0.0, -26.0)],
    )
    .unwrap();
    assert_eq!(times(&commit.points), [(0, 0.7), (960, 0.7), (1920, 0.5)]);
    let commit = gesture(&tall, line, NONE, &[]).unwrap();
    assert_eq!(commit.points.len(), 4);
    // Points still win over the band around them.
    assert_eq!(
        tall.hit(nudge(at(&tall, 960, 0.5), 2.0, 4.0)),
        Hit::Point(1)
    );
    // An All lanes row (a 36 pt plot) keeps a quarter of its height either side.
    let short = Lane::new(
        Plot {
            left: 0.0,
            width: 384.0,
            top: 0.0,
            height: 36.0,
        },
        (BeatTime(3840), Axis::Continuous, 0.0, 1.0, 240),
        &target(),
        &points,
    );
    let line = at(&short, 480, 0.5);
    assert_eq!(short.segment_reach(), 9.0);
    assert_eq!(short.hit(nudge(line, 0.0, 8.0)), Hit::Segment(0));
    assert_eq!(short.hit(nudge(line, 0.0, 11.0)), Hit::Empty);
}

#[test]
fn alt_drag_up_bends_a_segment_upward_whichever_way_it_runs() {
    for (from, to) in [(0.0, 1.0), (1.0, 0.0)] {
        let lane = float(&[p(0, from), p(3840, to)]);
        let middle = Point::new(192.0, lane.y(0.5));
        let commit = gesture(&lane, middle, ALT, &[nudge(middle, 0.0, -20.0)]).unwrap();
        assert!(
            value_at(&commit.points, 1920.0) > 0.5,
            "up raises the middle of {from} to {to}"
        );
        let commit = gesture(&lane, middle, ALT, &[nudge(middle, 0.0, 20.0)]).unwrap();
        assert!(
            value_at(&commit.points, 1920.0) < 0.5,
            "down lowers the middle of {from} to {to}"
        );
        // Curvature clamps at ±1.
        let commit = gesture(&lane, middle, ALT, &[nudge(middle, 0.0, -400.0)]).unwrap();
        assert_eq!(commit.points[0].curve.abs(), 1.0);
    }
    // Equal values bend nothing, and an Alt click neither adds nor deletes.
    let flat = float(&[p(0, 0.5), p(3840, 0.5)]);
    let middle = Point::new(192.0, flat.y(0.5));
    assert_eq!(
        gesture(&flat, middle, ALT, &[nudge(middle, 0.0, -20.0)]),
        None
    );
    assert_eq!(gesture(&flat, middle, ALT, &[]), None);
}

#[test]
fn alt_double_click_straightens_and_discrete_arguments_refuse_to_bend() {
    let mut points = vec![p(0, 0.0), p(3840, 1.0)];
    points[0].curve = 0.6;
    let lane = float(&points);
    let on_curve = Point::new(192.0, lane.y(value_at(&points, 1920.0)));
    let commit = lane.straighten(on_curve).unwrap();
    assert_eq!(commit.points[0].curve, 0.0);
    assert_eq!(float(&[p(0, 0.0), p(3840, 1.0)]).straighten(on_curve), None);
    let discrete = lane_with(Axis::Discrete, &[p(0, 0.0), p(1920, 1.0)]);
    let hold = Point::new(96.0, discrete.y(0.0));
    assert_eq!(
        gesture(&discrete, hold, ALT, &[nudge(hold, 0.0, -30.0)]),
        None
    );
    assert_eq!(discrete.straighten(hold), None);
}

fn lane_with(axis: Axis, points: &[Breakpoint]) -> Lane {
    lane(axis, (0.0, 1.0), points)
}

#[test]
fn shift_gives_finer_values_and_command_moves_off_the_grid() {
    let lane = float(&[p(0, 0.0), p(960, 0.5), p(3840, 0.0)]);
    let point = at(&lane, 960, 0.5);
    let up = nudge(point, 0.0, -20.0);
    let coarse = gesture(&lane, point, NONE, &[up]).unwrap();
    let fine = gesture(&lane, point, SHIFT, &[up]).unwrap();
    assert!((coarse.points[1].value - 1.0).abs() < 1e-9);
    assert!((fine.points[1].value - 0.55).abs() < 1e-9);
    // On the grid a drag lands on a sixteenth; with ⌘ it lands where the pointer is.
    let right = nudge(point, 13.0, 0.0);
    assert_eq!(
        gesture(&lane, point, NONE, &[right]).unwrap().points[1].time,
        BeatTime(1200)
    );
    assert_eq!(
        gesture(&lane, point, COMMAND, &[right]).unwrap().points[1].time,
        BeatTime(1090)
    );
}

#[test]
fn a_purely_vertical_drag_keeps_an_off_grid_time() {
    let lane = float(&[p(0, 0.0), p(1003, 0.5), p(3840, 0.0)]);
    let point = at(&lane, 1003, 0.5);
    let commit = gesture(&lane, point, NONE, &[nudge(point, 1.0, -10.0)]).unwrap();
    assert_eq!(commit.points[1].time, BeatTime(1003));
}

#[test]
fn a_dragged_point_passes_neighbours_which_vanish_and_return_with_it() {
    let lane = float(&[
        p(480, 0.2),
        p(960, 0.8),
        p(1440, 0.4),
        p(1920, 0.6),
        p(3360, 0.1),
    ]);
    let point = at(&lane, 960, 0.8);
    let mut g = Gesture::press(&lane, point, NONE, None).unwrap();
    // Past the points at 1440 and 1920: both leave the preview.
    g.drag(&lane, nudge(point, 120.0, 0.0), NONE);
    assert_eq!(
        times(g.preview().unwrap()),
        [(480, 0.2), (2160, 0.8), (3360, 0.1)]
    );
    assert_eq!(g.grabbed(), Some(1));
    // Back before 1920: that one returns, the one at 1440 stays passed.
    g.drag(&lane, nudge(point, 72.0, 0.0), NONE);
    assert_eq!(
        times(g.preview().unwrap()),
        [(480, 0.2), (1680, 0.8), (1920, 0.6), (3360, 0.1)]
    );
    // Back onto 1440: the one at 1920 returns and the dragged point, which has not passed
    // 1440, stays before it.
    g.drag(&lane, nudge(point, 48.0, 0.0), NONE);
    assert_eq!(
        times(g.preview().unwrap()),
        [
            (480, 0.2),
            (1440, 0.8),
            (1440, 0.4),
            (1920, 0.6),
            (3360, 0.1)
        ]
    );
    // Back before 1440: that one returns too.
    g.drag(&lane, nudge(point, 30.0, 0.0), NONE);
    assert_eq!(
        times(g.preview().unwrap()),
        [
            (480, 0.2),
            (1200, 0.8),
            (1440, 0.4),
            (1920, 0.6),
            (3360, 0.1)
        ]
    );
    // Releasing past both commits the one list without them.
    let commit = g.release(&lane, nudge(point, 120.0, 0.0), NONE).unwrap();
    assert_eq!(
        times(&commit.points),
        [(480, 0.2), (2160, 0.8), (3360, 0.1)]
    );
    assert_eq!(commit.select, Some(1));
}

#[test]
fn dragging_left_past_neighbours_removes_them_too() {
    let lane = float(&[p(480, 0.2), p(960, 0.8), p(1440, 0.4), p(1920, 0.6)]);
    let point = at(&lane, 1920, 0.6);
    let commit = gesture(&lane, point, NONE, &[nudge(point, -96.0, 0.0)]).unwrap();
    assert_eq!(times(&commit.points), [(480, 0.2), (960, 0.8), (960, 0.6)]);
    let commit = gesture(&lane, point, NONE, &[nudge(point, -140.0, 0.0)]).unwrap();
    // It lands on the point at 480 from the right, so it follows it.
    assert_eq!(times(&commit.points), [(480, 0.2), (480, 0.6)]);
}

#[test]
fn landing_on_a_neighbours_time_makes_a_vertical_step_and_never_a_third_point() {
    let lane = float(&[p(480, 0.2), p(960, 0.8), p(1440, 0.4)]);
    let point = at(&lane, 960, 0.8);
    // Onto the next point's exact time from the left: it has not passed that point, so it
    // stays first. The ramp runs to the dragged point's value, then steps to the old
    // neighbour's.
    let commit = gesture(&lane, point, NONE, &[nudge(point, 48.0, 0.0)]).unwrap();
    assert_eq!(
        times(&commit.points),
        [(480, 0.2), (1440, 0.8), (1440, 0.4)]
    );
    assert_eq!(commit.select, Some(1));
    // From the right it stays second: the ramp reaches the neighbour's value, then steps.
    let right = at(&lane, 1440, 0.4);
    let commit = gesture(&lane, right, NONE, &[nudge(right, -48.0, 0.0)]).unwrap();
    assert_eq!(times(&commit.points), [(480, 0.2), (960, 0.8), (960, 0.4)]);
    assert_eq!(commit.select, Some(2));
    // Onto an existing pair: the nearer of the two counts as passed.
    let lane = float(&[p(480, 0.1), p(960, 0.2), p(960, 0.9), p(1440, 0.5)]);
    let first = at(&lane, 480, 0.1);
    let commit = gesture(&lane, first, NONE, &[nudge(first, 48.0, 0.0)]).unwrap();
    assert_eq!(times(&commit.points), [(960, 0.1), (960, 0.9), (1440, 0.5)]);
    let last = at(&lane, 1440, 0.5);
    let commit = gesture(&lane, last, NONE, &[nudge(last, -48.0, 0.0)]).unwrap();
    assert_eq!(times(&commit.points), [(480, 0.1), (960, 0.2), (960, 0.5)]);
    // A click on empty space at a pair's time adds nothing.
    let at_pair = Point::new(lane.x(BeatTime(960)), lane.y(0.0));
    assert_eq!(gesture(&lane, at_pair, NONE, &[]), None);
    // A click on the step itself takes no gesture.
    assert!(
        Gesture::press(
            &lane,
            Point::new(lane.x(BeatTime(960)), lane.y(0.55)),
            NONE,
            None
        )
        .is_none()
    );
}

#[test]
fn empty_space_at_a_single_points_time_does_not_create_a_pair() {
    let lane = float(&[p(960, 0.2)]);
    let press = Point::new(lane.x(BeatTime(960)), lane.y(0.9));
    assert!(gesture(&lane, press, NONE, &[]).is_none());
}

#[test]
fn a_dragged_point_stays_inside_the_clip() {
    let lane = float(&[p(480, 0.5), p(960, 0.5)]);
    let point = at(&lane, 960, 0.5);
    let commit = gesture(&lane, point, NONE, &[nudge(point, 900.0, -900.0)]).unwrap();
    assert_eq!(times(&commit.points), [(480, 0.5), (3840, 1.0)]);
    let commit = gesture(&lane, point, NONE, &[nudge(point, -900.0, 900.0)]).unwrap();
    assert_eq!(times(&commit.points), [(0, 0.0)]);
}

#[test]
fn stepped_values_round_to_whole_numbers_and_keep_their_curvature() {
    let mut points = vec![p(0, 2.0), p(1920, 6.0)];
    points[0].curve = 0.5;
    let lane = lane(Axis::Stepped, (0.0, 10.0), &points);
    let point = at(&lane, 1920, 6.0);
    // 5 px is 1.25 of 10 over 40 px: 7.25 rounds to 7; Shift does not refine whole steps.
    let commit = gesture(&lane, point, NONE, &[nudge(point, 0.0, -5.0)]).unwrap();
    assert_eq!(commit.points[1].value, 7.0);
    let commit = gesture(&lane, point, SHIFT, &[nudge(point, 0.0, -5.0)]).unwrap();
    assert_eq!(commit.points[1].value, 7.0);
    assert_eq!(commit.points[0].curve, 0.5);
    // Segment drags keep whole steps too.
    let middle = Point::new(
        lane.x(BeatTime(960)),
        lane.y(value_at(&points, 960.0)) + 3.0,
    );
    let commit = gesture(&lane, middle, NONE, &[nudge(middle, 0.0, -9.0)]).unwrap();
    assert_eq!(times(&commit.points), [(0, 4.0), (1920, 8.0)]);
    // Empty space is the selection host's; Alt-drag still bends a Stepped segment.
    assert!(gesture(&lane, Point::new(300.0, 7.0), NONE, &[]).is_none());
    assert!(gesture(&lane, middle, ALT, &[nudge(middle, 0.0, -10.0)]).is_some());
}

#[test]
fn discrete_values_move_between_levels_and_never_bend() {
    // An Enum of four options: levels 0..3 over 40 px, 13.3 px apart.
    let lane = lane(Axis::Discrete, (0.0, 3.0), &[p(0, 0.0), p(1920, 1.0)]);
    let point = at(&lane, 1920, 1.0);
    let up = gesture(&lane, point, NONE, &[nudge(point, 0.0, -14.0)]).unwrap();
    assert_eq!(up.points[1].value, 2.0);
    // Shift has no effect on levels.
    let shifted = gesture(&lane, point, SHIFT, &[nudge(point, 0.0, -14.0)]).unwrap();
    assert_eq!(shifted.points[1].value, 2.0);
    let top = gesture(&lane, point, NONE, &[nudge(point, 0.0, -400.0)]).unwrap();
    assert_eq!(top.points[1].value, 3.0);
    assert_eq!(top.points[0].curve, 0.0);
    // A segment moves whole levels; its end stops at the top level.
    let hold = Point::new(lane.x(BeatTime(960)), lane.y(0.0) + 3.0);
    let commit = gesture(&lane, hold, NONE, &[nudge(hold, 0.0, -400.0)]).unwrap();
    assert_eq!(times(&commit.points), [(0, 2.0), (1920, 3.0)]);
    // Alt-drag does nothing on a held level.
    assert_eq!(gesture(&lane, hold, ALT, &[nudge(hold, 0.0, -30.0)]), None);
}

#[test]
fn a_continuous_drag_stops_at_the_range() {
    let lane = lane(Axis::Continuous, (-1.0, 1.0), &[p(0, 0.0), p(960, 0.0)]);
    let point = at(&lane, 960, 0.0);
    let commit = gesture(&lane, point, NONE, &[nudge(point, 0.0, -900.0)]).unwrap();
    assert_eq!(commit.points[1].value, 1.0);
    let commit = gesture(&lane, point, NONE, &[nudge(point, 0.0, 900.0)]).unwrap();
    assert_eq!(commit.points[1].value, -1.0);
}

#[test]
fn hovering_reads_a_points_value_or_the_lines() {
    let lane = float(&[p(0, 0.0), p(3840, 1.0)]);
    assert_eq!(lane.hover(at(&lane, 3840, 1.0)), Some(1.0));
    let on_line = Point::new(192.0, lane.y(0.5));
    assert!((lane.hover(on_line).unwrap() - 0.5).abs() < 0.02);
    assert_eq!(lane.hover(Point::new(192.0, 2.0)), None);
}

#[test]
fn move_point_keeps_the_dragged_points_own_curve_and_order() {
    let mut points = vec![p(0, 0.0), p(960, 0.5), p(1920, 1.0)];
    points[1].curve = -0.3;
    let (moved, at) = move_point(&points, 1, BeatTime(2400), 0.2);
    assert_eq!(at, 1);
    assert_eq!(times(&moved), [(0, 0.0), (2400, 0.2)]);
    assert_eq!(moved[1].curve, -0.3);
    let (same, at) = move_point(&points, 1, BeatTime(960), 0.9);
    assert_eq!((at, same.len()), (1, 3));
    // Landing on a neighbour's time keeps the side it came from.
    let (moved, at) = move_point(&points, 0, BeatTime(960), 0.9);
    assert_eq!(
        (times(&moved), at),
        (vec![(960, 0.9), (960, 0.5), (1920, 1.0)], 0)
    );
    let (moved, at) = move_point(&points, 2, BeatTime(960), 0.9);
    assert_eq!(
        (times(&moved), at),
        (vec![(0, 0.0), (960, 0.5), (960, 0.9)], 2)
    );
    // A partner at the old time stays behind.
    let pair = [p(960, 0.1), p(960, 0.9), p(1920, 0.5)];
    let (moved, at) = move_point(&pair, 1, BeatTime(480), 0.9);
    assert_eq!(
        (times(&moved), at),
        (vec![(480, 0.9), (960, 0.1), (1920, 0.5)], 0)
    );
}

/// 480 ticks from 960 across the same 384 px: 1.25 ticks a pixel, a sixteenth is 192 px.
fn zoomed(points: &[Breakpoint]) -> Lane {
    let mut lane = float(points);
    lane.time = TimeAxis::windowed(
        0.0,
        384.0,
        BeatTime(3840),
        crate::control::time_axis::Window {
            start: 960.0,
            visible: 480.0,
        },
    );
    lane
}

#[test]
fn a_lane_on_a_windowed_axis_finds_a_point_at_the_axis_x_for_its_time() {
    let lane = zoomed(&[p(1200, 0.5), p(2400, 0.5)]);
    assert_eq!(lane.x(BeatTime(1200)), 192.0);
    assert_eq!(lane.hit(at(&lane, 1200, 0.5)), Hit::Point(0));
    // A point past the window sits off its right edge, where nothing is drawn to grab.
    assert!(lane.x(BeatTime(2400)) > 384.0);
}

#[test]
fn a_press_on_a_windowed_axis_snaps_to_the_grid_in_ticks() {
    let lane = zoomed(&[]);
    // x 112 is tick 960 + 112 × 1.25 = 1100: nearest sixteenth is 1200, ⌘ keeps the tick.
    assert_eq!(lane.time_at(112.0, NONE), BeatTime(1200));
    assert_eq!(lane.time_at(112.0, COMMAND), BeatTime(1100));
    // Before the window's left the time still reads as the content's, held to the clip.
    assert_eq!(lane.time_at(-10_000.0, NONE), BeatTime(0));
    assert_eq!(lane.time_at(10_000.0, NONE), BeatTime(3840));
}

#[test]
fn a_point_drag_on_a_windowed_axis_moves_by_the_windows_ticks_a_pixel() {
    let lane = zoomed(&[p(1200, 0.5)]);
    // 96 px is 120 ticks here (it would be 960 on the fitted lane): 1200 → 1320 with ⌘ held,
    // which leaves the grid.
    let commit = gesture(
        &lane,
        at(&lane, 1200, 0.5),
        COMMAND,
        &[nudge(at(&lane, 1200, 0.5), 96.0, 0.0)],
    )
    .unwrap();
    assert_eq!(times(&commit.points), vec![(1320, 0.5)]);
}
