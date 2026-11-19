//! The one-lane canvas driving `point_gesture`: the same gestures as the overview, on a plot
//! that fills the view but for the value scale. The bounds are 600 x 400 at (20, 30), so the plot
//! runs from x 52 to 590 (VALUE_SCALE_WIDTH, GUTTER) and y 52 to 382 (RULER_HEIGHT, PAD_TOP,
//! PAD_BOTTOM).
use super::*;
use crate::inspector::lane_model::tests::mixed_project;
use canvas::Program;
use qtable_engine::model::{DeviceId, DeviceRef, FieldKey, ParameterKey, Project, TrackId};

fn argument(parameter: &str, field: &str) -> ArgumentRef {
    ArgumentRef {
        device: DeviceRef::Chain(DeviceId(1)),
        parameter: ParameterKey(parameter.into()),
        field: FieldKey(field.into()),
    }
}

fn lane_for(project: &Project, target: &ArgumentRef) -> OneLane {
    let clip = project.clip(TrackId(1), 0).unwrap();
    let model = LaneModel::build(project, TrackId(1), clip);
    let live = model.live(project, &[]);
    let chosen = crate::inspector::lane_view::chosen_row(project, TrackId(1), &model, target);
    let laned = clip
        .lanes
        .iter()
        .any(|l| l.envelope().is_some_and(|e| e.target == *target));
    OneLane {
        clip: clip.clone().into(),
        live_chosen: crate::inspector::lane_model::live_value(project, &[], &chosen.field),
        model: model.into(),
        live,
        chosen,
        laned,
        key: ClipKey {
            track: TrackId(1),
            slot: 0,
            id: clip.id,
            epoch: 0,
        },
        point: None,
        grid: Grid::Sixteenth,
        signature: TimeSignature::default(),
        playhead: None,
        context: Context::default(),
    }
}

fn bounds() -> Rectangle {
    Rectangle::new(Point::new(20.0, 30.0), Size::new(600.0, 400.0))
}
fn cursor(at: (f32, f32)) -> mouse::Cursor {
    mouse::Cursor::Available(Point::new(at.0 + 20.0, at.1 + 30.0))
}
const LEFT: f32 = detail::VALUE_SCALE_WIDTH;
fn x(ticks: f64) -> f32 {
    LEFT + (ticks / 3840.0 * f64::from(590.0 - LEFT)) as f32
}
fn y(value: f64) -> f32 {
    52.0 + 330.0 * (1.0 - value as f32)
}
fn motion(at: (f32, f32)) -> canvas::Event {
    canvas::Event::Mouse(mouse::Event::CursorMoved {
        position: Point::new(at.0 + 20.0, at.1 + 30.0),
    })
}
fn press() -> canvas::Event {
    canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
}
fn release() -> canvas::Event {
    canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
}
fn send(
    lane: &OneLane,
    state: &mut State,
    event: &canvas::Event,
    at: Option<(f32, f32)>,
) -> Option<Action> {
    lane.update(
        state,
        event,
        bounds(),
        at.map_or(mouse::Cursor::Unavailable, cursor),
    )
    .and_then(|action| match action.into_inner().0 {
        Some(Message::Timeline(action)) => Some(action),
        _ => None,
    })
}
fn list(points: &[Breakpoint]) -> Vec<(u64, f64)> {
    points
        .iter()
        .map(|p| (p.time.ticks(), (p.value * 1e3).round() / 1e3))
        .collect()
}
/// Opacity's 0.9 point (at 1200 ticks).
fn grab() -> (f32, f32) {
    (x(1200.0), y(0.9))
}

#[test]
fn the_plot_fills_the_view_below_the_ruler() {
    let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let plot = lane.plot(Size::new(600.0, 400.0));
    assert_eq!((plot.left, plot.width), (LEFT, 590.0 - LEFT));
    assert_eq!((plot.top, plot.height), (52.0, 330.0));
    // A short view keeps a usable plot.
    assert_eq!(lane.plot(Size::new(600.0, 60.0)).height, 24.0);
}

#[test]
fn a_press_publishes_nothing_a_click_selects_a_point_and_a_second_click_deletes_it() {
    let mut lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let mut state = State::default();
    assert!(send(&lane, &mut state, &press(), Some(grab())).is_none());
    // The first click selects it for the fields, publishing the unchanged list (choice A).
    let Some(Action::SetPoints(target, points, select)) =
        send(&lane, &mut state, &release(), Some(grab()))
    else {
        panic!("a click on a point commits SetPoints");
    };
    assert_eq!(target.target, argument("opacity", "value"));
    assert_eq!(
        list(&points),
        [(0, 0.2), (1200, 0.9), (2400, 0.3), (3840, 0.6)]
    );
    assert_eq!(select, Some(1));
    assert!(send(&lane, &mut state, &release(), Some(grab())).is_none());
    // Once selected, a click on it deletes it.
    lane.point = Some(1);
    send(&lane, &mut state, &press(), Some(grab()));
    let Some(Action::SetPoints(_, points, select)) =
        send(&lane, &mut state, &release(), Some(grab()))
    else {
        panic!("a click on the selected point commits SetPoints");
    };
    assert_eq!(list(&points), [(0, 0.2), (2400, 0.3), (3840, 0.6)]);
    assert_eq!(select, None);
}

