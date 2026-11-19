use super::*;
use canvas::Program;
use qtable_engine::model::{ClipId, DeviceId, OscAddress, Track, TrackId};
fn setup() -> Timeline {
    let mut clip = Clip::new(
        ClipId(1),
        "test",
        BeatTime(3840),
        qtable_engine::model::LIVE_CLIP_PALETTE[0],
    )
    .unwrap();
    let mut track = Track::new(TrackId(1), "t", 0);
    let x = OscAddress::Relative("/x".into());
    clip.events
        .push(track.hit(BeatTime(960), x, vec![], || DeviceId(1)));
    Timeline {
        playhead: None,
        lane_playheads: vec![],
        context: Context::default(),
        model: LaneModel::of_events(&clip.lane_refs()).into(),
        live: vec![None],
        envelope: None,
        point: None,
        clip: clip.into(),
        key: ClipKey {
            track: TrackId(1),
            slot: 0,
            id: ClipId(1),
            epoch: 0,
        },
        lane: None,
        selected: None,
        grid: Grid::Sixteenth,
        signature: TimeSignature::default(),
        part: Part::Lanes,
    }
}
fn bounds() -> Rectangle {
    Rectangle::new(Point::new(20.0, 30.0), Size::new(558.0, 134.0))
}
/// The pinned ruler header over the same clip: the brace, flags, zoom and insert strip.
fn headed(mut timeline: Timeline) -> Timeline {
    timeline.part = Part::Header;
    timeline
}
fn press() -> canvas::Event {
    canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
}
fn cursor(x: f32, y: f32) -> mouse::Cursor {
    mouse::Cursor::Available(Point::new(x + 20.0, y + 30.0))
}

#[test]
fn events_filter_keeps_hit_targets_and_maps_headers_back_to_mixed_rows() {
    let mut timeline = envelope_lanes::mixed(false);
    // Stored lanes do not change the device-generated row order.
    let clip = std::rc::Rc::make_mut(&mut timeline.clip);
    clip.lanes.rotate_left(1);
    let project = crate::inspector::lane_model::tests::mixed_project();
    timeline.model = LaneModel::build(&project, TrackId(1), &timeline.clip)
        .events()
        .into();
    timeline.live = vec![None];
    timeline.context.events_only = true;
    let layout = timeline.layout(bounds());
    assert_eq!(timeline.model.rows.len(), 1);
    assert_eq!(timeline.model.rows[0].top, 0.0);
    let mut state = State::default();
    let hit = cursor(layout.x(BeatTime(960)), RULER_HEIGHT + 12.0);
    assert!(matches!(
        timeline
            .update(&mut state, &press(), bounds(), hit)
            .unwrap()
            .into_inner()
            .0,
        Some(Message::Timeline(Action::SelectAt(HitTarget {
            index: 0,
            ..
        })))
    ));
    assert!(matches!(
        timeline
            .update(&mut state, &press(), bounds(), cursor(50.0, 50.0))
            .unwrap()
            .into_inner()
            .0,
        Some(Message::Timeline(Action::Header(_, 0)))
    ));
    assert!(state.header_drag.is_none());
    let right = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
    assert!(matches!(
        timeline
            .update(&mut state, &right, bounds(), cursor(50.0, 50.0))
            .unwrap()
            .into_inner()
            .0,
        Some(Message::Timeline(Action::LaneMenu(lanes::Target {
            row: 0,
            ..
        })))
    ));
    timeline.context.lane_range = Some((0, 1));
    assert!(timeline.chosen(0));
    assert_eq!(
        timeline.axis(&layout, style::TEXT).markers,
        timeline.clip.region()
    );
}

#[test]
fn changing_between_overview_and_events_cancels_pending_canvas_gestures() {
    let mut timeline = setup();
    let mut state = State::default();
    timeline.update(&mut state, &press(), bounds(), cursor(252.0, 50.0));
    assert!(state.drag.is_some());
    timeline.context.events_only = true;
    assert!(
        timeline.state_for_view(&state).is_none(),
        "the first redraw must ignore old gestures"
    );
    assert!(
        timeline
            .update(&mut state, &release(), bounds(), cursor(322.0, 50.0))
            .is_none()
    );
    assert!(state.drag.is_none());
}

#[test]
fn events_ranges_cross_only_event_rows_and_hits_keep_their_clip_indices() {
    let mut project = crate::inspector::lane_model::tests::mixed_project();
    let extra = project
        .hit(
            TrackId(1),
            BeatTime(1920),
            OscAddress::Relative("/snare".into()),
            vec![],
        )
        .unwrap();
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    clip.events.push(extra);
    let clip = project.clip(TrackId(1), 0).unwrap();
    let mut timeline = envelope_lanes::mixed(false);
    timeline.clip = clip.clone().into();
    timeline.model = LaneModel::build(&project, TrackId(1), clip).events().into();
    timeline.live = vec![None, None];
    timeline.context.events_only = true;
    let layout = timeline.layout(bounds());
    let mut state = State::default();
    let from = (layout.x(BeatTime(480)), RULER_HEIGHT + 12.0);
    let to = (layout.x(BeatTime(2880)), RULER_HEIGHT + 36.0);
    timeline.update(&mut state, &press(), bounds(), cursor(from.0, from.1));
    let Some(Message::Timeline(Action::Content(selection))) = timeline
        .update(
            &mut state,
            &motion(to.0, to.1),
            bounds(),
            cursor(to.0, to.1),
        )
        .unwrap()
        .into_inner()
        .0
    else {
        panic!("time selection");
    };
    assert_eq!(selection.lanes.len(), 2);
    assert!(
        selection
            .lanes
            .iter()
            .all(|id| matches!(id, lanes::Identity::Event(_)))
    );
    assert_eq!(selection.range, BeatTime(480)..BeatTime(2880));
    timeline.update(&mut state, &release(), bounds(), cursor(to.0, to.1));
    assert!(matches!(
        timeline
            .update(
                &mut state,
                &press(),
                bounds(),
                cursor(layout.x(BeatTime(1920)), RULER_HEIGHT + 36.0)
            )
            .unwrap()
            .into_inner()
            .0,
        Some(Message::Timeline(Action::SelectAt(HitTarget {
            index: 3,
            ..
        })))
    ));
}
#[test]
fn marker_selection_drag_preview_and_release_publish_once_with_grab_offset() {
    let timeline = setup();
    let mut state = State::default();
    let action = timeline
        .update(&mut state, &press(), bounds(), cursor(252.0, 50.0))
        .unwrap();
    assert!(matches!(
        action.into_inner().0,
        Some(Message::Timeline(Action::SelectAt(HitTarget {
            index: 0,
            ..
        })))
    ));
    let moved = canvas::Event::Mouse(mouse::Event::CursorMoved {
        position: Point::new(322.0, 80.0),
    });
    assert!(
        timeline
            .update(&mut state, &moved, bounds(), mouse::Cursor::Unavailable)
            .unwrap()
            .into_inner()
            .0
            .is_none()
    );
    assert_eq!(state.drag.unwrap().preview, BeatTime(1440));
    let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    assert!(matches!(
        timeline
            .update(&mut state, &release, bounds(), mouse::Cursor::Unavailable)
            .unwrap()
            .into_inner()
            .0,
        Some(Message::Timeline(Action::Move(
            HitTarget { index: 0, .. },
            BeatTime(1440)
        )))
    ));
    assert!(state.drag.is_none());
    assert!(
        timeline
            .update(&mut state, &release, bounds(), mouse::Cursor::Unavailable)
            .is_none()
    );
}
#[test]
fn double_click_empty_lane_adds_snapped_hit_and_lane_headers_select_the_lane() {
    let clock = crate::gesture_clock::test::Clock::new();
    let timeline = setup();
    let mut state = State::default();
    timeline
        .update(&mut state, &press(), bounds(), cursor(301.0, 50.0))
        .unwrap();
    clock.advance(std::time::Duration::from_millis(100));
    let result = timeline
        .update(&mut state, &press(), bounds(), cursor(301.0, 50.0))
        .unwrap()
        .into_inner()
        .0;
    assert!(matches!(
        result,
        Some(Message::Timeline(Action::Add(_, BeatTime(1440), _)))
    ));
    for _ in 0..2 {
        let result = timeline
            .update(&mut state, &press(), bounds(), cursor(50.0, 50.0))
            .unwrap()
            .into_inner()
            .0;
        assert!(matches!(
            result,
            Some(Message::Timeline(Action::Header(_, 0)))
        ));
    }
}
#[test]
fn tiny_click_does_not_snap_existing_off_grid_event_and_unfocus_cancels_drag() {
    let mut timeline = setup();
    std::rc::Rc::make_mut(&mut timeline.clip).events[0].position = BeatTime(1000);
    let mut state = State::default();
    let x = timeline.layout(bounds()).x(BeatTime(1000));
    timeline
        .update(&mut state, &press(), bounds(), cursor(x, 50.0))
        .unwrap();
    let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    assert!(
        timeline
            .update(&mut state, &release, bounds(), mouse::Cursor::Unavailable)
            .unwrap()
            .into_inner()
            .0
            .is_none()
    );
    timeline
        .update(&mut state, &press(), bounds(), cursor(x, 50.0))
        .unwrap();
    timeline
        .update(
            &mut state,
            &canvas::Event::Window(iced::window::Event::Unfocused),
            bounds(),
            mouse::Cursor::Unavailable,
        )
        .unwrap();
    assert!(state.drag.is_none());
}
#[test]
fn changing_clip_identity_cancels_pending_click_and_drag() {
    let mut timeline = setup();
    let mut state = State::default();
    timeline
        .update(&mut state, &press(), bounds(), cursor(248.0, 50.0))
        .unwrap();
    timeline.key.id = ClipId(2);
    let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    assert!(
        timeline
            .update(&mut state, &release, bounds(), mouse::Cursor::Unavailable)
            .is_none()
    );
    assert!(state.drag.is_none());
}

