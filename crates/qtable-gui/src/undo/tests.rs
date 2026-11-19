//! Cmd-Z and Cmd-Shift-Z: focus rules, the reset after a step, the status notice and the
//! unsaved marker (undo design, sections 8, 9 and 12).
use super::*;
use crate::drafts::{ClipField, DraftKey, TrackField};
use crate::messages::{EditMessage, Message};
use crate::mixer_band::MixerMessage;
use crate::{grid, inspector};
use iced::keyboard::Key;
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{FieldRef, Project, TrackId};

/// One message stands for one user action, so each ends its own gesture, as a release would;
/// otherwise a run of messages with no input event between them shares one undo step.
fn send(app: &mut App, message: Message) {
    let _ = crate::update::update(app, message);
    crate::gesture_steps::boundary(app);
}

fn press(app: &mut App, character: &str, modifiers: Modifiers, ignored: bool) {
    send(
        app,
        Message::KeyPressed {
            key: Key::Character(character.into()),
            modifiers,
            repeat: false,
            ignored,
        },
    );
}

fn undo(app: &mut App) {
    press(app, "z", Modifiers::COMMAND, true);
}

fn redo(app: &mut App) {
    press(app, "z", Modifiers::COMMAND | Modifiers::SHIFT, true);
}

/// Two tracks with a clip each, as the grid builds them.
fn app() -> App {
    let engine = Engine::new(Project::default()).expect("engine starts");
    let mut app = App::new(engine, String::new(), None);
    for message in [
        EditMessage::AddTrack,
        EditMessage::AddTrack,
        EditMessage::AddScene,
    ] {
        send(&mut app, Message::Edit(message));
    }
    for track in tracks(&app) {
        send(&mut app, Message::Grid(grid::Action::Select(track, 0)));
        send(&mut app, Message::Edit(EditMessage::CreateSelectedClip));
    }
    app
}

fn tracks(app: &App) -> Vec<TrackId> {
    app.project().tracks.iter().map(|track| track.id).collect()
}

fn clip_name(app: &App, track: TrackId) -> String {
    app.project().clip(track, 0).unwrap().name.clone()
}

fn rename(app: &mut App, track: TrackId, name: &str) {
    assert!(crate::edit_actions::run_edit(
        app,
        Command::Edit(ProjectEdit::RenameClip {
            track,
            slot: 0,
            name: name.into(),
        }),
    ));
}

fn volume(track: TrackId) -> FieldRef {
    FieldRef::volume(track)
}

#[test]
fn cmd_z_undoes_and_cmd_shift_z_redoes_and_the_status_bar_names_each_step() {
    let mut app = app();
    let track = tracks(&app)[0];
    let original = clip_name(&app, track);
    rename(&mut app, track, "Intro");

    undo(&mut app);
    assert_eq!(clip_name(&app, track), original);
    assert_eq!(app.status.message(), Some("Undo: Rename Clip"));
    assert_eq!(app.status.severity(), Severity::Notice);

    redo(&mut app);
    assert_eq!(clip_name(&app, track), "Intro");
    assert_eq!(app.status.message(), Some("Redo: Rename Clip"));
}

#[test]
fn it_works_whether_or_not_a_widget_took_the_key() {
    let mut app = app();
    let track = tracks(&app)[0];
    rename(&mut app, track, "Intro");
    press(&mut app, "z", Modifiers::COMMAND, false);
    assert_ne!(clip_name(&app, track), "Intro");
    press(&mut app, "Z", Modifiers::COMMAND | Modifiers::SHIFT, false);
    assert_eq!(clip_name(&app, track), "Intro");
}

#[test]
fn a_plain_z_and_a_held_key_do_nothing() {
    let mut app = app();
    let track = tracks(&app)[0];
    rename(&mut app, track, "Intro");
    press(&mut app, "z", Modifiers::NONE, true);
    press(&mut app, "z", Modifiers::SHIFT, true);
    send(
        &mut app,
        Message::KeyPressed {
            key: Key::Character("z".into()),
            modifiers: Modifiers::COMMAND,
            repeat: true,
            ignored: true,
        },
    );
    assert_eq!(clip_name(&app, track), "Intro");
}

#[cfg(not(target_os = "macos"))]
#[test]
fn ctrl_y_redoes_off_the_mac() {
    let mut app = app();
    let track = tracks(&app)[0];
    rename(&mut app, track, "Intro");
    undo(&mut app);
    press(&mut app, "y", Modifiers::COMMAND, true);
    assert_eq!(clip_name(&app, track), "Intro");
}

