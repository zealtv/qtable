//! Explicit headers and recursive slot effects remain separate through nested operations.
use super::*;
use crate::{
    app::App,
    messages::{Message, OutputMessage},
    selection::ColumnOwner,
};
use qtable_engine::{engine::Engine, model::Project};

fn app() -> App {
    App::new(
        Engine::new(super::nested_header_tests::project()).unwrap(),
        "120".into(),
        None,
    )
}
fn act(app: &mut App, action: Action) {
    let _ = crate::update::update(app, Message::Grid(action));
}
fn selected(app: &App) -> Vec<u64> {
    app.selection.tracks.iter().map(|id| id.0).collect()
}

#[test]
fn headers_ranges_and_slots_keep_explicit_roots_separate_from_leaves() {
    let mut app = app();
    act(&mut app, Action::SelectGroup(TrackId(1), false));
    assert_eq!(selected(&app), [1]);
    assert_eq!(
        crate::header_actions::grouping_plan(&app).unwrap().roots,
        [TrackId(1)]
    );
    act(
        &mut app,
        Action::SelectColumn(ColumnOwner::Group(TrackId(1)), Some(2)),
    );
    assert_eq!(selected(&app), [4, 5, 6, 7]);
    assert_eq!(crate::header_actions::selected_group(&app), None);
    // Even all descendant leaves never imply a selected parent.
    for id in [1, 2, 3] {
        assert!(!crate::selection::SelectionState::group_selected(
            &app.selection.tracks,
            &app.project().tracks,
            TrackId(id)
        ));
    }
    act(&mut app, Action::SelectGroup(TrackId(3), false));
    act(&mut app, Action::ToggleGroup(TrackId(3)));
    act(&mut app, Action::SelectTrack(TrackId(7), true));
    assert_eq!(
        selected(&app),
        [3, 6, 7],
        "range selects visible headers, not hidden descendants"
    );
    assert!(
        !crate::header_actions::can_group(&app),
        "mixed-parent partial range rejects"
    );
    act(&mut app, Action::SelectGroup(TrackId(2), false));
    act(&mut app, Action::SelectTrack(TrackId(7), true));
    assert_eq!(
        crate::header_actions::grouping_plan(&app).unwrap().roots,
        [TrackId(2), TrackId(7)]
    );
}

#[test]
fn ungroup_each_explicit_group_and_select_only_first_surviving_promoted_header() {
    let mut app = app();
    app.selection.tracks = vec![TrackId(2), TrackId(3)];
    let _ = crate::update::update(
        &mut app,
        Message::Outputs(OutputMessage::Ungroup(TrackId(2))),
    );
    assert!(app.project().track(TrackId(2)).is_none());
    assert!(app.project().track(TrackId(3)).is_none());
    assert_eq!(app.project()[TrackId(4)].group, Some(TrackId(1)));
    assert_eq!(selected(&app), [4]);
    let mut app = self::app();
    act(&mut app, Action::SelectGroup(TrackId(1), false));
    let _ = crate::update::update(
        &mut app,
        Message::Outputs(OutputMessage::Ungroup(TrackId(1))),
    );
    assert_eq!(selected(&app), [2]);
    assert_eq!(app.selection.owner, Some(ColumnOwner::Group(TrackId(2))));
}

/// V37-R1: a GUI selection must use the engine's atomic UngroupTracks operation.
#[test]
fn review_v37_r1_gui_ungroup_matches_the_atomic_batch_with_cancelling_delays() {
    use qtable_engine::{edit::ProjectEdit, model::Route};
    let mut project = super::nested_header_tests::project();
    // Outer(-600) -> Middle(+600) -> Inner(0) -> leaf(+600).
    // The initial and fully ungrouped sets are valid. Removing only Outer
    // temporarily makes the leaf's combined delay +1200 ms and must fail.
    project[TrackId(1)].delay_ms = -600.0;
    project[TrackId(2)].delay_ms = 600.0;
    project[TrackId(4)].delay_ms = 600.0;
    for track in &mut project.tracks {
        track.route = if track.group.is_some() {
            Route::Group
        } else {
            Route::single(TrackId(201))
        };
    }
    // Keep a live receiver route when Middle is promoted out of Outer.
    project[TrackId(2)].route = Route::single(TrackId(201));
    project.validate().unwrap();
    let groups = vec![TrackId(1), TrackId(2)];
    let mut expected = project.clone();
    qtable_engine::edit::apply(
        &mut expected,
        ProjectEdit::UngroupTracks {
            groups: groups.clone(),
        },
    )
    .unwrap();
    // Prove this is the intermediate-validation case, not an invalid final edit.
    let mut intermediate = project.clone();
    assert!(
        qtable_engine::edit::apply(
            &mut intermediate,
            ProjectEdit::Ungroup { group: TrackId(1) },
        )
        .is_err()
    );
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    app.selection.tracks = groups;
    let revision = app.project_revision();
    let _ = crate::update::update(
        &mut app,
        Message::Outputs(OutputMessage::Ungroup(TrackId(1))),
    );
    // The current GUI leaves Outer behind after rejecting its edit, then
    // successfully removes Middle. The atomic command removes both.
    assert!(app.project().track(TrackId(2)).is_none());
    assert!(
        app.project().track(TrackId(1)).is_none(),
        "V37-R1: selected Outer survived a partially applied GUI Ungroup"
    );
    assert_eq!(app.project(), &expected);
    assert_eq!(app.project_revision(), revision + 1);
}

