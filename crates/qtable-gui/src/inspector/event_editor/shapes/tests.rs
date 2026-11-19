use super::*;
use crate::inspector::event_editor::content::Selection;
use crate::inspector::lane_model::tests::mixed_project;
use qtable_engine::model::{AutomationEnvelope, FieldKey, Lane, Project};

fn setup() -> (App, ClipKey) {
    setup_project(mixed_project())
}
fn setup_project(project: Project) -> (App, ClipKey) {
    let mut app = App::new(
        qtable_engine::engine::Engine::new(project).unwrap(),
        "120".into(),
        None,
    );
    app.selection.clip = Some(crate::grid::Selection {
        track: TrackId(1),
        slot: 0,
    });
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    app.chrome.detail_open = true;
    app.focus = crate::focus::Focus::Detail;
    reconcile(&mut app);
    let key = ClipKey {
        track: TrackId(1),
        slot: 0,
        id: app.project().clip(TrackId(1), 0).unwrap().id,
        epoch: app.clip_view.editor.epoch,
    };
    (app, key)
}

/// The argument of the clip's envelope lane for `parameter` (and `field`, if given).
fn target(app: &App, key: ClipKey, parameter: &str, field: Option<&str>) -> ArgumentRef {
    clip_owned(app, key)
        .unwrap()
        .lanes
        .iter()
        .filter_map(|l| l.envelope())
        .find(|e| e.target.parameter.0 == parameter && field.is_none_or(|f| e.target.field.0 == f))
        .unwrap()
        .target
        .clone()
}
fn envelope(app: &App, key: ClipKey, target: &ArgumentRef) -> AutomationEnvelope {
    AutomationEnvelope {
        region: None,
        target: target.clone(),
        points: points_of(&clip_owned(app, key).unwrap(), target),
    }
}
/// A lane's points in `clip`; none for a lane the clip lacks.
fn points_of(clip: &Clip, target: &ArgumentRef) -> Vec<Breakpoint> {
    clip.lanes
        .iter()
        .filter_map(|l| l.envelope())
        .find(|e| e.target == *target)
        .map(|e| e.points.clone())
        .unwrap_or_default()
}
fn lane(target: &ArgumentRef) -> lanes::Identity {
    lanes::Identity::Envelope(target.clone())
}
/// A time selection of `span` on the given lanes.
fn select(app: &mut App, key: ClipKey, span: Range<u64>, lanes: Vec<lanes::Identity>) {
    content::apply(
        app,
        Selection::time(key, BeatTime(span.start)..BeatTime(span.end), lanes),
    );
}
fn insert(app: &mut App, key: ClipKey, shape: Shape, place: Place) {
    handle(
        app,
        Action::InsertShape(key, app.project_revision(), shape, place),
    );
}
fn simplify(app: &mut App, key: ClipKey) {
    handle(app, Action::Simplify(key, app.project_revision()));
}
/// How many undo steps the history holds; call it last in a test.
fn steps(app: &mut App) -> usize {
    let mut count = 0;
    while app.engine.can_undo() {
        app.engine.execute(Command::Undo).unwrap();
        count += 1;
    }
    for _ in 0..count {
        app.engine.execute(Command::Redo).unwrap();
    }
    count
}
fn values(e: &AutomationEnvelope, at: impl Iterator<Item = f64>) -> Vec<f64> {
    at.map(|t| e.value_at(t, Axis::Continuous).unwrap())
        .collect()
}

#[test]
fn a_shape_into_a_time_selection_is_one_edit_one_revision_and_one_undo_step() {
    for shape in Shape::ALL {
        let (mut app, key) = setup();
        let opacity = target(&app, key, "opacity", None);
        let before = clip_owned(&app, key).unwrap();
        select(&mut app, key, 960..1920, vec![lane(&opacity)]);
        insert(&mut app, key, shape, Place::Selection);
        assert_eq!(app.project_revision(), 1, "{shape:?}");
        assert_ne!(clip_owned(&app, key).unwrap(), before, "{shape:?}");
        // The shape's span stays selected.
        let selected = app.clip_view.editor.content.clone().unwrap();
        assert_eq!(selected.range, BeatTime(960)..BeatTime(1920));
        assert_eq!(selected.lanes, vec![lane(&opacity)]);
        // Undo restores the lane exactly, in that one step.
        app.engine.execute(Command::Undo).unwrap();
        assert_eq!(clip_owned(&app, key).unwrap(), before, "{shape:?}");
        assert!(!app.engine.can_undo());
    }
}