#[test]
fn a_drag_previews_and_commits_one_list_on_release() {
    let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let mut state = State::default();
    send(&lane, &mut state, &press(), Some(grab()));
    let up = (grab().0, y(0.5));
    send(&lane, &mut state, &motion(up), Some(up));
    let drag = lane.drag(&state).expect("the gesture is current");
    let preview = drag.gesture.preview().expect("a moving drag previews");
    assert!((preview[1].value - 0.5).abs() < 0.01);
    let Some(Action::SetPoints(_, points, select)) = send(&lane, &mut state, &release(), Some(up))
    else {
        panic!("a drag commits SetPoints");
    };
    assert_eq!(points.len(), 4);
    assert!((points[1].value - 0.5).abs() < 0.01);
    assert_eq!(select, Some(1));
}

#[test]
fn a_click_on_an_empty_argument_publishes_start_envelope_with_the_first_point() {
    let project = mixed_project();
    let target = argument("position", "x");
    let lane = lane_for(&project, &target);
    assert!(!lane.laned);
    let mut state = State::default();
    let at = (
        x(1920.0),
        lane.lane(bounds().size())
            .y(lane.live_chosen.unwrap().value),
    );
    send(&lane, &mut state, &press(), Some(at));
    let Some(Action::StartEnvelope(start, points, select)) =
        send(&lane, &mut state, &release(), Some(at))
    else {
        panic!("the first point starts the lane");
    };
    assert_eq!(start.target, target);
    assert_eq!(select, Some(0));
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].time, BeatTime(1920));
    // Position x runs 0 to 100.
    assert_eq!(points[0].value, lane.live_chosen.unwrap().value);
}

#[test]
fn changing_the_chosen_lane_mid_drag_cancels_the_drag() {
    let project = mixed_project();
    let opacity = lane_for(&project, &argument("opacity", "value"));
    let steps = lane_for(&project, &argument("steps", "value"));
    let mut state = State::default();
    send(&opacity, &mut state, &press(), Some(grab()));
    assert!(state.drag.is_some());
    // The same canvas now describes another lane: the release it sees commits nothing.
    assert!(send(&steps, &mut state, &release(), Some(grab())).is_none());
    assert!(state.drag.is_none());
}

#[test]
fn a_new_epoch_or_revision_also_cancels_it() {
    let project = mixed_project();
    let lane = lane_for(&project, &argument("opacity", "value"));
    let mut state = State::default();
    send(&lane, &mut state, &press(), Some(grab()));
    let mut later = lane_for(&project, &argument("opacity", "value"));
    later.key.epoch = 1;
    assert!(send(&later, &mut state, &release(), Some(grab())).is_none());
    send(&later, &mut state, &press(), Some(grab()));
    later.context.revision = 5;
    assert!(send(&later, &mut state, &release(), Some(grab())).is_none());
}

#[test]
fn escape_unfocus_and_the_pointer_leaving_cancel_without_a_commit() {
    let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let escape = canvas::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: Key::Named(Named::Escape),
        modified_key: Key::Named(Named::Escape),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::default(),
        text: None,
        repeat: false,
    });
    for cancel in [
        escape,
        canvas::Event::Window(iced::window::Event::Unfocused),
        canvas::Event::Mouse(mouse::Event::CursorLeft),
    ] {
        let mut state = State::default();
        send(&lane, &mut state, &press(), Some(grab()));
        let away = (grab().0, y(0.4));
        send(&lane, &mut state, &motion(away), Some(away));
        send(&lane, &mut state, &cancel, Some(grab()));
        assert!(state.drag.is_none());
        assert!(send(&lane, &mut state, &release(), Some(grab())).is_none());
    }
}

#[test]
fn presses_in_ruler_labels_or_the_right_gutter_start_nothing() {
    let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let mut state = State::default();
    // The label row is the zoom zone: the press starts a zoom and publishes nothing.
    let action = lane
        .update(&mut state, &press(), bounds(), cursor((100.0, 5.0)))
        .unwrap()
        .into_inner()
        .0;
    assert!(action.is_none());
    assert!(
        lane.update(&mut state, &press(), bounds(), cursor((595.0, 200.0)))
            .is_none()
    );
    assert!(state.drag.is_none());
}