#[test]
fn nothing_to_undo_or_redo_shows_nothing() {
    let engine = Engine::new(Project::default()).expect("engine starts");
    let mut app = App::new(engine, String::new(), None);
    undo(&mut app);
    redo(&mut app);
    assert_eq!(app.status.message(), None);

    // An error already showing is not wiped by a key that did nothing.
    app.status.show_error("Port 99999 is out of range");
    undo(&mut app);
    assert_eq!(app.status.message(), Some("Port 99999 is out of range"));
}

#[test]
fn unsaved_typing_is_dropped_the_field_released_and_one_step_undone() {
    let mut app = app();
    let track = tracks(&app)[0];
    let original = clip_name(&app, track);
    rename(&mut app, track, "Intro");
    send(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    let length = app.project().clip(track, 0).unwrap().length();
    let key = DraftKey::clip(track, 0, ClipField::Length);
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(key, "7".into())),
    );
    assert_eq!(app.pending_field, Some(key));
    let before = app.engine.content_id();

    undo(&mut app);

    assert_eq!(app.pending_field, None, "the field lost its pending state");
    assert!(app.drafts.values.is_empty(), "the draft is gone");
    assert_eq!(clip_name(&app, track), original, "one step was undone");
    assert_eq!(app.project().clip(track, 0).unwrap().length(), length);
    assert_ne!(app.engine.content_id(), before);

    // Nothing commits the draft afterwards, however the field's focus resolves.
    let content = app.engine.content_id();
    send(&mut app, Message::CommitField);
    send(&mut app, Message::CheckFieldFocus);
    send(&mut app, Message::Tick);
    assert_eq!(app.engine.content_id(), content);
    assert_eq!(app.project().clip(track, 0).unwrap().length(), length);
}

#[test]
fn redo_with_unsaved_typing_drops_it_too() {
    let mut app = app();
    let track = tracks(&app)[0];
    rename(&mut app, track, "Intro");
    undo(&mut app);
    let key = DraftKey::Track(track, TrackField::Name);
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(key, "Draft".into())),
    );
    assert_eq!(app.pending_field, Some(key));
    redo(&mut app);
    assert_eq!(app.pending_field, None);
    assert!(app.drafts.values.is_empty());
    assert_eq!(clip_name(&app, track), "Intro");
    assert_ne!(app.project().track(track).unwrap().name, "Draft");
}

#[test]
fn typing_with_nothing_to_undo_is_still_dropped() {
    let engine = Engine::new(Project::default()).expect("engine starts");
    let mut app = App::new(engine, String::new(), None);
    send(&mut app, Message::TempoChanged("93".into()));
    assert_eq!(app.pending_field, Some(DraftKey::Tempo));
    undo(&mut app);
    assert_eq!(app.pending_field, None);
    assert_eq!(app.tempo_input, "120.00");
    assert_eq!(app.status.message(), None);
}

#[test]
fn undo_drops_a_popover_a_rename_and_a_held_preview_and_keeps_a_surviving_selection() {
    let mut app = app();
    let [first, second] = [tracks(&app)[0], tracks(&app)[1]];
    send(&mut app, Message::Grid(grid::Action::Select(first, 0)));
    rename(&mut app, second, "Verse");

    app.open_overlay(crate::overlay::Overlay::SectionsMenu);
    app.parameter_gesture.hold(&volume(first), 0.25);
    app.header_actions.renaming = Some(grid::RenameTarget::Header(first));
    app.drafts.errors.insert(
        DraftKey::Track(first, TrackField::Path),
        "bad path".to_owned(),
    );
    let selection = app.selection.clip;
    assert!(selection.is_some());

    undo(&mut app);

    assert!(app.overlay.is_none(), "no popover survives");
    assert!(app.header_actions.renaming.is_none(), "no rename survives");
    assert!(app.parameter_gesture.is_empty(), "no preview survives");
    assert!(app.drafts.errors.is_empty(), "no field error survives");
    assert_eq!(app.selection.clip, selection, "a surviving selection stays");
}

