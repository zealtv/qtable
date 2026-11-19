//! Track-title and group-header menus: recolour, rename in place, delete, ungroup.
use super::update;
use crate::app::App;
use crate::grid;
use crate::messages::{EditMessage, Message, OutputMessage};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::TrackId;
use qtable_engine::model::{LIVE_CLIP_PALETTE, Project};

fn send(app: &mut App, message: EditMessage) {
    let _ = update(app, Message::Edit(message));
}

/// Three tracks, the first two grouped.
fn app_with_group() -> App {
    let engine = Engine::new(Project::default()).expect("engine starts");
    let mut app = App::new(engine, String::new(), None);
    for _ in 0..3 {
        send(&mut app, EditMessage::AddTrack);
    }
    let tracks = app.project().tracks.iter().take(2).map(|t| t.id).collect();
    app.engine
        .execute(Command::Edit(ProjectEdit::CreateGroup { tracks }))
        .expect("group created");
    app
}

fn open(app: &mut App, owner: TrackId) {
    app.cursor = iced::Point::new(50.0, 40.0);
    let _ = update(app, Message::Grid(grid::Action::HeaderMenu(owner)));
}

#[test]
fn right_click_on_a_track_title_selects_it_and_opens_its_menu() {
    let mut app = app_with_group();
    let track = app.project().tracks[1].id;
    open(&mut app, track);
    assert_eq!(app.selection.tracks, vec![track]);
    assert_eq!(
        app.context_menu().map(|m| m.target),
        Some(crate::context_menu::MenuTarget::Header(track))
    );
}

#[test]
fn a_swatch_recolours_the_track_or_the_group() {
    let mut app = app_with_group();
    let track = app.project().tracks[0].id;
    let group = app.project().group_tracks().next().unwrap().id;
    let colour = LIVE_CLIP_PALETTE[40];
    open(&mut app, track);
    send(&mut app, EditMessage::RecolourHeader(track, colour));
    assert_eq!(app.project().tracks[0].colour, colour);
    assert!(app.context_menu().is_none());
    let colour = LIVE_CLIP_PALETTE[41];
    send(&mut app, EditMessage::RecolourHeader(group, colour));
    assert_eq!(app.project().group_tracks().next().unwrap().colour, colour);
}

#[test]
fn header_rename_starts_with_the_name_and_commits_through_the_engine() {
    let mut app = app_with_group();
    let track = app.project().tracks[0].id;
    let group = app.project().group_tracks().next().unwrap().id;
    send(&mut app, EditMessage::BeginRenameHeader(track));
    assert_eq!(app.header_actions.draft, app.project().tracks[0].name);
    send(&mut app, EditMessage::CommitRenameHeader("Drums".into()));
    assert_eq!(app.project().tracks[0].name, "Drums");
    assert!(app.header_actions.renaming.is_none());
    send(&mut app, EditMessage::BeginRenameHeader(group));
    send(
        &mut app,
        EditMessage::RenameHeaderDraftChanged("Band".into()),
    );
    // Clicking elsewhere in the grid keeps the typed name.
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(track, false)),
    );
    assert_eq!(app.project().group_tracks().next().unwrap().name, "Band");
    assert!(app.header_actions.renaming.is_none());
}

#[test]
fn escape_drops_a_header_rename() {
    let mut app = app_with_group();
    let track = app.project().tracks[0].id;
    send(&mut app, EditMessage::BeginRenameHeader(track));
    send(
        &mut app,
        EditMessage::RenameHeaderDraftChanged("Nope".into()),
    );
    let _ = update(&mut app, Message::CancelField);
    assert!(app.header_actions.renaming.is_none());
    assert_ne!(app.project().tracks[0].name, "Nope");
}

#[test]
fn delete_removes_the_track_and_its_selection_but_never_the_last_track() {
    let mut app = app_with_group();
    let first = app.project().clip_tracks().next().unwrap().id;
    open(&mut app, first);
    send(&mut app, EditMessage::DeleteTrack(first));
    assert_eq!(app.project().clip_tracks().count(), 2);
    assert!(app.selection.tracks.is_empty());
    assert!(app.context_menu().is_none());
    for _ in 0..3 {
        let id = app.project().clip_tracks().next().unwrap().id;
        send(&mut app, EditMessage::DeleteTrack(id));
    }
    assert_eq!(app.project().clip_tracks().count(), 1);
    assert_eq!(app.project().group_tracks().count(), 0);
    assert!(app.status.message().is_some());
}

#[test]
fn ungroup_from_the_group_menu_keeps_the_tracks_and_closes_the_menu() {
    let mut app = app_with_group();
    let group = app.project().group_tracks().next().unwrap().id;
    open(&mut app, group);
    let _ = update(&mut app, Message::Outputs(OutputMessage::Ungroup(group)));
    assert!(app.project().group_tracks().next().is_none());
    assert_eq!(app.project().tracks.len(), 3);
    assert!(app.context_menu().is_none());
}

#[test]
fn right_click_on_a_scene_selects_it_and_its_menu_renames_it() {
    let mut app = app_with_group();
    send(&mut app, EditMessage::AddScene);
    let scene = app.project().scenes[0].id;
    app.cursor = iced::Point::new(50.0, 40.0);
    let _ = update(&mut app, Message::Grid(grid::Action::SceneMenu(scene)));
    assert_eq!(app.selection.scene, Some(scene));
    assert_eq!(
        app.context_menu().map(|m| m.target),
        Some(crate::context_menu::MenuTarget::Scene(scene))
    );
    send(&mut app, EditMessage::BeginRenameScene(scene));
    assert_eq!(app.header_actions.draft, app.project().scenes[0].name);
    send(&mut app, EditMessage::CommitRenameHeader("Chorus".into()));
    assert_eq!(app.project().scenes[0].name, "Chorus");
    assert!(app.header_actions.renaming.is_none());
}

#[test]
fn an_output_renames_from_its_menu_and_a_click_elsewhere_keeps_the_name() {
    let mut app = app_with_group();
    let _ = update(&mut app, Message::Outputs(OutputMessage::AddOutput));
    let output = app.project().outputs[0].id;
    let _ = update(
        &mut app,
        Message::Grid(crate::grid::Action::HeaderMenu(output)),
    );
    send(&mut app, EditMessage::BeginRenameHeader(output));
    assert!(app.menu_target().is_none());
    send(
        &mut app,
        EditMessage::RenameHeaderDraftChanged("Lasers".into()),
    );
    // A click elsewhere in the output columns keeps the typed name.
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectColumn(
            crate::selection::ColumnOwner::Track(output),
            None,
        )),
    );
    assert_eq!(app.project().outputs[0].name, "Lasers");
    assert!(app.header_actions.renaming.is_none());
}

#[test]
fn clicking_a_group_header_selects_its_explicit_id_without_recolouring() {
    let mut app = app_with_group();
    let group = app.project().group_tracks().next().unwrap().id;
    let before = app.project().group_tracks().next().unwrap().colour;
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectGroup(group, false)),
    );
    assert_eq!(app.selection.tracks, [group]);
    assert_eq!(app.project().group_tracks().next().unwrap().colour, before);
    // ⌘R now renames the group, not its first track.
    let _ = crate::header_actions::begin_rename_selected(&mut app);
    assert_eq!(
        app.header_actions.renaming,
        Some(grid::RenameTarget::Header(group))
    );
}
