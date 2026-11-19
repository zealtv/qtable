//! Saved bindings. Decoding gestures and takeover history belong to the remote client.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Source {
    Midi(MidiSource),
    Key(KeySource),
}

/// Channel numbers use the displayed MIDI convention, 1 through 16. No port is stored:
/// a mapping matches any enabled Remote input; runtime history remains per port.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MidiSource {
    pub channel: u8,
    pub message: MidiMessage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MidiMessage {
    Cc(u8),
    Note(u8),
    PitchBend,
}

/// A toolkit-independent key source: the one character the key types ("a", "A", "1", "!", "/").
/// Live maps typed characters only: letters in both cases, digits, shifted digit symbols and
/// punctuation. Named keys (Space, arrows, Return, Escape, F-keys) and modifiers never map, and
/// keys held with Command, Option or Control are reserved for shortcuts. The key adapter owns
/// translation from platform events; modifiers are not stored.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyName(pub String);

impl KeyName {
    /// Whether a typed character can be a key source: any printable character that is not
    /// whitespace (so not Space, Tab or Return) and not a control or private-use code point
    /// (which platforms use for arrows and function keys).
    pub fn accepts(c: char) -> bool {
        !c.is_whitespace()
            && !c.is_control()
            && !matches!(c, '\u{E000}'..='\u{F8FF}' | '\u{FFF0}'..='\u{FFFF}')
    }

    /// The source for exactly one typed character, case kept (`a` and `A` differ).
    pub fn from_typed(text: &str) -> Option<Self> {
        let mut chars = text.chars();
        let c = chars.next()?;
        (chars.next().is_none() && Self::accepts(c)).then(|| Self(text.to_owned()))
    }

    pub fn is_valid(&self) -> bool {
        Self::from_typed(&self.0).is_some()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeySource {
    pub key: KeyName,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SourceMode {
    /// A CC or pitch bend sends its value; on a note or key it is the Latch mode: a press
    /// flips or advances the control, a release does nothing (`mode: Absolute`, as in format 11).
    #[default]
    Absolute,
    Relative(RelativeEncoding),
    /// A note or key holds its control at Max (or On) while down and returns it to Min (or
    /// Off) on release. Format 12.
    Gate,
}

/// Live's relative CC encodings, including the variants without its step curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RelativeEncoding {
    SignedBit,
    SignedBit2,
    BinOffset,
    TwosComplement,
    SignedBitLinear,
    SignedBit2Linear,
    BinOffsetLinear,
    TwosComplementLinear,
}

/// Stored per mapping (decided 2026-11-03). Relative, key and note sources ignore it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Takeover {
    None,
    PickUp,
    /// Converges on the hardware smoothly instead of waiting to cross it (Live's third
    /// takeover mode). Format 12.
    ValueScaling,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteMapping {
    pub id: MappingId,
    pub source: Source,
    pub control: ControlRef,
    /// Control units; min > max inverts the mapping and equal ends hold one value.
    pub min: f64,
    pub max: f64,
    pub mode: SourceMode,
    pub takeover: Takeover,
    /// How far one step of a relative encoder moves, as a factor of today's step: 1.0 is the
    /// step format 11 always used. Read only by relative CC sources; stored and inactive
    /// otherwise. Format 12; absent from the file at 1.0.
    #[serde(
        default = "default_sensitivity",
        skip_serializing_if = "is_default_sensitivity"
    )]
    pub sensitivity: f64,
}

/// The factor that keeps a relative step as it was before sensitivity existed.
pub const DEFAULT_SENSITIVITY: f64 = 1.0;
/// The 1 to 1000 % range (design, 2026-11-15), as factors.
pub const SENSITIVITY_RANGE: [f64; 2] = [0.01, 10.0];

fn default_sensitivity() -> f64 {
    DEFAULT_SENSITIVITY
}

fn is_default_sensitivity(value: &f64) -> bool {
    *value == DEFAULT_SENSITIVITY
}

