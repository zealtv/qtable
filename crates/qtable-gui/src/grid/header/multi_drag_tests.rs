use super::*;
use crate::{
    app::App,
    grid::{cache::GridState, layout, nested_header_tests, test_support},
    messages::Message,
};
use iced::widget::canvas::Program;
use qtable_engine::{
    edit::{self, ProjectEdit},
    engine::Engine,
    model::{Project, Region, Route},
};

fn app() -> App {
    App::new(
        Engine::new(nested_header_tests::project()).unwrap(),
        "120".into(),
        None,
    )
}
fn send(app: &mut App, action: Action) {
    let _ = crate::update::update(app, Message::Grid(action));
}
fn select(app: &mut App, id: u64, additive: bool) {
    app.selection.command_down = additive;
    let action = if app.project()[TrackId(id)].is_group() {
        Action::SelectGroup(TrackId(id), false)
    } else {
        Action::SelectTrack(TrackId(id), false)
    };
    send(app, Action::PressHeader(Box::new(action)));
    app.selection.command_down = false;
}
fn point(app: &App, id: u64, fraction: f32) -> Point {
    let cols = layout::columns(app.project(), Region::Tracks);
    let xs = layout::x_positions(&cols);
    let i = cols
        .iter()
        .position(|c| c.key == ColumnKey::Track(TrackId(id)))
        .unwrap();
    Point::new(xs[i] + cols[i].width * fraction, cols[i].header_h - 4.)
}
fn event(app: &mut App, state: &mut GridState, event: canvas::Event, at: Point) -> Option<Action> {
    let action = {
        let mut grid = test_support::grid(app.project());
        grid.drag = app.header_gestures.tracks.clone();
        grid.revision = app.project_revision();
        grid.selected_tracks = &app.selection.tracks;
        grid.shift_down = app.selection.shift_down;
        grid.update(
            state,
            &event,
            Rectangle::with_size(Size::new(1200., 400.)),
            mouse::Cursor::Available(at),
        )
        .and_then(|a| a.into_inner().0)
    };
    if let Some(action) = action.clone() {
        send(app, action);
    }
    action
}
fn press_at(app: &mut App, state: &mut GridState, at: Point) {
    event(
        app,
        state,
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        at,
    );
}
fn release(app: &mut App, state: &mut GridState, at: Point) -> Option<Action> {
    event(
        app,
        state,
        canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        at,
    )
}

#[test]
fn selected_header_press_preserves_the_set_drag_moves_it_once_and_click_collapses() {
    let mut app = app();
    select(&mut app, 4, false);
    select(&mut app, 5, true);
    let mut state = GridState::default();
    let start = point(&app, 5, 0.2);
    press_at(&mut app, &mut state, start);
    assert_eq!(app.selection.tracks, [TrackId(4), TrackId(5)]);
    let at = point(&app, 8, 0.25);
    event(
        &mut app,
        &mut state,
        canvas::Event::Mouse(mouse::Event::CursorMoved { position: at }),
        at,
    );
    let drag = app.header_gestures.tracks.get().unwrap();
    let preview = {
        let mut grid = test_support::grid(app.project());
        grid.revision = app.project_revision();
        grid.pending_drop(&drag).unwrap()
    };
    assert_eq!(preview.source, [TrackId(4), TrackId(5)]);
    let revision = app.project_revision();
    assert_eq!(
        release(&mut app, &mut state, at),
        Some(Action::PlaceTracks {
            tracks: vec![TrackId(4), TrackId(5)],
            before: preview.before,
            group: preview.group,
        })
    );
    assert_eq!(app.project_revision(), revision + 1);
    assert!(release(&mut app, &mut state, at).is_none());
    assert_eq!(app.selection.tracks, [TrackId(4), TrackId(5)]);
    for id in [4, 5] {
        assert_eq!(app.project()[TrackId(id)].group, None);
    }
    let start = point(&app, 4, 0.2);
    press_at(&mut app, &mut state, start);
    release(&mut app, &mut state, start);
    assert_eq!(app.selection.tracks, [TrackId(4)]);
}

