//! A reusable mapping editor over the caller's typed text and rejections, keyed by the
//! caller's own field keys. Owners commit validated replacements through engine edits.

use super::widgets::ControlMessage;
use super::{Control, Controls, Number, SizeRole, ValueFormat};
use crate::control_focus::{ControlId, ControlKey, View};
use crate::style::detail;
use iced::Element;
use iced::widget::{column, row};
use qtable_engine::model::{Mapping, MappingError};
use std::collections::HashMap;

/// The fixed settings of a Mapping, not an extensible mapping-source kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MappingField {
    InputStart,
    InputEnd,
    OutputStart,
    OutputEnd,
    Exponent,
    Bias,
}

impl MappingField {
    pub(crate) const ALL: [Self; 6] = [
        Self::InputStart,
        Self::InputEnd,
        Self::OutputStart,
        Self::OutputEnd,
        Self::Exponent,
        Self::Bias,
    ];

    fn index(self) -> usize {
        self as usize
    }

    fn value(self, mapping: Mapping) -> f64 {
        match self {
            Self::InputStart => mapping.input()[0],
            Self::InputEnd => mapping.input()[1],
            Self::OutputStart => mapping.output()[0],
            Self::OutputEnd => mapping.output()[1],
            Self::Exponent => mapping.exponent(),
            Self::Bias => mapping.bias(),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::InputStart => "In Start",
            Self::InputEnd => "In End",
            Self::OutputStart => "Out Start",
            Self::OutputEnd => "Out End",
            Self::Exponent => "Exponent",
            Self::Bias => "Bias",
        }
    }

    fn replace(self, current: Mapping, value: f64) -> Result<Mapping, MappingError> {
        let mut input = current.input();
        let mut output = current.output();
        let mut exponent = current.exponent();
        let mut bias = current.bias();
        match self {
            Self::InputStart => input[0] = value,
            Self::InputEnd => input[1] = value,
            Self::OutputStart => output[0] = value,
            Self::OutputEnd => output[1] = value,
            Self::Exponent => exponent = value,
            Self::Bias => bias = value,
        }
        Mapping::new(input, output, exponent, bias)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MappingEditorMessage {
    Changed(MappingField, String),
    Submitted(MappingField),
}

impl MappingEditorMessage {
    /// The caller uses this key for its existing pending-field/focus path.
    pub(crate) fn key<Key: ControlKey>(&self, keys: &[Key; 6]) -> Key {
        let field = match self {
            Self::Changed(field, _) | Self::Submitted(field) => field,
        };
        keys[field.index()]
    }
}

/// Update the caller's typed text (`values`) and rejections (`errors`) and return a validated
/// replacement for the caller's ProjectEdit. The caller forgets a successful entry only after
/// the edit lands; failures retain the typed text. No entry means an untouched display.
pub(crate) fn update<Key: ControlKey>(
    values: &mut HashMap<Key, String>,
    errors: &mut HashMap<Key, String>,
    current: Mapping,
    keys: &[Key; 6],
    enabled: bool,
    message: MappingEditorMessage,
) -> Result<Option<Mapping>, MappingError> {
    if !enabled {
        return Ok(None);
    }
    let key = message.key(keys);
    match message {
        MappingEditorMessage::Changed(_, value) => {
            values.insert(key, value);
            errors.remove(&key);
            Ok(None)
        }
        MappingEditorMessage::Submitted(field) => {
            let Some(value) = values.get(&key) else {
                return Ok(None);
            };
            let candidate = value
                .trim()
                .parse::<f64>()
                .map_err(|_| MappingError("Enter a valid mapping number"))
                .and_then(|value| field.replace(current, value));
            match candidate {
                Ok(mapping) => {
                    errors.remove(&key);
                    Ok(Some(mapping))
                }
                Err(error) => {
                    errors.insert(key, error.to_string());
                    Err(error)
                }
            }
        }
    }
}

/// Read-only view context; keys are caller-owned identities in MappingField::ALL
/// order, distinct for each mapping owner. This editor stores no text itself.
pub(crate) struct MappingEditor<'a, Key: ControlKey> {
    pub current: Mapping,
    pub values: &'a HashMap<Key, String>,
    pub errors: &'a HashMap<Key, String>,
    pub keys: [Key; 6],
    pub location: View,
    pub enabled: bool,
    /// The help entry for each field, from the caller's registry.
    pub help: fn(MappingField) -> Key::Help,
}

impl<'a, Key: ControlKey> MappingEditor<'a, Key> {
    pub(crate) fn view<M: ControlMessage<Key> + 'a>(
        self,
        controls: Controls<'a, Key>,
        on_message: impl Fn(MappingEditorMessage) -> M + Clone + 'a,
    ) -> Element<'a, M> {
        let mut rows = column![].spacing(detail::DEVICE_ROW_GAP);
        for pair in MappingField::ALL.chunks_exact(2) {
            let mut fields = row![].spacing(detail::CONTROL_GAP);
            for &field in pair {
                let key = self.keys[field.index()];
                let value = self
                    .values
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| ValueFormat::Float.display(field.value(self.current)));
                let mut control = Control::new(
                    ControlId::field(self.location, key),
                    (self.help)(field),
                    Number::new("0"),
                )
                .role(SizeRole::DeviceNumber)
                .invalid(self.errors.contains_key(&key));
                control.state.enabled = self.enabled;
                let state = control.state;
                let changed = on_message.clone();
                let input = controls.number(
                    control,
                    &value,
                    move |text| changed(MappingEditorMessage::Changed(field, text)),
                    on_message(MappingEditorMessage::Submitted(field)),
                    None,
                );
                fields = fields.push(super::device_layout::labelled(
                    controls,
                    field.label(),
                    state,
                    input,
                ));
            }
            rows = rows.push(fields);
        }
        rows.into()
    }
}

#[cfg(test)]
mod tests;