fn release() -> canvas::Event {
    canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
}
#[test]
fn alt_drag_keeps_original_and_commits_one_copy_at_release_position() {
    let mut timeline = setup();
    timeline.context.alt_down = true;
    let mut state = State::default();
    timeline
        .update(&mut state, &press(), bounds(), cursor(248., 50.))
        .unwrap();
    assert!(state.drag.unwrap().copy);
    let moved = canvas::Event::Mouse(mouse::Event::CursorMoved {
        position: Point::new(322., 80.),
    });
    assert!(
        timeline
            .update(&mut state, &moved, bounds(), mouse::Cursor::Unavailable)
            .unwrap()
            .into_inner()
            .0
            .is_none()
    );
    assert_eq!(timeline.clip.events[0].position, BeatTime(960));
    let action = timeline
        .update(&mut state, &release(), bounds(), cursor(348., 50.))
        .unwrap()
        .into_inner()
        .0;
    assert!(matches!(
        action,
        Some(Message::Timeline(Action::Duplicate(
            HitTarget { index: 0, .. },
            BeatTime(1920)
        )))
    ));
    assert!(
        timeline
            .update(&mut state, &release(), bounds(), cursor(348., 50.))
            .is_none()
    );
}
#[test]
fn alt_click_is_no_copy_but_drag_snapped_back_to_origin_is_a_copy() {
    let mut timeline = setup();
    timeline.context.alt_down = true;
    let mut state = State::default();
    timeline
        .update(&mut state, &press(), bounds(), cursor(248., 50.))
        .unwrap();
    assert!(
        timeline
            .update(&mut state, &release(), bounds(), cursor(249., 50.))
            .unwrap()
            .into_inner()
            .0
            .is_none()
    );
    timeline
        .update(&mut state, &press(), bounds(), cursor(248., 50.))
        .unwrap();
    let action = timeline
        .update(&mut state, &release(), bounds(), cursor(252., 50.))
        .unwrap()
        .into_inner()
        .0;
    assert!(matches!(
        action,
        Some(Message::Timeline(Action::Duplicate(_, BeatTime(960))))
    ));
}
#[test]
fn ruler_and_empty_lane_clicks_set_snapped_cursor_and_right_clicks_open_content_menus() {
    let timeline = setup();
    let header = headed(setup());
    let mut state = State::default();
    // The ruler's bar-number row is the zoom zone: a press there starts a zoom, not a cursor.
    let mut header_state = State::default();
    let action = header
        .update(&mut header_state, &press(), bounds(), cursor(301., 10.))
        .unwrap()
        .into_inner()
        .0;
    assert!(action.is_none());
    assert!(header_state.zoom.dragging());
    header
        .update(&mut header_state, &release(), bounds(), cursor(301., 10.))
        .unwrap();
    let action = timeline
        .update(&mut state, &press(), bounds(), cursor(301., 50.))
        .unwrap()
        .into_inner()
        .0;
    assert!(matches!(
        action,
        Some(Message::Timeline(Action::Content(ref s))) if s.range == (BeatTime(1440)..BeatTime(1440))
    ));
    let right = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
    let action = timeline
        .update(&mut state, &right, bounds(), cursor(248., 50.))
        .unwrap()
        .into_inner()
        .0;
    assert!(matches!(
        action,
        Some(Message::Timeline(Action::ContentMenu(menu::Click {
            under: menu::Under::Hit(0),
            ..
        })))
    ));
    // Empty lane space opens the empty-space menu at the time a left click snaps to.
    let action = timeline
        .update(&mut state, &right, bounds(), cursor(301., 50.))
        .unwrap()
        .into_inner()
        .0;
    assert!(matches!(
        action,
        Some(Message::Timeline(Action::ContentMenu(menu::Click {
            under: menu::Under::Space(BeatTime(1440)),
            ..
        })))
    ));
    // The ruler takes no content menu.
    assert!(
        header
            .update(&mut header_state, &right, bounds(), cursor(301., 10.))
            .is_none()
    );
}
#[test]
fn changed_revision_cancels_a_drag_without_modifying_the_clip() {
    let mut timeline = setup();
    let mut state = State::default();
    timeline
        .update(&mut state, &press(), bounds(), cursor(248., 50.))
        .unwrap();
    timeline.context.revision += 1;
    assert!(
        timeline
            .update(&mut state, &release(), bounds(), cursor(400., 50.))
            .is_none()
    );
    assert!(state.drag.is_none());
    assert_eq!(timeline.clip.events.len(), 1);
}

#[test]
fn stale_drag_never_appears_in_a_different_clip_or_revision() {
    let mut timeline = setup();
    timeline.context.alt_down = true;
    let mut state = State::default();
    timeline
        .update(&mut state, &press(), bounds(), cursor(248., 50.))
        .unwrap();
    assert!(timeline.drag(&state).is_some());
    timeline.key.epoch += 1;
    assert!(timeline.drag(&state).is_none());
    timeline.key.epoch -= 1;
    timeline.context.revision += 1;
    assert!(timeline.drag(&state).is_none());
}
#[test]
fn releasing_beyond_the_clip_clamps_the_copy_to_the_last_grid_position() {
    let mut timeline = setup();
    timeline.context.alt_down = true;
    let mut state = State::default();
    timeline
        .update(&mut state, &press(), bounds(), cursor(248., 50.))
        .unwrap();
    let action = timeline
        .update(&mut state, &release(), bounds(), cursor(1000., 50.))
        .unwrap()
        .into_inner()
        .0;
    assert!(matches!(
        action,
        Some(Message::Timeline(Action::Duplicate(_, BeatTime(3600))))
    ));
}

#[test]
fn escape_cancels_alt_drag_and_release_has_no_edit_action() {
    let mut timeline = setup();
    timeline.context.alt_down = true;
    let mut state = State::default();
    timeline
        .update(&mut state, &press(), bounds(), cursor(248., 50.))
        .unwrap();
    let escape = canvas::Event::Keyboard(iced::keyboard::Event::KeyPressed {
        key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
        physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
        location: iced::keyboard::Location::Standard,
        modifiers: iced::keyboard::Modifiers::ALT,
        text: None,
        repeat: false,
    });
    assert!(matches!(
        timeline
            .update(&mut state, &escape, bounds(), cursor(400., 50.))
            .unwrap()
            .into_inner()
            .0,
        Some(Message::Timeline(Action::ClearContent))
    ));
    assert!(state.drag.is_none());
    assert!(
        timeline
            .update(&mut state, &release(), bounds(), cursor(400., 50.))
            .is_none()
    );
}

mod envelope_lanes {
    use super::*;
    use crate::inspector::lane_model::tests::mixed_project;
    use qtable_engine::model::FieldRef;

    pub(super) fn mixed(overridden: bool) -> Timeline {
        let project = mixed_project();
        let clip = project.clip(TrackId(1), 0).unwrap().clone();
        let model = LaneModel::build(&project, TrackId(1), &clip);
        let live = model
            .rows
            .iter()
            .map(|row| {
                matches!(row.kind, RowKind::Envelope(_)).then_some(Effective {
                    value: 0.5,
                    state: if overridden {
                        crate::parameter_gesture::AutomationState::Overridden
                    } else {
                        crate::parameter_gesture::AutomationState::Following
                    },
                })
            })
            .collect();
        Timeline {
            playhead: None,
            lane_playheads: vec![],
            context: Context::default(),
            model: model.into(),
            live,
            envelope: None,
            point: None,
            clip: clip.into(),
            key: ClipKey {
                track: TrackId(1),
                slot: 0,
                id: ClipId(1),
                epoch: 0,
            },
            lane: None,
            selected: None,
            grid: Grid::Sixteenth,
            signature: TimeSignature::default(),
            part: Part::Lanes,
        }
    }
    fn bounds() -> Rectangle {
        Rectangle::new(Point::new(20.0, 30.0), Size::new(558.0, 400.0))
    }
    /// The action a press at `(x, y)` publishes; rows start at y = 38 (the ruler).
    fn pressed(timeline: &Timeline, state: &mut State, x: f32, y: f32) -> Option<Action> {
        timeline
            .update(state, &press(), bounds(), cursor(x, y))
            .and_then(|action| match action.into_inner().0 {
                Some(Message::Timeline(action)) => Some(action),
                _ => None,
            })
    }
    fn volume_field() -> FieldRef {
        FieldRef::volume(TrackId(1))
    }