#[test]
fn a_shape_leaves_the_envelope_outside_the_selection_where_it_was() {
    // Opacity runs 0.2 up to 0.9 (curved) at 1200, down to 0.3 at 2400, up to 0.6 at 3840;
    // volume holds 0.7 and then rises to 0.9. Each range starts and ends on a point, on a
    // straight segment, or inside a curved one.
    let cases = [
        ("opacity", 2500..3500, 0.0),
        ("opacity", 0..960, 0.0),
        ("opacity", 600..1800, 0.03),
        ("opacity", 1200..2400, 0.0),
        ("opacity", 2400..3840, 0.0),
        ("opacity", 100..3700, 0.0),
        ("volume", 480..1440, 0.0),
        ("volume", 2000..3000, 0.0),
    ];
    for shape in Shape::ALL {
        for (name, span, slack) in &cases {
            let (mut app, key) = setup();
            let lane_target = target(&app, key, name, None);
            let before = envelope(&app, key, &lane_target);
            select(&mut app, key, span.clone(), vec![lane(&lane_target)]);
            insert(&mut app, key, shape, Place::Selection);
            let after = envelope(&app, key, &lane_target);
            let (start, end) = (span.start as f64, span.end as f64);
            // Strictly before the start and from the end on, whatever the shape.
            let outside = (0..=3840 * 2)
                .map(|i| f64::from(i) * 0.5)
                .filter(|&t| t < start - 0.25 || t >= end)
                .collect::<Vec<_>>();
            let (was, is) = (
                values(&before, outside.iter().copied()),
                values(&after, outside.iter().copied()),
            );
            // Where the range ends inside a curved segment, the rest of that segment is
            // refitted (`curve_after`, within 3% of the segment's range).
            for ((t, was), is) in outside.iter().zip(was).zip(is) {
                assert!(
                    (was - is).abs() <= slack * 0.7 + 1e-9,
                    "{shape:?} on {name} {span:?}: at {t} was {was}, is {is}"
                );
            }
            assert_eq!(
                app.project_revision(),
                1,
                "{shape:?} on {name} {span:?} made an edit"
            );
            after.validate().unwrap();
        }
    }
}

#[test]
fn ramp_in_ends_on_the_value_after_with_no_jump() {
    let (mut app, key) = setup();
    let opacity = target(&app, key, "opacity", None);
    let before = envelope(&app, key, &opacity);
    let (start, end) = (2600.0, 3400.0);
    let after_value = before.value_at(end, Axis::Continuous).unwrap();
    select(&mut app, key, 2600..3400, vec![lane(&opacity)]);
    insert(&mut app, key, Shape::RampIn, Place::Selection);
    let envelope = envelope(&app, key, &opacity);
    // It starts at the argument's bottom, after a jump from the value before.
    let at_start: Vec<_> = envelope
        .points
        .iter()
        .filter(|p| p.time == BeatTime(2600))
        .map(|p| p.value)
        .collect();
    assert_eq!(at_start.len(), 2);
    assert_eq!(at_start[1], 0.0);
    assert!((at_start[0] - before.value_at(start, Axis::Continuous).unwrap()).abs() < 1e-12);
    // It ends on the value after: one point there, no jump, and the line arrives on it.
    let at_end: Vec<_> = envelope
        .points
        .iter()
        .filter(|p| p.time == BeatTime(3400))
        .collect();
    assert_eq!(at_end.len(), 1);
    assert_eq!(at_end[0].value, after_value);
    let arriving = envelope.value_at(end - 1e-6, Axis::Continuous).unwrap();
    assert!((arriving - after_value).abs() < 1e-4, "{arriving}");
    // Fast at first: a quarter of the way along it is already past half the climb.
    let quarter = envelope.value_at(start + 200.0, Axis::Continuous).unwrap();
    assert!(quarter > 0.5 * after_value, "{quarter}");
}

#[test]
fn ramp_out_starts_on_the_value_before_with_no_jump() {
    let (mut app, key) = setup();
    let opacity = target(&app, key, "opacity", None);
    let before = envelope(&app, key, &opacity);
    let (start, end) = (2600.0, 3400.0);
    let before_value = before.value_at(start, Axis::Continuous).unwrap();
    let after_value = before.value_at(end, Axis::Continuous).unwrap();
    select(&mut app, key, 2600..3400, vec![lane(&opacity)]);
    insert(&mut app, key, Shape::RampOut, Place::Selection);
    let envelope = envelope(&app, key, &opacity);
    let at_start: Vec<_> = envelope
        .points
        .iter()
        .filter(|p| p.time == BeatTime(2600))
        .collect();
    assert_eq!(at_start.len(), 1, "no jump at the start");
    assert_eq!(at_start[0].value, before_value);
    assert_eq!(at_start[0].curve, 0.5);
    // It falls to the bottom, then jumps to the value after.
    let at_end: Vec<_> = envelope
        .points
        .iter()
        .filter(|p| p.time == BeatTime(3400))
        .map(|p| p.value)
        .collect();
    assert_eq!(at_end, vec![0.0, after_value]);
}

