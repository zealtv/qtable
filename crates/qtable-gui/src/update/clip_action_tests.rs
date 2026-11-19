//! Cut, copy, paste, duplicate, delete and rename on the selected clip.
use super::update;
use crate::app::App;
use crate::grid;
use crate::messages::{EditMessage, Message};
use qtable_engine::engine::Engine;
use qtable_engine::model::{Project, TrackId};

fn send(app: &mut App, message: EditMessage) {
    let _ = update(app, Message::Edit(message));
}

fn select(app: &mut App, track: TrackId, slot: usize) {
    let _ = update(app, Message::Grid(grid::Action::Select(track, slot)));
}

/// One track with a clip in slot 0 and three scenes (slots 0-2).
fn app_with_clip() -> (App, TrackId) {
    let engine = Engine::new(Project::default()).expect("engine starts");
    let mut app = App::new(engine, String::new(), None);
    send(&mut app, EditMessage::AddTrack);
    for _ in 0..3 {
        send(&mut app, EditMessage::AddScene);
    }
    let track = app.project().tracks[0].id;
    select(&mut app, track, 0);
    send(&mut app, EditMessage::CreateSelectedClip);
    (app, track)
}

fn clip_name(app: &App, slot: usize) -> Option<String> {
    app.project().tracks[0].slots[slot]
        .clip
        .as_ref()
        .map(|c| c.name.clone())
}

#[test]
fn copy_then_paste_fills_the_selected_slot_and_dirties_the_project() {
    let (mut app, track) = app_with_clip();
    send(&mut app, EditMessage::CopySelectedClip);
    app.mark_saved();
    select(&mut app, track, 2);
    send(&mut app, EditMessage::PasteClip);
    assert_eq!(clip_name(&app, 2).as_deref(), Some("Clip"));
    assert!(clip_name(&app, 0).is_some());
    assert!(app.is_dirty());
}

#[test]
fn cut_clears_the_slot_and_paste_restores_it_elsewhere() {
    let (mut app, track) = app_with_clip();
    send(&mut app, EditMessage::CutSelectedClip);
    assert_eq!(clip_name(&app, 0), None);
    select(&mut app, track, 1);
    send(&mut app, EditMessage::PasteClip);
    assert_eq!(clip_name(&app, 1).as_deref(), Some("Clip"));
}