    #[test]
    fn a_grey_dot_re_enables_its_argument() {
        let timeline = mixed(true);
        let mut state = State::default();
        // Volume is the sixth envelope row: it opens at 38 + 24 + 5 * 48.
        let top = 38.0 + 24.0 + 5.0 * 48.0;
        let action = pressed(&timeline, &mut state, 14.0, top + 11.0);
        assert!(matches!(action, Some(Action::ReEnable(field)) if field == volume_field()));
    }

    #[test]
    fn a_red_dot_or_the_name_selects_the_lane_instead() {
        let timeline = mixed(false);
        let mut state = State::default();
        let top = 38.0 + 24.0 + 5.0 * 48.0;
        for x in [14.0, 80.0] {
            let action = pressed(&timeline, &mut state, x, top + 11.0);
            assert!(matches!(&action, Some(Action::Header(..))), "{action:?}");
        }
        // Grey, but off the dot: select.
        let grey = mixed(true);
        let action = pressed(&grey, &mut state, 100.0, top + 11.0);
        assert!(matches!(action, Some(Action::Header(..))));
    }

    #[test]
    fn empty_envelope_content_places_a_marker_instead_of_adding_a_hit() {
        let timeline = mixed(false);
        let mut state = State::default();
        for _ in 0..2 {
            let action = pressed(&timeline, &mut state, 300.0, 38.0 + 24.0 + 20.0);
            assert!(matches!(action, Some(Action::Content(_))), "{action:?}");
        }
    }

    #[test]
    fn event_rows_still_take_hits_and_clicks_above_envelopes() {
        let timeline = mixed(false);
        let mut state = State::default();
        // The kick hit at beat 1 sits in the first (24 px) row.
        let x = timeline.layout(bounds()).x(BeatTime(960));
        let action = pressed(&timeline, &mut state, x, 38.0 + 12.0);
        assert!(matches!(
            action,
            Some(Action::SelectAt(HitTarget { index: 0, .. }))
        ));
    }

    #[test]
    fn a_previewing_lane_changes_the_curve_key_so_release_and_cancel_redraw_the_committed_list() {
        let timeline = mixed(false);
        assert_ne!(timeline.curve_key(Some(1)), timeline.curve_key(None));
    }

    #[test]
    fn curves_are_redrawn_only_when_the_clip_revision_or_an_override_changes() {
        let timeline = mixed(false);
        let key = timeline.curve_key(None);
        // A playing clip moves its playhead and its live values every frame.
        let mut playing = mixed(false);
        playing.playhead = Some(BeatTime(480));
        for live in playing.live.iter_mut().flatten() {
            live.value = 0.9;
        }
        assert_eq!(playing.curve_key(None), key);
        let mut edited = mixed(false);
        edited.context.revision = 1;
        assert_ne!(edited.curve_key(None), key);
        assert_ne!(mixed(true).curve_key(None), key);
    }
}

/// Point gestures on the mixed clip's Opacity row: rows start at y = 38 (the ruler), the event
/// row is 24 px, so Opacity's plot runs from y = 68 (value 1) to 104 (value 0), and time
/// 0..3840 runs over x = 148..548. Its points: (0, 0.2), (1200, 0.9), (2400, 0.3), (3840, 0.6).
mod point_gestures {
    use super::envelope_lanes::mixed;
    use super::*;
    use qtable_engine::model::Breakpoint;

