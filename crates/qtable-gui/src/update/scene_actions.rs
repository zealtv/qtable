//! Scene selection edits shared by the menu and keyboard.
use crate::{app::App, edit_actions::execute_structural_edit, grid};
use qtable_engine::{edit::ProjectEdit, engine::Command, model::SceneId};

pub(super) fn delete(app: &mut App, scene: SceneId) {
    let scenes = if app.selection.scenes.members.contains(&scene) {
        app.selection.scenes.members.clone()
    } else {
        vec![scene]
    };
    let Some(row) = app
        .project()
        .scenes
        .iter()
        .position(|s| scenes.contains(&s.id))
    else {
        return;
    };
    if execute_structural_edit(app, Command::Edit(ProjectEdit::RemoveScenes { scenes })) {
        let next = app.project().scenes[row.min(app.project().scenes.len() - 1)].id;
        super::grid_actions::update(app, grid::Action::SelectScene(next));
        app.header_actions.renaming = None;
        app.clip_actions.renaming = None;
    }
}

/// Programmatic navigation stays a single selection even while modifiers are held.
/// Only a pointer press extends or toggles the scene set.
pub(super) fn click(app: &mut App, scene: SceneId) {
    let mut selected = app.selection.scenes.clone();
    let primary = selected.click(
        &app.project().scenes,
        scene,
        app.selection.shift_down,
        app.selection.command_down,
    );
    if !app.project().scenes.iter().any(|s| s.id == scene) {
        return;
    }
    super::grid_actions::update(app, grid::Action::SelectScene(primary.unwrap_or(scene)));
    app.selection.scene = primary;
    app.selection.scenes = selected;
    if primary.is_none() {
        app.selection.clear_cell();
    }
}

pub(super) fn move_scenes(app: &mut App, scenes: Vec<SceneId>, boundary: usize) {
    // The engine owns row/slot movement; GUI selection remains stable-ID based.
    execute_structural_edit(
        app,
        Command::Edit(ProjectEdit::MoveScenes { scenes, boundary }),
    );
}

#[cfg(test)]
mod tests;
