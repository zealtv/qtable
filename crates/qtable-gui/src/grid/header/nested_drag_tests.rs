//! The nested-drag boundary rule, including engine agreement and cancelled gestures.
use super::*;
use crate::grid::{layout, nested_header_tests, test_support};
use iced::widget::canvas::Program;
use qtable_engine::{
    edit::{ProjectEdit, apply},
    model::{Project, Region, Route},
};

fn plan(p: &Project, source: u64, target: u64, fraction: f32) -> Option<Drop> {
    let cols = layout::columns(p, Region::Tracks);
    let xs = layout::x_positions(&cols);
    let ci = cols
        .iter()
        .position(|c| c.key == ColumnKey::Track(TrackId(target)))?;
    drop_at(
        &p.tracks,
        &cols,
        &xs,
        ColumnKey::Track(TrackId(source)),
        Point::new(xs[ci] + fraction * cols[ci].width, cols[ci].header_h - 3.),
    )
}

#[test]
fn acceptance_capture_commands_wrap_and_ungroup_without_losing_clips() {
    use crate::app::App;
    use qtable_engine::engine::Engine;
    for scenario in [
        "nested-wrap",
        "single-wrap",
        "inner-menu",
        "inner-ungroup",
        "all-ungroup",
    ] {
        let mut p = nested_header_tests::project();
        if scenario == "nested-wrap" {
            apply(&mut p, ProjectEdit::Ungroup { group: TrackId(2) }).unwrap();
        }
        let original = p.clone();
        let mut app = App::new(Engine::new(p).unwrap(), "120".into(), None);
        capture_session(&mut app, scenario);
        app.project().validate().unwrap();
        for track in original.clip_tracks() {
            assert_eq!(app.project()[track.id].slots, track.slots, "{scenario}");
        }
        match scenario {
            "nested-wrap" => {
                let parent = app.project()[TrackId(3)].group.unwrap();
                assert_ne!(parent, TrackId(1));
                assert_eq!(app.project()[TrackId(6)].group, Some(parent));
                assert_eq!(app.project()[parent].group, Some(TrackId(1)));
            }
            "single-wrap" => assert!(app.project()[TrackId(8)].group.is_some()),
            "inner-menu" => {
                assert!(matches!(
                    app.overlay,
                    Some(crate::overlay::Overlay::Context(_))
                ));
                assert_eq!(app.project(), &original);
                assert_eq!(app.selection.tracks, [TrackId(3)]);
            }
            "inner-ungroup" => {
                assert!(app.project().track(TrackId(3)).is_none());
                assert_eq!(app.project()[TrackId(4)].group, Some(TrackId(2)));
                assert_eq!(app.selection.tracks, [TrackId(4)]);
            }
            "all-ungroup" => {
                assert_eq!(app.project().group_tracks().count(), 0);
                assert!(app.project().tracks.iter().all(|t| t.group.is_none()));
            }
            _ => unreachable!(),
        }
    }
}

fn commit(p: &mut Project, source: u64, drop: &Drop) {
    assert_eq!(drop.before, p.tracks.get(drop.boundary).map(|t| t.id));
    assert_eq!(drop.source, p.subtree_ids(TrackId(source)).unwrap());
    let descendants: Vec<_> = drop
        .source
        .iter()
        .skip(1)
        .map(|id| p[*id].clone())
        .collect();
    apply(
        p,
        ProjectEdit::PlaceTrack {
            track: TrackId(source),
            before: drop.before,
            group: drop.group,
        },
    )
    .unwrap();
    assert_eq!(p[TrackId(source)].group, drop.group);
    for track in descendants {
        assert_eq!(p[track.id], track);
    }
    p.validate().unwrap();
}

#[test]
fn individual_track_leaves_three_levels_then_reenters_at_each_first_child() {
    // Outer( Middle( Inner(a,b), c), d), e, with clip gaps.
    let mut p = nested_header_tests::project();
    p[TrackId(4)].route = Route::Group;
    let original = p[TrackId(4)].slots.clone();
    for (target, parent) in [
        (6, Some(2)),
        (7, Some(1)),
        (8, None),
        (2, Some(1)),
        (3, Some(2)),
        (5, Some(3)),
    ] {
        let drop = plan(&p, 4, target, 0.1).unwrap();
        assert_eq!(drop.group, parent.map(TrackId));
        commit(&mut p, 4, &drop);
        assert_eq!(p[TrackId(4)].slots, original);
        assert_eq!(
            p[TrackId(4)].route,
            if parent.is_some() {
                Route::Group
            } else {
                Route::single(p.main.id)
            }
        );
    }
}