#[test]
fn presses_in_the_value_scale_start_nothing_even_beside_a_point_or_in_the_insert_strip() {
    let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let mut state = State::default();
    // The first point sits on the lane's left rule, at time 0; just left of it is the scale.
    for at in [
        (LEFT - 3.0, y(0.2)),
        (LEFT / 2.0, y(0.5)),
        (LEFT / 2.0, 32.0),
    ] {
        assert!(
            send(&lane, &mut state, &press(), Some(at)).is_none(),
            "{at:?}"
        );
        assert!(
            send(&lane, &mut state, &release(), Some(at)).is_none(),
            "{at:?}"
        );
    }
    assert!(state.drag.is_none() && state.range.is_none() && state.paint.is_none());
    // The same press just inside the rule still grabs the point.
    assert!(send(&lane, &mut state, &press(), Some((LEFT + 1.0, y(0.2)))).is_none());
    assert!(state.drag.is_some());
}

#[test]
fn the_value_scale_labels_sit_left_of_the_lane_centred_on_their_levels() {
    let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let plot = lane.plot(Size::new(600.0, 400.0));
    let at = Point::new(LEFT - detail::VALUE_SCALE_INSET, 0.0);
    assert_eq!(
        lane.scale(&plot),
        [
            ("0.00".to_owned(), Point { y: y(0.0), ..at }),
            ("1.00".to_owned(), Point { y: y(1.0), ..at }),
        ]
    );
    // A Discrete argument names each level while they have room.
    let mode = lane_for(&mixed_project(), &argument("mode", "value"));
    assert_eq!(mode.scale(&plot).len(), 4);
    assert!(mode.scale(&plot).iter().all(|(_, p)| p.x < plot.left));
}

#[test]
fn a_point_keeps_its_time_and_value_as_the_canvas_widens() {
    let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    for width in [400.0, 600.0, 1400.0] {
        let size = Size::new(width, 400.0);
        let mapped = lane.lane(size);
        let centre = Point::new(mapped.x(BeatTime(1200)), mapped.y(0.9));
        assert_eq!(mapped.ticks_at(centre.x).round(), 1200.0, "{width}");
        assert!((mapped.value_at_y(centre.y) - 0.9).abs() < 1e-6, "{width}");
        // A click there selects the same point, whatever the width.
        let bounds = Rectangle::new(Point::new(20.0, 30.0), size);
        let at = mouse::Cursor::Available(Point::new(centre.x + 20.0, centre.y + 30.0));
        let mut state = State::default();
        lane.update(&mut state, &press(), bounds, at);
        let released = lane
            .update(&mut state, &release(), bounds, at)
            .and_then(|action| action.into_inner().0);
        let Some(Message::Timeline(Action::SetPoints(_, points, select))) = released else {
            panic!("a click on the point at width {width} commits SetPoints");
        };
        assert_eq!(
            (list(&points), select),
            (
                vec![(0, 0.2), (1200, 0.9), (2400, 0.3), (3840, 0.6)],
                Some(1)
            ),
            "{width}"
        );
    }
}

#[test]
fn hovering_a_line_reads_its_value_and_a_still_pointer_publishes_nothing_more() {
    let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let mut state = State::default();
    let on_point = grab();
    let first = send(&lane, &mut state, &motion(on_point), Some(on_point));
    assert!(matches!(first, Some(Action::Hover(Some(text))) if text == "0.90"));
    assert!(send(&lane, &mut state, &motion(on_point), Some(on_point)).is_none());
    // Leaving the line clears it.
    let away = (x(600.0), y(0.95));
    assert!(matches!(
        send(&lane, &mut state, &motion(away), Some(away)),
        Some(Action::Hover(None))
    ));
}

#[test]
fn ghosts_are_the_other_lanes_and_take_no_clicks() {
    let project = mixed_project();
    let lane = lane_for(&project, &argument("opacity", "value"));
    // The model holds every envelope; the ghost pass skips the chosen one by target.
    let others = lane
        .model
        .rows
        .iter()
        .filter(|r| {
            matches!(&r.kind, RowKind::Envelope(e)
                if e.target != lane.chosen.target && !e.points.is_empty())
        })
        .count();
    assert_eq!(others, 4); // Steps, Strobe, Mode and Mixer Volume; Position y is empty.
    // A press on a ghost's point (Steps' 6 at 1920) is a press on empty lane space for this
    // lane: it selects time on Opacity, never moves the ghost.
    let at = (x(1920.0), y(0.75));
    let mut state = State::default();
    send(&lane, &mut state, &press(), Some(at));
    let Some(Action::Content(selection)) = send(&lane, &mut state, &release(), Some(at)) else {
        panic!("the press selects the chosen lane");
    };
    assert_eq!(
        selection.lanes,
        vec![lanes::Identity::Envelope(argument("opacity", "value"))]
    );
    assert!(selection.range.is_empty());
}

