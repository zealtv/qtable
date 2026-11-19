//! Track Detail controls over the engine's independent volume controller.
use crate::app::App;
use crate::control::device_layout::{cell, labelled};
use crate::control::mapping::{self, MappingEditor, MappingEditorMessage, MappingField};
use crate::control::path_field::Prefix;
use crate::control::{ControlKind, Dropdown, Number, PathField, SizeRole, State, Toggle};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::{Control, ControlId, DraftKey, MixerField};
use crate::info::{Id, Subject};
use crate::messages::{EditMessage, Message};
use crate::style::detail;
use iced::Element;
use iced::widget::{Space, column, container, row};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::{
    ControlRef, FieldRef, Parameter, ParameterRef, PathLevel, SendMode, THRESHOLD, Track, TrackId,
    VOLUME,
};

const VOLUME_WIDTH: f32 = 3.0 * detail::DEVICE_CONTROL_WIDTH + 4.0 * detail::CONTROL_GAP;
const CELL_HEIGHT: f32 = 2.0 * detail::DEVICE_CELL_EDGE
    + detail::DEVICE_CELL_STRIP
    + detail::DEVICE_ROW_GAP
    + 3.0 * detail::DEVICE_LABELLED_ROW
    + 2.0 * detail::DEVICE_ROW_GAP
    + detail::DEVICE_CELL_PADDING;