#[test]
fn every_preview_on_both_sides_of_every_column_is_an_accepted_engine_placement() {
    for folded in [false, true] {
        let mut p = nested_header_tests::project();
        p[TrackId(3)].fold_state = folded;
        let cols = layout::columns(&p, Region::Tracks);
        for source in &cols {
            let ColumnKey::Track(s) = source.key;
            for target in &cols {
                let ColumnKey::Track(t) = target.key;
                for fraction in [0.0, 0.25, 0.5, 0.75, 0.99] {
                    if let Some(drop) = plan(&p, s.0, t.0, fraction) {
                        commit(&mut p.clone(), s.0, &drop);
                    }
                }
            }
        }
    }
}

#[test]
fn recorded_whole_group_move_preserves_descendants_and_root_route_changes_once() {
    let mut p = nested_header_tests::project();
    // A separate destination wrapper around e; move Inner(a,b) before e.
    p.insert_group(
        Track::new_group(TrackId(90), "Destination", p.scenes.len()),
        &[TrackId(8)],
    );
    let drop = plan(&p, 3, 8, 0.25).unwrap();
    assert_eq!(drop.source, [TrackId(3), TrackId(4), TrackId(5)]);
    assert_eq!(
        (drop.before, drop.group),
        (Some(TrackId(8)), Some(TrackId(90)))
    );
    commit(&mut p, 3, &drop);
    assert_eq!(p[TrackId(3)].route, Route::Group);
    assert_eq!(
        p.subtree_ids(TrackId(90)).unwrap(),
        [90, 3, 4, 5, 8].map(TrackId)
    );
}

#[test]
fn shared_ends_exit_all_closed_groups() {
    let p = nested_header_tests::project();
    let drop = plan(&p, 8, 5, 0.75).unwrap();
    assert_eq!((drop.boundary, drop.group), (5, Some(TrackId(2))));
    let drop = plan(&p, 4, 7, 0.75).unwrap();
    assert_eq!((drop.boundary, drop.group), (7, None));
}

#[test]
fn folded_header_halves_place_subtrees_outside_or_first_inside_without_unfolding() {
    // Exercise both a nested and root target, moving leaves and whole groups.
    for target in [1, 3] {
        for source in [8, 90] {
            let mut p = nested_header_tests::project();
            if source == 90 {
                p.insert_group(
                    Track::new_group(TrackId(90), "Source", p.scenes.len()),
                    &[TrackId(8)],
                );
            }
            p[TrackId(target)].fold_state = true;
            let cols = layout::columns(&p, Region::Tracks);
            let ci = cols.iter().position(|c| id(c) == TrackId(target)).unwrap();
            let start = p
                .tracks
                .iter()
                .position(|t| t.id == TrackId(target))
                .unwrap();
            for fraction in [0., 0.25, 0.499, 0.5, 0.75, 0.999] {
                let drop = plan(&p, source, target, fraction).unwrap();
                let inside = fraction >= 0.5;
                let parent = if inside {
                    Some(TrackId(target))
                } else {
                    p[TrackId(target)].group
                };
                assert_eq!(drop.boundary, start + usize::from(inside));
                assert_eq!(drop.group, parent);
                assert_eq!(
                    drop.marker,
                    if inside {
                        Marker::Into(ci)
                    } else {
                        Marker::Before(ci)
                    }
                );
                assert_eq!(
                    drop.depth,
                    if inside {
                        cols[ci].ancestors.len() + 1
                    } else {
                        cols[ci].ancestors.len()
                    }
                );
                let mut result = p.clone();
                commit(&mut result, source, &drop);
                assert!(result[TrackId(target)].fold_state);
                let at = result
                    .tracks
                    .iter()
                    .position(|t| t.id == TrackId(target))
                    .unwrap();
                if inside {
                    assert_eq!(result.tracks[at + 1].id, TrackId(source));
                } else {
                    let source_end = result.subtree_ids(TrackId(source)).unwrap().len();
                    assert_eq!(result.tracks[at - source_end].id, TrackId(source));
                }
            }
        }
    }
}