#[test]
fn the_ramps_link_to_a_lane_with_no_points_by_its_stored_value() {
    let (mut app, key) = setup();
    let y = target(&app, key, "position", Some("y"));
    let stored = app
        .project()
        .track(key.track)
        .and_then(|t| t.argument_spec(&y))
        .map(|_| {
            app.project()
                .field(&y.on(key.track))
                .map(|f| f.value)
                .unwrap()
        })
        .unwrap();
    select(&mut app, key, 480..1440, vec![lane(&y)]);
    insert(&mut app, key, Shape::RampIn, Place::Selection);
    let points = envelope(&app, key, &y).points;
    assert_eq!(
        points
            .iter()
            .map(|p| (p.time.ticks(), p.value))
            .collect::<Vec<_>>(),
        [(480, 0.0), (1440, stored)]
    );
}

#[test]
fn a_saws_vertical_edges_come_from_the_join() {
    let (mut app, key) = setup();
    let volume = target(&app, key, "volume", None);
    select(&mut app, key, 480..1440, vec![lane(&volume)]);
    insert(&mut app, key, Shape::SawDown, Place::Selection);
    let e = envelope(&app, key, &volume);
    // Volume holds 0.7 outside; the saw drops from the top to the bottom.
    let at = |t: f64| e.value_at(t, Axis::Continuous).unwrap();
    assert_eq!(at(479.0), 0.7);
    assert!(at(480.0) > 0.99);
    assert!(at(1439.0) < 0.01);
    assert_eq!(at(1440.0), 0.7);
    // Two points at each edge, and no point of the saw's own between its ends.
    assert_eq!(
        e.points
            .iter()
            .filter(|p| (480..=1440).contains(&p.time.ticks()))
            .map(|p| (p.time.ticks(), p.value))
            .collect::<Vec<_>>(),
        [(480, 0.7), (480, 1.0), (1440, 0.0), (1440, 0.7)]
    );
}

#[test]
fn a_shape_reaches_every_selected_number_lane_and_skips_event_choice_and_switch_lanes() {
    let (mut app, key) = setup();
    let before = clip_owned(&app, key).unwrap();
    let all = lanes::identities(&before)
        .iter()
        .map(|i| i.owned())
        .collect();
    select(&mut app, key, 960..1920, all);
    insert(&mut app, key, Shape::Triangle, Place::Selection);
    assert_eq!(app.project_revision(), 1);
    let after = clip_owned(&app, key).unwrap();
    for name in ["opacity", "steps", "position", "volume"] {
        let t = before
            .lanes
            .iter()
            .filter_map(|l| l.envelope())
            .find(|e| e.target.parameter.0 == name)
            .unwrap()
            .target
            .clone();
        assert_ne!(
            points_of(&after, &t),
            points_of(&before, &t),
            "{name} takes the shape"
        );
        assert!(
            points_of(&after, &t)
                .iter()
                .any(|p| p.time == BeatTime(1440)),
            "{name} peaks mid-selection"
        );
    }
    for name in ["strobe", "mode"] {
        let t = target(&app, key, name, None);
        assert_eq!(
            points_of(&after, &t),
            points_of(&before, &t),
            "{name} holds a value"
        );
    }
    assert_eq!(after.events, before.events);
    after.validate().unwrap();
    assert_eq!(steps(&mut app), 1);
}

#[test]
fn whole_step_lanes_get_whole_values() {
    let (mut app, key) = setup();
    let steps_lane = target(&app, key, "steps", None);
    select(&mut app, key, 100..3700, vec![lane(&steps_lane)]);
    insert(&mut app, key, Shape::Sine, Place::Selection);
    let inside: Vec<_> = envelope(&app, key, &steps_lane)
        .points
        .into_iter()
        .filter(|p| (100..=3700).contains(&p.time.ticks()))
        .collect();
    assert!(inside.len() >= 7);
    assert!(
        inside
            .iter()
            .all(|p| p.value.fract() == 0.0 && (0.0..=8.0).contains(&p.value)),
        "{inside:?}"
    );
}

