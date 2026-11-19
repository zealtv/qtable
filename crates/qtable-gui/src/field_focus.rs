//! Pending edits commit before changing selection, and when widget focus leaves a field.
use crate::app::App;
use crate::drafts::{DraftKey, TrackField};
use crate::inspector;
use crate::messages::{EditMessage, Message};
use iced::advanced::widget::{
    Id, Operation,
    operation::{Focusable, Outcome},
};
use iced::{Rectangle, Task};

/// Refocus without submitting the draft, and put the cursor before the editable piece.
pub(crate) fn focus_at_start(key: DraftKey) -> Task<Message> {
    let id = key.input_id();
    iced::widget::operation::focus(id.clone())
        .chain(iced::widget::operation::move_cursor_to_front(id))
}

pub(crate) fn changed_field(message: &Message) -> Option<DraftKey> {
    match message {
        Message::Edit(EditMessage::MixerDevice(msg)) => crate::mixer_device::changed(msg),
        Message::Device(crate::device_chain::Action::Target(
            crate::device_chain::target_face::Action::Draft(key, _),
        )) => Some(*key),
        Message::TempoChanged(_) => Some(DraftKey::Tempo),
        Message::Edit(EditMessage::PathChanged(id, _)) => {
            Some(DraftKey::Track(*id, TrackField::Path))
        }
        Message::Inspector(
            inspector::Message::Draft(key, _) | inspector::Message::SceneDraft(key, _),
        ) => Some(*key),
        Message::Outputs(message) => crate::update::outputs::changed(message),
        Message::Map(crate::map_mode::Action::FieldChanged(id, cell, _)) => {
            Some(DraftKey::Mapping(*id, *cell))
        }
        _ => None,
    }
}

pub(crate) fn value(app: &App, key: DraftKey) -> Option<String> {
    if key == DraftKey::Tempo {
        Some(app.tempo_input.clone())
    } else {
        app.drafts.values.get(&key).cloned()
    }
}

/// Query after the event has reached the widgets. Missing widgets count as blurred.
pub(crate) fn check(app: &App) -> Task<Message> {
    let Some(key) = app.pending_field else {
        return Task::none();
    };
    let Some(value) = value(app, key) else {
        return Task::none();
    };
    let generation = app.project_generation;
    check_widget_focus(key.input_id())
        .map(move |focused| Message::FieldFocusChecked(key, value.clone(), generation, focused))
}

pub(crate) fn check_widget_focus(id: iced::widget::Id) -> Task<bool> {
    iced::advanced::widget::operate(FocusCheck { id, focused: false })
}

struct FocusCheck {
    id: Id,
    focused: bool,
}
impl Operation<bool> for FocusCheck {
    fn focusable(&mut self, id: Option<&Id>, _: Rectangle, state: &mut dyn Focusable) {
        // The same field can show twice (the Track tab and the mixer band's delay), so
        // any focused copy counts.
        if id == Some(&self.id) {
            self.focused |= state.is_focused();
        }
    }
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<bool>)) {
        operate(self);
    }
    fn finish(&self) -> Outcome<bool> {
        Outcome::Some(self.focused)
    }
}

pub(crate) fn cancel(app: &mut App) {
    if let Some(key) = app.pending_field.take() {
        app.drafts.forget(key);
        if key == DraftKey::Tempo {
            app.tempo_input = crate::fields::format_tempo(app.project().tempo.bpm());
        }
        app.status.clear_transient();
    }
}

/// Whether the shared rejection check still follows an owner's commit.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Report {
    Check,
    /// The owner recorded the outcome itself.
    Settled,
}

/// Reuse Enter's handlers, with IDs captured before selection or event indices change.
/// Each domain's owner turns its draft into an edit; this only dispatches.
pub(crate) fn commit(app: &mut App) {
    let Some(key) = app.pending_field.take() else {
        return;
    };
    let Some(value) = value(app, key) else { return };
    let revision = app.project_revision();
    match key {
        DraftKey::Tempo => crate::transport::commit_tempo(app),
        DraftKey::Clip { track, slot, field } => {
            if crate::inspector::clip_panel_update::commit(app, track, slot, field, value)
                == Report::Settled
            {
                return;
            }
        }
        DraftKey::Scene { scene, field } => {
            crate::inspector::scene_update::commit(app, scene, field, value)
        }
        DraftKey::Target(..) => crate::device_chain::target_face::submit(app, key),
        DraftKey::Track(track, field) => crate::track_detail::commit(app, track, field, value),
        DraftKey::Mixer(track, field) => crate::mixer_device::commit(app, track, field),
        DraftKey::Output(output, field) => crate::output_detail::commit(app, output, field, value),
        DraftKey::Mapping(mapping, cell) => crate::map_mode::commit(app, mapping, cell),
    }
    if let Some(error) = error(app, key) {
        app.drafts.errors.insert(key, error.clone());
        app.status.show_error(error);
    } else if app.project_revision() == revision
        && let Some(error) = app.status.message().map(str::to_owned)
    {
        app.drafts.errors.insert(key, error);
    }
}

pub(crate) fn submitted_field(message: &Message) -> Option<DraftKey> {
    match message {
        Message::Edit(EditMessage::MixerDevice(msg)) => crate::mixer_device::submitted(msg),
        Message::Device(crate::device_chain::Action::Target(
            crate::device_chain::target_face::Action::Submit(key),
        )) => Some(*key),
        Message::TempoSubmitted => Some(DraftKey::Tempo),
        Message::Edit(EditMessage::PathSubmitted(id, _)) => {
            Some(DraftKey::Track(*id, TrackField::Path))
        }
        Message::Outputs(message) => crate::update::outputs::submitted(message),
        Message::Map(crate::map_mode::Action::FieldSubmitted(id, cell)) => {
            Some(DraftKey::Mapping(*id, *cell))
        }
        Message::Inspector(message) => crate::inspector::scene_update::submitted(message)
            .or_else(|| crate::inspector::clip_panel_update::submitted(message)),
        _ => None,
    }
}

pub(crate) fn error(app: &App, key: DraftKey) -> Option<String> {
    app.drafts.errors.get(&key).cloned()
}