#[test]
fn undo_with_a_rename_in_progress_drops_it_without_committing() {
    let mut app = app();
    let track = tracks(&app)[0];
    send(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    let original = clip_name(&app, track);
    rename(&mut app, track, "Intro");
    crate::clip_actions::begin_rename(&mut app);
    app.clip_actions.draft = "Typed".into();
    assert!(app.clip_actions.renaming.is_some());

    undo(&mut app);

    assert!(app.clip_actions.renaming.is_none());
    assert_eq!(clip_name(&app, track), original);
    send(&mut app, Message::CommitField);
    assert_eq!(
        clip_name(&app, track),
        original,
        "the typed name never lands"
    );
}

#[test]
fn undoing_a_creation_drops_a_selection_that_no_longer_resolves() {
    let mut app = app();
    let track = tracks(&app)[1];
    send(&mut app, Message::Edit(EditMessage::AddTrack));
    let added = *tracks(&app).last().unwrap();
    send(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(added, false)),
    );
    assert!(app.selection.tracks.contains(&added));
    send(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    app.selection.tracks.push(added);
    app.selection.anchor = Some(added);

    undo(&mut app);

    assert!(app.project().track(added).is_none());
    assert!(!app.selection.tracks.contains(&added));
    assert_eq!(app.selection.anchor, None);
    assert_eq!(app.selection.clip.map(|sel| sel.track), Some(track));
}

#[test]
fn the_unsaved_marker_follows_the_content_through_undo_and_redo() {
    let mut app = app();
    let track = tracks(&app)[0];
    app.mark_saved();
    assert!(!app.is_dirty());

    rename(&mut app, track, "Intro");
    assert!(app.is_dirty());
    undo(&mut app);
    assert!(!app.is_dirty(), "save, edit, undo shows saved");
    redo(&mut app);
    assert!(app.is_dirty(), "redo shows unsaved again");
}

#[test]
fn undoing_past_a_save_shows_unsaved_and_redoing_back_shows_saved() {
    let mut app = app();
    let track = tracks(&app)[0];
    rename(&mut app, track, "Intro");
    app.mark_saved();
    undo(&mut app);
    assert!(app.is_dirty());
    redo(&mut app);
    assert!(!app.is_dirty());
}

#[test]
fn a_save_in_flight_marks_the_content_it_captured() {
    let mut app = app();
    let track = tracks(&app)[0];
    let stamp = crate::files::SaveStamp::capture(&app);
    rename(&mut app, track, "Intro");
    let _ = crate::files::finish_save(&mut app, Ok("a.ron".into()), stamp, None);
    assert!(app.is_dirty(), "the edit landed after the saved content");
    undo(&mut app);
    assert!(!app.is_dirty());
}

#[test]
fn a_fold_that_differs_from_the_snapshot_keeps_the_set_unsaved() {
    let mut app = app();
    let [first, second] = [tracks(&app)[0], tracks(&app)[1]];
    rename(&mut app, first, "Intro");
    assert!(crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::CreateGroup {
            tracks: vec![first, second],
        }),
    ));
    let group = app
        .project()
        .tracks
        .iter()
        .find(|track| track.is_group())
        .unwrap()
        .id;
    app.mark_saved();
    assert!(crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::SetGroupFolded {
            group,
            folded: true,
        }),
    ));
    assert!(app.is_dirty(), "folding changes the file");

    undo(&mut app);
    assert!(app.is_dirty(), "the carried fold differs from the snapshot");
    redo(&mut app);
    assert!(app.is_dirty(), "redo shows unsaved");
}

#[test]
fn a_mixer_edit_and_its_undo_leave_the_marker_and_the_volume_consistent() {
    let mut app = app();
    let track = tracks(&app)[0];
    let stored = app.project().field(&volume(track)).unwrap().value;
    app.mark_saved();
    send(
        &mut app,
        Message::Mixer(MixerMessage::SetVolume(track, 0.25)),
    );
    assert!(app.is_dirty());
    undo(&mut app);
    assert!(!app.is_dirty());
    assert_eq!(app.project().field(&volume(track)).unwrap().value, stored);
    assert_eq!(app.status.message(), Some("Undo: Set Value"));
}

#[test]
fn it_works_in_map_mode() {
    let mut app = app();
    let track = tracks(&app)[0];
    rename(&mut app, track, "Intro");
    send(&mut app, Message::Map(crate::map_mode::Action::Toggle));
    assert!(app.map_mode.on);
    undo(&mut app);
    assert_ne!(clip_name(&app, track), "Intro");
    assert!(app.map_mode.on, "Map Mode stays on");
}
