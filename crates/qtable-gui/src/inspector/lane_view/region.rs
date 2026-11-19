//! The envelope bar edits the same Region as the clip box and ruler, through SetRegion.
use super::*;
use crate::control::{Button, Control, Face, SizeRole, TimeField, Toggle};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::{ClipField, ControlId, Controls, DraftKey};
use crate::info::Id;
use crate::inspector::event_editor::{Action, PointTarget};
use crate::inspector::{Message, musical_time};
use crate::style::detail;
use iced::widget::{row, text as label};
use iced::{Element, alignment};
use qtable_engine::edit::{LaneEdit, LaneKey, ProjectEdit};
use qtable_engine::engine::Command;
use qtable_engine::model::region::Region;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Field {
    Start,
    End,
    Position,
    Length,
}
impl Field {
    const ALL: [Self; 4] = [Self::Start, Self::End, Self::Position, Self::Length];
    fn clip(self) -> ClipField {
        match self {
            Self::Start => ClipField::Start,
            Self::End => ClipField::End,
            Self::Position => ClipField::Position,
            Self::Length => ClipField::Length,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::End => "End",
            Self::Position => "Position",
            Self::Length => "Length",
        }
    }
    fn help(self) -> Id {
        match self {
            Self::Start => Id::ClipStart,
            Self::End => Id::ClipEnd,
            Self::Position => Id::ClipPosition,
            Self::Length => Id::ClipLength,
        }
    }
}

pub(crate) fn forget_drafts(app: &mut App, track: TrackId, slot: usize) {
    for field in Field::ALL {
        app.drafts
            .forget(DraftKey::clip(track, slot, ClipField::LaneMarker(field)));
    }
}

fn lane(app: &App, target: &PointTarget) -> Option<Option<Region>> {
    if !crate::inspector::event_editor::valid_point_target(app, target)
        || current(app, target.key.track, target.key.slot).one_lane() != Some(&target.target)
    {
        return None;
    }
    app.project()
        .clip(target.key.track, target.key.slot)?
        .lanes
        .iter()
        .find_map(|l| {
            l.envelope()
                .filter(|e| e.target == target.target)
                .map(|e| e.region)
        })
}

pub(crate) fn submit(
    app: &mut App,
    target: PointTarget,
    region: Option<Region>,
    draft: Option<DraftKey>,
) {
    let Some(_) = lane(app, &target) else {
        return;
    };
    crate::edit_actions::run_edit_report(
        app,
        Command::Edit(ProjectEdit::EditLane {
            track: target.key.track,
            slot: target.key.slot,
            edit: LaneEdit::SetRegion {
                lane: LaneKey::Envelope(target.target),
                region,
            },
        }),
        draft,
    );
}

pub(crate) fn text(app: &mut App, target: PointTarget, field: Field, value: String) {
    let Some(Some(region)) = lane(app, &target) else {
        return;
    };
    let signature = app.project().time_signature;
    let at = if field == Field::Length {
        musical_time::parse_duration(&value, signature)
    } else {
        musical_time::parse_position(&value, signature)
    };
    let key = DraftKey::clip(
        target.key.track,
        target.key.slot,
        ClipField::LaneMarker(field),
    );
    match at.and_then(|at| crate::inspector::clip_markers::change(region, field.clip(), at)) {
        Ok(region) => submit(app, target, Some(region), Some(key)),
        Err(error) => {
            app.drafts.errors.insert(key, error);
        }
    }
}

pub(crate) fn commit(app: &mut App, track: TrackId, slot: usize, field: Field, value: String) {
    let Some(target) = current(app, track, slot).one_lane().cloned() else {
        return;
    };
    let Some(key) = key_for(app, track, slot) else {
        return;
    };
    text(
        app,
        PointTarget {
            key,
            revision: app.project_revision(),
            target,
        },
        field,
        value,
    );
}

/// Beside the choosers; only unlinked envelopes show their own region controls.
pub(crate) fn bar(
    project: &Project,
    key: ClipKey,
    revision: u64,
    target: &ArgumentRef,
    controls: &Controls<'_>,
    drafts: &std::collections::HashMap<DraftKey, String>,
    errors: &std::collections::HashMap<DraftKey, String>,
) -> Element<'static, Message> {
    let clip = project.clip(key.track, key.slot).expect("selected clip");
    let envelope = clip
        .lanes
        .iter()
        .find_map(|l| l.envelope().filter(|e| e.target == *target));
    let region = envelope.and_then(|e| e.region);
    let target = PointTarget {
        key,
        revision,
        target: target.clone(),
    };
    let action = |name| {
        ControlId::action(
            View::Clip,
            name,
            ControlOwner::ClipSlot(key.track, key.slot),
        )
    };
    let link = controls.button(
        Control::new(
            action("lane-link"),
            Id::EnvelopeLink,
            Button {
                face: Face::Link(region.is_none()),
                latched: true,
            },
        )
        .lit(true)
        .enabled(envelope.is_some()),
        Message::Timeline(Action::LaneRegion(
            target.clone(),
            if region.is_some() {
                None
            } else {
                Some(clip.region())
            },
        )),
    );
    let Some(markers) = region else {
        return link;
    };
    let mut loop_region = markers;
    loop_region.looping = !markers.looping;
    let looping = controls.toggle(
        Control::new(
            action("lane-loop"),
            Id::Loop,
            Toggle {
                label: "Loop".into(),
            },
        )
        .lit(markers.looping),
        Message::Timeline(Action::LaneRegion(target.clone(), Some(loop_region))),
    );
    let mut bar = row![link, looping]
        .spacing(detail::CONTROL_GAP)
        .align_y(alignment::Vertical::Center);
    for field in Field::ALL {
        let time = match field {
            Field::Start => markers.start,
            Field::End => markers.end,
            Field::Position => markers.loop_start,
            Field::Length => {
                qtable_engine::model::BeatTime(markers.loop_end.0 - markers.loop_start.0)
            }
        };
        let key = DraftKey::clip(key.track, key.slot, ClipField::LaneMarker(field));
        let value = drafts.get(&key).cloned().unwrap_or_else(|| {
            if field == Field::Length {
                musical_time::format_duration(time, project.time_signature)
            } else {
                musical_time::format_position(time, project.time_signature)
            }
        });
        let number = controls.time_field(
            Control::new(
                ControlId::field(View::Clip, key),
                field.help(),
                if field == Field::Length {
                    TimeField::duration(project.time_signature)
                } else {
                    TimeField::position(project.time_signature)
                },
            )
            .role(SizeRole::DeviceNumber)
            .invalid(errors.contains_key(&key)),
            &value,
            move |text| Message::Draft(key, text),
            Message::Timeline(Action::LaneRegionText(target.clone(), field, value.clone())),
        );
        bar = bar
            .push(label(field.label()).size(detail::TEXT_SIZE))
            .push(number);
    }
    bar.into()
}

#[cfg(test)]
mod tests;