#[test]
fn paste_with_an_empty_clipboard_changes_nothing() {
    let (mut app, track) = app_with_clip();
    select(&mut app, track, 1);
    let revision = app.project_revision();
    send(&mut app, EditMessage::PasteClip);
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn duplicate_lands_below_and_delete_clears() {
    let (mut app, _) = app_with_clip();
    send(&mut app, EditMessage::DuplicateSelectedClip);
    assert_eq!(clip_name(&app, 1).as_deref(), Some("Clip copy"));
    send(&mut app, EditMessage::DeleteSelectedClip);
    assert_eq!(clip_name(&app, 0), None);
    assert!(clip_name(&app, 1).is_some());
}

#[test]
fn duplicate_at_bottom_expands_the_grid_and_dirties_the_project() {
    let (mut app, track) = app_with_clip();
    send(&mut app, EditMessage::CopySelectedClip);
    let bottom = app.project().scenes.len() - 1;
    select(&mut app, track, bottom);
    send(&mut app, EditMessage::PasteClip);
    app.mark_saved();
    let scenes = app.project().scenes.len();
    send(&mut app, EditMessage::DuplicateSelectedClip);
    assert_eq!(app.project().scenes.len(), scenes + 1);
    assert_eq!(clip_name(&app, bottom + 1).as_deref(), Some("Clip copy"));
    assert!(app.is_dirty());
    assert!(
        app.project()
            .tracks
            .iter()
            .all(|t| t.slots.len() == scenes + 1)
    );
}

#[test]
fn duplicate_inserts_before_an_occupied_next_slot_instead_of_skipping_it() {
    let (mut app, track) = app_with_clip();
    send(&mut app, EditMessage::CopySelectedClip);
    select(&mut app, track, 1);
    send(&mut app, EditMessage::PasteClip);
    let occupied = app.project().tracks[0].slots[1].clone();
    let scenes = app.project().scenes.clone();
    select(&mut app, track, 0);
    send(&mut app, EditMessage::DuplicateSelectedClip);
    assert_eq!(app.project().scenes.len(), scenes.len() + 1);
    assert_eq!(clip_name(&app, 1).as_deref(), Some("Clip copy"));
    assert_eq!(app.project().tracks[0].slots[2], occupied);
    assert_eq!(&app.project().scenes[2..], &scenes[1..]);
    assert_eq!(app.selection.clip, Some(grid::Selection { track, slot: 0 }));
}

#[test]
fn rename_ends_only_when_the_engine_accepts_the_name() {
    let (mut app, _) = app_with_clip();
    send(&mut app, EditMessage::BeginRenameClip);
    assert!(app.clip_actions.renaming.is_some());
    send(&mut app, EditMessage::CommitRenameClip("Intro".into()));
    assert_eq!(clip_name(&app, 0).as_deref(), Some("Intro"));
    assert!(app.clip_actions.renaming.is_none());
    send(&mut app, EditMessage::BeginRenameClip);
    let _ = update(&mut app, Message::CancelField);
    assert!(app.clip_actions.renaming.is_none());
    assert_eq!(clip_name(&app, 0).as_deref(), Some("Intro"));
}

fn right_click(app: &mut App, track: TrackId, slot: usize) {
    app.cursor = iced::Point::new(120.0, 80.0);
    let _ = update(app, Message::Grid(grid::Action::ContextMenu(track, slot)));
}

#[test]
fn right_click_selects_the_slot_and_opens_the_menu_at_the_cursor() {
    let (mut app, track) = app_with_clip();
    right_click(&mut app, track, 2);
    assert_eq!(app.selection.clip, Some(grid::Selection { track, slot: 2 }));
    assert_eq!(
        app.context_menu().map(|m| m.at),
        Some(iced::Point::new(120.0, 80.0))
    );
}

#[test]
fn a_menu_command_runs_and_closes_the_menu() {
    let (mut app, track) = app_with_clip();
    right_click(&mut app, track, 0);
    send(&mut app, EditMessage::DuplicateSelectedClip);
    assert!(app.context_menu().is_none());
    assert_eq!(clip_name(&app, 1).as_deref(), Some("Clip copy"));
}

#[test]
fn empty_slot_menu_creates_a_clip() {
    let (mut app, track) = app_with_clip();
    right_click(&mut app, track, 2);
    send(&mut app, EditMessage::CreateSelectedClip);
    assert!(clip_name(&app, 2).is_some());
}

#[test]
fn picking_a_swatch_recolours_the_clip() {
    let (mut app, track) = app_with_clip();
    right_click(&mut app, track, 0);
    let colour = qtable_engine::model::LIVE_CLIP_PALETTE[20];
    send(&mut app, EditMessage::RecolourSelected(colour));
    assert_eq!(
        app.project().tracks[0].slots[0]
            .clip
            .as_ref()
            .unwrap()
            .colour,
        colour
    );
    assert!(app.context_menu().is_none());
}

#[test]
fn dismissing_closes_the_menu_without_editing() {
    let (mut app, track) = app_with_clip();
    right_click(&mut app, track, 0);
    let revision = app.project_revision();
    let _ = update(&mut app, Message::CloseOverlay);
    assert!(app.context_menu().is_none());
    right_click(&mut app, track, 0);
    let _ = update(&mut app, Message::CancelField);
    assert!(app.context_menu().is_none());
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn rename_starts_with_the_current_name_and_escape_drops_the_edit() {
    let (mut app, _) = app_with_clip();
    send(&mut app, EditMessage::BeginRenameClip);
    assert_eq!(app.clip_actions.draft, "Clip");
    send(&mut app, EditMessage::RenameDraftChanged("Verse".into()));
    let _ = update(&mut app, Message::CancelField);
    assert!(app.clip_actions.renaming.is_none());
    assert_eq!(clip_name(&app, 0).as_deref(), Some("Clip"));
}

#[test]
fn clicking_elsewhere_mid_rename_keeps_the_typed_name() {
    let (mut app, track) = app_with_clip();
    send(&mut app, EditMessage::BeginRenameClip);
    send(&mut app, EditMessage::RenameDraftChanged("Verse".into()));
    select(&mut app, track, 1);
    assert_eq!(clip_name(&app, 0).as_deref(), Some("Verse"));
    assert!(app.clip_actions.renaming.is_none());
}
