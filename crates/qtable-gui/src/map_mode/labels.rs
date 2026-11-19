//! The wording and the number rules of the mapping list and inspector, apart from any widget:
//! how a source reads, the modes and takeovers it offers, what a row's Min and Max mean for its
//! control, how sensitivity reads, and the range a learned mapping starts with. `qtable-remote` decides what each policy does (`translate.rs`); this file
//! presents the same policies, so the list never shows one thing and the resolver does another.
use qtable_engine::model::{
    self, ControlDescriptor, ControlRef, ControlValue, MidiMessage, RelativeEncoding,
    SENSITIVITY_RANGE, Source, SourceMode, Takeover, format_float,
};

/// Live names MIDI note 60 "C3".
pub(crate) fn note_name(note: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!(
        "{}{}",
        NAMES[usize::from(note % 12)],
        i32::from(note / 12) - 2
    )
}

/// The Ch column: a MIDI source's channel (1–16); keys have none.
pub(crate) fn channel(source: &Source) -> String {
    match source {
        Source::Midi(midi) => midi.channel.to_string(),
        Source::Key(_) => String::new(),
    }
}

/// The Note/Control column, as Live writes it: `CC 21`, `Note C3`.
pub(crate) fn control_cell(source: &Source) -> String {
    match source {
        Source::Midi(midi) => match midi.message {
            MidiMessage::Cc(number) => format!("CC {number}"),
            MidiMessage::Note(note) => format!("Note {}", note_name(note)),
            MidiMessage::PitchBend => "Pitch Bend".into(),
        },
        Source::Key(key) => format!("Key {}", key.key.0),
    }
}

/// The few characters a mapped control wears in Map Mode, where a launch square leaves little room.
pub(crate) fn short(source: &Source) -> String {
    match source {
        Source::Midi(midi) => match midi.message {
            MidiMessage::Cc(number) => format!("CC{number}"),
            MidiMessage::Note(note) => note_name(note),
            MidiMessage::PitchBend => "PB".into(),
        },
        Source::Key(key) => key.key.0.clone(),
    }
}

/// Every mode there is, in the chooser's order. Which of them a source offers is the model's
/// call (`Source::supports_mode`); the GUI keeps no list of its own per source.
pub(crate) const MODES: [SourceMode; 10] = [
    SourceMode::Absolute,
    SourceMode::Gate,
    SourceMode::Relative(RelativeEncoding::SignedBit),
    SourceMode::Relative(RelativeEncoding::SignedBit2),
    SourceMode::Relative(RelativeEncoding::BinOffset),
    SourceMode::Relative(RelativeEncoding::TwosComplement),
    SourceMode::Relative(RelativeEncoding::SignedBitLinear),
    SourceMode::Relative(RelativeEncoding::SignedBit2Linear),
    SourceMode::Relative(RelativeEncoding::BinOffsetLinear),
    SourceMode::Relative(RelativeEncoding::TwosComplementLinear),
];

/// The modes `source` can be in, in the chooser's order: those the model accepts for it (a
/// CC's `Absolute` and eight relatives, a note's or key's Latch and Gate, pitch bend's
/// `Absolute` alone, which the inspector never offers as a choice).
pub(crate) fn modes(source: &Source) -> Vec<SourceMode> {
    MODES
        .into_iter()
        .filter(|mode| source.supports_mode(*mode))
        .collect()
}

/// Live's names for the modes (its status-bar chooser), with Linear marking the variants that
/// skip Live's step curve. A note or key reads its `Absolute` as Latch: a press flips or
/// advances the control and a release does nothing.
pub(crate) fn mode_label(source: &Source, mode: SourceMode) -> &'static str {
    use RelativeEncoding::*;
    let pressed = matches!(
        source,
        Source::Key(_)
            | Source::Midi(model::MidiSource {
                message: MidiMessage::Note(_),
                ..
            })
    );
    match mode {
        SourceMode::Absolute if pressed => "Latch",
        SourceMode::Absolute => "Absolute",
        SourceMode::Gate => "Gate",
        SourceMode::Relative(SignedBit) => "Relative (Signed Bit)",
        SourceMode::Relative(SignedBit2) => "Relative (Signed Bit 2)",
        SourceMode::Relative(BinOffset) => "Relative (Bin Offset)",
        SourceMode::Relative(TwosComplement) => "Relative (2's Comp.)",
        SourceMode::Relative(SignedBitLinear) => "Relative (Signed Bit) Linear",
        SourceMode::Relative(SignedBit2Linear) => "Relative (Signed Bit 2) Linear",
        SourceMode::Relative(BinOffsetLinear) => "Relative (Bin Offset) Linear",
        SourceMode::Relative(TwosComplementLinear) => "Relative (2's Comp.) Linear",
    }
}

/// The takeovers an absolute CC or pitch bend offers, in the chooser's order, with Live's names.
pub(crate) const TAKEOVERS: [Takeover; 3] =
    [Takeover::None, Takeover::PickUp, Takeover::ValueScaling];

pub(crate) fn takeover_label(takeover: Takeover) -> &'static str {
    match takeover {
        Takeover::None => "None",
        Takeover::PickUp => "Pick-Up",
        Takeover::ValueScaling => "Value Scaling",
    }
}