#[test]
fn a_shape_placed_at_a_time_fills_one_grid_step_from_it_and_selects_the_step() {
    for (grid, step) in [
        (Grid::Sixteenth, 240),
        (Grid::Quarter, 960),
        (Grid::EighthTriplet, 320),
        // Grid Off uses 1/16, as Draw Mode's hits do.
        (Grid::Off, 240),
    ] {
        // On the grid and off it: the step starts exactly where it is put.
        for at in [960, 1003] {
            let (mut app, key) = setup();
            app.clip_view.editor.grid = grid;
            let y = target(&app, key, "position", Some("y"));
            insert(
                &mut app,
                key,
                Shape::Triangle,
                Place::Cell(BeatTime(at), lane(&y)),
            );
            assert_eq!(app.project_revision(), 1, "{grid:?} {at}");
            let points = envelope(&app, key, &y).points;
            assert_eq!(
                points
                    .iter()
                    .map(|p| (p.time.ticks(), p.value))
                    .collect::<Vec<_>>(),
                [(at, 0.0), (at + step / 2, 100.0), (at + step, 0.0)],
                "{grid:?} {at}"
            );
            let selected = app.clip_view.editor.content.clone().unwrap();
            assert_eq!(selected.range, BeatTime(at)..BeatTime(at + step));
            assert_eq!(selected.lanes, vec![lane(&y)]);
        }
    }
}

#[test]
fn a_shape_placed_at_a_point_keeps_that_point_and_leaves_the_rest_alone() {
    let (mut app, key) = setup();
    let opacity = target(&app, key, "opacity", None);
    let before = envelope(&app, key, &opacity);
    // Opacity has a point at 1200 (0.9, curved). Select something else first: the place alone
    // decides where the shape goes.
    select(&mut app, key, 3000..3600, vec![]);
    insert(
        &mut app,
        key,
        Shape::SawDown,
        Place::Cell(BeatTime(1200), lane(&opacity)),
    );
    let after = envelope(&app, key, &opacity);
    assert_eq!(app.project_revision(), 1);
    // The clicked point stays, as the value the lane arrives at 1200 with; the saw follows it.
    let at_point: Vec<_> = after
        .points
        .iter()
        .filter(|p| p.time == BeatTime(1200))
        .map(|p| p.value)
        .collect();
    assert_eq!(at_point, vec![0.9, 1.0]);
    // Points after the step (and everything before the point) are as they were.
    for t in (0..=3840).step_by(60).filter(|t| !(1200..1440).contains(t)) {
        let t = f64::from(t);
        let (was, is) = (
            before.value_at(t, Axis::Continuous).unwrap(),
            after.value_at(t, Axis::Continuous).unwrap(),
        );
        assert!((was - is).abs() < 0.03 * 0.6, "at {t}: {was} vs {is}");
    }
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().range,
        BeatTime(1200)..BeatTime(1440)
    );
}

#[test]
fn a_cell_stops_at_the_clips_end_and_a_sliver_makes_no_edit() {
    let (mut app, key) = setup();
    let y = target(&app, key, "position", Some("y"));
    insert(
        &mut app,
        key,
        Shape::SawUp,
        Place::Cell(BeatTime(3720), lane(&y)),
    );
    assert_eq!(
        envelope(&app, key, &y).points.last().unwrap().time,
        BeatTime(3840)
    );
    let (mut app, key) = setup();
    insert(
        &mut app,
        key,
        Shape::SawUp,
        Place::Cell(BeatTime(3830), lane(&y)),
    );
    assert_eq!(app.project_revision(), 0);
    assert_eq!(
        app.status.message(),
        Some("Too close to the clip's end for a shape")
    );
}

#[test]
fn an_argument_with_no_lane_gets_one() {
    let (mut app, key) = setup();
    let y = target(&app, key, "position", Some("y"));
    let x = ArgumentRef {
        field: FieldKey("x".into()),
        ..y
    };
    assert!(
        !clip_owned(&app, key)
            .unwrap()
            .lanes
            .iter()
            .any(|l| l.envelope().is_some_and(|e| e.target == x))
    );
    insert(
        &mut app,
        key,
        Shape::Square,
        Place::Cell(BeatTime(480), lane(&x)),
    );
    assert_eq!(app.project_revision(), 1);
    let created = clip_owned(&app, key)
        .unwrap()
        .lanes
        .iter()
        .filter_map(|l| l.envelope())
        .find(|e| e.target == x)
        .cloned()
        .unwrap();
    assert_eq!(
        created
            .points
            .iter()
            .map(|p| (p.time.ticks(), p.value))
            .collect::<Vec<_>>(),
        // The argument's stored value (0 here) holds before the shape, with a jump into it; it
        // already meets the shape's last value, so no jump out is needed.
        [
            (480, 0.0),
            (480, 100.0),
            (600, 100.0),
            (600, 0.0),
            (720, 0.0)
        ]
    );
    created.validate().unwrap();
}

