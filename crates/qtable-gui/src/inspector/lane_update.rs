//! Hit argument edits from the strip under the timeline; types belong to the Target definition.
use super::{Message, event_editor, field_key};
use crate::app::App;
use crate::drafts::ClipField;
use crate::inspector_update::execute_inspector_edit;
use qtable_engine::edit::ProjectEdit;
use qtable_engine::model::{Event, Project, TrackId};

pub(crate) fn event(project: &Project, track: TrackId, slot: usize, index: usize) -> Option<Event> {
    super::clip_panel_update::clip(project, track, slot)?
        .events
        .get(index)
        .cloned()
}

/// Handles lane and hit messages; returns any other message for the caller.
pub(crate) fn handle(app: &mut App, message: Message) -> Option<Message> {
    match message {
        Message::GoToDevice(track, reference) => {
            crate::device_chain::go_to_event(app, track, reference)
        }
        Message::GoToParameter(track, argument) => {
            crate::device_chain::go_to_argument(app, track, &argument)
        }
        Message::OpenChooser(track, slot, which) => {
            super::lane_view::open_chooser(app, track, slot, which)
        }
        Message::ChooseDevice(track, slot, device) => {
            super::lane_view::choose_device(app, track, slot, device)
        }
        Message::ChooseControl(track, slot, target) => {
            super::lane_view::choose_control(app, track, slot, target)
        }
        Message::ChooseTab(track, slot, tab) => super::lane_view::choose_tab(app, track, slot, tab),
        Message::OnlyAdjusted(track, slot) => {
            super::lane_view::toggle_only_adjusted(app, track, slot)
        }
        Message::PointText(track, slot, field, text) => {
            event_editor::commit_text(app, track, slot, field, &text)
        }
        Message::PointValue(track, slot, value) => {
            event_editor::choose_value(app, track, slot, value)
        }
        Message::RemoveEvent(track, slot, event) => event_editor::remove(app, track, slot, event),
        Message::Argument(track, slot, index, argument, kind, raw) => {
            let key = field_key(
                track,
                slot,
                ClipField::EventArgument {
                    event: index,
                    argument,
                },
            );
            match super::parse_argument(&kind, &raw) {
                Ok(parsed) => {
                    if let Some(mut value) = event(app.project(), track, slot, index)
                        && let Some(slot_value) = value.arguments.get_mut(argument)
                    {
                        *slot_value = parsed;
                        set_argument(app, track, slot, index, argument, value);
                    }
                }
                Err(error) => {
                    app.drafts.errors.insert(key, error);
                }
            }
        }
        other => return Some(other),
    }
    None
}

fn set_argument(
    app: &mut App,
    track: TrackId,
    slot: usize,
    index: usize,
    argument: usize,
    value: Event,
) {
    execute_inspector_edit(
        app,
        field_key(
            track,
            slot,
            ClipField::EventArgument {
                event: index,
                argument,
            },
        ),
        ProjectEdit::UpdateEvent {
            track,
            slot,
            event: index,
            value,
        },
    );
}