#[test]
fn the_static_layer_depends_on_the_lane_grid_and_overrides_but_not_the_playhead() {
    let project = mixed_project();
    let mut lane = lane_for(&project, &argument("opacity", "value"));
    let key = lane.static_key();
    lane.playhead = Some(BeatTime(960));
    assert_eq!(lane.static_key(), key);
    lane.live[6] = Some(Effective {
        value: 0.4,
        state: crate::parameter_gesture::AutomationState::Overridden,
    });
    assert_ne!(lane.static_key(), key);
    let other = lane_for(&project, &argument("steps", "value"));
    assert_ne!(other.static_key(), key);
    let mut grid = lane_for(&project, &argument("opacity", "value"));
    grid.grid = Grid::Quarter;
    assert_ne!(grid.static_key(), key);
}

#[test]
fn background_drag_selects_only_the_chosen_lane_with_command_off_grid() {
    let mut lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    lane.context.command_down = true;
    let mut state = State::default();
    let at = (x(1001.0), y(0.01));
    assert!(
        matches!(send(&lane,&mut state,&press(),Some(at)),Some(Action::Content(s)) if s.range == (BeatTime(1001)..BeatTime(1001)))
    );
    let end = (x(2017.0), y(0.99));
    assert!(
        matches!(send(&lane,&mut state,&motion(end),Some(end)),Some(Action::Content(s)) if s.range == (BeatTime(1001)..BeatTime(2017)) && s.lanes == vec![lanes::Identity::Envelope(lane.chosen.target.clone())])
    );
    assert!(matches!(
        send(&lane, &mut state, &release(), Some(end)),
        Some(Action::Content(_))
    ));
}

#[test]
fn a_point_in_a_time_selection_routes_a_region_move_instead_of_set_points_even_out_and_back() {
    let mut lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let mut selection = content::Selection::time(
        lane.key,
        BeatTime(960)..BeatTime(2880),
        vec![lanes::Identity::Envelope(lane.chosen.target.clone())],
    );
    selection.resolve(&lane.clip);
    lane.context.content = Some(selection);
    lane.context.alt_down = true;
    let mut state = State::default();
    assert!(send(&lane, &mut state, &press(), Some(grab())).is_none());
    let end = (grab().0 + 30.0, grab().1 + 15.0);
    assert!(send(&lane, &mut state, &motion(end), Some(end)).is_none());
    send(&lane, &mut state, &motion(grab()), Some(grab()));
    assert!(
        matches!(send(&lane,&mut state,&release(),Some(grab())),Some(Action::RegionMove(m)) if m.copy && m.moved)
    );
    assert!(state.drag.is_none() && state.region.is_none());
}

#[test]
fn near_line_drag_moves_the_region_and_shift_point_click_picks_without_deleting() {
    let mut lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let target = lanes::Identity::Envelope(lane.chosen.target.clone());
    lane.context.content = Some(content::Selection::time(
        lane.key,
        BeatTime(240)..BeatTime(1100),
        vec![target],
    ));
    let geom = lane.lane(bounds().size());
    let time = BeatTime(720);
    let at = (
        geom.x(time),
        geom.y(geom.line_at(time.ticks() as f64).unwrap()) + 3.0,
    );
    let mut state = State::default();
    send(&lane, &mut state, &press(), Some(at));
    let end = (at.0 + 30.0, at.1 + 20.0);
    send(&lane, &mut state, &motion(end), Some(end));
    assert!(matches!(
        send(&lane, &mut state, &release(), Some(end)),
        Some(Action::RegionMove(_))
    ));
    lane.context.shift_down = true;
    assert!(matches!(
        send(&lane, &mut state, &press(), Some(grab())),
        Some(Action::PickPoint(_, 1))
    ));
    assert!(send(&lane, &mut state, &release(), Some(grab())).is_none());
}

#[test]
fn a_background_drag_is_cancelled_by_revision_change_or_focus_loss() {
    let mut lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let mut state = State::default();
    let at = (x(960.0), y(0.01));
    send(&lane, &mut state, &press(), Some(at));
    assert!(state.range.is_some());
    lane.context.revision += 1;
    assert!(send(&lane, &mut state, &release(), Some(at)).is_none());
    assert!(state.range.is_none());
    send(&lane, &mut state, &press(), Some(at));
    send(
        &lane,
        &mut state,
        &canvas::Event::Window(iced::window::Event::Unfocused),
        None,
    );
    assert!(state.range.is_none());
    assert!(send(&lane, &mut state, &release(), Some(at)).is_none());
}

