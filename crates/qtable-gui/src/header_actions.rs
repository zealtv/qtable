//! Commands on a track title or group header: rename in place, recolour and delete.
//! The same in-place editor also renames a scene (in Main) or an output (in its column).
//! Renaming mirrors the clip rename in `clip_actions`; only one editor is open at a time.
use crate::app::App;
use crate::drafts::OutputField;
use crate::drafts::TrackField;
use crate::drafts::{DraftKey, SceneField};
use crate::edit_actions::{execute_edit, execute_structural_edit, run_edit};
use crate::grid::{self, RenameTarget};
use crate::messages::{EditMessage, Message};
use iced::{Element, Length};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::{Colour, TrackId};
use std::rc::Rc;

/// GUI state for header commands.
#[derive(Default)]
pub(crate) struct HeaderActions {
    /// The header, scene or output whose name is being edited in place, if any.
    pub renaming: Option<RenameTarget>,
    /// The text in the rename editor while `renaming` is set.
    pub draft: String,
}

/// The header's name and colour as the model holds them now.
pub(crate) fn name_and_colour(app: &App, owner: TrackId) -> Option<(String, Colour)> {
    let project = app.project();
    let track = project.track(owner)?;
    Some((track.name.clone(), track.colour))
}

/// The target's name as the model holds it now; None once it has gone.
fn current_name(app: &App, target: RenameTarget) -> Option<String> {
    let project = app.project();
    match target {
        RenameTarget::Header(owner) => project
            .track_capabilities(owner)
            .filter(|caps| caps.renamable)
            .and_then(|_| name_and_colour(app, owner))
            .map(|(name, _)| name),
        RenameTarget::Scene(id) => Some(project.scenes.iter().find(|s| s.id == id)?.name.clone()),
    }
}

/// Starts in-place renaming, with the current name as the draft.
pub(crate) fn begin_rename(app: &mut App, target: RenameTarget) {
    crate::clip_actions::cancel_rename(app);
    let name = current_name(app, target);
    app.header_actions.renaming = name.as_ref().map(|_| target);
    app.header_actions.draft = name.unwrap_or_default();
}

/// A selected group header, using the same rule as highlights and clipboard commands.
pub(crate) fn selected_group(app: &App) -> Option<TrackId> {
    app.project()
        .group_tracks()
        .find(|g| {
            crate::selection::SelectionState::group_selected(
                &app.selection.tracks,
                &app.project().tracks,
                g.id,
            )
        })
        .map(|g| g.id)
}

/// ⌘R, as in Live's Edit ▸ Rename: renames the selected clip, else the selected scene, else
/// the selected group, else the selected track or output (the first of several). The task puts the
/// cursor in the editor that opened, if any.
pub(crate) fn begin_rename_selected(app: &mut App) -> iced::Task<Message> {
    let selection = &app.selection;
    if selection.clip.is_some() {
        crate::clip_actions::begin_rename(app);
        return if app.clip_actions.renaming.is_some() {
            crate::clip_actions::focus_rename_editor()
        } else {
            iced::Task::none()
        };
    }
    let target = selection.scene.map(RenameTarget::Scene).or_else(|| {
        if let Some(group) = selected_group(app) {
            return Some(RenameTarget::Header(group));
        }
        let track = selection
            .anchor
            .or_else(|| selection.tracks.first().copied())?;
        Some(RenameTarget::Header(track))
    });
    let Some(target) = target else {
        return iced::Task::none();
    };
    begin_rename(app, target);
    focus_rename_editor()
}

pub(crate) fn cancel_rename(app: &mut App) {
    app.header_actions.renaming = None;
}

/// Id of the grid's header rename field.
pub(crate) fn rename_editor_id() -> iced::widget::Id {
    iced::widget::Id::new("header-rename")
}

/// Puts the cursor in the rename editor with the whole name selected.
pub(crate) fn focus_rename_editor() -> iced::Task<Message> {
    iced::Task::batch([
        iced::widget::operation::focus(rename_editor_id()),
        iced::widget::operation::select_all(rename_editor_id()),
    ])
}

/// Commits the name; the edit mode ends only once the engine accepts it.
pub(crate) fn commit_rename(app: &mut App, name: String) {
    let Some(target) = app.header_actions.renaming else {
        return;
    };
    let (edit, draft) = match target {
        RenameTarget::Header(track) => (
            ProjectEdit::RenameTrack { track, name },
            DraftKey::Track(track, TrackField::Name),
        ),
        RenameTarget::Scene(scene) => (
            ProjectEdit::RenameScene { scene, name },
            DraftKey::scene(scene, SceneField::Name),
        ),
    };
    if run_edit(app, Command::Edit(edit)) {
        app.drafts.forget(draft);
        app.header_actions.renaming = None;
    }
}

