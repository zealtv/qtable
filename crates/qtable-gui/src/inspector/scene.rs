//! Scene properties supply the shared frame and the same follow layout as Clip.
use super::clip_panel_update::percent_b;
use super::follow_controls::FollowControls;
use super::musical_time::format_duration;
use super::{Message, choices::action_pair, scene_key};
use crate::control::{Number, SizeRole, State, Text, TimeField, Toggle};
use crate::control_focus::{ControlOwner, View};
use crate::detail_layout::{self, Header, Layout, Section};
use crate::drafts::{Control, ControlId, Controls, DraftKey, SceneField};
use crate::info::Id;
use crate::inspector_panel::Tab;
use iced::widget::{column, container};
use qtable_engine::model::{Project, SceneId};
use std::collections::HashMap;

pub fn scene_view(
    project: &Project,
    selected: Option<SceneId>,
    drafts: &HashMap<DraftKey, String>,
    errors: &HashMap<DraftKey, String>,
    state: &detail_layout::State,
    controls: Controls<'_>,
) -> Layout<'static, Message> {
    let Some(scene) = selected.and_then(|id| project.scenes.iter().find(|s| s.id == id)) else {
        return Layout::empty("Select a scene to inspect it.");
    };
    let id = scene.id;
    let name_key = scene_key(id, SceneField::Name);
    let time_key = scene_key(id, SceneField::FollowTime);
    let value = |key, model: String| drafts.get(&key).cloned().unwrap_or(model);
    let name = value(name_key, scene.name.clone());
    let time = value(
        time_key,
        format_duration(scene.follow_actions.time, project.time_signature),
    );
    let number = |key, role, help| {
        let kind = Number::new("").adjust_text(crate::control::adjustment::percent);
        Control::new(ControlId::field(View::Scene, key), help, kind)
            .role(role)
            .invalid(errors.contains_key(&key))
    };
    let actions = &scene.follow_actions.actions;
    let b = percent_b(actions.chance_a, actions.chance_b);
    let chance = |a| {
        let key = scene_key(
            id,
            if a {
                SceneField::PercentA
            } else {
                SceneField::PercentB
            },
        );
        let percent = b.map(|b| if a { 100 - b } else { b }).unwrap_or(0);
        let value = value(key, format!("{percent} %"));
        controls.number(
            number(key, SizeRole::Tag, Id::FollowChance),
            &value,
            move |text| Message::SceneDraft(key, text),
            Message::ScenePercent(id, a, value.clone()),
            None,
        )
    };
    let follow = FollowControls {
        toggle: controls.toggle(
            Control::new(
                ControlId::action(View::Scene, "follow-action", ControlOwner::Scene(id)),
                Id::FollowEnabled,
                Toggle {
                    label: "Follow Action".into(),
                },
            )
            .role(SizeRole::Fill)
            .lit(actions.enabled),
            Message::SceneEnabled(id, !actions.enabled),
        ),
        actions: action_pair(
            project,
            (controls, View::Scene, ControlOwner::Scene(id)),
            (actions.action_a, actions.action_b),
            move |choice| Message::SceneAction(id, true, choice.action()),
            move |choice| Message::SceneAction(id, false, choice.action()),
        ),
        chances: [chance(true), chance(false)],
        percentage_b: b,
        on_split: Box::new(move |b| Message::SceneChanceSplit(id, b.round() as u8)),
        timing: controls
            .labelled(
                "Follow Action Time",
                State::default(),
                container(
                    controls.time_field(
                        Control::new(
                            ControlId::field(View::Scene, time_key),
                            Id::SceneFollowTime,
                            TimeField::duration(project.time_signature),
                        )
                        .role(SizeRole::Fill)
                        .invalid(errors.contains_key(&time_key)),
                        &time,
                        move |text| Message::SceneDraft(time_key, text),
                        Message::SceneTime(id, time.clone()),
                    ),
                )
                .width(100),
            )
            .into(),
    }
    .view(Message::Info);
    Layout {
        header: Header {
            colour: crate::control::LIVE_DARK.selection_background,
            content: controls.text(
                Control::new(
                    ControlId::field(View::Scene, name_key),
                    Id::SceneName,
                    Text {
                        placeholder: "Scene name".into(),
                    },
                )
                .invalid(errors.contains_key(&name_key)),
                &name,
                move |text| Message::SceneDraft(name_key, text),
                Message::SceneName(id, name.clone()),
            ),
        },
        leading: None,
        sections: vec![Section {
            id: detail_layout::LAUNCH,
            title: "Launch",
            icon: None,
            expanded: state.expanded(Tab::Scene, detail_layout::LAUNCH),
            on_toggle: Message::ToggleSection(Tab::Scene, detail_layout::LAUNCH),
            help: Id::SceneLaunchSection,
            body: column![follow].into(),
        }],
        content: None,
    }
}
