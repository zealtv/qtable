//! Musical positions, validated tempo and time signatures.
use super::*;

/// An exact, non-negative musical position or duration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BeatTime(pub u64);

impl Serialize for BeatTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(self.0 as f64 / TICKS_PER_BEAT as f64)
    }
}

impl<'de> Deserialize<'de> for BeatTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BeatVisitor;

        impl Visitor<'_> for BeatVisitor {
            type Value = BeatTime;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a finite, non-negative beat value on the 960-tick grid")
            }

            fn visit_f64<E>(self, beats: f64) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                if !beats.is_finite() || beats < 0.0 {
                    return Err(E::custom("beat value must be finite and non-negative"));
                }
                let ticks = beats * TICKS_PER_BEAT as f64;
                let rounded = ticks.round();
                if !ticks.is_finite()
                    || rounded >= (u64::MAX as f64)
                    || (ticks - rounded).abs() > 1e-3
                {
                    return Err(E::custom(
                        "beat value is out of range or farther than 0.001 tick from the 960-tick grid",
                    ));
                }
                Ok(BeatTime(rounded as u64))
            }

            fn visit_i64<E>(self, beats: i64) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                if beats < 0 {
                    return Err(E::custom("beat value must be non-negative"));
                }
                self.visit_f64(beats as f64)
            }

            fn visit_u64<E>(self, beats: u64) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                self.visit_f64(beats as f64)
            }
        }

        deserializer.deserialize_any(BeatVisitor)
    }
}

impl BeatTime {
    pub const ZERO: Self = Self(0);

    /// Returns `None` instead of overflowing for an unrepresentable duration.
    pub fn from_beats(beats: u64) -> Option<Self> {
        beats.checked_mul(TICKS_PER_BEAT).map(Self)
    }

    pub const fn ticks(self) -> u64 {
        self.0
    }
}

/// Positive finite quarter-note beats per minute, without an arbitrary UI range.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedTempo")]
pub struct Tempo(f64);

impl Tempo {
    pub fn new(bpm: f64) -> ModelResult<Self> {
        require(
            bpm.is_finite() && bpm > 0.0,
            "tempo must be positive and finite",
        )?;
        Ok(Self(bpm))
    }

    pub const fn bpm(self) -> f64 {
        self.0
    }
}

impl Default for Tempo {
    fn default() -> Self {
        Self(120.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedTimeSignature")]
pub struct TimeSignature {
    numerator: u16,
    denominator: u16,
}

impl TimeSignature {
    pub fn new(numerator: u16, denominator: u16) -> ModelResult<Self> {
        require(numerator > 0, "time-signature numerator must be positive")?;
        require(
            denominator.is_power_of_two()
                && (4 * TICKS_PER_BEAT).is_multiple_of(u64::from(denominator)),
            "time-signature denominator must be a power of two representable in ticks",
        )?;
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub const fn numerator(self) -> u16 {
        self.numerator
    }

    pub const fn denominator(self) -> u16 {
        self.denominator
    }

    pub fn bar_length(self) -> BeatTime {
        BeatTime(u64::from(self.numerator) * (4 * TICKS_PER_BEAT / u64::from(self.denominator)))
    }
}

impl Default for TimeSignature {
    fn default() -> Self {
        Self {
            numerator: 4,
            denominator: 4,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename = "Tempo")]
struct UncheckedTempo(f64);
impl TryFrom<UncheckedTempo> for Tempo {
    type Error = ModelError;
    fn try_from(value: UncheckedTempo) -> Result<Self, Self::Error> {
        Self::new(value.0)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename = "TimeSignature")]
struct UncheckedTimeSignature {
    numerator: u16,
    denominator: u16,
}
impl TryFrom<UncheckedTimeSignature> for TimeSignature {
    type Error = ModelError;
    fn try_from(value: UncheckedTimeSignature) -> Result<Self, Self::Error> {
        Self::new(value.numerator, value.denominator)
    }
}