    fn x(ticks: f64) -> f32 {
        148.0 + (ticks / 3840.0 * 400.0) as f32
    }
    fn y(value: f64) -> f32 {
        68.0 + 36.0 * (1.0 - value as f32)
    }
    fn bounds() -> Rectangle {
        Rectangle::new(Point::new(20.0, 30.0), Size::new(558.0, 400.0))
    }
    fn motion(x: f32, y: f32) -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(x + 20.0, y + 30.0),
        })
    }
    fn send(
        timeline: &Timeline,
        state: &mut State,
        event: &canvas::Event,
        at: Option<(f32, f32)>,
    ) -> Option<Action> {
        let cursor = at.map_or(mouse::Cursor::Unavailable, |(x, y)| cursor(x, y));
        timeline
            .update(state, event, bounds(), cursor)
            .and_then(|action| match action.into_inner().0 {
                Some(Message::Timeline(action)) => Some(action),
                _ => None,
            })
    }
    fn press_at(timeline: &Timeline, state: &mut State, at: (f32, f32)) -> Option<Action> {
        send(timeline, state, &press(), Some(at))
    }
    fn release_at(timeline: &Timeline, state: &mut State, at: (f32, f32)) -> Option<Action> {
        let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        send(timeline, state, &release, Some(at))
    }
    fn drag_to(timeline: &Timeline, state: &mut State, at: (f32, f32)) -> Option<Action> {
        send(timeline, state, &motion(at.0, at.1), Some(at))
    }
    fn set_points(action: Option<Action>) -> (Vec<(u64, f64)>, Option<usize>) {
        match action {
            Some(Action::SetPoints(_, points, select)) => (
                points
                    .iter()
                    .map(|p: &Breakpoint| (p.time.ticks(), (p.value * 1e3).round() / 1e3))
                    .collect(),
                select,
            ),
            other => panic!("expected SetPoints, got {other:?}"),
        }
    }
    /// A press on the 1200 point, which sits at (273, 71.6).
    fn grab() -> (f32, f32) {
        (x(1200.0), y(0.9))
    }

    #[test]
    fn a_press_publishes_nothing_a_click_selects_and_a_second_click_deletes() {
        let mut timeline = mixed(false);
        let mut state = State::default();
        assert_eq!(
            press_at(&timeline, &mut state, grab()).map(|a| format!("{a:?}")),
            None
        );
        assert!(state.point.is_some());
        // The first click selects, publishing the unchanged list (choice A: a click that only selects).
        let release = release_at(&timeline, &mut state, grab());
        let Some(Action::SetPoints(target, _, _)) = &release else {
            panic!("expected SetPoints, got {release:?}");
        };
        let target = target.target.clone();
        let (points, select) = set_points(release);
        assert_eq!(points, [(0, 0.2), (1200, 0.9), (2400, 0.3), (3840, 0.6)]);
        assert_eq!(select, Some(1));
        // Nothing left to release.
        assert!(release_at(&timeline, &mut state, grab()).is_none());
        // A point selected on another lane doesn't count; on this one, a click deletes it.
        timeline.point = Some(1);
        press_at(&timeline, &mut state, grab());
        assert_eq!(
            set_points(release_at(&timeline, &mut state, grab())).1,
            Some(1)
        );
        timeline.envelope = Some(target);
        press_at(&timeline, &mut state, grab());
        let (points, select) = set_points(release_at(&timeline, &mut state, grab()));
        assert_eq!(points, [(0, 0.2), (2400, 0.3), (3840, 0.6)]);
        assert_eq!(select, None);
    }

    #[test]
    fn a_drag_previews_in_place_of_the_lane_and_commits_one_list_on_release() {
        let timeline = mixed(false);
        let mut state = State::default();
        press_at(&timeline, &mut state, grab());
        assert_eq!(
            timeline.previewing(&state),
            None,
            "a press alone previews nothing"
        );
        let mut published = 0;
        for step in 1..=12 {
            let action = drag_to(
                &timeline,
                &mut state,
                (grab().0 + step as f32 * 4.0, grab().1 + step as f32),
            );
            published += usize::from(matches!(action, Some(Action::SetPoints(..))));
        }
        assert_eq!(published, 0, "moving never edits");
        // Opacity is the second row; its committed curve is left out of the cached layer.
        assert_eq!(timeline.previewing(&state), Some(1));
        assert_ne!(timeline.curve_key(Some(1)), timeline.curve_key(None));
        let end = (grab().0 + 48.0, grab().1 + 12.0);
        let (points, select) = set_points(release_at(&timeline, &mut state, end));
        // 48 px is 480 ticks: 1680 on the grid, passing nothing; 12 px down is 0.333 lower.
        assert_eq!(points[1].0, 1680);
        assert!((points[1].1 - 0.567).abs() < 0.01, "{points:?}");
        assert_eq!(select, Some(1));
        assert_eq!(timeline.previewing(&state), None);
    }

    #[test]
    fn a_dragged_point_that_passes_neighbours_commits_the_list_without_them() {
        let timeline = mixed(false);
        let mut state = State::default();
        press_at(&timeline, &mut state, grab());
        drag_to(&timeline, &mut state, (x(3120.0), grab().1));
        let (points, select) = set_points(release_at(&timeline, &mut state, (x(3120.0), grab().1)));
        assert_eq!(points, [(0, 0.2), (3120, 0.9), (3840, 0.6)]);
        assert_eq!(select, Some(1));
    }

    #[test]
    fn empty_space_places_a_marker_then_drag_selects_time() {
        let timeline = mixed(false);
        let mut state = State::default();
        // Below the rising curve, outside the row's segment band (a quarter of its height).
        let at = (x(960.0), y(0.05));
        assert!(
            matches!(press_at(&timeline,&mut state,at), Some(Action::Content(s)) if s.range == (BeatTime(960)..BeatTime(960)))
        );
        assert!(
            matches!(release_at(&timeline,&mut state,at), Some(Action::Content(s)) if s.range.is_empty())
        );
        press_at(&timeline, &mut state, at);
        let end = (x(1920.0), at.1);
        assert!(
            matches!(drag_to(&timeline,&mut state,end),Some(Action::Content(s)) if s.range == (BeatTime(960)..BeatTime(1920)))
        );
        assert!(
            matches!(release_at(&timeline,&mut state,end),Some(Action::Content(s)) if s.range == (BeatTime(960)..BeatTime(1920)))
        );
    }

    #[test]
    fn alt_double_click_straightens_a_curved_segment() {
        let clock = crate::gesture_clock::test::Clock::new();
        let mut timeline = mixed(false);
        timeline.context.alt_down = true;
        let mut state = State::default();
        // The segment leaving the 1200 point bends (curve 0.5): at tick 1800 it reads
        // 0.9 - 0.6 * 0.5^4.
        let on_curve = (x(1800.0), y(0.9 - 0.6 * 0.0625));
        press_at(&timeline, &mut state, on_curve);
        let first = release_at(&timeline, &mut state, on_curve);
        assert!(first.is_none(), "{first:?}");
        clock.advance(std::time::Duration::from_millis(100));
        let action = press_at(&timeline, &mut state, on_curve);
        let Some(Action::SetPoints(_, points, None)) = action else {
            panic!("{action:?}");
        };
        assert_eq!(points[1].curve, 0.0);
        assert_eq!(points[1].time, BeatTime(1200));
        assert_eq!(points.len(), 4);
    }

    #[test]
    fn escape_focus_loss_and_leaving_the_window_cancel_with_no_edit() {
        for cancel in [
            canvas::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                physical_key: iced::keyboard::key::Physical::Code(
                    iced::keyboard::key::Code::Escape,
                ),
                location: iced::keyboard::Location::Standard,
                modifiers: iced::keyboard::Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            canvas::Event::Window(iced::window::Event::Unfocused),
            canvas::Event::Mouse(mouse::Event::CursorLeft),
        ] {
            let timeline = mixed(false);
            let mut state = State::default();
            press_at(&timeline, &mut state, grab());
            drag_to(&timeline, &mut state, (grab().0 + 40.0, grab().1));
            assert_eq!(timeline.previewing(&state), Some(1));
            let action = send(&timeline, &mut state, &cancel, None);
            assert!(!matches!(action, Some(Action::SetPoints(..))), "{cancel:?}");
            assert_eq!(
                timeline.previewing(&state),
                None,
                "no stale preview: {cancel:?}"
            );
            assert!(release_at(&timeline, &mut state, (grab().0 + 40.0, grab().1)).is_none());
        }
    }

    #[test]
    fn a_different_clip_or_a_new_revision_cancels_a_drag_and_never_previews_stale() {
        let started = mixed(false);
        let mut other_clip = mixed(false);
        other_clip.key.id = ClipId(2);
        let mut reselected = mixed(false);
        reselected.key.epoch = 1;
        let mut edited = mixed(false);
        edited.context.revision = 1;
        for later in [other_clip, reselected, edited] {
            let mut state = State::default();
            press_at(&started, &mut state, grab());
            drag_to(&started, &mut state, (grab().0 + 40.0, grab().1));
            assert_eq!(started.previewing(&state), Some(1));
            // The new view draws before it has seen an event: the stale gesture shows nothing.
            assert_eq!(later.previewing(&state), None);
            let action = release_at(&later, &mut state, (grab().0 + 40.0, grab().1));
            assert!(action.is_none(), "{action:?}");
            assert!(state.point.is_none());
        }
    }

    #[test]
    fn playback_and_automation_updates_do_not_cancel_a_drag() {
        let started = mixed(false);
        let mut state = State::default();
        press_at(&started, &mut state, grab());
        drag_to(&started, &mut state, (grab().0 + 40.0, grab().1));
        // The same clip and revision, now playing with new engine values and an override.
        let mut playing = mixed(true);
        playing.playhead = Some(BeatTime(480));
        for live in playing.live.iter_mut().flatten() {
            live.value = 0.9;
        }
        assert_eq!(playing.previewing(&state), Some(1));
        let (points, _) = set_points(release_at(
            &playing,
            &mut state,
            (grab().0 + 40.0, grab().1),
        ));
        assert_eq!(points[1].0, 1680);
    }
}

#[test]
fn only_envelope_headers_drag_and_both_kinds_route_context_targets() {
    let project = qtable_engine::file::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lanes.ron"),
    )
    .unwrap();
    let track = &project.tracks[0];
    let clip = track.slots[0].clip.as_ref().unwrap();
    let mut timeline = setup();
    timeline.clip = clip.clone().into();
    timeline.model = LaneModel::build(&project, track.id, clip).into();
    timeline.live = timeline.model.live(&project, &[]);
    let bounds = Rectangle::new(Point::ORIGIN, Size::new(600.0, 500.0));
    for row in [0, 1] {
        let mut state = State::default();
        let start = Point::new(80.0, RULER_HEIGHT + timeline.model.rows[row].top + 10.0);
        let action = timeline
            .update(
                &mut state,
                &press(),
                bounds,
                mouse::Cursor::Available(start),
            )
            .unwrap()
            .into_inner()
            .0;
        assert!(matches!(action,Some(Message::Timeline(Action::Header(_,i))) if i == row));
        let end = Point::new(80.0, 490.0);
        let moving = canvas::Event::Mouse(mouse::Event::CursorMoved { position: end });
        let action = timeline
            .update(&mut state, &moving, bounds, mouse::Cursor::Available(end))
            .and_then(|action| action.into_inner().0);
        assert!(action.is_none());
        if row == 0 {
            assert!(state.header_drag.is_none());
            continue;
        }
        assert!(state.header_drag.as_ref().unwrap().dragging());
        let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        let action = timeline
            .update(&mut state, &release, bounds, mouse::Cursor::Available(end))
            .unwrap()
            .into_inner()
            .0;
        assert!(
            matches!(action,Some(Message::Timeline(Action::MoveLanes(_,from,to))) if from == (row..row+1) && to == timeline.model.rows.len()-1)
        );
        assert!(
            timeline
                .update(&mut state, &release, bounds, mouse::Cursor::Available(end))
                .is_none()
        );
        let right = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
        let action = timeline
            .update(&mut state, &right, bounds, mouse::Cursor::Available(start))
            .unwrap()
            .into_inner()
            .0;
        assert!(
            matches!(action,Some(Message::Timeline(Action::LaneMenu(target))) if target.row == row)
        );
        timeline.update(
            &mut state,
            &press(),
            bounds,
            mouse::Cursor::Available(start),
        );
        timeline.context.revision += 1;
        assert!(
            timeline
                .update(&mut state, &release, bounds, mouse::Cursor::Available(end))
                .is_none()
        );
    }
}

fn motion(x: f32, y: f32) -> canvas::Event {
    canvas::Event::Mouse(mouse::Event::CursorMoved {
        position: Point::new(x + bounds().x, y + bounds().y),
    })
}

#[test]
fn background_drag_crosses_every_mixed_lane_and_can_select_to_the_clip_end() {
    let timeline = envelope_lanes::mixed(false);
    let layout = timeline.layout(bounds());
    let mut state = State::default();
    let from = (layout.x(BeatTime(480)), RULER_HEIGHT + 12.0);
    let to = (
        layout.x(BeatTime(3840)),
        RULER_HEIGHT + timeline.model.height() - 4.0,
    );
    timeline.update(&mut state, &press(), bounds(), cursor(from.0, from.1));
    let Some(Message::Timeline(Action::Content(s))) = timeline
        .update(
            &mut state,
            &motion(to.0, to.1),
            bounds(),
            cursor(to.0, to.1),
        )
        .unwrap()
        .into_inner()
        .0
    else {
        panic!("range selection");
    };
    assert_eq!(s.range, BeatTime(480)..BeatTime(3840));
    assert_eq!(s.lanes.len(), timeline.model.rows.len());
    assert!(
        s.lanes
            .iter()
            .any(|l| matches!(l, lanes::Identity::Event(_)))
    );
    assert!(
        s.lanes
            .iter()
            .any(|l| matches!(l, lanes::Identity::Envelope(_)))
    );
    assert!(matches!(
        timeline
            .update(&mut state, &release(), bounds(), cursor(to.0, to.1))
            .unwrap()
            .into_inner()
            .0,
        Some(Message::Timeline(Action::Content(_)))
    ));
}

