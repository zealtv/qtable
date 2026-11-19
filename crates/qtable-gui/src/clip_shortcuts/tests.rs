use super::*;
use crate::{drafts::DraftKey, files, grid, messages::EditMessage};
use qtable_engine::engine::Engine;

fn app() -> App {
    let mut app = App::new(
        Engine::new(files::new_project()).unwrap(),
        "120".into(),
        None,
    );
    let track = app.project().tracks[0].id;
    let _ = crate::update::update(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    let _ = crate::update::update(&mut app, Message::Edit(EditMessage::CreateSelectedClip));
    app.focus = Focus::Grid;
    app
}

fn press(app: &mut App, key: Key, modifiers: Modifiers) {
    let message = crate::focus::event_message(&key, modifiers, false, true).unwrap();
    let _ = crate::update::update(app, message);
}

fn command(app: &mut App, letter: &str) {
    press(app, Key::Character(letter.into()), Modifiers::COMMAND);
}

#[test]
fn shortcuts_copy_cut_paste_duplicate_and_delete_through_the_update_router() {
    let mut app = app();
    command(&mut app, "c");
    assert!(app.clip_actions.clipboard.is_some());
    command(&mut app, "x");
    assert!(app.project().tracks[0].slots[0].clip.is_none());
    command(&mut app, "v");
    assert!(app.project().tracks[0].slots[0].clip.is_some());
    command(&mut app, "d");
    assert!(app.project().tracks[0].slots[1].clip.is_some());
    press(&mut app, Key::Named(Named::Backspace), Modifiers::NONE);
    assert!(app.project().tracks[0].slots[0].clip.is_none());
    command(&mut app, "v");
    press(&mut app, Key::Named(Named::Delete), Modifiers::NONE);
    assert!(app.project().tracks[0].slots[0].clip.is_none());
    assert_eq!(app.focus, Focus::Grid);
    assert!(app.is_dirty());
}

#[test]
fn rename_opens_the_grid_editor_and_clip_keys_leave_its_draft_alone() {
    let mut app = app();
    let revision = app.project_revision();
    command(&mut app, "r");
    assert_eq!(app.clip_actions.renaming, app.selection.clip);
    assert_eq!(app.clip_actions.draft, "Clip");
    command(&mut app, "x");
    assert_eq!(app.project_revision(), revision);
    assert!(app.clip_actions.clipboard.is_none());
    assert!(app.clip_actions.renaming.is_some());
}

#[test]
fn other_regions_and_text_edits_block_clip_shortcuts_without_committing_drafts() {
    for focus in [Focus::Browser, Focus::Detail] {
        let mut app = app();
        app.focus = focus;
        let revision = app.project_revision();
        for shortcut in [
            Shortcut::Copy,
            Shortcut::Cut,
            Shortcut::Paste,
            Shortcut::Duplicate,
            Shortcut::Rename,
            Shortcut::Delete,
        ] {
            let _ = crate::update::update(&mut app, Message::ClipShortcut(shortcut));
        }
        assert_eq!(app.project_revision(), revision);
        assert!(app.clip_actions.clipboard.is_none());
        assert!(app.clip_actions.renaming.is_none());
    }
    let mut app = app();
    app.pending_field = Some(DraftKey::Tempo);
    app.tempo_input = "99".into();
    let revision = app.project_revision();
    command(&mut app, "x");
    assert_eq!(app.pending_field, Some(DraftKey::Tempo));
    assert_eq!(app.project_revision(), revision);
    assert!(app.clip_actions.clipboard.is_none());
    app.pending_field = None;
    app.header_actions.renaming = Some(grid::RenameTarget::Header(app.project().tracks[0].id));
    command(&mut app, "x");
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn captured_repeated_or_modified_keys_do_not_become_clip_commands() {
    for letter in ["c", "x", "v", "d", "r"] {
        let character = Key::Character(letter.into());
        assert!(key(&character, Modifiers::COMMAND, false, false).is_none());
        assert!(key(&character, Modifiers::COMMAND, true, true).is_none());
        for modifiers in [
            Modifiers::NONE,
            Modifiers::ALT,
            Modifiers::SHIFT | Modifiers::COMMAND,
        ] {
            assert!(key(&character, modifiers, false, true).is_none());
        }
    }
    assert!(key(&Key::Named(Named::Delete), Modifiers::NONE, false, false).is_none());
    assert!(
        key(
            &Key::Named(Named::Backspace),
            Modifiers::COMMAND,
            false,
            true
        )
        .is_none()
    );
}

#[test]
fn detail_delete_removes_the_selected_event_and_keeps_the_clip() {
    use crate::inspector::event_editor::{Action, ClipKey};
    use qtable_engine::model::{BeatTime, OscAddress};
    for delete_key in [Named::Delete, Named::Backspace] {
        let mut app = app();
        let selection = app.selection.clip.unwrap();
        let clip = app.project().tracks[0].slots[0].clip.as_ref().unwrap();
        let key = ClipKey {
            track: selection.track,
            slot: selection.slot,
            id: clip.id,
            epoch: app.clip_view.editor.epoch,
        };
        let path = OscAddress::Relative("/x".into());
        let x = app.project().track(key.track).unwrap().find_event(&path);
        let x = x.unwrap_or_else(|| {
            let edit = qtable_engine::edit::LaneEdit::AddPath("/x".into());
            crate::edit_actions::run_edit(
                &mut app,
                qtable_engine::engine::Command::Edit(qtable_engine::edit::ProjectEdit::EditLane {
                    track: key.track,
                    slot: key.slot,
                    edit,
                }),
            );
            app.project()
                .track(key.track)
                .unwrap()
                .find_event(&path)
                .unwrap()
        });
        let _ = crate::update::update(
            &mut app,
            Message::Inspector(crate::inspector::Message::Timeline(Action::Add(
                key,
                BeatTime(0),
                x.clone(),
            ))),
        );
        assert_eq!(app.focus, Focus::Detail);
        assert!(app.clip_view.editor.focused);
        press(&mut app, Key::Named(delete_key), Modifiers::NONE);
        let clip = app.project().tracks[0].slots[0].clip.as_ref().unwrap();
        assert!(clip.events.is_empty());
        assert_eq!(app.project().tracks.len(), 1);
    }
}

fn renamed(app: &mut App, name: &str) {
    let _ = crate::update::update(
        app,
        Message::Edit(EditMessage::RenameHeaderDraftChanged(name.into())),
    );
    let _ = crate::update::update(
        app,
        Message::Edit(EditMessage::CommitRenameHeader(name.into())),
    );
}

#[test]
fn rename_follows_the_selection_to_a_scene_a_track_or_an_output() {
    let mut app = app();
    let scene = app.project().scenes[0].id;
    let _ = crate::update::update(&mut app, Message::Grid(grid::Action::SelectScene(scene)));
    app.focus = Focus::Grid;
    command(&mut app, "r");
    assert_eq!(
        app.header_actions.renaming,
        Some(grid::RenameTarget::Scene(scene))
    );
    renamed(&mut app, "Verse");
    assert_eq!(app.project().scenes[0].name, "Verse");

    let track = app.project().tracks[0].id;
    let _ = crate::update::update(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(track, false)),
    );
    app.focus = Focus::Grid;
    command(&mut app, "r");
    assert_eq!(app.header_actions.draft, app.project().tracks[0].name);
    renamed(&mut app, "Bass");
    assert_eq!(app.project().tracks[0].name, "Bass");

    let _ = crate::update::update(
        &mut app,
        Message::Outputs(crate::messages::OutputMessage::AddOutput),
    );
    let output = app.project().outputs[0].id;
    let _ = crate::update::update(
        &mut app,
        Message::Grid(crate::grid::Action::HeaderMenu(output)),
    );
    assert_eq!(app.focus, Focus::Grid);
    command(&mut app, "r");
    assert_eq!(
        app.header_actions.renaming,
        Some(grid::RenameTarget::Header(output))
    );
    renamed(&mut app, "Lights");
    assert_eq!(app.project().outputs[0].name, "Lights");
}

#[test]
fn rename_with_nothing_selected_does_nothing() {
    let mut app = app();
    app.selection = Default::default();
    command(&mut app, "r");
    assert!(app.header_actions.renaming.is_none());
    assert!(app.clip_actions.renaming.is_none());
}

fn shift_command(app: &mut App, letter: &str) {
    press(
        app,
        Key::Character(letter.into()),
        Modifiers::COMMAND | Modifiers::SHIFT,
    );
}

#[test]
fn new_track_insert_scene_and_insert_clip_shortcuts_replace_the_button_row() {
    let mut app = app();
    let (tracks, scenes) = (app.project().tracks.len(), app.project().scenes.len());
    command(&mut app, "t");
    assert_eq!(app.project().tracks.len(), tracks + 1);
    command(&mut app, "i");
    assert_eq!(app.project().scenes.len(), scenes + 1);
    let track = app.project().tracks[0].id;
    let _ = crate::update::update(&mut app, Message::Grid(grid::Action::Select(track, 1)));
    assert!(app.project().tracks[0].slots[1].clip.is_none());
    shift_command(&mut app, "m");
    assert!(app.project().tracks[0].slots[1].clip.is_some());
}

#[test]
fn stop_button_shortcut_toggles_the_selected_slot() {
    let mut app = app();
    let before = app.project().tracks[0].slots[0].has_stop_button;
    command(&mut app, "e");
    assert_eq!(app.project().tracks[0].slots[0].has_stop_button, !before);
    command(&mut app, "e");
    assert_eq!(app.project().tracks[0].slots[0].has_stop_button, before);
}

#[test]
fn group_and_ungroup_shortcuts_act_on_the_selected_tracks() {
    let mut app = app();
    command(&mut app, "t");
    let ids: Vec<_> = app.project().tracks.iter().map(|t| t.id).collect();
    app.selection.clip = None;
    app.selection.tracks = ids.clone();
    command(&mut app, "g");
    assert_eq!(app.project().group_tracks().count(), 1);
    // Creation explicitly selects the new header; Ungroup returns selection to members.
    let group = app.project().group_tracks().next().unwrap().id;
    assert_eq!(
        app.selection.owner,
        Some(crate::selection::ColumnOwner::Group(group))
    );
    shift_command(&mut app, "g");
    assert!(app.project().group_tracks().next().is_none());
    assert_eq!(app.selection.tracks, [ids[0]]);
    assert_eq!(app.selection.anchor, Some(ids[0]));
}

#[test]
fn structure_shortcuts_wait_while_a_field_has_focus() {
    let mut app = app();
    app.focus = Focus::Detail;
    let tracks = app.project().tracks.len();
    command(&mut app, "t");
    assert_eq!(app.project().tracks.len(), tracks);
}

fn drag_fader(app: &mut App, volume: f64) -> qtable_engine::model::TrackId {
    use crate::control_focus::{ControlOwner, View};
    use crate::drafts::ControlId;
    let track = app.project().tracks[0].id;
    let id = ControlId::action(View::Mixer, "volume", ControlOwner::Track(track));
    let _ = crate::update::update(app, Message::ControlFocused(id));
    let _ = crate::update::update(
        app,
        Message::Mixer(crate::mixer_band::MixerMessage::SetVolume(track, volume)),
    );
    track
}

fn volume(app: &App, track: qtable_engine::model::TrackId) -> f64 {
    app.project()[track].mixer_device.volume.value
}

#[test]
fn delete_resets_a_fader_that_was_touched_last_and_keeps_the_selected_clip() {
    let mut app = app();
    let track = drag_fader(&mut app, 0.25);
    assert_eq!(volume(&app, track), 0.25);
    press(&mut app, Key::Named(Named::Delete), Modifiers::NONE);
    assert_eq!(volume(&app, track), 1.0);
    assert!(app.project().tracks[0].slots[0].clip.is_some());
}

#[test]
fn delete_removes_the_clip_clicked_after_a_fader_and_leaves_the_volume() {
    let mut app = app();
    let track = drag_fader(&mut app, 0.25);
    let _ = crate::update::update(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    press(&mut app, Key::Named(Named::Delete), Modifiers::NONE);
    assert!(app.project().tracks[0].slots[0].clip.is_none());
    assert_eq!(volume(&app, track), 0.25);
}

#[test]
fn a_text_field_keeps_its_backspace() {
    let mut app = app();
    let track = drag_fader(&mut app, 0.25);
    // A focused text field captures the key, so the router sees it as not ignored.
    assert!(
        crate::focus::event_message(&Key::Named(Named::Backspace), Modifiers::NONE, false, false)
            .is_none()
    );
    assert_eq!(volume(&app, track), 0.25);
    assert!(app.project().tracks[0].slots[0].clip.is_some());
}

#[test]
fn mains_focused_fader_resets_without_deleting_the_selected_clip() {
    use crate::control_focus::{ControlOwner, View};
    use crate::drafts::ControlId;
    use crate::mixer_band::MixerMessage;
    for key in [Named::Delete, Named::Backspace] {
        let mut app = app();
        let main = app.project().main.id;
        let id = ControlId::action(View::Mixer, "volume", ControlOwner::Track(main));
        let _ = crate::update::update(&mut app, Message::ControlFocused(id));
        let _ = crate::update::update(
            &mut app,
            Message::Mixer(MixerMessage::SetVolume(main, 0.25)),
        );
        assert_eq!(volume(&app, main), 0.25);
        press(&mut app, Key::Named(key), Modifiers::NONE);
        assert_eq!(volume(&app, main), 1.0);
        assert!(app.project().tracks[0].slots[0].clip.is_some());
    }
}

fn select_header(app: &mut App, track: qtable_engine::model::TrackId, extend: bool) {
    let _ = crate::update::update(app, Message::Grid(grid::Action::SelectTrack(track, extend)));
}

#[test]
fn both_delete_keys_remove_track_group_output_and_shift_range_as_one_edit() {
    for key in [Named::Delete, Named::Backspace] {
        for kind in ["track", "group", "output", "range"] {
            let mut app = app();
            command(&mut app, "t");
            command(&mut app, "t");
            let ids: Vec<_> = app.project().tracks.iter().map(|t| t.id).collect();
            let removed = match kind {
                "group" => {
                    select_header(&mut app, ids[0], false);
                    select_header(&mut app, ids[1], true);
                    command(&mut app, "g");
                    let group = app.project().group_tracks().next().unwrap().id;
                    let _ = crate::update::update(
                        &mut app,
                        Message::Grid(grid::Action::SelectGroup(group, false)),
                    );
                    vec![group, ids[0], ids[1]]
                }
                "output" => {
                    let _ =
                        crate::update::update(&mut app, Message::Outputs(OutputMessage::AddOutput));
                    let output = app.project().outputs[0].id;
                    select_header(&mut app, output, false);
                    vec![output]
                }
                "range" => {
                    select_header(&mut app, ids[0], false);
                    select_header(&mut app, ids[1], true);
                    vec![ids[0], ids[1]]
                }
                _ => {
                    select_header(&mut app, ids[0], false);
                    vec![ids[0]]
                }
            };
            let revision = app.project_revision();
            press(&mut app, Key::Named(key), Modifiers::NONE);
            assert_eq!(app.project_revision(), revision + 1, "{kind}");
            for id in removed {
                assert!(app.project().track(id).is_none(), "{kind}");
            }
            assert!(app.selection.tracks.is_empty());
            assert!(app.selection.anchor.is_none());
            assert!(app.selection.owner.is_none());
            assert!(app.selection.ghost_clip.is_none());
            assert_eq!(app.focus, Focus::Grid);
        }
    }
}

#[test]
fn main_is_filtered_from_header_deletion_and_last_track_rejection_is_atomic() {
    for key in [Named::Delete, Named::Backspace] {
        let mut app = app();
        let main = app.project().main.id;
        let _ = crate::update::update(
            &mut app,
            Message::Grid(grid::Action::SelectColumn(
                crate::selection::ColumnOwner::Track(main),
                None,
            )),
        );
        let revision = app.project_revision();
        press(&mut app, Key::Named(key), Modifiers::NONE);
        assert_eq!(app.project_revision(), revision);
        assert!(app.project().track(main).is_some());
        command(&mut app, "t");
        let ids: Vec<_> = app.project().tracks.iter().map(|t| t.id).collect();
        select_header(&mut app, ids[0], false);
        app.selection.tracks.push(main);
        press(&mut app, Key::Named(key), Modifiers::NONE);
        assert!(app.project().track(ids[0]).is_none());
        assert!(app.project().track(main).is_some());
        select_header(&mut app, ids[1], false);
        let before = app.project().clone();
        let revision = app.project_revision();
        press(&mut app, Key::Named(key), Modifiers::NONE);
        assert_eq!(app.project(), &before);
        assert_eq!(app.project_revision(), revision);
        assert_eq!(app.selection.tracks, vec![ids[1]]);
    }
}

#[test]
fn header_delete_waits_for_fields_and_both_rename_editors_before_fader_reset() {
    for key in [Named::Delete, Named::Backspace] {
        for blocker in ["field", "header rename", "clip rename"] {
            let mut app = app();
            command(&mut app, "t");
            let track = app.project().tracks[0].id;
            select_header(&mut app, track, false);
            drag_fader(&mut app, 0.25);
            match blocker {
                "field" => app.pending_field = Some(DraftKey::Tempo),
                "header rename" => {
                    app.header_actions.renaming = Some(grid::RenameTarget::Header(track))
                }
                _ => app.clip_actions.renaming = Some(grid::Selection { track, slot: 0 }),
            }
            let revision = app.project_revision();
            press(&mut app, Key::Named(key), Modifiers::NONE);
            assert_eq!(app.project_revision(), revision, "{blocker}");
            assert_eq!(volume(&app, track), 0.25);
            assert!(app.project().track(track).is_some());
        }
    }
}

#[test]
fn latched_fader_takes_header_delete_and_a_later_header_selection_deletes_the_column() {
    for key in [Named::Delete, Named::Backspace] {
        let mut app = app();
        command(&mut app, "t");
        let track = app.project().tracks[0].id;
        select_header(&mut app, track, false);
        drag_fader(&mut app, 0.25);
        press(&mut app, Key::Named(key), Modifiers::NONE);
        assert_eq!(volume(&app, track), 1.0);
        select_header(&mut app, track, false);
        press(&mut app, Key::Named(key), Modifiers::NONE);
        assert!(app.project().track(track).is_none());
    }
}

#[test]
fn mixed_header_selection_removes_group_members_and_outputs_in_one_edit() {
    use qtable_engine::model::Route;
    for key in [Named::Delete, Named::Backspace] {
        let mut app = app();
        command(&mut app, "t");
        command(&mut app, "t");
        let ids: Vec<_> = app.project().tracks.iter().map(|t| t.id).collect();
        select_header(&mut app, ids[0], false);
        select_header(&mut app, ids[1], true);
        command(&mut app, "g");
        let group = app.project().group_tracks().next().unwrap().id;
        let _ = crate::update::update(&mut app, Message::Outputs(OutputMessage::AddOutput));
        let output = app.project().outputs[0].id;
        crate::edit_actions::execute_edit(
            &mut app,
            qtable_engine::engine::Command::Edit(
                qtable_engine::edit::ProjectEdit::SetTrackRoutes {
                    routes: vec![(ids[2], Route::single(output))],
                },
            ),
        );
        let _ = crate::update::update(
            &mut app,
            Message::Grid(grid::Action::SelectGroup(group, false)),
        );
        app.selection.tracks.push(output);
        let revision = app.project_revision();
        press(&mut app, Key::Named(key), Modifiers::NONE);
        assert_eq!(app.project_revision(), revision + 1);
        assert!(app.project().track(group).is_none());
        assert!(app.project().track(ids[0]).is_none());
        assert!(app.project().track(ids[1]).is_none());
        assert!(app.project().outputs.is_empty());
        assert_eq!(app.project().track(ids[2]).unwrap().route, Route::empty());
    }
}

#[test]
fn detail_and_browser_focus_do_not_delete_selected_headers() {
    for focus in [Focus::Detail, Focus::Browser] {
        for key in [Named::Delete, Named::Backspace] {
            let mut app = app();
            command(&mut app, "t");
            let track = app.project().tracks[0].id;
            select_header(&mut app, track, false);
            app.focus = focus;
            let revision = app.project_revision();
            press(&mut app, Key::Named(key), Modifiers::NONE);
            assert_eq!(app.project_revision(), revision);
            assert!(app.project().track(track).is_some());
        }
    }
}

#[test]
fn selected_output_cells_and_scene_rows_do_not_become_header_deletions() {
    for key in [Named::Delete, Named::Backspace] {
        let mut app = app();
        let _ = crate::update::update(&mut app, Message::Outputs(OutputMessage::AddOutput));
        let output = app.project().outputs[0].id;
        let _ = crate::update::update(
            &mut app,
            Message::Grid(grid::Action::SelectColumn(
                crate::selection::ColumnOwner::Track(output),
                Some(0),
            )),
        );
        let revision = app.project_revision();
        press(&mut app, Key::Named(key), Modifiers::NONE);
        assert_eq!(app.project_revision(), revision);
        assert!(app.project().track(output).is_some());
        let scene = app.project().scenes[0].id;
        let _ = crate::update::update(&mut app, Message::Grid(grid::Action::SelectScene(scene)));
        press(&mut app, Key::Named(key), Modifiers::NONE);
        assert_eq!(app.project_revision(), revision + 1);
        assert!(!app.project().scenes.iter().any(|s| s.id == scene));
        assert!(app.project().track(output).is_some());
        assert_eq!(app.project().tracks.len(), 1);
    }
}

#[test]
fn command_g_and_header_menu_group_one_or_many_through_the_same_message() {
    use crate::context_menu::{self, Entry, MenuTarget};
    use qtable_engine::model::Route;
    for count in [1, 2] {
        for keyboard in [false, true] {
            let mut app = app();
            if count > 1 {
                command(&mut app, "t");
            }
            let ids: Vec<_> = app
                .project()
                .tracks
                .iter()
                .take(count)
                .map(|t| t.id)
                .collect();
            select_header(&mut app, ids[0], false);
            if count > 1 {
                select_header(&mut app, ids[1], true);
            }
            let _ =
                crate::update::update(&mut app, Message::Grid(grid::Action::HeaderMenu(ids[0])));
            let entries = context_menu::current_entries(&app, MenuTarget::Header(ids[0]));
            let message = entries
                .iter()
                .find_map(|entry| match entry {
                    Entry::Command {
                        label: "Group Tracks",
                        enabled: true,
                        message,
                        ..
                    } => Some(message.clone()),
                    _ => None,
                })
                .expect("selected plain tracks can group from the menu");
            assert!(matches!(
                message,
                Message::Outputs(OutputMessage::CreateGroupAroundSelected)
            ));
            let revision = app.project_revision();
            if keyboard {
                command(&mut app, "g");
            } else {
                let _ = crate::update::update(&mut app, message);
            }
            assert_eq!(app.project_revision(), revision + 1);
            let group = app.project().group_tracks().next().unwrap().id;
            assert_eq!(app.project().group_member_ids(group).unwrap(), ids);
            for id in ids {
                let track = app.project().track(id).unwrap();
                assert_eq!(track.route, Route::Group);
                assert_eq!(track.group, Some(group));
            }
            assert!(app.menu_target().is_none());
        }
    }
}

#[test]
fn command_g_and_menu_exclude_main_and_outputs() {
    use crate::context_menu::{self, Entry, MenuTarget};
    let mut app = app();
    let member = app.project().tracks[0].id;
    select_header(&mut app, member, false);
    command(&mut app, "g");
    let group = app.project().group_tracks().next().unwrap().id;
    let _ = crate::update::update(&mut app, Message::Outputs(OutputMessage::AddOutput));
    assert!(app.project().track(group).is_some());
    for id in [app.project().main.id, app.project().outputs[0].id] {
        let _ = crate::update::update(&mut app, Message::Grid(grid::Action::HeaderMenu(id)));
        let entries = context_menu::current_entries(&app, MenuTarget::Header(id));
        assert!(!entries.iter().any(|entry| matches!(
            entry,
            Entry::Command {
                label: "Group Tracks",
                ..
            }
        )));
        let before = app.project().clone();
        let revision = app.project_revision();
        command(&mut app, "g");
        assert_eq!(app.project(), &before);
        assert_eq!(app.project_revision(), revision);
    }
}

#[test]
fn grouping_a_selected_group_header_wraps_it_without_flattening() {
    let mut app = app();
    let member = app.project().tracks[0].id;
    select_header(&mut app, member, false);
    command(&mut app, "g");
    let inner = app.project().group_tracks().next().unwrap().id;
    let revision = app.project_revision();
    command(&mut app, "g");
    let outer = app.project()[inner].group.unwrap();
    assert_eq!(app.project()[member].group, Some(inner));
    assert_eq!(app.project()[outer].group, None);
    assert_eq!(app.project_revision(), revision + 1);
}
