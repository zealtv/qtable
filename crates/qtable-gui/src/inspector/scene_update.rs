//! Explicit Scene percentage edits; stored follow weights remain engine data.
use super::Message;
use crate::app::App;
use crate::drafts::{DraftKey, SceneField};
use crate::edit_actions::execute_edit;
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::SceneId;

pub(crate) fn handle(app: &mut App, message: Message) -> Option<Message> {
    match message {
        Message::SceneChanceSplit(scene, b) => set_percent(app, scene, b.min(100)),
        Message::ScenePercent(scene, a, input) => {
            let field = if a {
                SceneField::PercentA
            } else {
                SceneField::PercentB
            };
            let key = super::scene_key(scene, field);
            // Displaying an existing arbitrary ratio must not normalize it on Enter.
            if !app.drafts.values.contains_key(&key) {
                return None;
            }
            match input.trim().trim_end_matches('%').trim().parse::<u8>() {
                Ok(percent) if percent <= 100 => {
                    set_percent(app, scene, if a { 100 - percent } else { percent })
                }
                _ => {
                    app.drafts
                        .errors
                        .insert(key, "Enter a whole percentage from 0 to 100".into());
                }
            }
        }
        message => return Some(message),
    }
    None
}

fn set_percent(app: &mut App, scene: SceneId, b: u8) {
    let Some(mut follow_actions) = app
        .project()
        .scenes
        .iter()
        .find(|s| s.id == scene)
        .map(|s| s.follow_actions.clone())
    else {
        return;
    };
    follow_actions.actions.chance_a = u32::from(100 - b);
    follow_actions.actions.chance_b = u32::from(b);
    let revision = app.project_revision();
    execute_edit(
        app,
        Command::Edit(ProjectEdit::SetSceneFollowActions {
            scene,
            follow_actions,
        }),
    );
    if app.project_revision() != revision {
        for field in [
            SceneField::PercentA,
            SceneField::PercentB,
            SceneField::ChanceA,
            SceneField::ChanceB,
        ] {
            app.drafts.forget(super::scene_key(scene, field));
        }
    }
}

/// Commits a draft of one of the scene's fields through its Enter message.
pub(crate) fn commit(app: &mut App, scene: SceneId, field: SceneField, value: String) {
    let message = match field {
        SceneField::Name => Message::SceneName(scene, value),
        SceneField::FollowTime => Message::SceneTime(scene, value),
        SceneField::ChanceA => Message::SceneChance(scene, true, value),
        SceneField::ChanceB => Message::SceneChance(scene, false, value),
        SceneField::PercentA => Message::ScenePercent(scene, true, value),
        SceneField::PercentB => Message::ScenePercent(scene, false, value),
    };
    crate::inspector_update::handle_inspector(app, message);
}

/// The scene field an inspector message submits.
pub(crate) fn submitted(message: &Message) -> Option<DraftKey> {
    let (scene, field) = match message {
        Message::SceneName(id, _) => (id, SceneField::Name),
        Message::SceneTime(id, _) => (id, SceneField::FollowTime),
        Message::ScenePercent(id, a, _) => (
            id,
            if *a {
                SceneField::PercentA
            } else {
                SceneField::PercentB
            },
        ),
        Message::SceneChance(id, a, _) => (
            id,
            if *a {
                SceneField::ChanceA
            } else {
                SceneField::ChanceB
            },
        ),
        _ => return None,
    };
    Some(DraftKey::scene(*scene, field))
}