pub(crate) fn width(track: &Track) -> f32 {
    2.0 * detail::DEVICE_BODY_INSET
        + VOLUME_WIDTH
        + if track.capabilities().destination {
            0.0
        } else {
            detail::DEVICE_CELL_WIDTH + detail::CONTROL_GAP
        }
        + detail::SCROLLBAR_GUTTER
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MixerMessage {
    Draft(DraftKey, String),
    Submit(DraftKey),
    Mapping(TrackId, MappingEditorMessage),
    Toggle(TrackId),
    SendMode(TrackId, SendMode),
    /// A double click on the Threshold box: back to [`THRESHOLD`]'s default.
    ResetThreshold(TrackId),
}
fn message(message: MixerMessage) -> Message {
    Message::Edit(EditMessage::MixerDevice(message))
}
/// The Info View entry for each field of the Volume's mapping editor.
fn mapping_help(field: MappingField) -> Id {
    match field {
        MappingField::InputStart | MappingField::InputEnd => Id::MappingInput,
        MappingField::OutputStart | MappingField::OutputEnd => Id::MappingOutput,
        MappingField::Exponent => Id::MappingExponent,
        MappingField::Bias => Id::MappingBias,
    }
}

pub(crate) fn mapping_keys(id: TrackId) -> [DraftKey; 6] {
    MappingField::ALL.map(|field| DraftKey::Mixer(id, MixerField::Mapping(field)))
}
pub(crate) fn owner(key: DraftKey) -> Option<TrackId> {
    match key {
        DraftKey::Mixer(id, _) => Some(id),
        _ => None,
    }
}
pub(crate) fn changed(message: &MixerMessage) -> Option<DraftKey> {
    match message {
        MixerMessage::Draft(key, _) => Some(*key),
        MixerMessage::Mapping(id, msg @ MappingEditorMessage::Changed(..)) => {
            Some(msg.key(&mapping_keys(*id)))
        }
        _ => None,
    }
}
pub(crate) fn submitted(message: &MixerMessage) -> Option<DraftKey> {
    match message {
        MixerMessage::Submit(key) => Some(*key),
        MixerMessage::Mapping(id, msg @ MappingEditorMessage::Submitted(..)) => {
            Some(msg.key(&mapping_keys(*id)))
        }
        _ => None,
    }
}
pub(crate) fn update(app: &mut App, msg: MixerMessage) {
    let id = match &msg {
        MixerMessage::Draft(key, _) | MixerMessage::Submit(key) => owner(*key),
        MixerMessage::Mapping(id, _)
        | MixerMessage::Toggle(id)
        | MixerMessage::SendMode(id, _)
        | MixerMessage::ResetThreshold(id) => Some(*id),
    };
    if !matches!(msg, MixerMessage::Toggle(_))
        && !id.is_some_and(|id| {
            app.project()
                .track(id)
                .is_some_and(|track| track.mixer_device.volume.output.enabled)
        })
    {
        return;
    }
    if matches!(
        &msg,
        MixerMessage::Draft(DraftKey::Mixer(_, MixerField::Threshold), _)
            | MixerMessage::Submit(DraftKey::Mixer(_, MixerField::Threshold))
            | MixerMessage::ResetThreshold(_)
    ) && id
        .and_then(|id| app.project().track(id))
        .is_some_and(|t| t.mixer_device.volume.output.send_mode == SendMode::EveryFrame)
    {
        return;
    }
    if let MixerMessage::Draft(key, value) = msg {
        app.drafts.values.insert(key, value);
        app.drafts.errors.remove(&key);
        return;
    }
    let key = submitted(&msg);
    let Some(id) = id else {
        return;
    };
    let target = ParameterRef::volume(id);
    let Some(parameter) = app.project().parameter(&target) else {
        return;
    };
    let mut output = parameter.output.clone().into_owned();
    let result = match msg {
        MixerMessage::Submit(key @ DraftKey::Mixer(_, MixerField::Threshold)) => {
            let Some(value) = app.drafts.values.get(&key) else {
                return;
            };
            THRESHOLD.parse(value).map(|threshold| {
                output.threshold = threshold;
                Some(ProjectEdit::SetParameterOutput { target, output })
            })
        }
        MixerMessage::Submit(key @ DraftKey::Mixer(_, MixerField::Volume)) => {
            let Some(value) = app.drafts.values.get(&key) else {
                return;
            };
            VOLUME
                .parse(value)
                .map(|volume| Some(ProjectEdit::set_value(ParameterRef::volume(id), volume)))
        }
        MixerMessage::Submit(key @ DraftKey::Mixer(_, MixerField::Address)) => {
            let Some(value) = app.drafts.values.get(&key) else {
                return;
            };
            output.address = value.clone();
            Ok(Some(ProjectEdit::SetParameterOutput {
                target: ParameterRef::volume(id),
                output,
            }))
        }
        MixerMessage::Submit(DraftKey::Mixer(_, MixerField::Mapping(field))) => {
            update(
                app,
                MixerMessage::Mapping(id, MappingEditorMessage::Submitted(field)),
            );
            return;
        }
        MixerMessage::Mapping(_, msg) => mapping::update(
            &mut app.drafts.values,
            &mut app.drafts.errors,
            output.mapping,
            &mapping_keys(id),
            true,
            msg,
        )
        .map(|mapping| {
            mapping.map(|mapping| {
                output.mapping = mapping;
                ProjectEdit::SetParameterOutput {
                    target: ParameterRef::volume(id),
                    output,
                }
            })
        })
        .map_err(|e| e.to_string()),
        MixerMessage::SendMode(_, mode) => {
            app.parameter_gesture.end(&volume_field(id));
            app.drafts
                .forget(DraftKey::Mixer(id, MixerField::Threshold));
            output.send_mode = mode;
            Ok(Some(ProjectEdit::SetParameterOutput {
                target: ParameterRef::volume(id),
                output,
            }))
        }
        MixerMessage::ResetThreshold(_) => {
            app.drafts
                .forget(DraftKey::Mixer(id, MixerField::Threshold));
            output.threshold = THRESHOLD.default;
            Ok(Some(ProjectEdit::SetParameterOutput { target, output }))
        }
        MixerMessage::Toggle(_) => {
            output.enabled = !output.enabled;
            Ok(Some(ProjectEdit::SetParameterOutput {
                target: ParameterRef::volume(id),
                output,
            }))
        }
        _ => return,
    };
    match result {
        Ok(Some(edit)) => {
            if crate::edit_actions::run_edit(app, Command::Edit(edit)) {
                if let Some(key) = key {
                    app.drafts.forget(key);
                }
            } else if let Some(key) = key
                && let Some(error) = app.status.message()
            {
                app.drafts.errors.insert(key, error.to_owned());
            }
        }
        Err(error) => {
            if let Some(key) = key {
                app.drafts.errors.insert(key, error.clone());
            }
            app.status.show_error(error);
        }
        Ok(None) => {}
    }
}
/// Commits a draft of one of the Mixer's fields, as Enter does.
pub(crate) fn commit(app: &mut App, track: TrackId, field: MixerField) {
    update(app, MixerMessage::Submit(DraftKey::Mixer(track, field)));
}

fn field_control<K: ControlKind>(
    app: &App,
    location: View,
    key: DraftKey,
    help: Id,
    kind: K,
) -> Control<K> {
    Control::new(ControlId::field(location, key), help, kind).enabled(
        owner(key)
            .and_then(|id| app.project().track(id))
            .is_some_and(|track| track.mixer_device.volume.output.enabled),
    )
}

/// Mixer Volume's one argument, as the gesture (and later automation) knows it.
pub(crate) fn volume_field(track: TrackId) -> FieldRef {
    FieldRef::volume(track)
}

/// The volume the fader and the Volume field show, through the one parameter gesture.
pub(crate) fn shown_volume(app: &App, track: &Track) -> f64 {
    crate::parameter_gesture::shown_value(app, &volume_field(track.id))
        .unwrap_or(track.mixer_device.volume.value)
}

pub(crate) fn view<'a>(app: &'a App, track: &'a Track, location: View) -> Element<'a, Message> {
    let id = track.id;
    let device = &track.mixer_device;
    let state = State {
        enabled: device.volume.output.enabled,
        ..State::default()
    };
    let controls = app.controls();
    let volume_key = DraftKey::Mixer(id, MixerField::Volume);
    let address_key = DraftKey::Mixer(id, MixerField::Address);
    let address_error = app.drafts.address_error(
        address_key,
        qtable_engine::model::validate_address(&device.volume.output.address),
    );
    let volume = app
        .drafts
        .values
        .get(&volume_key)
        .cloned()
        .unwrap_or_else(|| Parameter::new(&VOLUME, shown_volume(app, track)).display());
    let prefix = app
        .drafts
        .values
        .get(&address_key)
        .cloned()
        .unwrap_or_else(|| device.volume.output.address.clone());
    let address_field = labelled(
        controls,
        "Path",
        state,
        controls.path(
            field_control(
                app,
                location,
                address_key,
                Id::VolumePath,
                PathField {
                    placeholder: "/volume",
                    prefix: Prefix::of(app.project(), id, PathLevel::Under, false),
                    absolute: None,
                    subject: Subject::Volume(id),
                },
            )
            .invalid(address_error.is_some()),
            &prefix,
            move |v| message(MixerMessage::Draft(address_key, v)),
            message(MixerMessage::Submit(address_key)),
            Message::Edit(EditMessage::FocusField(address_key)),
            None,
        ),
    );
    let key = DraftKey::Mixer(id, MixerField::Threshold);
    let threshold_state = State {
        enabled: state.enabled && device.volume.output.send_mode != SendMode::EveryFrame,
        ..state
    };
    let threshold = app
        .drafts
        .values
        .get(&key)
        .cloned()
        .unwrap_or_else(|| THRESHOLD.format(device.volume.output.threshold));
    let send_mode = labelled(
        controls,
        "Send Mode",
        state,
        controls.dropdown(
            Control::new(
                ControlId::action(location, "send-mode", ControlOwner::Track(id)),
                Id::MixerVolumeSend,
                Dropdown,
            )
            .role(SizeRole::Fill)
            .enabled(state.enabled),
            &SendMode::ALL[..],
            Some(device.volume.output.send_mode),
            move |mode| message(MixerMessage::SendMode(id, mode)),
        ),
    );
    let threshold = labelled(
        controls,
        "Threshold",
        threshold_state,
        controls.number(
            field_control(app, location, key, Id::MixerThreshold, Number::new("0"))
                .enabled(threshold_state.enabled)
                .role(SizeRole::DeviceNumber)
                .invalid(app.drafts.errors.contains_key(&key)),
            &threshold,
            move |v| message(MixerMessage::Draft(key, v)),
            message(MixerMessage::Submit(key)),
            Some(message(MixerMessage::ResetThreshold(id))),
        ),
    );
    let volume = labelled(
        controls,
        "Value",
        state,
        crate::parameter_gesture::menu(
            app,
            app.controls().number(
                field_control(
                    app,
                    location,
                    volume_key,
                    Id::MixerVolume,
                    Number::new("0–1"),
                )
                .drives(ControlRef::Argument(volume_field(id)))
                .nudge(crate::control::Nudge::NORMALIZED)
                .role(SizeRole::DeviceNumber)
                .invalid(app.drafts.errors.contains_key(&volume_key)),
                &volume,
                move |v| message(MixerMessage::Draft(volume_key, v)),
                message(MixerMessage::Submit(volume_key)),
                // The same message the fader's Delete key sends: one edit that ends any hold.
                Some(Message::Mixer(crate::mixer_band::MixerMessage::SetVolume(
                    id,
                    VOLUME.default,
                ))),
            ),
            &Subject::Volume(id),
        ),
    );
    let toggle = column![
        Space::new().height(detail::DEVICE_LABEL_ROW + detail::LABEL_GAP),
        controls.toggle(
            Control::new(
                ControlId::action(location, "emit-volume", ControlOwner::Track(id)),
                Id::MixerVolumeOutput,
                Toggle {
                    label: "Enable".into()
                },
            )
            .role(SizeRole::Fill)
            .lit(device.volume.output.enabled),
            message(MixerMessage::Toggle(id)),
        ),
    ];
    let mapping = MappingEditor {
        current: device.volume.output.mapping,
        values: &app.drafts.values,
        errors: &app.drafts.errors,
        keys: mapping_keys(id),
        location,
        enabled: state.enabled,
        help: mapping_help,
    }
    .view(controls, move |msg| message(MixerMessage::Mapping(id, msg)));
    let mut cells = row![cell(
        app.theme,
        "Volume Slider",
        VOLUME_WIDTH,
        CELL_HEIGHT,
        row![
            column![toggle, address_field, volume]
                .spacing(detail::DEVICE_ROW_GAP)
                .width(detail::DEVICE_CONTROL_WIDTH),
            column![send_mode, threshold]
                .spacing(detail::DEVICE_ROW_GAP)
                .width(detail::DEVICE_CONTROL_WIDTH),
            container(mapping).width(detail::DEVICE_CONTROL_WIDTH),
        ]
        .spacing(detail::CONTROL_GAP),
    )]
    .spacing(detail::CONTROL_GAP);
    if !track.capabilities().destination {
        cells = cells.push(cell(
            app.theme,
            "Delay",
            detail::DEVICE_CELL_WIDTH,
            CELL_HEIGHT,
            labelled(
                controls,
                "Track Delay (ms)",
                State::default(),
                crate::delay_fields::delay_input(app, id, location, SizeRole::DeviceNumber),
            ),
        ));
    }
    cells
        .padding(detail::DEVICE_BODY_INSET)
        .width(width(track) - detail::SCROLLBAR_GUTTER)
        .into()
}
#[cfg(test)]
mod tests;