#[test]
fn nothing_happens_without_a_time_range_or_on_a_lane_that_holds_a_value() {
    let (mut app, key) = setup();
    let opacity = target(&app, key, "opacity", None);
    // An insert marker has no range.
    select(&mut app, key, 960..960, vec![lane(&opacity)]);
    insert(&mut app, key, Shape::Sine, Place::Selection);
    // Nor does a picked point, though its range is not empty.
    let clip = clip_owned(&app, key).unwrap();
    let pick = Selection::items(&clip)
        .into_iter()
        .map(|(item, _)| item)
        .find(|item| item.lane == lane(&opacity))
        .unwrap();
    let mut picked = Selection::time(key, BeatTime(0)..BeatTime(1), vec![]);
    picked.picks = Some(vec![pick]);
    content::apply(&mut app, picked);
    insert(&mut app, key, Shape::Sine, Place::Selection);
    // No selection at all.
    content::clear(&mut app.clip_view.editor);
    insert(&mut app, key, Shape::Sine, Place::Selection);
    // A choice lane and an event lane named by a place take no shape.
    let mode = target(&app, key, "mode", None);
    insert(
        &mut app,
        key,
        Shape::Sine,
        Place::Cell(BeatTime(0), lane(&mode)),
    );
    let kick = clip.lane_refs()[0].clone();
    insert(
        &mut app,
        key,
        Shape::Sine,
        Place::Cell(BeatTime(0), lanes::Identity::Event(kick)),
    );
    assert_eq!(app.project_revision(), 0);
    assert_eq!(clip_owned(&app, key).unwrap(), clip);
}

#[test]
fn a_too_short_span_makes_no_edit_and_says_so() {
    let (mut app, key) = setup();
    let opacity = target(&app, key, "opacity", None);
    select(&mut app, key, 100..115, vec![lane(&opacity)]);
    insert(&mut app, key, Shape::Sine, Place::Selection);
    assert_eq!(app.project_revision(), 0);
    assert_eq!(
        app.status.message(),
        Some("Selection too short for a shape")
    );
    // Sixteen ticks is enough.
    select(&mut app, key, 100..116, vec![lane(&opacity)]);
    insert(&mut app, key, Shape::Sine, Place::Selection);
    assert_eq!(app.project_revision(), 1);
}