#[test]
fn draw_mode_paints_steps_and_commits_one_replacement_on_release() {
    let project = mixed_project();
    let target = argument("brightness", "value");
    let mut lane = lane_for(&project, &target);
    lane.context.draw = true;
    let mut state = State::default();
    let size = bounds().size();
    let (low, high) = (0.25, 0.75);
    let down = send(&lane, &mut state, &press(), Some((x(10.0), y(low))));
    assert!(
        down.is_none(),
        "a press commits nothing and selects nothing"
    );
    send(
        &lane,
        &mut state,
        &motion((x(300.0), y(high))),
        Some((x(300.0), y(high))),
    );
    let Some(Action::Draw(Draw::Points {
        replacement,
        lane: drawn,
    })) = send(&lane, &mut state, &release(), Some((x(300.0), y(high))))
    else {
        panic!("one stroke on release");
    };
    assert_eq!(drawn.target, target);
    // Cells 0 and 1 of a sixteenth (240 ticks) each, at their own pointer heights.
    assert_eq!(replacement.range, BeatTime(0)..BeatTime(481));
    assert_eq!(replacement.points.len(), 4);
    let first = lane.lane(size).value_at_y(y(low));
    assert!((replacement.points[0].value - first).abs() < 1e-9);
    assert_eq!(replacement.points[1].time, BeatTime(240));
    assert_eq!(replacement.points[2].time, BeatTime(240));
}

#[test]
fn insert_strip_uses_the_chosen_lane_and_grid_without_editing_points() {
    let mut lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let mut state = State::default();
    let y = crate::style::detail::INSERT_STRIP_TOP + 3.0;
    let Some(Action::Content(selection)) = send(&lane, &mut state, &press(), Some((x(333.0), y)))
    else {
        panic!("strip must place content")
    };
    assert_eq!(selection.insert_mark(), Some(BeatTime(240)));
    assert_eq!(
        selection.lanes,
        vec![lanes::Identity::Envelope(lane.chosen.target.clone())]
    );
    lane.context.command_down = true;
    let Some(Action::Content(selection)) = send(&lane, &mut state, &press(), Some((x(333.0), y)))
    else {
        panic!("strip must place content")
    };
    assert_eq!(selection.insert_mark(), Some(BeatTime(333)));
    assert!(state.drag.is_none());
}

#[test]
fn loop_brace_handles_work_in_one_lane_with_draw_mode_and_command_precision() {
    let project = mixed_project();
    for draw in [false, true] {
        let mut view = lane_for(&project, &argument("opacity", "value"));
        view.context.draw = draw;
        view.context.command_down = true;
        let mut state = State::default();
        let at = (x(3840.0), 24.0);
        view.update(&mut state, &press(), bounds(), cursor(at));
        assert!(state.marker.is_some());
        assert!(state.paint.is_none());
        let to = (x(3001.0), 24.0);
        view.update(&mut state, &motion(to), bounds(), cursor(to));
        let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        let action = view
            .update(&mut state, &release, bounds(), cursor(to))
            .unwrap();
        let Some(Message::Timeline(Action::Markers(_, _, markers))) = action.into_inner().0 else {
            panic!("marker release")
        };
        assert_eq!(markers.loop_end, BeatTime(3001));
        assert_eq!(markers.end, BeatTime(3840));
    }
}

#[test]
fn unlinked_ruler_edits_lane_markers_and_cancels_stale_gestures() {
    let project = mixed_project();
    let mut view = lane_for(&project, &argument("opacity", "value"));
    let mut chosen = (*view.chosen).clone();
    chosen.region = Some(qtable_engine::model::region::Region {
        start: BeatTime(240),
        end: BeatTime(2880),
        loop_start: BeatTime(480),
        loop_end: BeatTime(1920),
        looping: true,
    });
    view.chosen = chosen.into();
    view.context.draw = true;
    let mut state = State::default();
    for (from, to) in [
        ((x(240.0), 14.0), (x(720.0), 14.0)),
        ((x(1920.0), 24.0), (x(2400.0), 24.0)),
    ] {
        send(&view, &mut state, &press(), Some(from));
        assert!(state.marker.is_some());
        assert!(state.paint.is_none());
        send(&view, &mut state, &motion(to), Some(to));
        let Some(Action::LaneRegion(target, Some(region))) =
            send(&view, &mut state, &release(), Some(to))
        else {
            panic!("lane marker edit");
        };
        assert_eq!(target.target, view.chosen.target);
        assert_eq!(region.end, BeatTime(2880));
        assert!(region.start == BeatTime(720) || region.loop_end == BeatTime(2400));
    }
    send(&view, &mut state, &press(), Some((x(1920.0), 24.0)));
    view.context.revision += 1;
    assert!(send(&view, &mut state, &release(), Some((x(2400.0), 24.0))).is_none());
    assert!(state.marker.is_none());
}