#[test]
fn command_background_click_is_off_grid_and_selected_hit_drag_routes_a_region_move() {
    let mut timeline = setup();
    timeline.context.command_down = true;
    let layout = timeline.layout(bounds());
    let mut state = State::default();
    let from = (layout.x(BeatTime(1001)), RULER_HEIGHT + 12.0);
    // Stay clear of the hit at 960.
    let from = (from.0 + 10.0, from.1);
    let Some(Message::Timeline(Action::Content(s))) = timeline
        .update(&mut state, &press(), bounds(), cursor(from.0, from.1))
        .unwrap()
        .into_inner()
        .0
    else {
        panic!("marker");
    };
    assert_ne!(s.range.start.ticks() % timeline.grid.ticks(), 0);
    let reference = timeline.clip.events[0].definition.clone();
    timeline.context.content = Some(content::Selection::time(
        timeline.key,
        BeatTime(480)..BeatTime(1920),
        vec![lanes::Identity::Event(reference)],
    ));
    let at = (
        layout.x(timeline.clip.events[0].position),
        RULER_HEIGHT + 12.0,
    );
    timeline.update(&mut state, &press(), bounds(), cursor(at.0, at.1));
    let to = (at.0 + 30.0, at.1);
    timeline.update(
        &mut state,
        &motion(to.0, to.1),
        bounds(),
        cursor(to.0, to.1),
    );
    assert!(matches!(
        timeline
            .update(&mut state, &release(), bounds(), cursor(to.0, to.1))
            .unwrap()
            .into_inner()
            .0,
        Some(Message::Timeline(Action::RegionMove(_)))
    ));
}

mod draw_mode {
    use super::*;

    fn drawing() -> Timeline {
        let mut timeline = setup();
        timeline.context.draw = true;
        timeline
    }
    fn x(ticks: u64) -> f32 {
        let layout = Layout::fitted(BeatTime(3840), 558.0);
        layout.x(BeatTime(ticks))
    }
    fn motion(x: f32, y: f32) -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(x + 20.0, y + 30.0),
        })
    }
    fn published(action: Option<canvas::Action<Message>>) -> Option<Action> {
        match action?.into_inner().0 {
            Some(Message::Timeline(action)) => Some(action),
            _ => None,
        }
    }

    #[test]
    fn a_click_on_a_hit_publishes_an_erase_on_release_and_nothing_before() {
        let timeline = drawing();
        let mut state = State::default();
        let y = RULER_HEIGHT + 10.0;
        let down = timeline.update(&mut state, &press(), bounds(), cursor(x(960), y));
        assert!(published(down).is_none(), "a press selects nothing");
        let up = timeline.update(&mut state, &release(), bounds(), cursor(x(960), y));
        assert!(matches!(
            published(up),
            Some(Action::Draw(Draw::Erase(HitTarget { index: 0, .. })))
        ));
    }

    #[test]
    fn a_drag_over_empty_cells_publishes_one_stroke_with_a_hit_per_cell() {
        let timeline = drawing();
        let mut state = State::default();
        let y = RULER_HEIGHT + 10.0;
        let down = timeline.update(&mut state, &press(), bounds(), cursor(x(250), y));
        assert!(published(down).is_none());
        let moved = timeline.update(&mut state, &motion(x(700), y), bounds(), cursor(x(700), y));
        assert!(published(moved).is_none(), "nothing publishes mid-stroke");
        let up = timeline.update(&mut state, &release(), bounds(), cursor(x(700), y));
        let Some(Action::Draw(Draw::Hits {
            span, positions, ..
        })) = published(up)
        else {
            panic!("expected one stroke");
        };
        assert_eq!(span, BeatTime(240)..BeatTime(720));
        assert_eq!(positions, vec![BeatTime(240), BeatTime(480)]);
    }

    #[test]
    fn with_draw_mode_off_the_same_press_still_selects() {
        let timeline = setup();
        let mut state = State::default();
        let y = RULER_HEIGHT + 10.0;
        let down = timeline.update(&mut state, &press(), bounds(), cursor(x(960), y));
        assert!(matches!(published(down), Some(Action::SelectAt(..))));
    }
}

#[test]
fn insert_strip_clicks_use_the_content_selection_including_end_and_off_grid() {
    let mut timeline = headed(setup());
    let mut state = State::default();
    let y = super::super::event_timeline_math::INSERT_STRIP_TOP + 3.0;
    let at = timeline.layout(bounds()).x(BeatTime(3840));
    let action = timeline
        .update(&mut state, &press(), bounds(), cursor(at, y))
        .unwrap();
    let Some(Message::Timeline(Action::Content(selection))) = action.into_inner().0 else {
        panic!("strip must place content")
    };
    assert_eq!(selection.insert_mark(), Some(BeatTime(3840)));
    assert_eq!(selection.lanes.len(), 1);
    timeline.context.command_down = true;
    let at = timeline.layout(bounds()).x(BeatTime(333));
    let action = timeline
        .update(&mut state, &press(), bounds(), cursor(at, y))
        .unwrap();
    let Some(Message::Timeline(Action::Content(selection))) = action.into_inner().0 else {
        panic!("strip must place content")
    };
    assert_eq!(selection.insert_mark(), Some(BeatTime(333)));
}

#[test]
fn clip_markers_drag_with_draw_mode_and_cancel_on_revision_change() {
    for draw in [false, true] {
        let mut timeline = headed(setup());
        timeline.context.draw = draw;
        let mut clip = (*timeline.clip).clone();
        clip.start = BeatTime(480);
        clip.end = BeatTime(5760);
        clip.loop_start = BeatTime(960);
        clip.loop_end = BeatTime(2880);
        timeline.clip = clip.into();
        let layout = timeline.layout(bounds());
        assert_eq!(layout.length(), BeatTime(7680));
        let mut state = State::default();
        let start_x = layout.x(BeatTime(480));
        timeline.update(&mut state, &press(), bounds(), cursor(start_x, 15.0));
        assert!(state.marker.is_some());
        assert!(state.paint.is_none());
        let end_x = layout.x(BeatTime(1200));
        let moved = canvas::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(end_x + 20.0, 45.0),
        });
        let action = timeline
            .update(&mut state, &moved, bounds(), mouse::Cursor::Unavailable)
            .unwrap();
        assert!(action.into_inner().0.is_none());
        let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        let action = timeline
            .update(&mut state, &release, bounds(), mouse::Cursor::Unavailable)
            .unwrap();
        let Some(Message::Timeline(Action::Markers(_, _, markers))) = action.into_inner().0 else {
            panic!("marker release")
        };
        assert_eq!(markers.start, BeatTime(1200));
        assert_eq!(markers.end, BeatTime(5760));
        assert_eq!(markers.loop_start, BeatTime(960));
        assert_eq!(markers.loop_end, BeatTime(2880));
        timeline.update(&mut state, &press(), bounds(), cursor(start_x, 15.0));
        timeline.context.revision += 1;
        assert!(
            timeline
                .update(&mut state, &release, bounds(), mouse::Cursor::Unavailable)
                .is_none()
        );
        assert!(state.marker.is_none());
    }
}

#[test]
fn unlinked_overview_point_and_draw_presses_are_read_only() {
    for draw in [false, true] {
        let mut timeline = envelope_lanes::mixed(false);
        let model = std::rc::Rc::make_mut(&mut timeline.model);
        let RowKind::Envelope(envelope) = &mut model.rows[1].kind else {
            panic!("envelope row");
        };
        std::rc::Rc::make_mut(envelope).region = Some(timeline.clip.region());
        timeline.context.draw = draw;
        let mut state = State::default();
        // An existing breakpoint, a segment and empty background all take no edit gesture.
        for (x, y) in [(273.0, 71.6), (200.0, 90.0), (450.0, 100.0)] {
            let action = timeline.update(
                &mut state,
                &press(),
                Rectangle::new(Point::new(20.0, 30.0), Size::new(558.0, 400.0)),
                cursor(x, y),
            );
            assert!(action.and_then(|a| a.into_inner().0).is_none());
            assert!(state.point.is_none());
            assert!(state.paint.is_none());
            assert!(state.range.is_none());
            assert!(state.region.is_none());
        }
    }
}

