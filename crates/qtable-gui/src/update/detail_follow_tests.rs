//! Selecting a track, output or clip moves the Detail View to the matching tab.
use super::update;
use crate::app::App;
use crate::inspector_panel::Tab;
use crate::messages::{Message, OutputMessage};
use crate::{grid, selection};
use qtable_engine::engine::Engine;
use qtable_engine::model::Project;

fn app() -> App {
    let mut app = App::new(
        Engine::new(crate::files::new_project()).unwrap(),
        String::new(),
        None,
    );
    let _ = update(&mut app, Message::Outputs(OutputMessage::AddOutput));
    app
}

#[test]
fn selecting_a_track_header_opens_the_track_tab() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(track, false)),
    );
    assert_eq!(app.selection.inspector_tab, Tab::Track);
    assert_eq!(app.selection.track(), Some(track));
}

#[test]
fn selecting_an_output_opens_the_output_tab() {
    let mut app = app();
    let output = app.project().outputs[0].id;
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectColumn(
            crate::selection::ColumnOwner::Track(output),
            None,
        )),
    );
    assert_eq!(app.selection.inspector_tab, Tab::Output);
    assert_eq!(app.selection.anchor, Some(output));
}

#[test]
fn opening_a_clip_selects_it_and_expands_a_collapsed_detail_view() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    app.chrome.detail_open = false;
    app.chrome.browser_open = false;
    app.selection.inspector_tab = Tab::Output;
    let _ = update(&mut app, Message::Grid(grid::Action::OpenClip(track, 0)));
    assert!(app.chrome.detail_open);
    assert!(!app.chrome.browser_open);
    assert_eq!(app.selection.inspector_tab, Tab::Clip);
    assert_eq!(app.selection.clip, Some(grid::Selection { track, slot: 0 }));
}

#[test]
fn the_track_name_field_acts_on_a_selected_track_without_a_clip() {
    let mut state = selection::SelectionState::default();
    assert_eq!(state.track(), None);
    let project = Project::default();
    assert!(project.tracks.is_empty());
    state.anchor = Some(qtable_engine::model::TrackId(4));
    assert_eq!(state.track(), Some(qtable_engine::model::TrackId(4)));
}

#[test]
fn selecting_a_scene_then_a_clip_follows_selection_without_opening_the_browser() {
    let mut app = app();
    let scene = app.project().scenes[0].id;
    let track = app.project().tracks[0].id;
    let _ = update(&mut app, Message::Grid(grid::Action::SelectScene(scene)));
    assert_eq!(app.selection.inspector_tab, Tab::Scene);
    assert_eq!(app.selection.scene, Some(scene));
    let _ = update(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    assert_eq!(app.selection.inspector_tab, Tab::Clip);
    assert_eq!(app.selection.scene, None);
    assert!(!app.chrome.browser_open);
}

#[test]
fn a_cell_selects_its_track_and_a_title_keeps_only_a_ghost() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let _ = update(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    assert_eq!(app.selection.tracks, vec![track]);
    assert_eq!(
        app.selection.owner,
        Some(selection::ColumnOwner::Track(track))
    );
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(track, false)),
    );
    assert_eq!(app.selection.clip, None);
    assert_eq!(
        app.selection.ghost_clip,
        Some(grid::Selection { track, slot: 0 })
    );
    assert!(!app.selection.cell.unwrap().active);
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectBlankColumn(
            selection::ColumnOwner::Track(track),
        )),
    );
    assert_eq!(app.selection.cell, None);
    assert_eq!(app.selection.ghost_clip, None);
}

#[test]
fn output_and_main_cells_share_active_ghost_and_blank_states() {
    let mut app = app();
    let output = app.project().outputs[0].id;
    let main = app.project().main.id;
    for owner in [
        selection::ColumnOwner::Track(output),
        selection::ColumnOwner::Track(main),
    ] {
        let _ = update(
            &mut app,
            Message::Grid(grid::Action::SelectColumn(owner, Some(2))),
        );
        assert_eq!(app.selection.owner, Some(owner));
        assert_eq!(
            app.selection.cell,
            Some(selection::ColumnCell {
                owner,
                row: 2,
                active: true
            })
        );
        let _ = update(
            &mut app,
            Message::Grid(grid::Action::SelectColumn(owner, None)),
        );
        assert!(!app.selection.cell.unwrap().active);
        let _ = update(
            &mut app,
            Message::Grid(grid::Action::SelectBlankColumn(owner)),
        );
        assert_eq!(app.selection.cell, None);
    }
}

#[test]
fn main_title_and_blank_select_its_pane_while_scene_cells_select_scene_properties() {
    let mut app = app();
    let revision = app.project_revision();
    let scene = app.project().scenes[0].id;
    let owner = selection::ColumnOwner::Track(app.project().main.id);
    let _ = update(&mut app, Message::Grid(grid::Action::SelectScene(scene)));
    assert_eq!(app.selection.inspector_tab, Tab::Scene);
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectColumn(owner, None)),
    );
    assert_eq!(app.selection.inspector_tab, Tab::Main);
    assert_eq!(app.selection.scene, None);
    assert_eq!(
        app.selection.cell,
        Some(selection::ColumnCell {
            owner,
            row: 0,
            active: false
        })
    );
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectBlankColumn(owner)),
    );
    assert_eq!(app.selection.inspector_tab, Tab::Main);
    assert_eq!(app.selection.cell, None);
    assert_eq!(app.project_revision(), revision);
}