#[test]
fn shift_range_is_captured_after_selection_and_additive_click_can_remove_a_track() {
    let mut app = app();
    select(&mut app, 4, false);
    app.selection.shift_down = true;
    let at = point(&app, 6, 0.2);
    let mut state = GridState::default();
    press_at(&mut app, &mut state, at);
    assert_eq!(app.selection.tracks, [TrackId(4), TrackId(5), TrackId(6)]);
    assert_eq!(
        app.header_gestures.tracks.get().unwrap().selected,
        app.selection.tracks
    );
    app.selection.shift_down = false;
    release(&mut app, &mut state, at);
    select(&mut app, 5, true);
    assert_eq!(app.selection.tracks, [TrackId(4), TrackId(6)]);
}

#[test]
fn all_batch_previews_agree_with_engine_for_disjoint_mixed_and_folded_selections() {
    for folded in [false, true] {
        let mut p = nested_header_tests::project();
        p[TrackId(3)].fold_state = folded;
        for selected in [vec![4, 5], vec![3, 4, 6], vec![4, 8], vec![3, 7, 8]] {
            let ids: Vec<_> = selected.iter().map(|id| TrackId(*id)).collect();
            let cols = layout::columns(&p, Region::Tracks);
            let xs = layout::x_positions(&cols);
            // The pressed source remains visible; hidden selected roots are deliberately retained.
            let Some(key) = cols.iter().find(|c| ids.contains(&id(c))).map(|c| c.key) else {
                continue;
            };
            for (i, col) in cols.iter().enumerate() {
                for fraction in [0.25, 0.75] {
                    let at = Point::new(xs[i] + col.width * fraction, col.header_h - 4.);
                    if let Some(drop) = drop_selected_at(&p.tracks, &cols, &xs, &ids, key, at) {
                        let mut candidate = p.clone();
                        edit::apply(
                            &mut candidate,
                            ProjectEdit::PlaceTracks {
                                tracks: ids.clone(),
                                before: drop.before,
                                group: drop.group,
                            },
                        )
                        .unwrap();
                        assert_ne!(candidate, p);
                        candidate.validate().unwrap();
                    }
                }
            }
        }
    }
}