#[test]
fn the_info_view_follows_the_pointer_over_the_brace_flags_and_line() {
    use crate::style::detail::{CLIP_BRACE_TOP, CLIP_FLAG_BOTTOM, CLIP_FLAG_TOP, INSERT_STRIP_TOP};
    let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let size = Size::new(600.0, 400.0);
    let axis = lane.axis(&lane.plot(size));
    let (flag, brace) = (
        (CLIP_FLAG_TOP + CLIP_FLAG_BOTTOM) / 2.0,
        (CLIP_BRACE_TOP + INSERT_STRIP_TOP) / 2.0,
    );
    let r = axis.markers;
    let middle = (axis.x(r.loop_start) + axis.x(r.loop_end)) / 2.0;
    for ((x, y), id) in [
        ((axis.x(r.start) + 2.0, flag), Id::StartMarker),
        ((axis.x(r.end) - 2.0, flag), Id::EndMarker),
        ((axis.x(r.loop_start) + 2.0, brace), Id::LoopStart),
        ((axis.x(r.loop_end) - 2.0, brace), Id::LoopEnd),
        ((middle, brace), Id::LoopBrace),
        (grab(), Id::EnvelopeLine),
        ((middle, 395.0), Id::OneLane),
        ((middle, 2.0), Id::RulerZoom),
    ] {
        assert_eq!(lane.info_at(Point::new(x, y), size), id, "{x}, {y}");
    }
}

mod zoom {
    use super::*;
    use crate::control::time_axis::Window;