/// The source written out for the inspector: `MIDI Ch 1 · CC 22`, `Key a`.
pub(crate) fn source_line(source: &Source) -> String {
    match source {
        Source::Midi(midi) => format!("MIDI Ch {} · {}", midi.channel, control_cell(source)),
        Source::Key(_) => control_cell(source),
    }
}

/// A sensitivity factor as the inspector shows it: a percentage, `100 %` for 1.0, in the form
/// the Number field's arrow keys write (`control::adjustment::percent_in`).
pub(crate) fn format_sensitivity(factor: f64) -> String {
    format!("{} %", trim_fraction(factor * 100.0))
}

/// A typed sensitivity, as a factor, or the reason it was refused (for the status bar).
pub(crate) fn parse_sensitivity(text: &str) -> Result<f64, String> {
    let text = text.trim();
    let percent = text.strip_suffix('%').unwrap_or(text).trim();
    let [low, high] = SENSITIVITY_RANGE.map(|factor| factor * 100.0);
    match percent.parse::<f64>() {
        Ok(value) if value.is_finite() && (low..=high).contains(&value) => Ok(value / 100.0),
        _ => Err(format!(
            "Enter a percentage between {} and {}",
            trim_fraction(low),
            trim_fraction(high)
        )),
    }
}

/// The Path column: Live separates the levels with `|`.
pub(crate) fn path(descriptor: &ControlDescriptor) -> String {
    descriptor.path.replace(" / ", " | ")
}

/// What a row's Min and Max mean for its control: the model's rule (`model::Limits`), with the
/// choice names an Enum shows. The list and `qtable-remote`'s translation share that rule.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Limits {
    /// A launch, stop or bang: nothing to scale, so the row has no Min or Max (as Live's slot row).
    None,
    /// Control units, an optional `[low, high]` the control accepts.
    Number {
        integer: bool,
        range: Option<[f64; 2]>,
    },
    /// An Enum: the first and last choice; the hardware range splits into equal bins between them.
    Choice(Vec<String>),
    /// A switch (Solo, Follow Actions): Off or On at each end of the hardware range.
    Switch,
    /// An Activator: the window of hardware values (0–100%) in which the track is active.
    Window,
}

pub(crate) fn limits(control: &ControlRef, descriptor: &ControlDescriptor) -> Limits {
    named(
        model::control_limits(control, &ControlValue::from(descriptor)),
        descriptor,
    )
}

/// The model's `limits` with the choice names `descriptor` gives an Enum.
pub(crate) fn named(limits: model::Limits, descriptor: &ControlDescriptor) -> Limits {
    match limits {
        model::Limits::None => Limits::None,
        model::Limits::Number { integer, range } => Limits::Number { integer, range },
        model::Limits::Choice { .. } => Limits::Choice(descriptor.choices.clone()),
        model::Limits::Switch => Limits::Switch,
        model::Limits::Window => Limits::Window,
    }
}

/// The text a Min or Max shows, in the units its `Limits` use.
pub(crate) fn format_limit(limits: &Limits, value: f64) -> String {
    match limits {
        Limits::None => String::new(),
        Limits::Number { integer: true, .. } => format!("{}", value.round() as i64),
        Limits::Number { .. } => format_float(value, Some(4)),
        Limits::Choice(choices) => choices
            .get(value.round().max(0.0) as usize)
            .cloned()
            .unwrap_or_else(|| format!("{}", value.round() as i64)),
        Limits::Switch => if value >= 0.5 { "On" } else { "Off" }.into(),
        Limits::Window => format!("{}%", trim_fraction(value * 100.0)),
    }
}

/// One decimal at most, without a trailing `.0`: 100 and 50.4.
fn trim_fraction(value: f64) -> String {
    let text = format!("{value:.1}");
    text.strip_suffix(".0").map_or(text.clone(), str::to_owned)
}

/// A typed Min or Max, in control units, or the reason it was refused (for the status bar).
pub(crate) fn parse_limit(limits: &Limits, text: &str) -> Result<f64, String> {
    let text = text.trim();
    match limits {
        Limits::Window => {
            let percent = text.strip_suffix('%').unwrap_or(text).trim();
            match percent.parse::<f64>() {
                Ok(value) if (0.0..=100.0).contains(&value) => Ok(value / 100.0),
                _ => Err("Enter a percentage between 0 and 100".into()),
            }
        }
        Limits::Number { integer, range } => {
            let value = text
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .ok_or("Enter a valid number")?;
            let value = if *integer { value.round() } else { value };
            match range {
                Some([low, high]) if value < *low || value > *high => Err(format!(
                    "Enter a value between {} and {}",
                    format_float(*low, Some(4)),
                    format_float(*high, Some(4))
                )),
                _ => Ok(value),
            }
        }
        Limits::None | Limits::Choice(_) | Limits::Switch => {
            Err("Choose a value from the list".into())
        }
    }
}

/// Min and Max a freshly learned mapping starts with: the control's whole range, as Live does
/// (the model's `learn_range`).
pub(crate) fn learn_range(control: &ControlRef, descriptor: &ControlDescriptor) -> (f64, f64) {
    model::learn_range(control, &ControlValue::from(descriptor))
}

#[cfg(test)]
mod tests;