#[test]
fn folded_halves_apply_only_to_titles_and_reject_self_or_ancestor_sources() {
    let mut p = nested_header_tests::project();
    p[TrackId(3)].fold_state = true;
    for fraction in [0.25, 0.75] {
        for source in [1, 2, 3] {
            assert!(plan(&p, source, 3, fraction).is_none());
        }
    }
    let cols = layout::columns(&p, Region::Tracks);
    let xs = layout::x_positions(&cols);
    let ci = cols.iter().position(|c| id(c) == TrackId(3)).unwrap();
    for fraction in [0.25, 0.75] {
        assert!(
            drop_at(
                &p.tracks,
                &cols,
                &xs,
                ColumnKey::Track(TrackId(8)),
                Point::new(xs[ci] + cols[ci].width * fraction, 0.),
            )
            .is_none()
        );
    }
    let body = drop_at(
        &p.tracks,
        &cols,
        &xs,
        ColumnKey::Track(TrackId(8)),
        Point::new(xs[ci] + cols[ci].width * 0.75, cols[ci].header_h + 20.),
    )
    .unwrap();
    assert_eq!(
        (body.before, body.group),
        (Some(TrackId(6)), Some(TrackId(2)))
    );
}

#[test]
fn self_descendants_and_ancestor_bands_never_offer_a_target() {
    let p = nested_header_tests::project();
    for source in [1, 2, 3] {
        for target in p.subtree_ids(TrackId(source)).unwrap() {
            assert!(plan(&p, source, target.0, 0.25).is_none());
        }
    }
    let cols = layout::columns(&p, Region::Tracks);
    let xs = layout::x_positions(&cols);
    assert!(
        drop_at(
            &p.tracks,
            &cols,
            &xs,
            ColumnKey::Track(TrackId(8)),
            Point::new(xs[4] + 10., 0.)
        )
        .is_none()
    );
}

fn gesture(
    grid: &Grid<'_>,
    state: &mut cache::GridState,
    event: canvas::Event,
    cursor: mouse::Cursor,
) -> Option<Action> {
    grid.update(
        state,
        &event,
        Rectangle::with_size(Size::new(1200., 400.)),
        cursor,
    )
    .and_then(|a| a.into_inner().0)
}
fn active(grid: &Grid<'_>) -> cache::GridState {
    let mut state = cache::GridState::default();
    let start = Point::new(grid.layout.xs[7] + 10., grid.layout.header_h - 5.);
    gesture(
        grid,
        &mut state,
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        mouse::Cursor::Available(start),
    );
    let at = Point::new(grid.layout.xs[4] + 10., grid.layout.header_h - 5.);
    gesture(
        grid,
        &mut state,
        canvas::Event::Mouse(mouse::Event::CursorMoved { position: at }),
        mouse::Cursor::Available(at),
    );
    state
}

#[test]
fn preview_and_release_publish_exactly_the_same_placement_once() {
    let p = nested_header_tests::project();
    let grid = test_support::grid(&p);
    let mut state = active(&grid);
    let drag = grid.drag.get().unwrap();
    let preview = grid.pending_drop(&drag).unwrap();
    let event = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    let action = gesture(
        &grid,
        &mut state,
        event.clone(),
        mouse::Cursor::Available(drag.cursor),
    );
    assert_eq!(
        action,
        Some(Action::PlaceTrack {
            track: TrackId(8),
            before: preview.before,
            group: preview.group
        })
    );
    assert!(
        gesture(
            &grid,
            &mut state,
            event,
            mouse::Cursor::Available(drag.cursor)
        )
        .is_none()
    );
    let mut app = crate::app::App::new(
        qtable_engine::engine::Engine::new(p.clone()).unwrap(),
        "120".into(),
        None,
    );
    let revision = app.project_revision();
    let _ = crate::update::update(&mut app, crate::messages::Message::Grid(action.unwrap()));
    assert_eq!(app.project_revision(), revision + 1);
    let mut expected = p.clone();
    commit(&mut expected, 8, &preview);
    assert_eq!(app.project(), &expected);
}