    fn wheel(x: f32, y: f32) -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x, y },
        })
    }
    fn zoomed_lane(start: f64, visible: f64) -> OneLane {
        let mut lane = lane_for(&mixed_project(), &argument("opacity", "value"));
        lane.context.window = Some(Window { start, visible });
        lane
    }
    fn width() -> f64 {
        f64::from(590.0 - LEFT)
    }
    fn window_of(action: Option<Action>) -> Option<Window> {
        match action {
            Some(Action::Zoom(_, window)) => window,
            other => panic!("expected a zoom, got {other:?}"),
        }
    }
    /// The canvas x of `ticks` on the lane as it is zoomed now.
    fn at_tick(lane: &OneLane, ticks: u64) -> f32 {
        lane.lane(bounds().size()).x(BeatTime(ticks))
    }

    #[test]
    fn dragging_down_the_ruler_zooms_in_about_the_pressed_time() {
        let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
        let mut state = State::default();
        let from = (x(1000.0), 10.0);
        assert!(send(&lane, &mut state, &press(), Some(from)).is_none());
        assert!(state.zoom.dragging());
        let down = (from.0, 60.0);
        let window = window_of(send(&lane, &mut state, &motion(down), Some(down)));
        let w = window.expect("zoomed in");
        assert!((w.visible - 1920.0).abs() < 1e-6, "{w:?}");
        // The time that was under the pointer is still under it.
        let under = w.start + (f64::from(from.0 - LEFT) / width()) * w.visible;
        assert!((under - 1000.0).abs() < 1e-3, "{under}");
        send(&lane, &mut state, &release(), Some(down));
        assert!(!state.zoom.dragging());
    }

    #[test]
    fn a_right_press_during_a_ruler_drag_ends_it_where_it_is_and_opens_no_menu() {
        let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
        let mut state = State::default();
        let from = (x(1000.0), 10.0);
        send(&lane, &mut state, &press(), Some(from));
        // Dragging down puts the pointer over lane space, where a right-press opens a menu.
        let down = (from.0, y(0.05));
        let w = window_of(send(&lane, &mut state, &motion(down), Some(down)));
        assert!(w.is_some() && state.zoom.dragging());
        let pressed = send(&lane, &mut state, &right(), Some(down));
        assert!(pressed.is_none(), "no menu, no restore: {pressed:?}");
        assert!(!state.zoom.dragging(), "the drag has ended");
        // The window stays as the last move left it: the release changes nothing.
        let released = send(&lane, &mut state, &release(), Some(down));
        assert!(!matches!(released, Some(Action::Zoom(..))), "{released:?}");
        // A right-press with no drag in progress opens its menu as before.
        let again = send(&lane, &mut state, &right(), Some(down));
        assert!(matches!(again, Some(Action::ContentMenu(_))), "{again:?}");
    }

    #[test]
    fn dragging_sideways_scrolls_and_a_flag_press_keeps_its_marker_drag() {
        let lane = zoomed_lane(960.0, 1920.0);
        let mut state = State::default();
        let from = (x(1000.0), 10.0);
        send(&lane, &mut state, &press(), Some(from));
        let left = (from.0 - width() as f32 / 8.0, 10.0);
        let w = window_of(send(&lane, &mut state, &motion(left), Some(left))).unwrap();
        // An eighth of the width is an eighth of the window: 240 ticks, later.
        assert!((w.visible - 1920.0).abs() < 1e-6);
        assert!((w.start - 1200.0).abs() < 1e-3, "{w:?}");
        // The Start flag (tick 0 is off the left edge, so use the End flag on a window
        // that shows tick 3840).
        let lane = zoomed_lane(1920.0, 1920.0);
        let mut state = State::default();
        let axis = lane.axis(&lane.plot(bounds().size()));
        let flag = (axis.x(axis.markers.end) - 2.0, 15.0);
        let grabbed = send(&lane, &mut state, &press(), Some(flag));
        assert!(matches!(grabbed, Some(Action::Focus(_))), "{grabbed:?}");
        assert!(state.marker.is_some() && !state.zoom.dragging());
    }

    #[test]
    fn a_point_drag_the_insert_marker_and_a_time_selection_land_on_the_zoomed_times() {
        // 1920 ticks from tick 1920: the 2400 point sits a quarter of the way across.
        let lane = zoomed_lane(1920.0, 1920.0);
        let size = bounds().size();
        let wlane = lane.lane(size);
        let eighth = width() as f32 / 8.0; // 240 ticks
        // Dragging the 2400 point right by an eighth of the width moves it to 2640.
        let point = (at_tick(&lane, 2400), y(0.3));
        let mut state = State::default();
        send(&lane, &mut state, &press(), Some(point));
        let to = (point.0 + eighth, point.1);
        send(&lane, &mut state, &motion(to), Some(to));
        let Some(Action::SetPoints(_, points, _)) = send(&lane, &mut state, &release(), Some(to))
        else {
            panic!("the drag commits");
        };
        assert_eq!(points[2].time, BeatTime(2640));
        // The insert strip puts the marker on the zoomed time under the pointer.
        let mut state = State::default();
        let strip = (wlane.x(BeatTime(2880)), 33.0);
        let marker = send(&lane, &mut state, &press(), Some(strip));
        assert!(
            matches!(marker, Some(Action::Content(ref s)) if s.range == (BeatTime(2880)..BeatTime(2880))),
            "{marker:?}"
        );
        // A background drag selects between the zoomed times under its two ends.
        let empty_y = y(0.95);
        let begin = (at_tick(&lane, 2000), empty_y);
        let end = (at_tick(&lane, 3120), empty_y);
        let mut state = State::default();
        send(&lane, &mut state, &press(), Some(begin));
        let dragged = send(&lane, &mut state, &motion(end), Some(end));
        assert!(
            matches!(dragged, Some(Action::Content(ref s)) if s.range == (BeatTime(1920)..BeatTime(3120))),
            "{dragged:?}"
        );
    }

    #[test]
    fn a_right_click_on_lane_space_places_its_marker_on_the_zoomed_time() {
        let lane = zoomed_lane(1920.0, 1920.0);
        let mut state = State::default();
        let on = (at_tick(&lane, 2880), y(0.05));
        let Some(Action::ContentMenu(click)) = send(&lane, &mut state, &right(), Some(on)) else {
            panic!("a content menu click");
        };
        assert_eq!(click.under, menu::Under::Space(BeatTime(2880)));
    }

    #[test]
    fn a_press_on_the_brace_row_above_the_value_scale_grabs_nothing_when_zoomed() {
        // The loop (0..3840) starts left of the window, so its body spans the value scale's x.
        let lane = zoomed_lane(960.0, 1920.0);
        let mut state = State::default();
        let above_scale = (LEFT / 2.0, 24.0);
        let published = send(&lane, &mut state, &press(), Some(above_scale));
        assert!(
            state.marker.is_none(),
            "no brace drag started: {published:?}"
        );
        let idle = State::default();
        assert_ne!(
            lane.mouse_interaction(&idle, bounds(), cursor(above_scale)),
            mouse::Interaction::Move
        );
        // The brace inside the plot still moves the loop.
        let inside = send(&lane, &mut state, &press(), Some((LEFT + 100.0, 24.0)));
        assert!(matches!(inside, Some(Action::Focus(_))), "{inside:?}");
        assert!(state.marker.is_some());
    }

    #[test]
    fn a_point_just_left_of_the_window_cannot_be_grabbed_by_its_sliver() {
        // The 1200 point sits about 2 px left of the plot: its dot is clipped to a sliver.
        let lane = zoomed_lane(1210.0, 1920.0);
        let wlane = lane.lane(bounds().size());
        assert!(wlane.x(BeatTime(1200)) < LEFT);
        assert!(wlane.x(BeatTime(1200)) > LEFT - 4.0);
        // The line through it can still be grabbed, but the point itself cannot.
        let hit = wlane.hit(Point::new(LEFT + 1.0, wlane.y(0.9)));
        assert!(!matches!(hit, Hit::Point(_)), "{hit:?}");
        // The same press with the point inside the window grabs it.
        let inside = zoomed_lane(1190.0, 1920.0);
        let w = inside.lane(bounds().size());
        let at = Point::new(w.x(BeatTime(1200)), w.y(0.9));
        assert_eq!(w.hit(at), Hit::Point(1));
    }

    #[test]
    fn the_wheels_zoom_and_scroll_the_window_and_a_plain_scroll_passes_through() {
        let lane = zoomed_lane(960.0, 1920.0);
        let mut state = State::default();
        let over = (x(2000.0), y(0.5));
        assert!(
            lane.update(&mut state, &wheel(0.0, 1.0), bounds(), cursor(over))
                .is_none(),
            "a plain vertical scroll is not captured"
        );
        let scrolled = window_of(send(&lane, &mut state, &wheel(1.0, 0.0), Some(over))).unwrap();
        assert!(scrolled.start < 960.0);
        let mut zoom = lane.clone();
        zoom.context.command_down = true;
        let before = 960.0 + f64::from(over.0 - LEFT) / width() * 1920.0;
        let w = window_of(send(&zoom, &mut state, &wheel(0.0, 1.0), Some(over))).unwrap();
        assert!(w.visible < 1920.0);
        let after = w.start + f64::from(over.0 - LEFT) / width() * w.visible;
        assert!((after - before).abs() < 1e-3, "{before} vs {after}");
        // During a gesture the wheel is ignored.
        let mut state = State::default();
        send(&zoom, &mut state, &press(), Some((x(1200.0), y(0.9))));
        assert!(
            zoom.update(&mut state, &wheel(0.0, 1.0), bounds(), cursor(over))
                .is_none()
        );
        // And a fitted lane has nowhere to scroll.
        let fitted = lane_for(&mixed_project(), &argument("opacity", "value"));
        let mut idle = State::default();
        assert!(
            fitted
                .update(&mut idle, &wheel(1.0, 0.0), bounds(), cursor(over))
                .is_none()
        );
    }

    #[test]
    fn a_double_click_asks_for_zoom_to_and_the_pointer_is_a_magnifier_in_the_zone() {
        let clock = crate::gesture_clock::test::Clock::new();
        let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
        let mut state = State::default();
        let at = (x(1000.0), 10.0);
        send(&lane, &mut state, &press(), Some(at));
        send(&lane, &mut state, &release(), Some(at));
        clock.advance(std::time::Duration::from_millis(150));
        assert!(matches!(
            send(&lane, &mut state, &press(), Some(at)),
            Some(Action::ZoomTo(_))
        ));
        let idle = State::default();
        let over = |at: (f32, f32)| lane.mouse_interaction(&idle, bounds(), cursor(at));
        assert_eq!(over(at), mouse::Interaction::ZoomIn);
        assert_ne!(
            over((x(1000.0), 24.0)),
            mouse::Interaction::ZoomIn,
            "the brace row"
        );
        assert_ne!(
            over((x(1000.0), 200.0)),
            mouse::Interaction::ZoomIn,
            "the lane"
        );
    }
}

