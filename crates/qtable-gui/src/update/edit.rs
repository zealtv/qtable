//! Edit messages, routed as a complete typed domain.
use crate::app::App;
use crate::clip_actions;
use crate::drafts::TrackField;
use crate::edit_actions::{create_clip, execute_edit, execute_structural_edit, shifted_index};
use crate::grid::RenameTarget;
use crate::header_actions;
use crate::messages::{EditMessage, Message};
use iced::Task;
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;

fn begin_rename_target(app: &mut App, target: RenameTarget) -> Task<Message> {
    header_actions::begin_rename(app, target);
    header_actions::focus_rename_editor()
}

pub(super) fn update(app: &mut App, message: EditMessage) -> Task<Message> {
    match message {
        EditMessage::MixerDevice(message) => crate::mixer_device::update(app, message),
        EditMessage::PathChanged(track, value) => {
            let key = crate::drafts::DraftKey::Track(track, TrackField::Path);
            app.drafts.values.insert(key, value);
            app.drafts.errors.remove(&key);
        }
        EditMessage::PathSubmitted(track, path) => {
            let key = crate::drafts::DraftKey::Track(track, TrackField::Path);
            if crate::edit_actions::run_edit(
                app,
                Command::Edit(ProjectEdit::SetTrackPath { track, path }),
            ) {
                app.drafts.forget(key);
            } else if let Some(error) = app.status.message().map(str::to_owned) {
                app.drafts.errors.insert(key, error);
            }
        }
        EditMessage::FocusField(key) => {
            return crate::field_focus::focus_at_start(key);
        }
        EditMessage::AddTrack => execute_edit(
            app,
            Command::Edit(ProjectEdit::AddTrack {
                name: format!("Track {}", app.project().clip_tracks().count() + 1),
            }),
        ),
        EditMessage::AddScene => execute_edit(
            app,
            Command::Edit(ProjectEdit::AddScene {
                at: app.project().scenes.len(),
                name: format!("Scene {}", app.project().scenes.len() + 1),
            }),
        ),
        EditMessage::DeleteScenes(scene) => super::scene_actions::delete(app, scene),
        EditMessage::DeleteSelectedClip => clip_actions::delete(app),
        EditMessage::CutSelectedClip => clip_actions::cut(app),
        EditMessage::CopySelectedClip => clip_actions::copy(app),
        EditMessage::PasteClip => clip_actions::paste(app),
        EditMessage::CopyTracks => crate::track_clipboard::copy(app),
        EditMessage::PasteTracks => crate::track_clipboard::paste_held(app),
        EditMessage::DuplicateTracks => crate::track_clipboard::duplicate(app),
        EditMessage::BeginRenameClip => {
            clip_actions::begin_rename(app);
            return clip_actions::focus_rename_editor();
        }
        EditMessage::RenameDraftChanged(name) => app.clip_actions.draft = name,
        EditMessage::BeginRenameHeader(owner) => {
            return begin_rename_target(app, RenameTarget::Header(owner));
        }
        EditMessage::BeginRenameScene(scene) => {
            return begin_rename_target(app, RenameTarget::Scene(scene));
        }
        EditMessage::RenameHeaderDraftChanged(name) => app.header_actions.draft = name,
        EditMessage::CommitRenameHeader(name) => header_actions::commit_rename(app, name),
        EditMessage::RecolourHeader(owner, colour) => header_actions::recolour(app, owner, colour),
        EditMessage::DeleteTrack(track) => header_actions::delete_track(app, track),
        EditMessage::CommitRenameClip(name) => clip_actions::commit_rename(app, name),
        EditMessage::CreateSelectedClip => {
            if let Some(sel) = app.selection.clip {
                create_clip(app, sel.track, sel.slot);
            }
        }
        EditMessage::ToggleSelectedStop => {
            if let Some(sel) = app.selection.clip {
                let enabled = app
                    .project()
                    .clip_slot(sel.track, sel.slot)
                    .is_some_and(|s| !s.has_stop_button);
                execute_edit(
                    app,
                    Command::Edit(ProjectEdit::SetSlotStopButton {
                        track: sel.track,
                        slot: sel.slot,
                        enabled,
                    }),
                );
            }
        }
        EditMessage::ReorderTrack(delta) => {
            // A selected clip names its track; otherwise the selected track title does.
            let track = app
                .selection
                .clip
                .map(|sel| sel.track)
                .or(app.selection.anchor);
            if let Some(track) = track
                && let Some(model) = app.project().track(track)
            {
                // Reorder whole sibling subtrees at the existing parent level.
                // This is not a keyboard reparent gesture.
                let siblings: Vec<_> = app
                    .project()
                    .tracks
                    .iter()
                    .filter(|t| t.group == model.group)
                    .map(|t| t.id)
                    .collect();
                if let Some(index) = siblings.iter().position(|id| *id == track)
                    && let Some(target) = shifted_index(index, delta, siblings.len())
                    && let Some(source) = app.project().subtree_span(track)
                    && let Some(destination) = app.project().subtree_span(siblings[target])
                {
                    let to = if target < index {
                        destination.start
                    } else {
                        destination.end - source.len()
                    };
                    execute_edit(
                        app,
                        Command::Edit(ProjectEdit::MoveTrack {
                            from: source.start,
                            to,
                        }),
                    );
                }
            }
        }

        EditMessage::ReorderScene(delta) => {
            // A selected clip names its scene row; otherwise the selected scene does.
            let from = app.selection.clip.map(|sel| sel.slot).or_else(|| {
                let scene = app.selection.scene?;
                app.project().scenes.iter().position(|s| s.id == scene)
            });
            if let Some(from) = from
                && let Some(to) = shifted_index(from, delta, app.project().scenes.len())
                && execute_structural_edit(app, Command::Edit(ProjectEdit::MoveScene { from, to }))
                && let Some(selected) = app.selection.clip.as_mut()
            {
                selected.slot = to;
            }
        }
        EditMessage::DuplicateSelectedClip => clip_actions::duplicate(app),
        EditMessage::ToggleMoveClipMode => app.selection.move_clip = !app.selection.move_clip,
        EditMessage::RecolourSelected(colour) => {
            if let Some(sel) = app.selection.clip
                && clip_actions::selected_has_clip(app)
            {
                execute_edit(
                    app,
                    Command::Edit(ProjectEdit::RecolourClip {
                        track: sel.track,
                        slot: sel.slot,
                        colour,
                    }),
                );
            }
        }
    }
    Task::none()
}
