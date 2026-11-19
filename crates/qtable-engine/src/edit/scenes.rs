//! Scene edits: add, remove, rename, follow actions and reordering.
use crate::model::{FollowAction, ModelError, Project, Scene, SceneFollowActions, SceneId, Slot};

fn err(message: &str) -> ModelError {
    ModelError(message.to_owned())
}

pub(super) fn add(project: &mut Project, at: usize, name: String) -> Result<(), ModelError> {
    if at > project.scenes.len() {
        return Err(err("scene index out of range"));
    }
    let id = SceneId(project.scenes.iter().map(|s| s.id.0).max().unwrap_or(0) + 1);
    project.scenes.insert(
        at,
        Scene {
            id,
            name,
            follow_actions: Default::default(),
        },
    );
    for track in &mut project.tracks {
        track.slots.insert(at, Slot::default());
    }
    Ok(())
}

pub(super) fn remove(project: &mut Project, scenes: &[SceneId]) -> Result<(), ModelError> {
    for scene in scenes {
        if !project.scenes.iter().any(|s| s.id == *scene) {
            return Err(err("unknown scene"));
        }
    }
    let rows: Vec<_> = project
        .scenes
        .iter()
        .enumerate()
        .filter_map(|(row, scene)| scenes.contains(&scene.id).then_some(row))
        .collect();
    require_remaining_scene(project.scenes.len(), rows.len())?;
    for scene in scenes {
        reject_jump_reference(project, *scene)?;
    }
    for row in rows.into_iter().rev() {
        project.scenes.remove(row);
        for track in &mut project.tracks {
            track.slots.remove(row);
        }
    }
    Ok(())
}

/// Last-scene policy: keep the existing model invariant and reject an all-scene
/// removal atomically. Manager direction, 2026-11-06: treat the last scene as
/// undeletable; rec-232107 ends with one scene (the final keypress is not visible).
fn require_remaining_scene(total: usize, removed: usize) -> Result<(), ModelError> {
    if removed == total {
        Err(err("a set needs at least one scene"))
    } else {
        Ok(())
    }
}

pub(super) fn rename(
    project: &mut Project,
    scene: SceneId,
    name: String,
) -> Result<(), ModelError> {
    project
        .scenes
        .iter_mut()
        .find(|s| s.id == scene)
        .ok_or_else(|| err("unknown scene"))?
        .name = name;
    Ok(())
}

pub(super) fn set_follow_actions(
    project: &mut Project,
    scene: SceneId,
    follow_actions: SceneFollowActions,
) -> Result<(), ModelError> {
    project
        .scenes
        .iter_mut()
        .find(|s| s.id == scene)
        .ok_or_else(|| err("unknown scene"))?
        .follow_actions = follow_actions;
    Ok(())
}

pub(super) fn move_scene(project: &mut Project, from: usize, to: usize) -> Result<(), ModelError> {
    move_index(&mut project.scenes, from, to)?;
    for track in &mut project.tracks {
        move_index(&mut track.slots, from, to)?;
    }
    Ok(())
}

fn move_index<T>(items: &mut Vec<T>, from: usize, to: usize) -> Result<(), ModelError> {
    if from >= items.len() || to >= items.len() {
        return Err(err("move index out of range"));
    }
    let item = items.remove(from);
    items.insert(to, item);
    Ok(())
}

fn reject_jump_reference(project: &Project, scene: SceneId) -> Result<(), ModelError> {
    let has_jump = project
        .scenes
        .iter()
        .flat_map(|s| {
            [
                s.follow_actions.actions.action_a,
                s.follow_actions.actions.action_b,
            ]
        })
        .chain(project.tracks.iter().flat_map(|t| {
            t.slots
                .iter()
                .filter_map(|s| s.clip.as_ref())
                .flat_map(|c| {
                    [
                        c.follow_actions.actions.action_a,
                        c.follow_actions.actions.action_b,
                    ]
                })
        }))
        .any(|a| a == FollowAction::Jump(scene));
    if has_jump {
        Err(err(
            "cannot remove a scene referenced by a Jump follow action",
        ))
    } else {
        Ok(())
    }
}

/// Collect even a disjoint selection in project order; boundaries precede removal.
pub(super) fn move_scenes(
    project: &mut Project,
    scenes: &[SceneId],
    boundary: usize,
) -> Result<(), ModelError> {
    if boundary > project.scenes.len() {
        return Err(err("scene boundary out of range"));
    }
    if scenes
        .iter()
        .any(|id| !project.scenes.iter().any(|s| s.id == *id))
    {
        return Err(err("unknown scene"));
    }
    let rows: Vec<_> = project
        .scenes
        .iter()
        .enumerate()
        .filter_map(|(row, scene)| scenes.contains(&scene.id).then_some(row))
        .collect();
    let at = boundary - rows.iter().filter(|&&row| row < boundary).count();
    move_rows(&mut project.scenes, &rows, at);
    for track in &mut project.tracks {
        move_rows(&mut track.slots, &rows, at);
    }
    Ok(())
}

fn move_rows<T>(items: &mut Vec<T>, rows: &[usize], at: usize) {
    let mut moved: Vec<_> = rows.iter().rev().map(|&row| items.remove(row)).collect();
    moved.reverse();
    items.splice(at..at, moved);
}