#[test]
fn right_clicks_on_overview_envelope_rows_report_a_point_or_space_and_unlinked_rows_open_nothing() {
    let area = Rectangle::new(Point::new(20.0, 30.0), Size::new(558.0, 400.0));
    let right = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
    let mut timeline = envelope_lanes::mixed(false);
    let mut state = State::default();
    let ask = |timeline: &Timeline, state: &mut State, at: (f32, f32)| {
        timeline
            .update(state, &right, area, cursor(at.0, at.1))
            .and_then(|action| match action.into_inner().0 {
                Some(Message::Timeline(Action::ContentMenu(click))) => Some(click),
                _ => None,
            })
    };
    // Opacity's breakpoint, then empty background in the same lane.
    let on_point = ask(&timeline, &mut state, (273.0, 71.6)).unwrap();
    assert!(matches!(on_point.lane, lanes::Identity::Envelope(_)));
    assert!(matches!(on_point.under, menu::Under::Point(_)));
    let on_space = ask(&timeline, &mut state, (450.0, 100.0)).unwrap();
    assert!(matches!(on_space.lane, lanes::Identity::Envelope(_)));
    assert!(matches!(on_space.under, menu::Under::Space(_)));
    // The ruler and the label column open nothing here (the header has its own menu).
    assert!(ask(&timeline, &mut state, (300.0, 10.0)).is_none());
    // An unlinked lane has its own timeline: only its One lane view edits it.
    let model = std::rc::Rc::make_mut(&mut timeline.model);
    let RowKind::Envelope(envelope) = &mut model.rows[1].kind else {
        panic!("envelope row");
    };
    std::rc::Rc::make_mut(envelope).region = Some(timeline.clip.region());
    assert!(ask(&timeline, &mut state, (273.0, 71.6)).is_none());
    assert!(ask(&timeline, &mut state, (450.0, 100.0)).is_none());
}

mod info_regions {
    use super::envelope_lanes::mixed;
    use super::*;
    use crate::style::detail::{CLIP_BRACE_TOP, CLIP_FLAG_BOTTOM, CLIP_FLAG_TOP, INSERT_STRIP_TOP};

    #[test]
    fn the_info_view_follows_the_pointer_over_the_brace_flags_and_linked_lines() {
        let timeline = mixed(false);
        let size = Size::new(558.0, 400.0);
        let layout = timeline.layout(Rectangle::new(Point::ORIGIN, size));
        let axis = timeline.axis(&layout, style::TEXT);
        let (flag, brace) = (
            (CLIP_FLAG_TOP + CLIP_FLAG_BOTTOM) / 2.0,
            (CLIP_BRACE_TOP + INSERT_STRIP_TOP) / 2.0,
        );
        let r = axis.markers;
        let middle = (axis.x(r.loop_start) + axis.x(r.loop_end)) / 2.0;
        // Opacity's second point, on its row.
        let row = (1..timeline.model.rows.len())
            .find(|row| timeline.envelope(*row).is_some())
            .unwrap();
        let lane = timeline.lane(row, &layout).unwrap();
        let point = lane.points()[1].clone();
        let on_point = Point::new(lane.x(point.time), lane.y(point.value));
        for (at, id) in [
            (Point::new(axis.x(r.start) + 2.0, flag), Id::StartMarker),
            (Point::new(axis.x(r.loop_start) + 2.0, brace), Id::LoopStart),
            (Point::new(axis.x(r.loop_end) - 2.0, brace), Id::LoopEnd),
            (Point::new(middle, brace), Id::LoopBrace),
            (on_point, Id::EnvelopeLine),
            (Point::new(20.0, on_point.y), Id::EventTimeline),
            (Point::new(middle, RULER_HEIGHT + 4.0), Id::EventTimeline),
        ] {
            assert_eq!(timeline.info_at(at, size), id, "{at:?}");
        }
    }
}

#[test]
fn unused_device_event_row_accepts_double_click_and_draw_without_a_clip_index() {
    let clock = crate::gesture_clock::test::Clock::new();
    let project = crate::inspector::lane_model::tests::track_event_rows_project();
    let clip = project.clip(TrackId(1), 0).unwrap();
    let mut timeline = setup();
    timeline.clip = clip.clone().into();
    timeline.model = LaneModel::build(&project, TrackId(1), clip).events().into();
    timeline.live = vec![None; timeline.model.rows.len()];
    timeline.context.events_only = true;
    let RowKind::Event {
        index: None,
        reference,
    } = timeline.model.rows[1].kind.clone()
    else {
        panic!()
    };
    let bounds = Rectangle::new(Point::ORIGIN, Size::new(600.0, 300.0));
    let at = Point::new(350.0, RULER_HEIGHT + timeline.model.rows[1].top + 12.0);
    let cursor = mouse::Cursor::Available(at);
    let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    let mut state = State::default();
    timeline.update(&mut state, &press(), bounds, cursor);
    timeline.update(&mut state, &release, bounds, cursor);
    clock.advance(std::time::Duration::from_millis(100));
    let action = timeline
        .update(&mut state, &press(), bounds, cursor)
        .unwrap()
        .into_inner()
        .0;
    assert!(
        matches!(action, Some(Message::Timeline(Action::Add(_, _, lane))) if lane == reference)
    );
    timeline.context.draw = true;
    let mut state = State::default();
    timeline.update(&mut state, &press(), bounds, cursor);
    let action = timeline
        .update(&mut state, &release, bounds, cursor)
        .unwrap()
        .into_inner()
        .0;
    assert!(
        matches!(action, Some(Message::Timeline(Action::Draw(Draw::Hits { lane, positions, .. }))) if lane == reference && positions.len() == 1)
    );
}

/// Each caller keeps the clamp and snap rule it had before the shared time axis: the press on
/// Events snaps inside a half-open clip, a point snaps inside a closed one, the range selection
/// snaps to the extent. These read the old formulas out of the new code over a table of widths.
mod axis_rules {
    use super::*;
    use crate::inspector::event_timeline_math::snap;
    use crate::inspector::point_gesture::{Keys, Lane, Plot};

    const WIDTHS: [f32; 4] = [300.0, 558.0, 1000.0, 1913.5];
    const LENGTHS: [u64; 3] = [960, 3840, 7680];
    const GRIDS: [Grid; 4] = [
        Grid::Off,
        Grid::Sixteenth,
        Grid::Quarter,
        Grid::EighthTriplet,
    ];

    fn xs(width: f32) -> Vec<f32> {
        (-20..=(width as i32 + 20))
            .step_by(7)
            .map(|x| x as f32 + 0.37)
            .collect()
    }

    #[test]
    fn a_hit_press_still_snaps_inside_a_half_open_clip() {
        for width in WIDTHS {
            for length in LENGTHS {
                let layout = Layout::fitted(BeatTime(length), width);
                let span = (width - LABEL_WIDTH - 10.0).max(1.0);
                for grid in GRIDS {
                    for x in xs(width) {
                        let old = ((x - LABEL_WIDTH) as f64 / span as f64 * length as f64).max(0.0);
                        assert_eq!(
                            layout.position_at(x, grid),
                            snap(old, BeatTime(length), grid),
                        );
                        let dx = x - 100.0;
                        let old = 960.0 + dx as f64 / span as f64 * length as f64;
                        assert_eq!(
                            layout.dragged(BeatTime(960), dx, grid),
                            snap(old, BeatTime(length), grid),
                        );
                    }
                }
                // The last grid cell is the clip's last tick, never its end.
                assert!(layout.position_at(width * 2.0, Grid::Off) < BeatTime(length));
            }
        }
    }

    #[test]
    fn a_point_press_still_snaps_inside_a_closed_clip() {
        for width in WIDTHS {
            for length in LENGTHS {
                let plot = Plot {
                    left: 148.0,
                    width: (width - 158.0).max(1.0),
                    top: 0.0,
                    height: 40.0,
                };
                let lane = Lane::new(
                    plot,
                    (
                        BeatTime(length),
                        qtable_engine::model::Axis::Continuous,
                        0.0,
                        1.0,
                        240,
                    ),
                    &qtable_engine::model::ArgumentRef::volume(),
                    &[],
                );
                for command in [false, true] {
                    let step = if command { 1 } else { 240 };
                    let keys = Keys {
                        command,
                        ..Keys::default()
                    };
                    for x in xs(width) {
                        let ticks = ((f64::from(x - plot.left) / f64::from(plot.width.max(1.0)))
                            * length as f64)
                            .clamp(0.0, length as f64);
                        let old = (((ticks / step as f64).round() as u64) * step).min(length);
                        assert_eq!(lane.time_at(x, keys), BeatTime(old));
                    }
                }
                // Closed: a press past the right edge lands on the clip's end.
                assert_eq!(lane.time_at(width * 3.0, Keys::default()), BeatTime(length));
            }
        }
    }

