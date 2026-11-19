use super::*;
use crate::grid::TRACK_H;
use crate::grid::cache::GridState;
use crate::grid::{Action, TRACK_W, layout::track_columns, layout::x_positions};
use iced::mouse;
use iced::widget::canvas::{self, Program};
use qtable_engine::model::{LIVE_CLIP_PALETTE, Project, Route};

/// Tracks 1–4; tracks 2 and 3 form group 1. Columns: 1 | G | 2 | 3 | 4.
fn project(folded: bool) -> Project {
    let mut project = Project {
        tracks: (1..=4)
            .map(|i| Track::new(TrackId(i), format!("T{i}"), 0))
            .collect(),
        ..Project::default()
    };
    let mut group = Track::new_group(TrackId(101), "G", 0);
    group.colour = LIVE_CLIP_PALETTE[0];
    group.fold_state = folded;
    project.insert_group(group, &[TrackId(2), TrackId(3)]);
    project
}

/// The drop of track index `dragged` at a fraction across column `ci` (or past the end).
fn drop_at_column(
    project: &Project,
    dragged: usize,
    ci: f32,
) -> Option<(Option<u64>, Option<u64>)> {
    let cols = track_columns(&project.tracks);
    let xs = x_positions(&cols);
    let x = GAP + ci * (TRACK_W + GAP);
    drop_at(
        &project.tracks,
        &cols,
        &xs,
        cols.iter().find(|col| col.index == Some(dragged))?.key,
        Point::new(x, 10.),
    )
    .map(|d| (d.before.map(|t| t.0), d.group.map(|g| g.0)))
}

#[test]
fn drops_between_members_or_on_the_group_track_join_and_elsewhere_leave() {
    let p = project(false);
    // Track 1 dragged: left half of member 3's column → before 3, in the group.
    assert_eq!(drop_at_column(&p, 0, 3.25), Some((Some(3), Some(101))));
    // Right half of the group track's first member → before 3 too.
    assert_eq!(drop_at_column(&p, 0, 2.75), Some((Some(3), Some(101))));
    // Expanded title right half → between its header and first member.
    assert_eq!(drop_at_column(&p, 0, 1.5), Some((Some(2), Some(101))));
    // Track 4 dragged before the group track → before its first member, no group.
    assert_eq!(drop_at_column(&p, 4, 1.25), Some((Some(101), None)));
    // Member 2 dragged past the end → last, out of the group.
    assert_eq!(drop_at_column(&p, 2, 7.0), Some((None, None)));
    // Member 3 dragged to the boundary after the group's last member leaves the group in place.
    assert_eq!(drop_at_column(&p, 3, 4.25), Some((Some(4), None)));
}

#[test]
fn drops_that_change_nothing_resolve_to_none() {
    let p = project(false);
    // Track 1 just before itself or just after itself stays put.
    assert_eq!(drop_at_column(&p, 0, 0.25), None);
    assert_eq!(drop_at_column(&p, 0, 0.75), None);
    // The expanded title right half is the first-member boundary.
    assert_eq!(drop_at_column(&p, 3, 1.5), Some((Some(2), Some(101))));
}

#[test]
fn a_folded_group_track_still_takes_drops() {
    let p = project(true);
    // Columns: 1 | G | 4. The right half joins G before its first hidden member.
    assert_eq!(drop_at_column(&p, 4, 1.5), Some((Some(2), Some(101))));
    // The left half places before G at root level.
    assert_eq!(drop_at_column(&p, 4, 1.25), Some((Some(101), None)));
}

fn event(
    grid: &crate::grid::Grid<'_>,
    state: &mut GridState,
    event: canvas::Event,
    at: Point,
) -> Option<Action> {
    let bounds = iced::Rectangle::new(Point::ORIGIN, iced::Size::new(1000.0, 400.0));
    grid.update(state, &event, bounds, mouse::Cursor::Available(at))
        .and_then(|a| a.into_inner().0)
}

#[test]
fn press_drag_release_on_a_track_title_places_the_track_and_a_click_only_selects() {
    use mouse::{Button, Event::*};
    let p = project(false);
    let grid = crate::grid::test_support::grid(&p);
    let title = |ci: f32| Point::new(GAP + ci * (TRACK_W + GAP), TRACK_H / 2.0);
    let mut state = GridState::default();
    let press = event(
        &grid,
        &mut state,
        canvas::Event::Mouse(ButtonPressed(Button::Left)),
        title(4.5),
    );
    assert_eq!(
        press,
        Some(Action::PressHeader(Box::new(Action::SelectTrack(
            TrackId(4),
            false
        ))))
    );
    let moved = canvas::Event::Mouse(CursorMoved {
        position: title(2.75),
    });
    assert_eq!(event(&grid, &mut state, moved, title(2.75)), None);
    assert!(grid.drag.get().is_some_and(|d| d.dragging()));
    let release = canvas::Event::Mouse(ButtonReleased(Button::Left));
    assert_eq!(
        event(&grid, &mut state, release.clone(), title(2.75)),
        Some(Action::PlaceTrack {
            track: TrackId(4),
            before: Some(TrackId(3)),
            group: Some(TrackId(101)),
        })
    );
    assert!(grid.drag.get().is_none());
    // A press and release in place is a click: nothing to place.
    let _ = event(
        &grid,
        &mut state,
        canvas::Event::Mouse(ButtonPressed(Button::Left)),
        title(4.5),
    );
    assert_eq!(event(&grid, &mut state, release, title(4.5)), None);
    // Presses below the titles never start a drag.
    let _ = event(
        &grid,
        &mut state,
        canvas::Event::Mouse(ButtonPressed(Button::Left)),
        Point::new(20.0, grid.layout.header_h + 4.0),
    );
    assert!(grid.drag.get().is_none());
}