#[test]
fn cancellation_stale_revision_and_invalid_release_mutate_nothing() {
    use iced::keyboard::{Event, Key, key::Named};
    let p = nested_header_tests::project();
    let engine = qtable_engine::engine::Engine::new(p.clone()).unwrap();
    let mut app = crate::app::App::new(engine, "120".into(), None);
    let revision = app.project_revision();
    let grid = test_support::grid(&p);
    for cancel in [
        canvas::Event::Keyboard(Event::KeyPressed {
            key: Key::Named(Named::Escape),
            modified_key: Key::Named(Named::Escape),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        }),
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        canvas::Event::Window(iced::window::Event::Unfocused),
    ] {
        let mut state = active(&grid);
        assert!(gesture(&grid, &mut state, cancel, mouse::Cursor::Unavailable).is_none());
        assert!(grid.drag.get().is_none());
    }
    for at in [
        mouse::Cursor::Unavailable,
        mouse::Cursor::Available(Point::new(1300., 20.)),
        mouse::Cursor::Available(Point::new(grid.layout.xs[4] + 10., 0.)),
    ] {
        let mut state = active(&grid);
        assert!(
            gesture(
                &grid,
                &mut state,
                canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                at
            )
            .is_none()
        );
        assert!(grid.drag.get().is_none());
    }
    let mut state = active(&grid);
    let mut changed = test_support::grid(&p);
    changed.revision = grid.revision + 1;
    changed.drag = grid.drag.clone();
    assert!(changed.pending_drop(&grid.drag.get().unwrap()).is_none());
    let action = gesture(
        &changed,
        &mut state,
        canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        mouse::Cursor::Available(Point::new(10., 20.)),
    );
    assert!(action.is_none());
    assert!(grid.drag.get().is_none());
    // Route every emitted action (there are none) through the real update router.
    if let Some(action) = action {
        let _ = crate::update::update(&mut app, crate::messages::Message::Grid(action));
    }
    assert_eq!(app.project(), &p);
    assert_eq!(app.project_revision(), revision);
}

#[test]
#[ignore = "writes the native drag fixture into the supplied directory"]
fn write_native_drag_fixture() {
    let dir = std::env::var("QTABLE_NESTED_CAPTURE_DIR").expect("artifact directory");
    let mut p = nested_header_tests::project();
    p.insert_group(
        Track::new_group(TrackId(90), "Destination", p.scenes.len()),
        &[TrackId(8)],
    );
    apply(
        &mut p,
        ProjectEdit::PlaceTrack {
            track: TrackId(90),
            before: Some(TrackId(1)),
            group: None,
        },
    )
    .unwrap();
    qtable_engine::file::save(std::path::Path::new(&dir).join("drag.ron"), &p).unwrap();
    p[TrackId(3)].fold_state = true;
    qtable_engine::file::save(std::path::Path::new(&dir).join("folded.ron"), &p).unwrap();
}

#[test]
fn coincident_group_ends_select_root_and_folded_first_member_precedes_hidden_tracks() {
    let mut p = nested_header_tests::project();
    p[TrackId(6)].group = Some(TrackId(3));
    p[TrackId(7)].group = Some(TrackId(3));
    let drop = plan(&p, 4, 7, 0.9).unwrap();
    assert_eq!((drop.boundary, drop.group, drop.depth), (7, None, 0));
    commit(&mut p, 4, &drop);

    let mut p = nested_header_tests::project();
    let mut target = Track::new_group(TrackId(90), "Folded", p.scenes.len());
    target.fold_state = true;
    p.insert_group(target, &[TrackId(8)]);
    apply(
        &mut p,
        ProjectEdit::PlaceTrack {
            track: TrackId(90),
            before: Some(TrackId(1)),
            group: None,
        },
    )
    .unwrap();
    let drop = plan(&p, 1, 90, 0.5).unwrap();
    assert_eq!(
        (drop.before, drop.group),
        (Some(TrackId(8)), Some(TrackId(90)))
    );
    commit(&mut p, 1, &drop);
    assert!(p[TrackId(90)].fold_state);
}

#[test]
fn leaving_the_canvas_removes_the_preview_without_committing_the_last_hover() {
    let p = nested_header_tests::project();
    let grid = test_support::grid(&p);
    let mut state = active(&grid);
    assert!(grid.pending_drop(&grid.drag.get().unwrap()).is_some());
    let outside = Point::new(1300., 100.);
    gesture(
        &grid,
        &mut state,
        canvas::Event::Mouse(mouse::Event::CursorMoved { position: outside }),
        mouse::Cursor::Available(outside),
    );
    assert!(grid.pending_drop(&grid.drag.get().unwrap()).is_none());
    assert!(
        gesture(
            &grid,
            &mut state,
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            mouse::Cursor::Available(outside)
        )
        .is_none()
    );
    assert!(grid.drag.get().is_none());
}

