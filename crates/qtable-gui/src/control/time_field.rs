//! A musical field with one focus target and three independently edited segments.
//! No panel knowledge: callers supply their help id through `Control<TimeField>`.
pub(crate) mod beats;
mod value;
use super::{ControlKind, Interaction, Look, SizeRole, State, Theme};
use qtable_engine::model::{BeatTime, TICKS_PER_BEAT, TimeSignature};
pub(crate) use value::{format_duration, format_position, parse_duration, parse_position};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimeKind {
    Duration,
    Position,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TimeField {
    pub signature: TimeSignature,
    pub kind: TimeKind,
}
impl TimeField {
    pub(crate) fn duration(signature: TimeSignature) -> Self {
        Self {
            signature,
            kind: TimeKind::Duration,
        }
    }
    pub(crate) fn position(signature: TimeSignature) -> Self {
        Self {
            signature,
            kind: TimeKind::Position,
        }
    }
    pub(crate) fn parse(self, text: &str) -> Result<BeatTime, String> {
        match self.kind {
            TimeKind::Duration => parse_duration(text, self.signature),
            TimeKind::Position => parse_position(text, self.signature),
        }
    }
    pub(crate) fn format(self, time: BeatTime) -> String {
        match self.kind {
            TimeKind::Duration => format_duration(time, self.signature),
            TimeKind::Position => format_position(time, self.signature),
        }
    }
    /// Nudge tick time rather than wrapping a digit, so carries and borrows follow the meter.
    pub(crate) fn step(self, text: &str, segment: usize, steps: i64) -> Option<String> {
        let value = self.parse(text).ok()?;
        let bar = self.signature.bar_length().ticks();
        let beat = bar / u64::from(self.signature.numerator());
        let unit = match segment {
            0 => bar,
            1 => beat,
            _ => (TICKS_PER_BEAT / 4).min(beat),
        };
        let ticks = i128::from(value.ticks()) + i128::from(steps) * i128::from(unit);
        if self.kind == TimeKind::Duration && ticks <= 0 {
            return None;
        }
        Some(self.format(BeatTime(ticks.clamp(0, i128::from(u64::MAX)) as u64)))
    }
}
impl ControlKind for TimeField {
    fn look(&self, theme: &Theme, state: State, interaction: Interaction) -> Look {
        super::kinds::field_look(theme, state, interaction)
    }
    fn role(&self) -> SizeRole {
        SizeRole::Float
    }
}

/// Byte/grapheme ranges coincide for valid musical text (ASCII). Trim spaces so the
/// segment highlight covers the digits, while the separators remain on the normal face.
pub(super) fn ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut offset = 0;
    text.split('.')
        .take(3)
        .map(|part| {
            let start = offset + part.len() - part.trim_start().len();
            let end = offset + part.trim_end().len();
            offset += part.len() + 1;
            start..end.max(start)
        })
        .collect()
}

#[cfg(test)]
mod tests;
