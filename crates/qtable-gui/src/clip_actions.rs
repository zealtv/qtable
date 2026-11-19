//! Clip commands on the selected slot: cut, copy, paste, duplicate, delete and rename.
//! The clipboard belongs to this GUI session, not the project or the OS.
use crate::app::App;
use crate::edit_actions::{execute_structural_edit, run_edit};
use crate::grid;
use crate::messages::{EditMessage, Message};
use iced::{Element, Length};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::{Clip, Track};
use std::rc::Rc;

/// What the grid's one clipboard holds, as Live's single clipboard does: a copy of tracks
/// replaces a copied clip and the reverse.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GridClipboard {
    /// A copied or cut clip, kept whole so paste reproduces it, with the definitions it plays
    /// so a paste onto another track adopts them.
    Clip {
        clip: Clip,
        carried: Vec<qtable_engine::edit::Carried>,
    },
    /// Copied columns of one family, in project order: a group track precedes its members.
    Tracks {
        generation: u64,
        tracks: Vec<Track>,
    },
    Devices(Vec<qtable_engine::model::Device>),
}

/// GUI state for clip commands.
#[derive(Default)]
pub(crate) struct ClipActions {
    pub clipboard: Option<GridClipboard>,
    /// The slot whose name the grid is editing in place, if any.
    pub renaming: Option<grid::Selection>,
    /// The text in the rename editor while `renaming` is set.
    pub draft: String,
}

fn selected_clip(app: &App) -> Option<(grid::Selection, GridClipboard)> {
    let sel = app.selection.clip?;
    let track = app.project().track(sel.track)?;
    let clip = track.clip(sel.slot)?.clone();
    let carried = qtable_engine::edit::Carried::from_clip(track, &clip);
    Some((sel, GridClipboard::Clip { clip, carried }))
}

/// True when the selected slot holds a clip.
pub(crate) fn selected_has_clip(app: &App) -> bool {
    selected_clip(app).is_some()
}

/// True when the clipboard holds a clip a slot can take.
pub(crate) fn has_clip_to_paste(app: &App) -> bool {
    matches!(app.clip_actions.clipboard, Some(GridClipboard::Clip { .. }))
}

/// One availability rule for the grid shortcut and slot menu.
pub(crate) fn can_paste(app: &App) -> bool {
    crate::device_chain::can_paste_grid(app)
        || crate::track_clipboard::holds_tracks(app)
        || (has_clip_to_paste(app) && app.selection.clip.is_some())
}

/// A selected slot wins over its track: ⌘C copies the clip there, else the selected columns.
pub(crate) fn copy(app: &mut App) {
    if app.selection.clip.is_some() {
        if let Some((_, clip)) = selected_clip(app) {
            app.clip_actions.clipboard = Some(clip);
        }
    } else {
        crate::track_clipboard::copy(app);
    }
}

/// Cut keeps the clipboard only if the slot really clears.
pub(crate) fn cut(app: &mut App) {
    if let Some((sel, clip)) = selected_clip(app)
        && delete_slot(app, sel)
    {
        app.clip_actions.clipboard = Some(clip);
    }
}

pub(crate) fn paste(app: &mut App) {
    if !can_paste(app) {
        return;
    }
    match app.clip_actions.clipboard.clone() {
        Some(GridClipboard::Clip { clip, carried }) => {
            if let Some(sel) = app.selection.clip {
                execute_structural_edit(
                    app,
                    Command::Edit(ProjectEdit::PasteClip {
                        track: sel.track,
                        slot: sel.slot,
                        clip: Box::new(clip),
                        carried,
                    }),
                );
            }
        }
        Some(GridClipboard::Tracks { generation, tracks })
            if generation == app.project_generation =>
        {
            crate::track_clipboard::paste(app, tracks)
        }
        Some(GridClipboard::Devices(_)) => {
            crate::device_chain::paste(app, false);
        }
        Some(GridClipboard::Tracks { .. }) => {}
        None => {}
    }
}

pub(crate) fn duplicate(app: &mut App) {
    if app.selection.clip.is_none() {
        crate::track_clipboard::duplicate(app);
    } else if let Some((sel, _)) = selected_clip(app) {
        // The engine owns both the copy and any required scene insertion, so
        // the GUI submits one atomic edit and refreshes the whole grid.
        execute_structural_edit(
            app,
            Command::Edit(ProjectEdit::DuplicateClip {
                track: sel.track,
                slot: sel.slot,
            }),
        );
    }
}

pub(crate) fn delete(app: &mut App) {
    if let Some((sel, _)) = selected_clip(app) {
        delete_slot(app, sel);
    }
}

fn delete_slot(app: &mut App, sel: grid::Selection) -> bool {
    app.clip_actions.renaming = None;
    execute_structural_edit(
        app,
        Command::Edit(ProjectEdit::DeleteClip {
            track: sel.track,
            slot: sel.slot,
        }),
    )
}

/// Starts in-place renaming of the selected clip, with its current name as the draft.
pub(crate) fn begin_rename(app: &mut App) {
    crate::header_actions::cancel_rename(app);
    let selected = selected_clip(app);
    app.clip_actions.draft = selected
        .as_ref()
        .and_then(|(_, held)| match held {
            GridClipboard::Clip { clip, .. } => Some(clip.name.clone()),
            GridClipboard::Tracks { .. } | GridClipboard::Devices(_) => None,
        })
        .unwrap_or_default();
    app.clip_actions.renaming = selected.map(|(sel, _)| sel);
}

/// Id of the grid's rename text field.
pub(crate) fn rename_editor_id() -> iced::widget::Id {
    iced::widget::Id::new("clip-rename")
}

/// Puts the cursor in the rename editor with the whole name selected, as Live does.
pub(crate) fn focus_rename_editor() -> iced::Task<crate::messages::Message> {
    iced::Task::batch([
        iced::widget::operation::focus(rename_editor_id()),
        iced::widget::operation::select_all(rename_editor_id()),
    ])
}

/// Keeps the typed name when something else in the grid is clicked mid-rename.
pub(crate) fn commit_draft(app: &mut App) {
    if app.clip_actions.renaming.is_some() {
        let name = app.clip_actions.draft.clone();
        commit_rename(app, name);
        // A name the engine rejects must not leave the editor stuck open.
        app.clip_actions.renaming = None;
    }
}

pub(crate) fn cancel_rename(app: &mut App) {
    app.clip_actions.renaming = None;
}

/// Commits the name; the edit mode ends only once the engine accepts it.
pub(crate) fn commit_rename(app: &mut App, name: String) {
    if let Some(sel) = app.clip_actions.renaming
        && run_edit(
            app,
            Command::Edit(ProjectEdit::RenameClip {
                track: sel.track,
                slot: sel.slot,
                name,
            }),
        )
    {
        app.clip_actions.renaming = None;
    }
}

/// The grid's in-place name field for the slot being renamed: Enter keeps the name, Escape
/// (the app-wide cancel) drops it.
pub(crate) fn rename_editor<'a>(app: &'a App) -> Option<grid::RenameEditor<'a, Message>> {
    let sel = app.clip_actions.renaming?;
    let draft = app.clip_actions.draft.clone();
    let editor = move |width: f32| -> Element<'a, Message> {
        crate::style::input("Name", &draft)
            .id(rename_editor_id())
            .on_input(|name| Message::Edit(EditMessage::RenameDraftChanged(name)))
            .on_submit(Message::Edit(EditMessage::CommitRenameClip(draft.clone())))
            .width(Length::Fixed(width))
            .into()
    };
    Some((sel, Rc::new(editor)))
}