    #[test]
    fn a_range_selection_still_snaps_to_the_extent() {
        for width in WIDTHS {
            for length in LENGTHS {
                let mut timeline = setup();
                let layout = Layout::fitted(BeatTime(length), width);
                let extent = layout.length();
                let span = layout.span();
                for command in [false, true] {
                    timeline.context.command_down = command;
                    let step = if command { 1 } else { timeline.grid.ticks() };
                    for x in xs(width) {
                        let ticks = ((x - LABEL_WIDTH) as f64 / span as f64
                            * extent.ticks() as f64)
                            .max(0.0);
                        let old = BeatTime(
                            (((ticks / step as f64).round() as u64) * step).min(extent.ticks()),
                        );
                        assert_eq!(timeline.range_time(&layout, x), old);
                    }
                    assert_eq!(timeline.range_time(&layout, width * 3.0).ticks() % step, 0);
                }
            }
        }
    }
}

mod zoom {
    use super::*;
    use crate::control::time_axis::Window;
    use crate::gesture_clock::test::Clock;

    /// The 3840-tick test clip across 400 px from x 148, in canvas coordinates.
    const LEFT: f32 = 148.0;

    fn motion(x: f32, y: f32) -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(x + 20.0, y + 30.0),
        })
    }
    fn release() -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
    }
    fn wheel(x: f32, y: f32) -> canvas::Event {
        canvas::Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x, y },
        })
    }
    fn windowed(start: f64, visible: f64) -> Timeline {
        let mut timeline = setup();
        timeline.context.window = Some(Window { start, visible });
        timeline
    }
    /// The ruler header's canvas on the same clip: where the ruler gestures live.
    fn header(timeline: &Timeline) -> Timeline {
        headed(timeline.clone())
    }
    /// The action an event publishes, or `None` when it publishes nothing.
    fn action(
        timeline: &Timeline,
        state: &mut State,
        event: &canvas::Event,
        at: (f32, f32),
    ) -> Option<Action> {
        timeline
            .update(state, event, bounds(), cursor(at.0, at.1))
            .and_then(|action| match action.into_inner().0 {
                Some(Message::Timeline(action)) => Some(action),
                _ => None,
            })
    }
    fn zoomed(action: Option<Action>) -> Option<Window> {
        match action {
            Some(Action::Zoom(_, window)) => window,
            other => panic!("expected a zoom, got {other:?}"),
        }
    }
    /// The tick under canvas x on a window.
    fn tick_under(window: Option<Window>, x: f32) -> f64 {
        let w = window.unwrap_or(Window {
            start: 0.0,
            visible: 3840.0,
        });
        w.start + f64::from(x - LEFT) / 400.0 * w.visible
    }

    #[test]
    fn a_ruler_drag_down_zooms_in_about_the_pressed_time_and_up_zooms_out() {
        let timeline = headed(setup());
        let mut state = State::default();
        let press_at = (300.0, 10.0);
        assert!(action(&timeline, &mut state, &press(), press_at).is_none());
        assert!(state.zoom.dragging());
        let window = zoomed(action(
            &timeline,
            &mut state,
            &motion(300.0, 60.0),
            (300.0, 60.0),
        ));
        let w = window.unwrap();
        assert!(
            (w.visible - 1920.0).abs() < 1e-6,
            "one octave of travel halves it"
        );
        assert!((tick_under(window, 300.0) - tick_under(None, 300.0)).abs() < 1e-6);
        // The same press dragged up and out again is back to the whole clip.
        let back = action(&timeline, &mut state, &motion(300.0, 10.0), (300.0, 10.0));
        assert_eq!(zoomed(back), None);
        action(&timeline, &mut state, &release(), (300.0, 10.0));
        assert!(!state.zoom.dragging());
    }

    #[test]
    fn a_right_press_during_a_ruler_drag_ends_it_where_it_is_and_opens_no_menu() {
        let lanes = setup();
        let timeline = headed(setup());
        let mut state = State::default();
        let right = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
        action(&timeline, &mut state, &press(), (300.0, 10.0));
        // Dragging down puts the pointer over lane space, where a right-press opens a menu.
        let down = (300.0, 50.0);
        let w = zoomed(action(&timeline, &mut state, &motion(down.0, down.1), down)).unwrap();
        assert!(state.zoom.dragging());
        let pressed = action(&timeline, &mut state, &right, down);
        assert!(pressed.is_none(), "no menu, no restore: {pressed:?}");
        assert!(!state.zoom.dragging(), "the drag has ended");
        // The window stays as the last move left it: later moves and the release change nothing.
        let later = action(&timeline, &mut state, &motion(300.0, 90.0), (300.0, 90.0));
        assert!(!matches!(later, Some(Action::Zoom(..))), "{later:?}");
        let released = action(&timeline, &mut state, &release(), (300.0, 90.0));
        assert!(
            !matches!(released, Some(Action::Zoom(..))),
            "{released:?} after {w:?}"
        );
        // A right-press with no drag in progress opens its menu as before.
        let again = action(&lanes, &mut State::default(), &right, (LEFT + 100.0, 50.0));
        assert!(matches!(again, Some(Action::ContentMenu(_))), "{again:?}");
    }

    #[test]
    fn a_sideways_ruler_drag_scrolls_without_changing_the_scale() {
        let timeline = header(&windowed(960.0, 1920.0));
        let mut state = State::default();
        action(&timeline, &mut state, &press(), (300.0, 10.0));
        let moved = zoomed(action(
            &timeline,
            &mut state,
            &motion(260.0, 10.0),
            (260.0, 10.0),
        ))
        .unwrap();
        // 40 px at 4.8 ticks a pixel: the content follows the pointer, so the window moves later.
        assert!((moved.visible - 1920.0).abs() < 1e-6);
        assert!((moved.start - (960.0 + 192.0)).abs() < 1e-6, "{moved:?}");
    }

    #[test]
    fn a_press_on_a_flag_keeps_its_marker_drag_and_does_not_zoom() {
        let timeline = headed(setup());
        let mut state = State::default();
        let start = timeline
            .axis(&timeline.layout(bounds()), style::TEXT)
            .x(BeatTime(0));
        let published = action(&timeline, &mut state, &press(), (start + 2.0, 15.0));
        assert!(matches!(published, Some(Action::Focus(_))));
        assert!(state.marker.is_some());
        assert!(!state.zoom.dragging());
    }

    #[test]
    fn a_double_click_on_the_ruler_asks_to_zoom_to_the_selection() {
        let clock = Clock::new();
        let timeline = headed(setup());
        let mut state = State::default();
        action(&timeline, &mut state, &press(), (300.0, 10.0));
        action(&timeline, &mut state, &release(), (300.0, 10.0));
        clock.advance(std::time::Duration::from_millis(200));
        let second = action(&timeline, &mut state, &press(), (301.0, 10.0));
        assert!(matches!(second, Some(Action::ZoomTo(_))));
        assert!(!state.zoom.dragging());
        // Too slow, or too far, is two single presses.
        let mut state = State::default();
        action(&timeline, &mut state, &press(), (300.0, 10.0));
        action(&timeline, &mut state, &release(), (300.0, 10.0));
        clock.advance(std::time::Duration::from_millis(900));
        assert!(action(&timeline, &mut state, &press(), (300.0, 10.0)).is_none());
        assert!(state.zoom.dragging());
    }

    #[test]
    fn the_insert_marker_a_time_selection_and_a_flag_land_on_the_zoomed_times() {
        // 1920 ticks across 400 px from tick 1920: 4.8 ticks a pixel.
        let timeline = windowed(1920.0, 1920.0);
        let ruler = header(&timeline);
        let mut state = State::default();
        let at = |x: f32| (x, 50.0);
        // The insert strip puts the marker at the zoomed tick under the pointer.
        let marker = action(
            &ruler,
            &mut State::default(),
            &press(),
            (LEFT + 100.0, 33.0),
        );
        assert!(
            matches!(marker, Some(Action::Content(ref s)) if s.range == (BeatTime(2400)..BeatTime(2400))),
            "{marker:?}"
        );
        // A press on empty lane space begins a time selection there; dragging extends it.
        let begun = action(&timeline, &mut state, &press(), at(LEFT + 100.0));
        assert!(
            matches!(begun, Some(Action::Content(ref s)) if s.range.start == BeatTime(2400)),
            "{begun:?}"
        );
        let dragged = action(
            &timeline,
            &mut state,
            &motion(LEFT + 200.0, 50.0),
            at(LEFT + 200.0),
        );
        assert!(
            matches!(dragged, Some(Action::Content(ref s)) if s.range == (BeatTime(2400)..BeatTime(2880))),
            "{dragged:?}"
        );
        // The End flag, at the window's right edge, drags by the window's ticks a pixel.
        let mut state = State::default();
        let edge = LEFT + 400.0 - 2.0;
        let grabbed = action(&ruler, &mut state, &press(), (edge, 15.0));
        assert!(matches!(grabbed, Some(Action::Focus(_))), "{grabbed:?}");
        action(
            &ruler,
            &mut state,
            &motion(edge - 50.0, 15.0),
            (edge - 50.0, 15.0),
        );
        let moved = action(&ruler, &mut state, &release(), (edge - 50.0, 15.0));
        assert!(
            matches!(moved, Some(Action::Markers(_, _, r)) if r.end == BeatTime(3600)),
            "{moved:?}"
        );
    }

    #[test]
    fn a_right_click_on_lane_space_places_its_marker_on_the_zoomed_time() {
        let timeline = windowed(1920.0, 1920.0);
        let mut state = State::default();
        let right = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
        let clicked = action(&timeline, &mut state, &right, (LEFT + 100.0, 50.0));
        let Some(Action::ContentMenu(click)) = clicked else {
            panic!("a content menu click, got {clicked:?}");
        };
        assert_eq!(click.under, menu::Under::Space(BeatTime(2400)));
    }

    #[test]
    fn a_press_on_the_brace_row_above_the_header_column_grabs_nothing_when_zoomed() {
        // The loop (0..3840) starts left of the window, so its body spans the header column's x.
        let timeline = header(&windowed(960.0, 1920.0));
        let mut state = State::default();
        let above_headers = (70.0, 24.0);
        let published = action(&timeline, &mut state, &press(), above_headers);
        assert!(
            state.marker.is_none(),
            "no brace drag started: {published:?}"
        );
        assert!(!matches!(published, Some(Action::Focus(_))));
        let idle = State::default();
        assert_ne!(
            timeline.mouse_interaction(&idle, bounds(), cursor(above_headers.0, above_headers.1)),
            mouse::Interaction::Move
        );
        // The same press on the brace inside the time area still moves the loop.
        let on_brace = action(&timeline, &mut state, &press(), (LEFT + 100.0, 24.0));
        assert!(matches!(on_brace, Some(Action::Focus(_))));
        assert!(state.marker.is_some());
    }

    #[test]
    fn selection_shading_stays_in_the_time_area_when_zoomed() {
        let range = BeatTime(0)..BeatTime(2880);
        let spans = |window: Option<Window>| {
            let mut timeline = setup();
            timeline.context.window = window;
            let axis = timeline.axis(&timeline.layout(bounds()), style::TEXT);
            (
                super::super::ruler::selection_span(&axis, &range),
                (axis.time.left, axis.time.left + axis.time.span),
            )
        };
        // Selected 0..2880, zoomed to 1920..3840: the shading starts at the window's left edge,
        // not 400 px left of it over the lane headers, and ends at 2880.
        let ((left, right), (lo, hi)) = spans(Some(Window {
            start: 1920.0,
            visible: 1920.0,
        }));
        assert_eq!(left, lo);
        assert!(right > lo && right < hi, "{right}");
        // A selection that runs past the window's right edge stops at it, short of the gutter.
        let ((left, right), (lo, hi)) = spans(Some(Window {
            start: 0.0,
            visible: 1920.0,
        }));
        assert_eq!((left, right), (lo, hi));
        // A fitted axis is exactly what it was: the range's own x.
        let ((left, right), (lo, hi)) = spans(None);
        assert_eq!(left, lo);
        assert!(right > lo && right < hi);
    }

    #[test]
    fn a_hit_left_of_the_window_is_neither_drawn_nor_hit() {
        let layout_for = |start| {
            let timeline = windowed(start, 1920.0);
            (timeline.layout(bounds()), timeline)
        };
        // The hit at 960 sits 3 px left of the time area: its marker would overlap it by 2 px.
        let (layout, timeline) = layout_for(975.0);
        let row = Point::new(LEFT + 1.0, RULER_HEIGHT + 12.0);
        assert!(layout.x(BeatTime(960)) < LEFT);
        assert_eq!(
            layout.hit(row, &timeline.clip.events, &timeline.model),
            None
        );
        assert!(!layout.axis.shows(BeatTime(960), 0.0));
        // Inside the window it is hit where it is drawn.
        let (layout, timeline) = layout_for(900.0);
        let at = Point::new(layout.x(BeatTime(960)), RULER_HEIGHT + 12.0);
        assert_eq!(
            layout.hit(at, &timeline.clip.events, &timeline.model),
            Some(0)
        );
    }

    #[test]
    fn a_plain_vertical_scroll_is_not_captured_but_the_zoom_wheels_are() {
        let timeline = windowed(960.0, 1920.0);
        let mut state = State::default();
        let over = (300.0, 50.0);
        // Plain vertical scroll goes on to the lane list.
        assert!(
            timeline
                .update(
                    &mut state,
                    &wheel(0.0, 1.0),
                    bounds(),
                    cursor(over.0, over.1)
                )
                .is_none()
        );
        // Sideways moves the window; the content follows the scroll, so the window goes back.
        let scrolled = zoomed(action(&timeline, &mut state, &wheel(1.0, 0.0), over)).unwrap();
        assert!(scrolled.start < 960.0 && (scrolled.visible - 1920.0).abs() < 1e-9);
        // Shift with a vertical scroll does the same.
        let mut shifted = timeline.clone();
        shifted.context.shift_down = true;
        let by_shift = zoomed(action(&shifted, &mut state, &wheel(0.0, 1.0), over)).unwrap();
        assert_eq!(by_shift, scrolled);
        // Command-scroll zooms about the time under the pointer.
        let mut zoom = timeline.clone();
        zoom.context.command_down = true;
        let before = tick_under(zoom.context.window, over.0);
        let narrower = action(&zoom, &mut state, &wheel(0.0, 1.0), over);
        let w = zoomed(narrower).unwrap();
        assert!(w.visible < 1920.0);
        assert!((tick_under(Some(w), over.0) - before).abs() < 1e-6);
        let wider = zoomed(action(&zoom, &mut state, &wheel(0.0, -1.0), over)).unwrap();
        assert!(wider.visible > 1920.0);
    }

    #[test]
    fn a_wheel_that_changes_nothing_is_not_captured_and_none_works_during_a_gesture() {
        // Fitted: a sideways scroll and a zoom-out have nowhere to go.
        let fitted = setup();
        let mut state = State::default();
        let over = cursor(300.0, 50.0);
        assert!(
            fitted
                .update(&mut state, &wheel(1.0, 0.0), bounds(), over)
                .is_none()
        );
        let mut out = fitted.clone();
        out.context.command_down = true;
        assert!(
            out.update(&mut state, &wheel(0.0, -1.0), bounds(), over)
                .is_none()
        );
        // Over the header column there is no time to zoom about.
        assert!(
            out.update(&mut state, &wheel(0.0, 1.0), bounds(), cursor(50.0, 50.0))
                .is_none()
        );
        // A wheel during a gesture is ignored, though the same wheel would zoom.
        let mut zoom = fitted.clone();
        zoom.context.command_down = true;
        let mut state = State::default();
        zoom.update(&mut state, &press(), bounds(), cursor(LEFT + 100.0, 50.0));
        assert!(state.range.is_some() || state.drag.is_some());
        assert!(
            zoom.update(&mut state, &wheel(0.0, 1.0), bounds(), over)
                .is_none()
        );
        let mut idle = State::default();
        assert!(
            zoom.update(&mut idle, &wheel(0.0, 1.0), bounds(), over)
                .is_some()
        );
    }

    #[test]
    fn the_pointer_is_a_magnifier_over_the_zoom_zone_only() {
        let timeline = setup();
        let state = State::default();
        let interaction =
            |x: f32, y: f32| timeline.mouse_interaction(&state, bounds(), cursor(x, y));
        assert_eq!(interaction(300.0, 10.0), mouse::Interaction::ZoomIn);
        assert_ne!(
            interaction(300.0, 24.0),
            mouse::Interaction::ZoomIn,
            "the brace row"
        );
        assert_ne!(
            interaction(300.0, 50.0),
            mouse::Interaction::ZoomIn,
            "a lane"
        );
        assert_ne!(
            interaction(50.0, 10.0),
            mouse::Interaction::ZoomIn,
            "the header column"
        );
    }

    #[test]
    fn a_zoomed_window_asks_for_labels_down_to_sixteenths_and_a_fitted_one_does_not() {
        let labels = |window: Option<Window>| {
            let mut timeline = setup();
            timeline.context.window = window;
            let layout = timeline.layout(bounds());
            let axis = timeline.axis(&layout, style::TEXT);
            super::super::ruler::label_texts(&axis)
        };
        assert_eq!(
            labels(None),
            vec!["1", "2"],
            "a fitted clip shows bar numbers only"
        );
        let beats = labels(Some(Window {
            start: 0.0,
            visible: 3840.0 - 480.0,
        }));
        assert!(beats.contains(&"1.2".to_string()), "{beats:?}");
        let sixteenths = labels(Some(Window {
            start: 0.0,
            visible: 960.0,
        }));
        assert!(sixteenths.contains(&"1.1.2".to_string()), "{sixteenths:?}");
        assert!(sixteenths.contains(&"1.2".to_string()), "{sixteenths:?}");
    }
}