#[test]
fn a_stale_clip_or_revision_does_nothing() {
    let (mut app, key) = setup();
    let opacity = target(&app, key, "opacity", None);
    select(&mut app, key, 960..1920, vec![lane(&opacity)]);
    let revision = app.project_revision();
    handle(
        &mut app,
        Action::InsertShape(key, revision + 1, Shape::Sine, Place::Selection),
    );
    handle(&mut app, Action::Simplify(key, revision + 1));
    let stale = ClipKey {
        epoch: key.epoch + 1,
        ..key
    };
    handle(
        &mut app,
        Action::InsertShape(stale, revision, Shape::Sine, Place::Selection),
    );
    handle(&mut app, Action::Simplify(stale, revision));
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn inserting_a_shape_leaves_the_content_clipboard_alone() {
    let (mut app, key) = setup();
    let opacity = target(&app, key, "opacity", None);
    select(&mut app, key, 0..960, vec![lane(&opacity)]);
    crate::inspector::event_editor::regions::shortcut(
        &mut app,
        crate::clip_shortcuts::Shortcut::Copy,
    );
    let copied = app.clip_view.editor.clipboard.clone();
    assert!(copied.is_some());
    select(&mut app, key, 1920..2880, vec![lane(&opacity)]);
    insert(&mut app, key, Shape::Sine, Place::Selection);
    assert_eq!(app.project_revision(), 1);
    assert_eq!(app.clip_view.editor.clipboard, copied);
}

/// The mixed project with dense, noisy-free envelopes on opacity and volume, for Simplify.
fn dense() -> Project {
    let mut project = mixed_project();
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    for lane in &mut clip.lanes {
        let Lane::Envelope(e) = lane else { continue };
        let line = |from: f64, to: f64| {
            (0..=16).map(move |i| Breakpoint {
                time: BeatTime(i * 240),
                value: from + (to - from) * i as f64 / 16.0,
                curve: 0.0,
            })
        };
        match e.target.parameter.0.as_str() {
            "opacity" => e.points = line(0.0, 1.0).collect(),
            "volume" => e.points = line(1.0, 0.0).collect(),
            _ => {}
        }
    }
    project.validate().unwrap();
    project
}

#[test]
fn simplify_on_a_time_selection_is_one_edit_over_all_its_lanes() {
    let (mut app, key) = setup_project(dense());
    let (opacity, volume) = (
        target(&app, key, "opacity", None),
        target(&app, key, "volume", None),
    );
    let before = clip_owned(&app, key).unwrap();
    select(
        &mut app,
        key,
        480..2880,
        vec![lane(&opacity), lane(&volume)],
    );
    simplify(&mut app, key);
    assert_eq!(app.project_revision(), 1);
    let after = clip_owned(&app, key).unwrap();
    for t in [&opacity, &volume] {
        let times: Vec<_> = points_of(&after, t)
            .iter()
            .map(|p| p.time.ticks())
            .collect();
        // The points on the selection's edges stay, so do those outside; the straight
        // line between has no need of the ones strictly inside.
        assert_eq!(times, [0, 240, 480, 2880, 3120, 3360, 3600, 3840]);
    }
    // The line is the same line.
    for t in [&opacity, &volume] {
        let (was, is) = (
            AutomationEnvelope {
                region: None,
                target: t.clone(),
                points: points_of(&before, t),
            },
            envelope(&app, key, t),
        );
        for tick in (0..=3840).step_by(40) {
            let tick = f64::from(tick);
            assert!(
                (was.value_at(tick, Axis::Continuous).unwrap()
                    - is.value_at(tick, Axis::Continuous).unwrap())
                .abs()
                    < 1e-9
            );
        }
    }
    assert_eq!(app.status.message(), Some("Simplified 34 points to 16"));
    // The selection stays, and undo restores both lanes in one step.
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().range,
        BeatTime(480)..BeatTime(2880)
    );
    app.engine.execute(Command::Undo).unwrap();
    assert_eq!(clip_owned(&app, key).unwrap(), before);
    assert!(!app.engine.can_undo());
}

#[test]
fn simplify_without_a_time_selection_takes_the_whole_envelope() {
    let (mut app, key) = setup_project(dense());
    let (opacity, volume) = (
        target(&app, key, "opacity", None),
        target(&app, key, "volume", None),
    );
    // An insert marker on one lane names that lane alone.
    select(&mut app, key, 960..960, vec![lane(&opacity)]);
    simplify(&mut app, key);
    assert_eq!(app.project_revision(), 1);
    let after = clip_owned(&app, key).unwrap();
    assert_eq!(
        points_of(&after, &opacity)
            .iter()
            .map(|p| p.time.ticks())
            .collect::<Vec<_>>(),
        [0, 3840]
    );
    assert_eq!(points_of(&after, &volume).len(), 17);
    assert_eq!(app.status.message(), Some("Simplified 17 points to 2"));
    assert_eq!(steps(&mut app), 1);
}

#[test]
fn simplify_with_nothing_to_remove_makes_no_edit_and_says_so() {
    let (mut app, key) = setup();
    let opacity = target(&app, key, "opacity", None);
    select(&mut app, key, 0..3840, vec![lane(&opacity)]);
    simplify(&mut app, key);
    assert_eq!(app.project_revision(), 0);
    assert_eq!(app.status.message(), Some("Nothing to simplify"));
    // No lane named at all says the same.
    content::clear(&mut app.clip_view.editor);
    simplify(&mut app, key);
    assert_eq!(app.project_revision(), 0);
}

#[test]
fn simplify_thins_a_choice_lane_only_where_nothing_changes() {
    let mut project = mixed_project();
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    for lane in &mut clip.lanes {
        let Lane::Envelope(e) = lane else { continue };
        if e.target.parameter.0 == "strobe" {
            e.points = [(0, 0.0), (480, 0.0), (960, 1.0), (1440, 1.0), (1920, 0.0)]
                .map(|(t, value)| Breakpoint {
                    time: BeatTime(t),
                    value,
                    curve: 0.0,
                })
                .to_vec();
        }
    }
    project.validate().unwrap();
    let (mut app, key) = setup_project(project);
    let strobe = target(&app, key, "strobe", None);
    select(&mut app, key, 0..0, vec![lane(&strobe)]);
    simplify(&mut app, key);
    assert_eq!(
        envelope(&app, key, &strobe)
            .points
            .iter()
            .map(|p| (p.time.ticks(), p.value))
            .collect::<Vec<_>>(),
        [(0, 0.0), (960, 1.0), (1920, 0.0)]
    );
}