#[test]
fn capture_scenarios_hold_real_gestures_or_dispatch_one_placement() {
    for (scenario, source, parent, before) in [
        ("drag-inner", 8, Some(3), Some(5)),
        ("drag-first", 8, Some(3), Some(4)),
        ("drag-outside", 4, Some(2), Some(6)),
        ("drag-root", 4, None, Some(1)),
        ("drag-group", 3, Some(90), Some(8)),
        ("drag-folded-left", 8, Some(2), Some(3)),
        ("drag-folded-right", 8, Some(3), Some(4)),
    ] {
        let mut p = nested_header_tests::project();
        p.insert_group(
            Track::new_group(TrackId(90), "Destination", p.scenes.len()),
            &[TrackId(8)],
        );
        apply(
            &mut p,
            ProjectEdit::PlaceTrack {
                track: TrackId(90),
                before: Some(TrackId(1)),
                group: None,
            },
        )
        .unwrap();
        if scenario.starts_with("drag-folded-") {
            p[TrackId(3)].fold_state = true;
        }
        for dropped in [false, true] {
            let mut app = crate::app::App::new(
                qtable_engine::engine::Engine::new(p.clone()).unwrap(),
                "120".into(),
                None,
            );
            let revision = app.project_revision();
            capture_session(
                &mut app,
                &format!("{scenario}{}", if dropped { "-drop" } else { "" }),
            );
            if dropped {
                assert!(app.header_gestures.tracks.get().is_none());
                assert_eq!(app.project_revision(), revision + 1);
                assert_eq!(app.project()[TrackId(source)].group, parent.map(TrackId));
                assert_eq!(
                    app.project()[TrackId(3)].fold_state,
                    p[TrackId(3)].fold_state
                );
                let index = app
                    .project()
                    .tracks
                    .iter()
                    .position(|t| t.id == TrackId(source))
                    .unwrap();
                let end = index + app.project().subtree_ids(TrackId(source)).unwrap().len();
                assert_eq!(
                    app.project().tracks.get(end).map(|t| t.id),
                    before.map(TrackId)
                );
            } else {
                assert_eq!(app.project(), &p);
                assert_eq!(app.project_revision(), revision);
                let layout = app.grid_layout.get(app.project(), revision);
                let drag = app.header_gestures.tracks.get().unwrap();
                let preview = drop_at(
                    &p.tracks,
                    &layout.columns,
                    &layout.xs,
                    drag.key,
                    drag.cursor,
                )
                .unwrap();
                assert_eq!(
                    (preview.group, preview.before),
                    (parent.map(TrackId), before.map(TrackId))
                );
            }
        }
    }
}

#[test]
fn pinned_header_and_slot_canvas_share_one_preview_and_one_release() {
    let p = nested_header_tests::project();
    let header = test_support::grid(&p);
    let body = header.clone();
    let mut hs = cache::GridState::default();
    let mut bs = cache::GridState::default();
    let header_bounds = Rectangle::with_size(Size::new(1200., header.layout.header_h));
    let body_bounds = Rectangle::with_size(Size::new(1200., 400.));
    let start = Point::new(header.layout.xs[7] + 10., header.layout.header_h - 4.);
    let _ = header.update(
        &mut hs,
        &canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        header_bounds,
        mouse::Cursor::Available(start),
    );
    let at = Point::new(header.layout.xs[4] + 10., 150.);
    let _ = header.update(
        &mut hs,
        &canvas::Event::Mouse(mouse::Event::CursorMoved { position: at }),
        header_bounds,
        mouse::Cursor::Levitating(at),
    );
    let drag = header.drag.get().unwrap();
    assert!(drag.inside);
    let preview = header.pending_drop(&drag).unwrap();
    assert_eq!(
        body.pending_drop(&body.drag.get().unwrap()),
        Some(preview.clone())
    );
    let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    let action = body
        .update(&mut bs, &release, body_bounds, mouse::Cursor::Available(at))
        .and_then(|a| a.into_inner().0);
    assert_eq!(
        action,
        Some(Action::PlaceTrack {
            track: TrackId(8),
            before: preview.before,
            group: preview.group
        })
    );
    assert!(
        header
            .update(
                &mut hs,
                &release,
                header_bounds,
                mouse::Cursor::Levitating(at)
            )
            .and_then(|a| a.into_inner().0)
            .is_none()
    );
    assert!(header.drag.get().is_none());
}

