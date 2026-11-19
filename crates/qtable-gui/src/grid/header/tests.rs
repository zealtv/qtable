use super::*;
use crate::{
    app::App,
    grid::{
        Action,
        layout::{self, ColumnKey},
    },
    messages::Message,
    selection::{ColumnCell, ColumnOwner, SelectionState},
};
use qtable_engine::{
    engine::Engine,
    model::{Project, Region, Scene, SceneId, TrackId},
};

fn project() -> Project {
    let mut project = Project {
        tracks: (1..=3)
            .map(|id| Track::new(TrackId(id), format!("T{id}"), 1))
            .collect(),
        outputs: vec![Track::new_output(TrackId(201), "Lights", "localhost", 9000)],
        scenes: vec![Scene {
            id: SceneId(1),
            name: "Scene 1".into(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    };
    let group = Track::new_group(TrackId(101), "Group", 1);
    project.insert_group(group, &[TrackId(1), TrackId(2)]);
    project
}

#[test]
fn region_columns_share_keys_capabilities_and_optional_numbers() {
    let project = project();
    let tracks = layout::columns(&project, Region::Tracks);
    assert_eq!(tracks[0].key, ColumnKey::Track(TrackId(101)));
    assert!(tracks[0].caps.foldable);
    assert_eq!(tracks[0].number, Some(1));
    assert_eq!(tracks[1].parent, Some(TrackId(101)));
    assert_eq!(tracks[1].number, Some(2));
    let outputs = layout::columns(&project, Region::Outputs);
    assert_eq!(outputs[0].key, ColumnKey::Track(TrackId(201)));
    assert!(outputs[0].caps.destination && !outputs[0].caps.holds_clips);
    assert_eq!(outputs[0].number, None);
    assert_eq!(
        layout::columns(&project, Region::Main)[0].key,
        ColumnKey::Track(project.main.id)
    );
}

#[test]
fn main_is_selected_after_its_header_and_not_after_a_scene() {
    let mut app = App::new(Engine::new(project()).unwrap(), "120".into(), None);
    let main = app.project().main.id;
    let col = Column::main(main);
    let _ = crate::update::update(
        &mut app,
        Message::Grid(Action::SelectColumn(ColumnOwner::Track(main), None)),
    );
    assert!(app.selection.view(&[]).column_selected(&col));
    let look = look(
        &col,
        &HeaderFacts::track(&app.project().main),
        &app.selection.view(&[]),
    );
    assert!(look.selected);
    assert_eq!(look.fill, crate::control::LIVE_DARK.selection_background);
    let _ = crate::update::update(&mut app, Message::Grid(Action::SelectScene(SceneId(1))));
    assert!(!app.selection.view(&[]).column_selected(&col));
}

#[test]
fn groups_selected_by_header_range_or_owner_use_the_shared_look() {
    let project = project();
    let col = &layout::columns(&project, Region::Tracks)[0];
    let mut selection = SelectionState {
        tracks: vec![TrackId(1), TrackId(2)],
        ..SelectionState::default()
    };
    let facts = HeaderFacts::track(&project.tracks[0]);
    assert!(!look(col, &facts, &selection.view(&project.tracks)).selected);
    selection.tracks.push(TrackId(101));
    let selected = look(col, &facts, &selection.view(&project.tracks));
    assert!(selected.selected);
    assert_eq!(
        selected.fill,
        crate::control::LIVE_DARK.selection_background
    );
    assert_eq!(selected.title, "1 Group");
    selection.tracks.pop();
    assert!(!look(col, &facts, &selection.view(&project.tracks)).selected);
    selection.owner = Some(ColumnOwner::Group(TrackId(101)));
    assert!(!look(col, &facts, &selection.view(&project.tracks)).selected);
    selection.tracks.push(TrackId(101));
    selection.cell = Some(ColumnCell {
        owner: col.owner(),
        row: 0,
        active: true,
    });
    assert_eq!(
        look(col, &facts, &selection.view(&project.tracks)).fill,
        facts.colour
    );
}

#[test]
fn output_titles_are_names_and_member_bands_are_capability_driven() {
    let project = project();
    let col = &layout::columns(&project, Region::Outputs)[0];
    let selection = SelectionState {
        tracks: vec![TrackId(201)],
        ..SelectionState::default()
    };
    let look = look(
        col,
        &HeaderFacts::track(&project.outputs[0]),
        &selection.view(&project.outputs),
    );
    assert_eq!(look.title, "Lights");
    assert!(look.selected);
    assert!(col.ancestors.is_empty());
    let tracks = layout::columns(&project, Region::Tracks);
    assert_eq!(tracks[1].ancestors.len(), 1);
    assert_eq!(tracks[1].ancestors[0].0, project.tracks[0].id);
}

#[test]
fn selected_group_band_follows_its_title_at_every_depth_and_fold_combination() {
    for selected in 1..=3 {
        for folds in 0..8 {
            let mut app = App::new(
                Engine::new(crate::grid::nested_header_tests::project()).unwrap(),
                "120".into(),
                None,
            );
            let _ = crate::update::update(
                &mut app,
                Message::Grid(Action::SelectGroup(TrackId(selected), false)),
            );
            for id in 1..=3 {
                if folds & (1 << (id - 1)) != 0 {
                    let _ = crate::update::update(
                        &mut app,
                        Message::Grid(Action::ToggleGroup(TrackId(id))),
                    );
                }
            }
            // Folding must retain even a hidden selected group's identity.
            assert_eq!(app.selection.tracks, [TrackId(selected)]);
            let p = app.project();
            let selection = app.selection.view(&p.tracks);
            for col in layout::columns(p, Region::Tracks) {
                let track = &p.tracks[col.index.unwrap()];
                let look = look(&col, &HeaderFacts::track(track), &selection);
                assert_eq!(
                    look.fill,
                    if track.id == TrackId(selected) {
                        crate::control::LIVE_DARK.selection_background
                    } else {
                        draw::colour(track.colour)
                    }
                );
                assert_eq!(look.band_colours.len(), col.ancestors.len());
                for ((ancestor, colour), fill) in col.ancestors.iter().zip(look.band_colours) {
                    assert_eq!(
                        fill,
                        if *ancestor == TrackId(selected) {
                            crate::control::LIVE_DARK.selection_background
                        } else {
                            *colour
                        },
                        "selected={selected}, folds={folds}, column={:?}, ancestor={ancestor:?}",
                        col.key,
                    );
                }
            }
        }
    }
}

#[test]
fn ancestor_bands_use_explicit_headers_and_the_same_active_cell_rule_as_titles() {
    let p = crate::grid::nested_header_tests::project();
    // All leaves selected never implies any ancestor header is selected.
    for selected in [vec![4, 5, 6, 7], vec![1, 2, 3]] {
        for active in [false, true] {
            let selection = SelectionState {
                tracks: selected.iter().copied().map(TrackId).collect(),
                cell: Some(ColumnCell {
                    owner: ColumnOwner::Group(TrackId(2)),
                    row: 0,
                    active,
                }),
                ..Default::default()
            };
            for col in layout::columns(&p, Region::Tracks) {
                let look = look(
                    &col,
                    &HeaderFacts::track(&p.tracks[col.index.unwrap()]),
                    &selection.view(&p.tracks),
                );
                for ((id, colour), fill) in col.ancestors.iter().zip(look.band_colours) {
                    assert_eq!(
                        fill,
                        if selected.contains(&id.0) && !(active && id.0 == 2) {
                            crate::control::LIVE_DARK.selection_background
                        } else {
                            *colour
                        },
                    );
                }
            }
        }
    }
}

#[test]
fn main_menu_omits_palette_rename_and_delete_and_remains_valid() {
    use crate::context_menu::{self, Entry, MenuTarget};
    let mut app = App::new(Engine::new(project()).unwrap(), "120".into(), None);
    let owner = app.project().main.id;
    let entries = context_menu::header_entries(
        owner,
        app.project().track_capabilities(owner).unwrap(),
        qtable_engine::model::Main::COLOUR,
        false,
        false,
    );
    assert!(
        !entries
            .iter()
            .any(|entry| matches!(entry, Entry::Palette { .. }))
    );
    let _ = crate::update::update(&mut app, Message::Grid(Action::HeaderMenu(owner)));
    assert_eq!(app.menu_target(), Some(MenuTarget::Header(owner)));
    assert!(
        app.selection
            .view(&[])
            .column_selected(&Column::main(app.project().main.id))
    );
    let revision = app.project_revision();
    crate::header_actions::begin_rename(&mut app, crate::grid::RenameTarget::Header(owner));
    crate::header_actions::recolour(&mut app, owner, qtable_engine::model::LIVE_CLIP_PALETTE[0]);
    assert!(app.header_actions.renaming.is_none());
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn main_header_facts_come_from_the_project_and_a_clip_moves_the_selection_off_it() {
    let mut app = App::new(Engine::new(project()).unwrap(), "120".into(), None);
    let main = app.project().main.clone();
    let col = Column::main(main.id);
    let unselected = look(&col, &HeaderFacts::track(&main), &app.selection.view(&[]));
    assert_eq!(unselected.title, "Main");
    assert_eq!(
        unselected.fill,
        draw::colour(qtable_engine::model::Main::COLOUR)
    );
    let _ = crate::update::update(
        &mut app,
        Message::Grid(Action::SelectColumn(ColumnOwner::Track(main.id), None)),
    );
    assert!(app.selection.view(&[]).column_selected(&col));
    let track = app.project().tracks[0].id;
    let _ = crate::update::update(&mut app, Message::Grid(Action::Select(track, 0)));
    assert!(!app.selection.view(&[]).column_selected(&col));
}

#[test]
fn main_band_rows_go_through_the_shared_band_builder() {
    use crate::grid::band::{self, Row};
    let app = App::new(Engine::new(project()).unwrap(), "120".into(), None);
    let columns = layout::columns(app.project(), Region::Main);
    let layout = crate::grid::cache::Layout {
        header_h: crate::grid::HEADER_H,
        xs: layout::x_positions(&columns),
        columns,
    };
    let seen = std::cell::RefCell::new(Vec::new());
    let _ = band::tracks::<Message>(
        &layout,
        &app.selection.view(&[]),
        &|cell| {
            seen.borrow_mut().push(cell == project().main.id);
            iced::widget::Space::new().into()
        },
        Row::Mixer,
    );
    assert_eq!(*seen.borrow(), [true]);
}