#[test]
fn a_drop_reaches_the_engine_through_the_update_router() {
    use crate::messages::Message;
    let engine = qtable_engine::engine::Engine::new(project(false)).unwrap();
    let mut app = crate::app::App::new(engine, "120".into(), None);
    let _ = crate::update::update(
        &mut app,
        Message::Grid(Action::PlaceTrack {
            track: TrackId(4),
            before: Some(TrackId(3)),
            group: Some(TrackId(101)),
        }),
    );
    assert_eq!(
        app.project()
            .members(TrackId(101))
            .map(|t| t.id)
            .collect::<Vec<_>>(),
        vec![TrackId(2), TrackId(4), TrackId(3)]
    );
    assert_eq!(app.project()[TrackId(4)].route, Route::Group);
}

#[test]
fn group_drags_reject_their_own_subtree_and_enter_other_groups() {
    let p = project(false);
    let cols = track_columns(&p.tracks);
    let xs = x_positions(&cols);
    for x in [xs[1], xs[2] + TRACK_W / 4.0, xs[3] + TRACK_W * 0.75, xs[4]] {
        assert_eq!(
            drop_at(&p.tracks, &cols, &xs, cols[1].key, Point::new(x, 100.)),
            None
        );
    }
    let before = drop_at(&p.tracks, &cols, &xs, cols[1].key, Point::new(-20.0, 100.)).unwrap();
    assert_eq!(
        (before.before, before.group, before.marker),
        (Some(TrackId(1)), None, Marker::Before(0))
    );
    let last = drop_at(&p.tracks, &cols, &xs, cols[1].key, Point::new(1000.0, 100.)).unwrap();
    assert_eq!(
        (last.before, last.group, last.marker),
        (None, None, Marker::Before(cols.len()))
    );
    // A second group can enter the first group before a direct child.
    let mut p = p;
    p.insert_group(Track::new_group(TrackId(102), "Other", 0), &[TrackId(4)]);
    let cols = track_columns(&p.tracks);
    let xs = x_positions(&cols);
    let drop = drop_at(
        &p.tracks,
        &cols,
        &xs,
        cols[4].key,
        Point::new(xs[2] + TRACK_W / 4.0, 100.),
    )
    .unwrap();
    assert_eq!(
        (drop.before, drop.group, drop.marker),
        (Some(TrackId(2)), Some(TrackId(101)), Marker::Before(2))
    );
}

#[test]
fn output_drags_offer_region_boundaries_and_never_into_targets() {
    let p = Project {
        outputs: (201..=203)
            .map(|id| Track::new_output(TrackId(id), "Out", "localhost", 9000))
            .collect(),
        ..Project::default()
    };
    let cols = crate::grid::layout::columns(&p, qtable_engine::model::Region::Outputs);
    let xs = x_positions(&cols);
    for (dragged, boundary) in [(2, 0), (2, 1), (0, 2), (0, 3)] {
        let x = if boundary == cols.len() {
            1000.0
        } else {
            xs[boundary]
        };
        let drop = drop_at(
            &p.outputs,
            &cols,
            &xs,
            cols[dragged].key,
            Point::new(x, 100.),
        )
        .unwrap();
        assert_eq!(drop.marker, Marker::Before(boundary));
        assert_eq!(drop.group, None);
    }
}

#[test]
fn main_selects_without_drag_and_the_fold_button_only_toggles() {
    let main_id = project(false).main.id;
    let main = Column::main(main_id);
    let (action, drag) = press(&main, Point::new(50.0, 10.0), 0.0, false, 0);
    assert_eq!(
        action,
        Action::SelectColumn(crate::selection::ColumnOwner::Track(main_id), None)
    );
    assert!(drag.is_none());
    let p = project(false);
    let cols = track_columns(&p.tracks);
    let xs = x_positions(&cols);
    let (action, drag) = press(
        &cols[1],
        Point::new(xs[1] + TRACK_W - 2.0, 10.0),
        xs[1],
        false,
        0,
    );
    assert_eq!(action, Action::ToggleGroup(TrackId(101)));
    assert!(drag.is_none());
    let (action, drag) = press(&cols[1], Point::new(xs[1] + 10.0, 10.0), xs[1], false, 0);
    assert_eq!(action, Action::SelectGroup(TrackId(101), false));
    assert_eq!(drag.as_ref().unwrap().key, ColumnKey::Track(TrackId(101)));
}