#[test]
fn can_simplify_needs_a_lane_with_three_points() {
    let (mut app, key) = setup();
    assert!(!can_simplify(&app), "nothing is selected");
    let y = target(&app, key, "position", Some("y"));
    select(&mut app, key, 0..0, vec![lane(&y)]);
    assert!(!can_simplify(&app), "an empty lane");
    let opacity = target(&app, key, "opacity", None);
    select(&mut app, key, 0..0, vec![lane(&y), lane(&opacity)]);
    assert!(can_simplify(&app));
}

/// The mixed clip with its loop brace on 960..2880: a loop length of 1920 ticks, while its
/// content (and so the editor's extent) runs to 3840.
fn later_loop() -> (App, ClipKey) {
    let mut project = mixed_project();
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    (clip.start, clip.end) = (BeatTime(960), BeatTime(2880));
    (clip.loop_start, clip.loop_end) = (BeatTime(960), BeatTime(2880));
    clip.looping = true;
    project.validate().unwrap();
    let (app, key) = setup_project(project);
    let clip = clip_owned(&app, key).unwrap();
    assert_eq!(clip.length(), BeatTime(1920));
    assert_eq!(
        crate::inspector::clip_markers::extent(&clip, app.project().time_signature),
        BeatTime(3840)
    );
    (app, key)
}

#[test]
fn on_a_clip_looping_later_a_cell_at_the_loop_end_still_fills_one_grid_step() {
    // Clamped to the loop length (1920), a cell at 2880 had no room and made no shape.
    for (time, span) in [(2880, 240), (1800, 240), (3000, 240)] {
        let (mut app, key) = later_loop();
        let y = target(&app, key, "position", Some("y"));
        insert(
            &mut app,
            key,
            Shape::SawUp,
            Place::Cell(BeatTime(time), lane(&y)),
        );
        assert_eq!(app.project_revision(), 1, "cell at {time}");
        let selected = app.clip_view.editor.content.clone().unwrap();
        assert_eq!(
            selected.range,
            BeatTime(time)..BeatTime(time + span),
            "cell at {time}"
        );
        assert_eq!(
            envelope(&app, key, &y).points.last().unwrap().time,
            BeatTime(time + span)
        );
    }
}

#[test]
fn on_a_clip_looping_later_a_cell_stops_at_the_editors_extent_not_the_loop_length() {
    let (mut app, key) = later_loop();
    let y = target(&app, key, "position", Some("y"));
    insert(
        &mut app,
        key,
        Shape::SawUp,
        Place::Cell(BeatTime(3720), lane(&y)),
    );
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().range,
        BeatTime(3720)..BeatTime(3840)
    );
    // Within sixteen ticks of the extent there is no room, and the message says why.
    let (mut app, key) = later_loop();
    insert(
        &mut app,
        key,
        Shape::SawUp,
        Place::Cell(BeatTime(3830), lane(&y)),
    );
    assert_eq!(app.project_revision(), 0);
    assert_eq!(
        app.status.message(),
        Some("Too close to the clip's end for a shape")
    );
}

#[test]
fn from_the_menus_on_a_clip_looping_later_a_shape_fills_a_step_at_the_loop_end() {
    use crate::inspector::event_editor::menu::{Click, ContentCommand, Under};
    let (mut app, key) = later_loop();
    let opacity = lane(&target(&app, key, "opacity", None));
    app.close_overlay();
    let revision = app.project_revision();
    handle(
        &mut app,
        Action::ContentMenu(Click {
            key,
            revision,
            lane: opacity,
            under: Under::Space(BeatTime(2880)),
        }),
    );
    let Some(crate::overlay::Overlay::Context(crate::context_menu::OpenMenu {
        target: crate::context_menu::MenuTarget::Content(menu),
        ..
    })) = app.overlay.clone()
    else {
        panic!("a content menu");
    };
    handle(
        &mut app,
        Action::ContentCommand(menu, ContentCommand::Shape(Shape::Sine)),
    );
    assert_eq!(app.project_revision(), 1);
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().range,
        BeatTime(2880)..BeatTime(3120)
    );
}

/// Set `x`'s stored value (the one its dotted line shows) and give back its argument.
fn with_stored_value(app: &mut App, key: ClipKey, value: f64) -> ArgumentRef {
    let y = target(app, key, "position", Some("y"));
    let x = ArgumentRef {
        field: FieldKey("x".into()),
        ..y
    };
    let field = x.on(key.track);
    assert!(crate::edit_actions::run_edit(
        app,
        Command::Edit(ProjectEdit::SetParameter {
            target: field.parameter.clone(),
            values: vec![(field.field.clone(), value)],
        })
    ));
    assert_eq!(app.project().field(&field).unwrap().value, value);
    x
}