#[test]
fn folded_delete_uses_explicit_subtree_and_cleans_selection_without_touching_outside() {
    let mut app = app();
    act(&mut app, Action::SelectGroup(TrackId(1), false));
    act(&mut app, Action::ToggleGroup(TrackId(1)));
    crate::header_actions::delete_selected(&mut app);
    assert_eq!(
        app.project()
            .tracks
            .iter()
            .map(|t| t.id.0)
            .collect::<Vec<_>>(),
        [8]
    );
    assert!(app.selection.tracks.is_empty());
    assert!(app.selection.owner.is_none());
    assert!(app.selection.cell.is_none());
}

#[test]
fn descendant_summaries_ignore_headers_and_keep_empty_rows_and_playing_precedence() {
    for folded in [false, true] {
        let mut p: Project = super::nested_header_tests::project();
        p.tracks[0].fold_state = folded;
        let mut g = test_support::grid(&p);
        let col = g.layout.columns[0].clone();
        assert_eq!(col.leaves, [3, 4, 5, 6]);
        for (row, leaf) in [(0, 5), (1, 3), (2, 4), (3, 6)] {
            assert_eq!(
                group_column::summary(&g, &col, row).stripes,
                Some(draw::colour(p.tracks[leaf].colour))
            );
        }
        assert_eq!(group_column::summary(&g, &col, 4).stripes, None);
        for cells in &mut g.cells {
            cells[4].stop_button = false;
        }
        g.cells[0][4].stop_button = true; // Group headers must not synthesize stops.
        assert!(!group_column::summary(&g, &col, 4).stop_button);
        let point = Point::new(GAP + 4.0, g.layout.header_h + 4.0 * (CELL_H + GAP) + 4.0);
        assert_eq!(g.button_hit(point), None);
        g.cells[3][0].clip = p.tracks[3].slots[1].clip.as_ref();
        assert_eq!(
            group_column::summary(&g, &col, 0).stripes,
            Some(draw::colour(p.tracks[3].colour))
        );
        g.cells[5][0].status = CellStatus::Playing(0.5);
        g.cells[3][0].status = CellStatus::Triggered;
        let s = group_column::summary(&g, &col, 0);
        assert_eq!(s.stripes, Some(draw::colour(p.tracks[5].colour)));
        assert!(s.playing && s.triggered);
        g.cells[4][2].selected = true;
        assert!(group_column::summary(&g, &col, 2).selected);
    }
}