fn right() -> canvas::Event {
    canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
}

#[test]
fn a_right_click_on_a_point_or_lane_space_publishes_what_was_clicked_and_snaps_as_a_left_click() {
    let mut lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let mut state = State::default();
    let click = |action| match action {
        Some(Action::ContentMenu(click)) => click,
        other => panic!("a content menu click, got {other:?}"),
    };
    let on_point = click(send(&lane, &mut state, &right(), Some(grab())));
    assert_eq!(on_point.under, menu::Under::Point(1));
    assert_eq!(
        on_point.lane,
        lanes::Identity::Envelope(argument("opacity", "value"))
    );
    // Empty space: the time a left press on the same spot would put its insert marker at.
    let empty = (x(1800.0), y(0.1));
    let on_space = click(send(&lane, &mut state, &right(), Some(empty)));
    let time = lane
        .lane(Size::new(600.0, 400.0))
        .time_at(empty.0, lane.keys());
    assert_eq!(on_space.under, menu::Under::Space(time));
    // ⌘ leaves the time off the grid, as it does for a left click.
    lane.context.command_down = true;
    let off = click(send(&lane, &mut state, &right(), Some((x(1801.0), y(0.1)))));
    let exact = lane
        .lane(Size::new(600.0, 400.0))
        .time_at(x(1801.0), lane.keys());
    assert_eq!(off.under, menu::Under::Space(exact));
    assert_ne!(exact, time);
    // Draw Mode changes the left press only: a right-click still asks for a menu.
    lane.context.command_down = false;
    lane.context.draw = true;
    assert!(matches!(
        send(&lane, &mut state, &right(), Some(grab())),
        Some(Action::ContentMenu(_))
    ));
}

#[test]
fn a_right_click_on_the_ruler_the_value_scale_or_the_gutter_opens_nothing_and_cancels_a_drag() {
    let lane = lane_for(&mixed_project(), &argument("opacity", "value"));
    let mut state = State::default();
    for at in [
        (100.0, 5.0),
        (LEFT / 2.0, y(0.5)),
        (LEFT - 3.0, y(0.2)),
        (595.0, 200.0),
    ] {
        assert!(
            send(&lane, &mut state, &right(), Some(at)).is_none(),
            "{at:?}"
        );
    }
    // A right press in the middle of a point gesture drops the gesture.
    send(&lane, &mut state, &press(), Some(grab()));
    assert!(state.drag.is_some());
    assert!(send(&lane, &mut state, &right(), Some((LEFT / 2.0, y(0.5)))).is_none());
    assert!(state.drag.is_none());
}