#[test]
fn composed_app_drag_keeps_its_preview_below_the_pinned_header_and_places_once() {
    use iced::advanced::{Layout as WidgetLayout, Shell, layout as widget_layout, widget::Tree};
    fn pointer(app: &crate::app::App, phase: usize) -> Vec<crate::messages::Message> {
        let renderer = crate::grid::test_renderer();
        let size = Size::new(1280., 800.);
        let mut widget = crate::view::view(app);
        let mut tree = Tree::new(widget.as_widget());
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &widget_layout::Limits::new(Size::ZERO, size),
        );
        let tracks = crate::grid::last_row_tests::tracks_layout(&node).bounds();
        let rows = crate::grid::last_row_tests::rows_layout(&node).bounds();
        let layout = app.grid_layout.get(app.project(), app.project_revision());
        let ci = if phase == 0 { 7 } else { 4 };
        let at = Point::new(
            tracks.x + layout.xs[ci] + 10.,
            rows.y + layout.header_h + if phase == 0 { -4. } else { 45. },
        );
        let event = iced::Event::Mouse(match phase {
            0 => mouse::Event::ButtonPressed(mouse::Button::Left),
            1 => mouse::Event::CursorMoved { position: at },
            _ => mouse::Event::ButtonReleased(mouse::Button::Left),
        });
        let mut messages = Vec::new();
        widget.as_widget_mut().update(
            &mut tree,
            &event,
            WidgetLayout::new(&node),
            mouse::Cursor::Available(at),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
        messages
    }
    let p = nested_header_tests::project();
    let mut app = crate::app::App::new(
        qtable_engine::engine::Engine::new(p).unwrap(),
        "120".into(),
        None,
    );
    let revision = app.project_revision();
    for phase in 0..3 {
        let messages = pointer(&app, phase);
        if phase == 2 {
            assert_eq!(
                messages
                    .iter()
                    .filter(|m| matches!(
                        m,
                        crate::messages::Message::Grid(Action::PlaceTrack { .. })
                    ))
                    .count(),
                1
            );
        }
        for message in messages {
            let _ = crate::update::update(&mut app, message);
        }
        if phase == 1 {
            let drag = app.header_gestures.tracks.get().unwrap();
            assert!(drag.inside);
            let layout = app.grid_layout.get(app.project(), app.project_revision());
            assert!(
                drop_at(
                    &app.project().tracks,
                    &layout.columns,
                    &layout.xs,
                    drag.key,
                    drag.cursor
                )
                .is_some()
            );
        }
    }
    assert_eq!(app.project_revision(), revision + 1);
    assert_eq!(app.project()[TrackId(8)].group, Some(TrackId(3)));
    assert!(app.header_gestures.tracks.get().is_none());
}

#[test]
fn insertion_line_endpoint_identifies_destination_depth_and_folded_halves() {
    let mut p = nested_header_tests::project();
    for (source, target, fraction, depth) in [
        (8, 2, 0.25, 1),
        (8, 3, 0.25, 2),
        (8, 5, 0.25, 3),
        (4, 7, 0.75, 0),
    ] {
        let drop = plan(&p, source, target, fraction).unwrap();
        let cols = layout::columns(&p, Region::Tracks);
        let xs = layout::x_positions(&cols);
        let line = placement::insertion_line(&cols, &xs, &drop, 600.);
        assert_eq!(
            line.y,
            depth as f32 * crate::style::detail::GROUP_HEADER_STEP
        );
        assert_eq!(line.y + line.height, 600.);
        assert_eq!(line.width, 2.);
        let Marker::Before(boundary) = drop.marker else {
            panic!("expanded boundary")
        };
        let x = xs
            .get(boundary)
            .copied()
            .unwrap_or_else(|| xs.last().unwrap() + cols.last().unwrap().width + GAP);
        assert_eq!(line.center_x(), x - GAP / 2.);
    }
    p[TrackId(3)].fold_state = true;
    let cols = layout::columns(&p, Region::Tracks);
    let xs = layout::x_positions(&cols);
    let left = plan(&p, 8, 3, 0.25).unwrap();
    let right = plan(&p, 8, 3, 0.75).unwrap();
    let a = placement::insertion_line(&cols, &xs, &left, 600.);
    let b = placement::insertion_line(&cols, &xs, &right, 600.);
    assert_eq!(a.y, 2. * crate::style::detail::GROUP_HEADER_STEP);
    assert_eq!(b.y, 3. * crate::style::detail::GROUP_HEADER_STEP);
    let ci = cols
        .iter()
        .position(|c| c.key == ColumnKey::Track(TrackId(3)))
        .unwrap();
    assert_eq!(a.center_x(), xs[ci] - GAP / 2.);
    assert_eq!(b.center_x(), xs[ci] + cols[ci].width + GAP / 2.);
}
