//! Event argument signatures and value conversion.
use super::*;

impl FieldType {
    pub fn of(argument: &OscArgument) -> Self {
        match argument {
            OscArgument::Int(_) => Self::Int,
            OscArgument::Float(_) => Self::Float,
            OscArgument::String(_) => Self::String,
            OscArgument::Bool(_) => Self::Bool,
        }
    }

    /// The value a missing argument takes when hits are conformed.
    pub fn default_argument(self) -> OscArgument {
        match self {
            Self::Int => OscArgument::Int(0),
            Self::Float => OscArgument::Float(0.0),
            Self::String => OscArgument::String(String::new()),
            Self::Bool => OscArgument::Bool(false),
        }
    }

    /// The letter a signature shows: `i f s b`.
    pub fn letter(self) -> char {
        match self {
            Self::Int => 'i',
            Self::Float => 'f',
            Self::String => 's',
            Self::Bool => 'b',
        }
    }
}

impl OscArgument {
    /// Retype a value by the target design's conversion table.
    pub fn convert(self, kind: FieldType) -> Self {
        match (self, kind) {
            (value, kind) if FieldType::of(&value) == kind => value,
            (Self::Int(v), FieldType::Float) => Self::Float(v as f32),
            (Self::Int(v), FieldType::String) => Self::String(v.to_string()),
            (Self::Int(v), _) => Self::Bool(v != 0),
            // `as` rounds toward zero and saturates; round first.
            (Self::Float(v), FieldType::Int) => Self::Int(v.round() as i32),
            (Self::Float(v), FieldType::String) => Self::String(format_float(v, None)),
            (Self::Float(v), _) => Self::Bool(v != 0.0),
            (Self::String(v), FieldType::Int) => Self::Int(v.trim().parse().unwrap_or(0)),
            (Self::String(v), FieldType::Float) => Self::Float(v.trim().parse().unwrap_or(0.0)),
            (Self::String(v), _) => Self::Bool(
                ["true", "on", "1"]
                    .iter()
                    .any(|word| v.trim().eq_ignore_ascii_case(word)),
            ),
            (Self::Bool(v), FieldType::Int) => Self::Int(v.into()),
            (Self::Bool(v), FieldType::Float) => Self::Float(if v { 1.0 } else { 0.0 }),
            (Self::Bool(v), _) => Self::String(v.to_string()),
        }
    }
}

/// A signature as dim letters (`i f`), or "no arguments".
pub fn signature_text(signature: &[FieldType]) -> String {
    if signature.is_empty() {
        return "no arguments".into();
    }
    signature
        .iter()
        .map(|kind| kind.letter().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

impl Event {
    pub fn signature(&self) -> Vec<FieldType> {
        self.arguments.iter().map(FieldType::of).collect()
    }

    /// Match `signature` by position: convert retyped values, append defaults, drop extras.
    /// Returns whether a value the hit held was retyped or dropped; appending a default
    /// loses nothing, so it alone returns false.
    pub fn conform(&mut self, signature: &[FieldType]) -> bool {
        let mut converted = self.arguments.len() > signature.len();
        self.arguments.truncate(signature.len());
        for (index, kind) in signature.iter().enumerate() {
            match self.arguments.get_mut(index) {
                Some(argument) if FieldType::of(argument) != *kind => {
                    let value = std::mem::replace(argument, OscArgument::Bool(false));
                    *argument = value.convert(*kind);
                    converted = true;
                }
                Some(_) => {}
                None => self.arguments.push(kind.default_argument()),
            }
        }
        converted
    }
}

impl FieldView<'_> {
    /// Convert a numeric control value once, before it enters the send queue.
    pub fn argument(&self) -> OscArgument {
        if self.control == Control::Enum {
            return self.options[self.value.round() as usize].value.clone();
        }
        match self.value_type {
            FieldType::Float => OscArgument::Float(self.value as f32),
            FieldType::Int => OscArgument::Int(self.value.round() as i32),
            FieldType::Bool => OscArgument::Bool(self.value != 0.0),
            FieldType::String => unreachable!("String parameters use Enum"),
        }
    }
}