#[test]
fn capture_cases_use_production_paths_and_preserve_hidden_selection_and_subtrees() {
    for mode in [
        "multi-into",
        "multi-out",
        "multi-mixed",
        "multi-hidden",
        "multi-invalid",
    ] {
        let mut preview = app();
        capture_session(&mut preview, mode);
        let drag = preview.header_gestures.tracks.get().unwrap();
        let mut grid = test_support::grid(preview.project());
        grid.revision = preview.project_revision();
        let plan = grid.pending_drop(&drag);
        let mut dropped = app();
        let before = dropped.project().clone();
        capture_session(&mut dropped, &format!("{mode}-drop"));
        dropped.project().validate().unwrap();
        if mode == "multi-invalid" {
            assert!(plan.is_none());
            assert_eq!(dropped.project(), &before);
            continue;
        }
        let plan = plan.unwrap();
        let mut expected = preview.project().clone();
        edit::apply(
            &mut expected,
            ProjectEdit::PlaceTracks {
                tracks: drag.selected,
                before: plan.before,
                group: plan.group,
            },
        )
        .unwrap();
        assert_eq!(dropped.project(), &expected);
        for old in before.clip_tracks() {
            assert_eq!(old.slots, dropped.project()[old.id].slots);
        }
        match mode {
            "multi-into" => {
                for id in [7, 8] {
                    assert_eq!(expected[TrackId(id)].group, Some(TrackId(3)));
                    assert_eq!(expected[TrackId(id)].route, Route::Group);
                }
            }
            "multi-out" => {
                for id in [4, 5] {
                    assert_eq!(expected[TrackId(id)].group, None);
                }
            }
            "multi-mixed" => {
                assert!(expected.track(TrackId(2)).is_none());
                assert_eq!(expected[TrackId(4)], before[TrackId(4)]);
            }
            "multi-hidden" => {
                assert!(expected[TrackId(3)].fold_state);
                assert_eq!(dropped.selection.tracks, [TrackId(4), TrackId(8)]);
                assert_eq!(expected[TrackId(4)].group, Some(TrackId(2)));
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn batch_self_descendant_and_noop_drops_have_no_marker_or_command() {
    let mut app = app();
    capture_session(&mut app, "multi-invalid");
    let before: Project = app.project().clone();
    let mut drag = app.header_gestures.tracks.get().unwrap();
    let layout = app.grid_layout.get(app.project(), app.project_revision());
    let at = drag.cursor;
    assert!(
        release_action(
            &app.project().tracks,
            &layout.columns,
            &layout.xs,
            &mut drag,
            at,
            Rectangle::with_size(Size::new(1200., 400.))
        )
        .is_none()
    );
    assert_eq!(app.project(), &before);
    // A batch of adjacent root tracks placed just after itself changes nothing.
    let p = Project {
        tracks: (1..=3).map(|n| Track::new(TrackId(n), "T", 0)).collect(),
        ..Default::default()
    };
    let cols = layout::columns(&p, Region::Tracks);
    let xs = layout::x_positions(&cols);
    let at = Point::new(xs[2] + 10., cols[2].header_h - 4.);
    assert!(
        drop_selected_at(
            &p.tracks,
            &cols,
            &xs,
            &[TrackId(1), TrackId(2)],
            ColumnKey::Track(TrackId(2)),
            at
        )
        .is_none()
    );
}

#[test]
fn batch_folded_halves_keep_the_fold_and_choose_parent_levels() {
    let mut p = nested_header_tests::project();
    p[TrackId(3)].fold_state = true;
    let cols = layout::columns(&p, Region::Tracks);
    let xs = layout::x_positions(&cols);
    let ci = cols.iter().position(|c| id(c) == TrackId(3)).unwrap();
    for (fraction, before, group) in [(0.25, 3, 2), (0.75, 4, 3)] {
        let at = Point::new(xs[ci] + cols[ci].width * fraction, cols[ci].header_h - 4.);
        let drop = drop_selected_at(
            &p.tracks,
            &cols,
            &xs,
            &[TrackId(8), TrackId(7)],
            ColumnKey::Track(TrackId(8)),
            at,
        )
        .unwrap();
        assert_eq!(
            (drop.before, drop.group),
            (Some(TrackId(before)), Some(TrackId(group)))
        );
        let mut candidate = p.clone();
        edit::apply(
            &mut candidate,
            ProjectEdit::PlaceTracks {
                tracks: vec![TrackId(8), TrackId(7)],
                before: drop.before,
                group: drop.group,
            },
        )
        .unwrap();
        assert!(candidate[TrackId(3)].fold_state);
        let ids = candidate.tracks.iter().map(|t| t.id).collect::<Vec<_>>();
        let start = ids.iter().position(|id| *id == TrackId(7)).unwrap();
        assert_eq!(&ids[start..start + 2], &[TrackId(7), TrackId(8)]);
    }
}

#[test]
fn batch_gesture_cancels_on_revision_change_without_replaying_its_sources() {
    let mut app = app();
    select(&mut app, 4, false);
    select(&mut app, 5, true);
    let mut state = GridState::default();
    let start = point(&app, 5, 0.2);
    press_at(&mut app, &mut state, start);
    send(&mut app, Action::ToggleGroup(TrackId(3)));
    let before = app.project().clone();
    let at = point(&app, 8, 0.25);
    assert!(release(&mut app, &mut state, at).is_none());
    assert!(app.header_gestures.tracks.get().is_none());
    assert_eq!(app.project(), &before);
    assert_eq!(app.selection.tracks, [TrackId(4), TrackId(5)]);
}