#[test]
fn group_header_press_move_release_publishes_a_block_placement() {
    use mouse::{Button, Event::*};
    let p = project(false);
    let grid = crate::grid::test_support::grid(&p);
    let mut state = GridState::default();
    let at = Point::new(GAP + TRACK_W + GAP + 10.0, 10.0);
    assert_eq!(
        event(
            &grid,
            &mut state,
            canvas::Event::Mouse(ButtonPressed(Button::Left)),
            at
        ),
        Some(Action::PressHeader(Box::new(Action::SelectGroup(
            TrackId(101),
            false
        ))))
    );
    let to = Point::new(900.0, 10.0);
    let _ = event(
        &grid,
        &mut state,
        canvas::Event::Mouse(CursorMoved { position: to }),
        to,
    );
    assert_eq!(
        event(
            &grid,
            &mut state,
            canvas::Event::Mouse(ButtonReleased(Button::Left)),
            to
        ),
        Some(Action::PlaceTrack {
            track: TrackId(101),
            before: None,
            group: None
        })
    );
}

#[test]
fn levitating_output_cursor_reaches_the_last_boundary_in_content_coordinates() {
    let project = Project {
        outputs: (201..=205)
            .map(|id| Track::new_output(TrackId(id), "Out", "localhost", 9000))
            .collect(),
        ..Project::default()
    };
    let columns = crate::grid::layout::columns(&project, qtable_engine::model::Region::Outputs);
    let xs = x_positions(&columns);
    let bounds = Rectangle::new(Point::new(50., 20.), Size::new(530., 400.));
    let at = Point::new(xs[0] + 10., 10.);
    let (_, mut drag) = press(&columns[0], at, xs[0], false, 0);
    let point = Point::new(bounds.x + bounds.width, bounds.y + 10.);
    drag_update(
        (&project.outputs, 0),
        &columns,
        &xs,
        &mut drag,
        &canvas::Event::Mouse(mouse::Event::CursorMoved { position: point }),
        mouse::Cursor::Levitating(point),
        bounds,
    );
    assert_eq!(drag.as_ref().unwrap().cursor.x, bounds.width);
    let action = drag_update(
        (&project.outputs, 0),
        &columns,
        &xs,
        &mut drag,
        &canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Levitating(point),
        bounds,
    )
    .unwrap()
    .into_inner()
    .0;
    assert_eq!(
        action,
        Some(Action::PlaceTrack {
            track: TrackId(201),
            before: None,
            group: None
        })
    );
    assert!(drag.is_none());
}

#[test]
fn layout_changes_resolve_the_dragged_identity_and_removed_keys_cancel() {
    let p = project(false);
    let cols = track_columns(&p.tracks);
    let xs = x_positions(&cols);
    let (_, mut drag) = press(&cols[4], Point::new(xs[4] + 10., 10.), xs[4], false, 0);
    let bounds = Rectangle::with_size(Size::new(1000., 400.));
    let point = Point::new(0., 10.);
    drag_update(
        (&p.tracks, 0),
        &cols,
        &xs,
        &mut drag,
        &canvas::Event::Mouse(mouse::Event::CursorMoved { position: point }),
        mouse::Cursor::Available(point),
        bounds,
    );
    // A fold hides two earlier columns: the original index is now out of bounds.
    let mut changed = p.clone();
    changed
        .tracks
        .iter_mut()
        .find(|t| t.id == TrackId(101))
        .unwrap()
        .fold_state = true;
    let changed_cols = track_columns(&changed.tracks);
    let changed_xs = x_positions(&changed_cols);
    let drop = drop_at(
        &changed.tracks,
        &changed_cols,
        &changed_xs,
        drag.as_ref().unwrap().key,
        Point::new(0., 100.),
    )
    .unwrap();
    assert_eq!(drop.before, Some(TrackId(1)));
    let action = drag_update(
        (&changed.tracks, 0),
        &changed_cols,
        &changed_xs,
        &mut drag,
        &canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(point),
        bounds,
    )
    .unwrap()
    .into_inner()
    .0;
    assert_eq!(
        action,
        Some(Action::PlaceTrack {
            track: TrackId(4),
            before: Some(TrackId(1)),
            group: None
        })
    );

    let (_, mut drag) = press(&cols[4], Point::new(xs[4] + 10., 10.), xs[4], false, 0);
    changed.tracks.retain(|t| t.id != TrackId(4));
    let changed_cols = track_columns(&changed.tracks);
    let changed_xs = x_positions(&changed_cols);
    assert_eq!(
        drop_at(
            &changed.tracks,
            &changed_cols,
            &changed_xs,
            drag.as_ref().unwrap().key,
            Point::new(0., 100.)
        ),
        None
    );
    let action = drag_update(
        (&changed.tracks, 0),
        &changed_cols,
        &changed_xs,
        &mut drag,
        &canvas::Event::Mouse(mouse::Event::CursorMoved { position: point }),
        mouse::Cursor::Available(point),
        bounds,
    )
    .unwrap()
    .into_inner()
    .0;
    assert_eq!(action, None);
    assert!(drag.is_none());
}