/// The envelope's value just before `from`, just after `to`, and at the clip's two ends.
fn outside(e: &AutomationEnvelope, from: u64, to: u64) -> [f64; 4] {
    [from as f64 - 1.0, to as f64 + 1.0, 0.0, 3839.0]
        .map(|t| e.value_at(t, Axis::Continuous).unwrap())
}

#[test]
fn on_a_lane_with_no_points_a_shape_leaves_the_stored_value_before_and_after_its_span() {
    for shape in [Shape::Sine, Shape::Square, Shape::Triangle, Shape::SawDown] {
        for selection in [true, false] {
            let (mut app, key) = setup();
            let x = with_stored_value(&mut app, key, 37.0);
            let before = app.project_revision();
            let (from, to) = if selection {
                select(&mut app, key, 960..1920, vec![lane(&x)]);
                insert(&mut app, key, shape, Place::Selection);
                (960, 1920)
            } else {
                // A grid step (1/16 = 240 ticks), as from a point's or empty space's menu.
                insert(&mut app, key, shape, Place::Cell(BeatTime(960), lane(&x)));
                (960, 1200)
            };
            assert_eq!(app.project_revision(), before + 1, "{shape:?} {selection}");
            let created = envelope(&app, key, &x);
            created.validate().unwrap();
            assert_eq!(
                outside(&created, from, to),
                [37.0; 4],
                "{shape:?}, selection {selection}: {:?}",
                created.points
            );
            // Inside the span it is the shape, not the stored value.
            let middle = created
                .value_at(((from + to) / 2) as f64, Axis::Continuous)
                .unwrap();
            assert!(
                shape == Shape::Sine || (middle - 37.0).abs() > 1e-9,
                "{shape:?}: {middle}"
            );
        }
    }
}

#[test]
fn the_jumps_at_the_boundaries_are_pairs_at_one_tick() {
    let (mut app, key) = setup();
    let x = with_stored_value(&mut app, key, 37.0);
    insert(
        &mut app,
        key,
        Shape::Square,
        Place::Cell(BeatTime(480), lane(&x)),
    );
    let points: Vec<_> = envelope(&app, key, &x)
        .points
        .iter()
        .map(|p| (p.time.ticks(), p.value))
        .collect();
    assert_eq!(
        points,
        [
            (480, 37.0),
            (480, 100.0),
            (600, 100.0),
            (600, 0.0),
            (720, 0.0),
            (720, 37.0)
        ]
    );
}

#[test]
fn a_ramp_on_a_lane_with_no_points_also_holds_the_stored_value_outside() {
    // Ramp In already links its end to the stored value; its start jump comes from the same rule.
    let (mut app, key) = setup();
    let x = with_stored_value(&mut app, key, 37.0);
    select(&mut app, key, 480..1440, vec![lane(&x)]);
    insert(&mut app, key, Shape::RampIn, Place::Selection);
    let created = envelope(&app, key, &x);
    assert_eq!(outside(&created, 480, 1440), [37.0; 4]);
    assert_eq!(created.value_at(480.0, Axis::Continuous), Some(0.0));
    // Ramp Out likewise, from the stored value down to the bottom and back up to it.
    let (mut app, key) = setup();
    let x = with_stored_value(&mut app, key, 37.0);
    select(&mut app, key, 480..1440, vec![lane(&x)]);
    insert(&mut app, key, Shape::RampOut, Place::Selection);
    assert_eq!(outside(&envelope(&app, key, &x), 480, 1440), [37.0; 4]);
}

#[test]
fn a_lane_that_has_points_is_unchanged_by_the_stored_value() {
    // Only an empty lane takes the stored value; with points, the lane's own curve joins the shape.
    let (mut app, key) = setup();
    let opacity = target(&app, key, "opacity", None);
    select(&mut app, key, 960..1920, vec![lane(&opacity)]);
    insert(&mut app, key, Shape::Square, Place::Selection);
    let points = envelope(&app, key, &opacity).points;
    let (before, after) = (
        points.iter().filter(|p| p.time == BeatTime(960)).count(),
        points.iter().filter(|p| p.time == BeatTime(1920)).count(),
    );
    // One jump pair at each boundary, as `seam` has always made.
    assert_eq!((before, after), (2, 2));
}