/// Keeps the typed name when something else in the grid is clicked mid-rename.
pub(crate) fn commit_draft(app: &mut App) {
    if app.header_actions.renaming.is_some() {
        let name = app.header_actions.draft.clone();
        commit_rename(app, name);
        // A name the engine rejects must not leave the editor stuck open.
        app.header_actions.renaming = None;
    }
}

pub(crate) fn recolour(app: &mut App, owner: TrackId, colour: Colour) {
    if app
        .project()
        .track_capabilities(owner)
        .is_some_and(|caps| caps.recolourable)
    {
        let track = owner;
        execute_edit(
            app,
            Command::Edit(ProjectEdit::RecolourTrack { track, colour }),
        );
    }
}

/// Menu and keyboard deletion share one atomic engine edit and UI cleanup.
pub(crate) fn delete_track(app: &mut App, track: TrackId) {
    if app.selection.tracks.contains(&track) {
        delete_selected(app);
    } else {
        delete_tracks(app, vec![track]);
    }
}

pub(crate) fn delete_selected(app: &mut App) {
    delete_tracks(app, app.selection.tracks.clone());
}

fn delete_tracks(app: &mut App, mut tracks: Vec<TrackId>) {
    tracks.retain(|id| {
        app.project()
            .track_capabilities(*id)
            .is_some_and(|caps| caps.removable)
    });
    tracks.sort_unstable_by_key(|id| id.0);
    tracks.dedup();
    if tracks.is_empty() {
        return;
    }
    let before: Vec<_> = app
        .project()
        .tracks
        .iter()
        .chain(&app.project().outputs)
        .map(|t| t.id)
        .collect();
    if execute_structural_edit(app, Command::Edit(ProjectEdit::RemoveTracks { tracks })) {
        let removed: Vec<_> = before
            .into_iter()
            .filter(|id| app.project().track(*id).is_none())
            .collect();
        for track in &removed {
            for key in [
                DraftKey::Track(*track, TrackField::Name),
                DraftKey::Output(*track, OutputField::Host),
                DraftKey::Output(*track, OutputField::Port),
                DraftKey::Track(*track, TrackField::Path),
                DraftKey::Output(*track, OutputField::Latency),
            ] {
                app.drafts.forget(key);
            }
        }
        app.selection.tracks.retain(|id| !removed.contains(id));
        if app.selection.anchor.is_some_and(|id| removed.contains(&id)) {
            app.selection.anchor = None;
        }
        if app
            .selection
            .clip
            .is_some_and(|sel| removed.contains(&sel.track))
        {
            app.selection.clip = None;
        }
        if app.selection.owner.is_some_and(|owner| match owner {
            crate::selection::ColumnOwner::Track(id) | crate::selection::ColumnOwner::Group(id) => {
                removed.contains(&id)
            }
        }) {
            app.selection.owner = None;
            app.selection.clear_cell();
        }
        app.header_actions.renaming = None;
        app.clip_actions.renaming = None;
    }
}

/// The in-place name field for the header, scene or output being renamed: Enter keeps the
/// name, Escape (the app-wide cancel) drops it.
pub(crate) fn rename_editor<'a>(app: &'a App) -> Option<grid::NameEditor<'a, Message>> {
    let target = app.header_actions.renaming?;
    let draft = app.header_actions.draft.clone();
    let editor = move |width: f32| -> Element<'a, Message> {
        crate::style::input("Name", &draft)
            .id(rename_editor_id())
            .on_input(|name| Message::Edit(EditMessage::RenameHeaderDraftChanged(name)))
            .on_submit(Message::Edit(EditMessage::CommitRenameHeader(
                draft.clone(),
            )))
            .width(Length::Fixed(width))
            .into()
    };
    Some((target, Rc::new(editor)))
}

/// Translate the current explicit header owner for the shared engine planner.
/// Header ids remain distinct from affected descendant leaves.
pub(crate) fn grouping_plan(app: &App) -> Option<qtable_engine::model::SiblingRun> {
    qtable_engine::edit::grouping_plan(app.project(), &app.selection.tracks).ok()
}

/// Menu and shortcut eligibility use the same plan as submission.
pub(crate) fn can_group(app: &App) -> bool {
    grouping_plan(app).is_some()
}