#[test]
fn folds_preserve_hidden_selection_launch_scope_and_saved_mappings() {
    use qtable_engine::{
        edit::{MappingEdit, ProjectEdit},
        engine::Command,
        model::{ControlRef, KeyName, KeySource, Source},
    };
    let mut app = app();
    for control in [
        ControlRef::TrackSelect(TrackId(1)),
        ControlRef::TrackStop(TrackId(3)),
        ControlRef::ClipSlot {
            track: TrackId(1),
            scene: app.project().scenes[1].id,
        },
        ControlRef::TrackSolo(TrackId(4)),
    ] {
        app.engine
            .execute(Command::Edit(ProjectEdit::EditMapping(MappingEdit::Add {
                source: Source::Key(KeySource {
                    key: KeyName("a".into()),
                }),
                control,
                min: 0.0,
                max: 1.0,
            })))
            .unwrap();
    }
    let mappings = app.project().mappings.clone();
    act(&mut app, Action::Select(TrackId(4), 1));
    for group in [TrackId(3), TrackId(1), TrackId(1), TrackId(3)] {
        act(&mut app, Action::ToggleGroup(group));
        assert_eq!(
            app.selection.clip,
            Some(Selection {
                track: TrackId(4),
                slot: 1
            })
        );
        assert_eq!(app.project().mappings, mappings);
        assert_eq!(
            app.project().descendant_clip_track_ids(TrackId(1)).unwrap(),
            [TrackId(4), TrackId(5), TrackId(6), TrackId(7)]
        );
    }
    act(&mut app, Action::SelectGroup(TrackId(3), false));
    let _ = crate::update::update(
        &mut app,
        Message::Outputs(OutputMessage::Ungroup(TrackId(3))),
    );
    assert_eq!(app.project().mappings.len(), 3);
    assert!(
        !app.project()
            .mappings
            .iter()
            .any(|m| m.control == ControlRef::TrackStop(TrackId(3)))
    );
    assert!(
        app.project()
            .mappings
            .iter()
            .any(|m| m.control == ControlRef::TrackSolo(TrackId(4)))
    );
}

#[test]
fn keyboard_reorders_whole_sibling_subtrees_and_header_after_slot_does_not_launch() {
    use crate::{
        focus::{Direction, Focus, GridKey},
        messages::EditMessage,
    };
    let mut app = app();
    act(
        &mut app,
        Action::SelectColumn(ColumnOwner::Group(TrackId(2)), Some(0)),
    );
    act(&mut app, Action::SelectGroup(TrackId(2), false));
    app.focus = Focus::Grid;
    let _ = crate::update::update(&mut app, Message::GridKey(GridKey::Launch));
    assert!(
        !app.engine.snapshot().playing,
        "inactive remembered cell is a header cursor"
    );
    let _ = crate::update::update(&mut app, Message::Edit(EditMessage::ReorderTrack(1)));
    assert_eq!(
        app.project()
            .tracks
            .iter()
            .map(|t| t.id.0)
            .collect::<Vec<_>>(),
        [1, 7, 2, 3, 4, 5, 6, 8]
    );
    assert_eq!(app.project()[TrackId(2)].group, Some(TrackId(1)));
    assert_eq!(app.project()[TrackId(3)].group, Some(TrackId(2)));
    // The generic Edit message changes focus to Detail; return to the grid.
    app.focus = Focus::Grid;
    // Arrow traversal still uses the shared visible projection after the structural edit.
    let _ = crate::update::update(&mut app, Message::GridKey(GridKey::Move(Direction::Left)));
    assert_eq!(app.selection.anchor, Some(TrackId(7)));
}

#[test]
fn keyboard_launch_retains_hidden_leaf_and_group_slot_scope_across_ancestor_folds() {
    use crate::focus::{Focus, GridKey};
    use std::time::{Duration, Instant};
    for group_slot in [false, true] {
        for folded in [false, true] {
            let mut project = super::nested_header_tests::project();
            project.global_quantisation = qtable_engine::model::GlobalQuantisation::None;
            // Same row in a direct Outer leaf makes accidental widening observable.
            project.tracks[6].slots[1].clip = project.tracks[3].slots[1].clip.clone();
            project.tracks[6].slots[1].clip.as_mut().unwrap().id =
                qtable_engine::model::ClipId(999);
            let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
            act(
                &mut app,
                if group_slot {
                    Action::SelectColumn(ColumnOwner::Group(TrackId(3)), Some(1))
                } else {
                    Action::Select(TrackId(4), 1)
                },
            );
            if folded {
                act(&mut app, Action::ToggleGroup(TrackId(1)));
            }
            app.focus = Focus::Grid;
            let _ = crate::update::update(&mut app, Message::GridKey(GridKey::Launch));
            let deadline = Instant::now() + Duration::from_secs(1);
            loop {
                let snapshot = app.engine.snapshot();
                if snapshot
                    .tracks
                    .iter()
                    .any(|t| t.id == TrackId(4) && t.playing_slot == Some(1))
                {
                    assert!(
                        !snapshot
                            .tracks
                            .iter()
                            .any(|t| t.id == TrackId(7) && t.playing_slot.is_some())
                    );
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "retained selection did not launch: group={group_slot}, folded={folded}"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
