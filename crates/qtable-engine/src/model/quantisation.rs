//! Supported note values and launch grids.
use super::*;

/// Note denominators use whole-note notation (4 = quarter, 8 = eighth).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoteValue {
    Whole,
    Half,
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl NoteValue {
    /// Whole-note denominator: 4 for a quarter, 8 for an eighth.
    pub const fn denominator(self) -> u16 {
        match self {
            Self::Whole => 1,
            Self::Half => 2,
            Self::Quarter => 4,
            Self::Eighth => 8,
            Self::Sixteenth => 16,
            Self::ThirtySecond => 32,
            Self::SixtyFourth => 64,
        }
    }

    pub const fn duration(self) -> BeatTime {
        BeatTime(4 * TICKS_PER_BEAT / self.denominator() as u64)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuantisationValue {
    Bars(u16),
    Note(NoteValue),
    Triplet(NoteValue),
}

impl QuantisationValue {
    pub fn validate(self) -> ModelResult {
        require(
            matches!(
                self,
                Self::Bars(1 | 2 | 4 | 8)
                    | Self::Note(
                        NoteValue::Half
                            | NoteValue::Quarter
                            | NoteValue::Eighth
                            | NoteValue::Sixteenth
                            | NoteValue::ThirtySecond
                    )
                    | Self::Triplet(
                        NoteValue::Half
                            | NoteValue::Quarter
                            | NoteValue::Eighth
                            | NoteValue::Sixteenth
                    )
            ),
            "unsupported launch quantisation value",
        )
    }

    /// Length on the song-position grid; note values are independent of meter.
    pub fn duration(self, signature: TimeSignature) -> ModelResult<BeatTime> {
        self.validate()?;
        Ok(match self {
            Self::Bars(bars) => BeatTime(signature.bar_length().ticks() * u64::from(bars)),
            Self::Note(note) => note.duration(),
            Self::Triplet(note) => BeatTime(note.duration().ticks() * 2 / 3),
        })
    }
}

impl GlobalQuantisation {
    /// The launch quantisation choices in the order of Live's chooser. The transport's
    /// dropdown and the control descriptor's enum choices read this one list.
    pub const CHOICES: [GlobalQuantisation; 14] = {
        use GlobalQuantisation::{None, Value};
        use NoteValue::*;
        use QuantisationValue::*;
        [
            None,
            Value(Bars(8)),
            Value(Bars(4)),
            Value(Bars(2)),
            Value(Bars(1)),
            Value(Note(Half)),
            Value(Triplet(Half)),
            Value(Note(Quarter)),
            Value(Triplet(Quarter)),
            Value(Note(Eighth)),
            Value(Triplet(Eighth)),
            Value(Note(Sixteenth)),
            Value(Triplet(Sixteenth)),
            Value(Note(ThirtySecond)),
        ]
    };

    /// Live's chooser label: "None", "1 Bar", "2 Bars", "1/4", "1/8T".
    pub fn label(self) -> String {
        match self {
            Self::None => "None".to_owned(),
            Self::Value(QuantisationValue::Bars(1)) => "1 Bar".to_owned(),
            Self::Value(QuantisationValue::Bars(bars)) => format!("{bars} Bars"),
            Self::Value(QuantisationValue::Note(note)) => format!("1/{}", note.denominator()),
            Self::Value(QuantisationValue::Triplet(note)) => format!("1/{}T", note.denominator()),
        }
    }

    pub fn validate(self) -> ModelResult {
        match self {
            Self::None => Ok(()),
            Self::Value(value) => value.validate(),
        }
    }

    /// Follow-originated launches use the first boundary at or after `at`: a timer
    /// that lands exactly on the grid launches there, not one quantum later.
    pub fn boundary_at_or_after(
        self,
        at: BeatTime,
        signature: TimeSignature,
    ) -> ModelResult<BeatTime> {
        match self {
            Self::None => Ok(at),
            Self::Value(value) => {
                let length = value.duration(signature)?.ticks();
                at.ticks()
                    .checked_add(length.saturating_sub(1))
                    .and_then(|n| n.checked_div(length))
                    .and_then(|n| n.checked_mul(length))
                    .map(BeatTime)
                    .ok_or_else(|| ModelError("quantisation boundary overflow".into()))
            }
        }
    }

    /// Quantised requests use the strictly next boundary, even at an exact tick.
    /// None uses the current tick and is dispatched on the next planning pass.
    pub fn next_boundary(self, now: BeatTime, signature: TimeSignature) -> ModelResult<BeatTime> {
        match self {
            Self::None => Ok(now),
            Self::Value(value) => {
                let length = value.duration(signature)?.ticks();
                now.ticks()
                    .checked_div(length)
                    .and_then(|n| n.checked_add(1))
                    .and_then(|n| n.checked_mul(length))
                    .map(BeatTime)
                    .ok_or_else(|| ModelError("quantisation boundary overflow".into()))
            }
        }
    }
}

/// The project cannot recursively inherit its own global setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GlobalQuantisation {
    None,
    Value(QuantisationValue),
}

impl Default for GlobalQuantisation {
    fn default() -> Self {
        Self::Value(QuantisationValue::Bars(1))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchQuantisation {
    #[default]
    Global,
    None,
    Value(QuantisationValue),
}