impl Source {
    /// A control can have one binding per source kind (MIDI and key independently).
    pub(crate) fn kind(&self) -> std::mem::Discriminant<Self> {
        std::mem::discriminant(self)
    }

    pub fn supports_relative(&self) -> bool {
        matches!(
            self,
            Self::Midi(MidiSource {
                message: MidiMessage::Cc(_),
                ..
            })
        )
    }

    /// Whether this source can be in `mode`: every source is Absolute, a CC alone can be
    /// Relative, and a note or key alone can Gate. Pitch bend has only Absolute.
    pub fn supports_mode(&self, mode: SourceMode) -> bool {
        match mode {
            SourceMode::Absolute => true,
            SourceMode::Relative(_) => self.supports_relative(),
            SourceMode::Gate => matches!(
                self,
                Self::Key(_)
                    | Self::Midi(MidiSource {
                        message: MidiMessage::Note(_),
                        ..
                    })
            ),
        }
    }

    pub fn uses_takeover(&self, mode: SourceMode) -> bool {
        mode == SourceMode::Absolute
            && matches!(
                self,
                Self::Midi(MidiSource {
                    message: MidiMessage::Cc(_) | MidiMessage::PitchBend,
                    ..
                })
            )
    }

    pub fn validate(&self) -> ModelResult {
        match self {
            Self::Midi(source) => {
                require(
                    (1..=16).contains(&source.channel),
                    "MIDI channel must be between 1 and 16",
                )?;
                if let MidiMessage::Cc(number) | MidiMessage::Note(number) = source.message {
                    require(number <= 127, "MIDI CC or note must be between 0 and 127")?;
                }
            }
            Self::Key(source) => require(
                source.key.is_valid(),
                "key source must be one typed letter, digit or punctuation character",
            )?,
        }
        Ok(())
    }
}

impl RemoteMapping {
    /// Freshly learned sources start Absolute with no takeover and the standard sensitivity,
    /// for every source kind.
    pub fn new(id: MappingId, source: Source, control: ControlRef, min: f64, max: f64) -> Self {
        Self {
            id,
            source,
            control,
            min,
            max,
            mode: SourceMode::Absolute,
            takeover: Takeover::None,
            sensitivity: DEFAULT_SENSITIVITY,
        }
    }

    pub fn validate(&self, project: &Project) -> ModelResult {
        self.source.validate()?;
        require(
            self.min.is_finite() && self.max.is_finite(),
            "mapping range must be finite",
        )?;
        require(
            self.sensitivity.is_finite()
                && (SENSITIVITY_RANGE[0]..=SENSITIVITY_RANGE[1]).contains(&self.sensitivity),
            "sensitivity must be between 1% and 1000%",
        )?;
        require(
            !matches!(self.mode, SourceMode::Relative(_)) || self.source.supports_relative(),
            "relative mode requires a MIDI CC source",
        )?;
        require(
            self.mode != SourceMode::Gate || self.source.supports_mode(SourceMode::Gate),
            "gate mode requires a MIDI note or key source",
        )?;
        // A control that holds no value (a trigger, or an argument retyped to String) keeps
        // a stored Gate: the resolver acts on it as Latch, and the inspector offers no Gate.
        describe_value(project, &self.control)
            .map(|_| ())
            .ok_or_else(|| ModelError("mapping references missing control".into()))
    }
}

impl Project {
    pub(crate) fn validate_mappings(&self) -> ModelResult {
        let mut ids = HashSet::new();
        let mut targets = HashSet::new();
        for mapping in &self.mappings {
            require(ids.insert(mapping.id), "duplicate mapping ID")?;
            require(
                targets.insert((&mapping.control, mapping.source.kind())),
                "control already has a mapping of this source kind",
            )?;
            mapping.validate(self)?;
        }
        Ok(())
    }

    /// Follow the project's other id spaces: allocate after the largest retained id.
    pub fn next_mapping_id(&self) -> ModelResult<MappingId> {
        self.mappings
            .iter()
            .map(|m| m.id.0)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .map(MappingId)
            .ok_or_else(|| ModelError("mapping IDs exhausted".into()))
    }
}
