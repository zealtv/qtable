//! Inspector edits: translate clip and scene inspector messages into project edits.
use crate::app::App;
use crate::drafts::{ClipField, DraftKey, SceneField};
use crate::edit_actions::execute_edit;
use crate::inspector;
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::{Project, TrackId};

pub(crate) fn execute_inspector_edit(app: &mut App, key: DraftKey, edit: ProjectEdit) {
    crate::edit_actions::run_edit_report(app, Command::Edit(edit), Some(key));
}

pub(crate) fn handle_inspector(app: &mut App, message: inspector::Message) {
    let Some(message) = inspector::clip_panel_update::handle(app, message)
        .and_then(|message| inspector::scene_update::handle(app, message))
        .and_then(|message| inspector::lane_update::handle(app, message))
    else {
        return;
    };
    use inspector::Message as M;
    use qtable_engine::model::BeatTime;
    let current_follow = |project: &Project, track: TrackId, slot: usize| {
        project.clip(track, slot).map(|c| c.follow_actions.clone())
    };
    match message {
        M::CommitNumber => crate::field_focus::commit(app),
        M::Info(_)
        | M::ToggleSection(..)
        | M::TogglePalette
        | M::ClipName(..)
        | M::ChanceSplit(..)
        | M::ScenePercent(..)
        | M::SceneChanceSplit(..)
        | M::FollowPercent(..) => unreachable!("panel messages are handled above"),
        M::Timeline(action) => inspector::event_editor::handle(app, action),
        M::SceneName(scene, name) => execute_inspector_edit(
            app,
            inspector::scene_key(scene, SceneField::Name),
            ProjectEdit::RenameScene { scene, name },
        ),
        M::SceneEnabled(scene, enabled) => {
            if let Some(mut f) = app
                .project()
                .scenes
                .iter()
                .find(|s| s.id == scene)
                .map(|s| s.follow_actions.clone())
            {
                f.actions.enabled = enabled;
                execute_edit(
                    app,
                    Command::Edit(ProjectEdit::SetSceneFollowActions {
                        scene,
                        follow_actions: f,
                    }),
                );
            }
        }
        M::SceneAction(scene, a, action) => {
            if let Some(mut f) = app
                .project()
                .scenes
                .iter()
                .find(|s| s.id == scene)
                .map(|s| s.follow_actions.clone())
            {
                if a {
                    f.actions.action_a = action
                } else {
                    f.actions.action_b = action
                };
                execute_edit(
                    app,
                    Command::Edit(ProjectEdit::SetSceneFollowActions {
                        scene,
                        follow_actions: f,
                    }),
                );
            }
        }
        M::SceneChance(scene, a, value) => {
            let key = inspector::scene_key(
                scene,
                if a {
                    SceneField::ChanceA
                } else {
                    SceneField::ChanceB
                },
            );
            match value.parse::<u32>() {
                Ok(value) => {
                    if let Some(mut f) = app
                        .project()
                        .scenes
                        .iter()
                        .find(|s| s.id == scene)
                        .map(|s| s.follow_actions.clone())
                    {
                        if a {
                            f.actions.chance_a = value
                        } else {
                            f.actions.chance_b = value
                        };
                        execute_inspector_edit(
                            app,
                            key,
                            ProjectEdit::SetSceneFollowActions {
                                scene,
                                follow_actions: f,
                            },
                        );
                    }
                }
                Err(_) => {
                    app.drafts
                        .errors
                        .insert(key, "Chance must be a non-negative whole number".into());
                }
            }
        }
        M::SceneTime(scene, value) => {
            let key = inspector::scene_key(scene, SceneField::FollowTime);
            match inspector::musical_time::parse_duration(&value, app.project().time_signature) {
                Ok(time) if time > BeatTime::ZERO => {
                    if let Some(mut f) = app
                        .project()
                        .scenes
                        .iter()
                        .find(|s| s.id == scene)
                        .map(|s| s.follow_actions.clone())
                    {
                        f.time = time;
                        execute_inspector_edit(
                            app,
                            key,
                            ProjectEdit::SetSceneFollowActions {
                                scene,
                                follow_actions: f,
                            },
                        );
                    }
                }
                _ => {
                    app.drafts
                        .errors
                        .insert(key, "Enter a positive time in bars.beats.sixteenths".into());
                }
            }
        }

        M::Draft(key, value) | M::SceneDraft(key, value) => {
            app.drafts.errors.remove(&key);
            app.drafts.values.insert(key, value);
        }
        M::Length(track, slot, s) => {
            inspector::clip_markers::text(app, track, slot, ClipField::Length, s)
        }
        M::ToggleLoop(track, slot) => inspector::clip_markers::toggle(app, track, slot),
        M::ToggleLegato(track, slot) => {
            let legato = app.project().clip(track, slot).is_some_and(|c| !c.legato);
            execute_edit(
                app,
                Command::Edit(ProjectEdit::SetClipLegato {
                    track,
                    slot,
                    legato,
                }),
            );
        }
        M::Quantisation(track, slot, q) => execute_edit(
            app,
            Command::Edit(ProjectEdit::SetClipQuantisation {
                track,
                slot,
                quantisation: q.value(),
            }),
        ),
        M::FollowEnabled(track, slot, enabled) => {
            if let Some(mut f) = current_follow(app.project(), track, slot) {
                f.actions.enabled = enabled;
                execute_edit(
                    app,
                    Command::Edit(ProjectEdit::SetClipFollowActions {
                        track,
                        slot,
                        follow_actions: f,
                    }),
                );
            }
        }
        M::FollowA(track, slot, action) => {
            if let Some(mut f) = current_follow(app.project(), track, slot) {
                f.actions.action_a = action;
                execute_edit(
                    app,
                    Command::Edit(ProjectEdit::SetClipFollowActions {
                        track,
                        slot,
                        follow_actions: f,
                    }),
                );
            }
        }
        M::FollowB(track, slot, action) => {
            if let Some(mut f) = current_follow(app.project(), track, slot) {
                f.actions.action_b = action;
                execute_edit(
                    app,
                    Command::Edit(ProjectEdit::SetClipFollowActions {
                        track,
                        slot,
                        follow_actions: f,
                    }),
                );
            }
        }
        M::FollowTime(track, slot, s) => {
            match inspector::musical_time::parse_duration(&s, app.project().time_signature) {
                Ok(time) if time > BeatTime::ZERO => {
                    if let Some(mut f) = current_follow(app.project(), track, slot) {
                        f.time = time;
                        execute_inspector_edit(
                            app,
                            inspector::field_key(track, slot, ClipField::FollowTime),
                            ProjectEdit::SetClipFollowActions {
                                track,
                                slot,
                                follow_actions: f,
                            },
                        );
                    }
                }
                _ => {
                    app.drafts.errors.insert(
                        inspector::field_key(track, slot, ClipField::FollowTime),
                        "Enter a positive time in bars.beats.sixteenths".into(),
                    );
                }
            }
        }
        M::FollowChance(track, slot, is_a, value) => {
            let key = inspector::field_key(
                track,
                slot,
                if is_a {
                    ClipField::ChanceA
                } else {
                    ClipField::ChanceB
                },
            );
            if let Ok(chance) = value.parse::<u32>() {
                if let Some(mut f) = current_follow(app.project(), track, slot) {
                    if is_a {
                        f.actions.chance_a = chance
                    } else {
                        f.actions.chance_b = chance
                    };
                    execute_inspector_edit(
                        app,
                        key,
                        ProjectEdit::SetClipFollowActions {
                            track,
                            slot,
                            follow_actions: f,
                        },
                    );
                }
            } else {
                app.drafts
                    .errors
                    .insert(key, "Enter a whole number from 0 to 4294967295".into());
            }
        }
        M::Multiplier(track, slot, text) => {
            let key = inspector::field_key(track, slot, ClipField::Multiplier);
            match inspector::musical_time::parse_multiplier(&text) {
                Ok(multiplier) => {
                    if let Some(mut f) = current_follow(app.project(), track, slot) {
                        f.multiplier = multiplier;
                        execute_inspector_edit(
                            app,
                            key,
                            ProjectEdit::SetClipFollowActions {
                                track,
                                slot,
                                follow_actions: f,
                            },
                        );
                    }
                }
                Err(error) => {
                    app.drafts.errors.insert(key, error);
                }
            }
        }
        M::FollowLinked(track, slot, linked) => {
            if let Some(mut f) = current_follow(app.project(), track, slot) {
                f.linked = linked;
                execute_edit(
                    app,
                    Command::Edit(ProjectEdit::SetClipFollowActions {
                        track,
                        slot,
                        follow_actions: f,
                    }),
                );
            }
        }
        M::Colour(track, slot, colour) => execute_edit(
            app,
            Command::Edit(ProjectEdit::RecolourClip {
                track,
                slot,
                colour,
            }),
        ),
        // Panel, lane and hit messages were handled above.
        _ => {}
    }
}
