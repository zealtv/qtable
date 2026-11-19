//! A described, adjustable value after Live's `DeviceParameter` (2026-10-12).
//!
//! This describes stored timing offsets and MixerDevice volume. Addresses, curves and automation attach
//! later without changing these fields.
mod flow;
mod identity;
mod mapping;
mod output;
mod routing;
pub use flow::parameter_value_flow;
pub use identity::*;
pub use mapping::{Mapping, MappingError};
pub use output::*;

use super::*;

/// What a value measures. Variants arrive with their first user.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Milliseconds,
    Seconds,
    Hertz,
    MessagesPerSecond,
    None,
}

impl Unit {
    pub fn suffix(self) -> &'static str {
        match self {
            Unit::Milliseconds => " ms",
            Unit::Seconds => " s",
            Unit::Hertz => " Hz",
            Unit::MessagesPerSecond => " msg/s",
            Unit::None => "",
        }
    }
}

/// LOM `is_quantized` and `value_items` in one: how values within the range are chosen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParameterKind {
    Continuous,
    /// Whole multiples of `step` counted from `min`.
    Quantised {
        step: f64,
    },
    /// Named items; the value is the item's index.
    Enum(&'static [&'static str]),
}

/// The description shared by every instance of one parameter.
#[derive(Clone, Debug, PartialEq)]
pub struct ParameterSpec {
    pub name: std::borrow::Cow<'static, str>,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub kind: ParameterKind,
    pub unit: Unit,
}

/// Signed send offset shared by track delay and output latency, within the model's limit.
pub const TRACK_DELAY: ParameterSpec = ParameterSpec {
    name: std::borrow::Cow::Borrowed("Track Delay"),
    min: -MAX_TRACK_DELAY_MS,
    max: MAX_TRACK_DELAY_MS,
    default: 0.0,
    kind: ParameterKind::Continuous,
    unit: Unit::Milliseconds,
};
pub const OUTPUT_LATENCY: ParameterSpec = ParameterSpec {
    name: std::borrow::Cow::Borrowed("Latency"),
    min: -MAX_TRACK_DELAY_MS,
    max: MAX_TRACK_DELAY_MS,
    default: 0.0,
    kind: ParameterKind::Continuous,
    unit: Unit::Milliseconds,
};
pub const STREAM_RATE: ParameterSpec = ParameterSpec {
    name: std::borrow::Cow::Borrowed("Rate"),
    min: 1.0,
    max: 60.0,
    default: 30.0,
    kind: ParameterKind::Continuous,
    unit: Unit::Hertz,
};

// Keep the former file invariant so existing sets retain their chosen rate.
pub(crate) const STORED_STREAM_RATE: ParameterSpec = ParameterSpec {
    max: 200.0,
    ..STREAM_RATE
};

/// Float text with at least two decimal places. `None` preserves the native
/// type's shortest round-tripping representation; a limit rounds only the
/// display and trims trailing zeros beyond the second decimal place.
/// Pass f32 values directly to avoid exposing precision from widening to f64.
pub fn format_float(value: impl fmt::Display, max_decimals: Option<usize>) -> String {
    let mut text = match max_decimals {
        Some(decimals) => format!("{value:.precision$}", precision = decimals.max(2)),
        None => value.to_string(),
    };
    if matches!(text.as_str(), "NaN" | "inf" | "-inf") {
        return text;
    }
    if let Some(point) = text.find('.') {
        while text.len() > point + 3 && text.ends_with('0') {
            text.pop();
        }
    }
    match text.split_once('.') {
        None => text.push_str(".00"),
        Some((_, fraction)) if fraction.len() == 1 => text.push('0'),
        _ => {}
    }
    text
}

impl ParameterSpec {
    /// What messages call the value: its name, or "Value" for a spec nobody named.
    fn subject(&self) -> &str {
        if self.name.is_empty() {
            "Value"
        } else {
            &self.name
        }
    }

    /// Text for an edit field: floats show two to four decimals and no unit; the model keeps
    /// full precision.
    pub fn format(&self, value: f64) -> String {
        match self.kind {
            ParameterKind::Continuous => format_float(value, Some(4)),
            ParameterKind::Quantised { step } if step.fract() == 0.0 => format!("{value:.0}"),
            ParameterKind::Quantised { .. } => format_float(value, Some(4)),
            ParameterKind::Enum(items) => items
                .get(value.round().max(0.0) as usize)
                .map_or_else(String::new, |item| (*item).to_owned()),
        }
    }

    /// `format` plus the unit, for read-only places such as tooltips.
    pub fn format_with_unit(&self, value: f64) -> String {
        format!("{}{}", self.format(value), self.unit.suffix())
    }

    /// Reads typed text into a value within range, snapping quantised values.
    pub fn parse(&self, input: &str) -> Result<f64, String> {
        let text = input.trim();
        if let ParameterKind::Enum(items) = self.kind {
            let value = items
                .iter()
                .position(|item| item.eq_ignore_ascii_case(text))
                .map(|index| index as f64)
                .ok_or_else(|| format!("Choose one of: {}", items.join(", ")))?;
            return self.check(value);
        }
        let value = text.parse::<f64>().map_err(|_| {
            format!(
                "Enter a valid {}{}",
                self.subject().to_lowercase(),
                self.unit.words()
            )
        })?;
        self.check(value)
    }

    /// Checks a numeric value directly, using the same range and snapping as typed edits.
    pub fn check(&self, value: f64) -> Result<f64, String> {
        if !value.is_finite() {
            return Err(format!("{} must be finite", self.subject()));
        }
        if value < self.min || value > self.max {
            return Err(format!(
                "{} must be between {} and {}",
                self.subject(),
                self.format(self.min),
                self.format_with_unit(self.max)
            ));
        }
        Ok(match self.kind {
            ParameterKind::Quantised { step } => {
                self.min + ((value - self.min) / step).round() * step
            }
            ParameterKind::Enum(_) => value.round(),
            _ => value,
        })
    }

    /// True when `input` is the display of `model`, i.e. not edited.
    pub fn is_untouched_display(&self, input: &str, model: f64) -> bool {
        input == self.format(model)
    }
}

impl Unit {
    fn words(self) -> &'static str {
        match self {
            Unit::Milliseconds => " in milliseconds",
            Unit::Seconds => " in seconds",
            Unit::Hertz => " in hertz",
            Unit::MessagesPerSecond => " in messages per second",
            Unit::None => "",
        }
    }
}

/// A spec with its current value, as a view over what the model stores.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parameter<'a> {
    pub spec: &'a ParameterSpec,
    pub value: f64,
    /// Whether the control currently accepts edits.
    pub is_enabled: bool,
}

impl<'a> Parameter<'a> {
    pub fn new(spec: &'a ParameterSpec, value: f64) -> Self {
        Self {
            spec,
            value,
            is_enabled: true,
        }
    }

    pub fn display(&self) -> String {
        self.spec.format(self.value)
    }
}

impl Track {
    pub fn delay_parameter(&self) -> Parameter<'static> {
        Parameter::new(&TRACK_DELAY, self.delay_ms)
    }
}

impl Track {
    /// An output's Latency (stored as its `delay_ms`).
    pub fn latency_parameter(&self) -> Parameter<'static> {
        Parameter::new(&OUTPUT_LATENCY, self.delay_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_text_pads_without_losing_native_precision() {
        for (value, expected) in [
            (1.0, "1.00"),
            (0.0, "0.00"),
            (-0.0, "-0.00"),
            (-12.5, "-12.50"),
            (0.125, "0.125"),
            (120.126, "120.126"),
        ] {
            assert_eq!(format_float(value, None), expected);
        }
        assert_eq!(format_float(0.123_f32, None), "0.123");
        for value in [f32::MIN_POSITIVE, f32::MAX, f32::from_bits(1), -0.0] {
            let text = format_float(value, None);
            assert_eq!(text.parse::<f32>().unwrap().to_bits(), value.to_bits());
            assert!(text.split_once('.').unwrap().1.len() >= 2);
        }
        for value in [f64::MIN_POSITIVE, f64::MAX, f64::from_bits(1), -0.0] {
            let text = format_float(value, None);
            assert_eq!(text.parse::<f64>().unwrap().to_bits(), value.to_bits());
            assert!(text.split_once('.').unwrap().1.len() >= 2);
        }
    }

    #[test]
    fn fractional_parameters_and_volume_keep_extra_digits() {
        let fractional = ParameterSpec {
            kind: ParameterKind::Quantised { step: 0.125 },
            ..TRACK_DELAY
        };
        assert_eq!(fractional.format(0.125), "0.125");
        assert_eq!(VOLUME.format(0.1234), "0.1234");
        assert_eq!(VOLUME.format(1.0), "1.00");
        assert_eq!(VOLUME.format(0.73456789), "0.7346");
        assert_eq!(fractional.format(0.12345678), "0.1235");
    }

    #[test]
    fn limited_float_text_rounds_and_trims_only_beyond_two_places() {
        for (value, expected) in [
            (0.5, "0.50"),
            (0.125, "0.125"),
            (0.73456789, "0.7346"),
            (0.50001, "0.50"),
            (0.12501, "0.125"),
            (9.99999, "10.00"),
            (-0.73456789, "-0.7346"),
        ] {
            assert_eq!(format_float(value, Some(4)), expected);
        }
        assert_eq!(format_float(0.73456789, None), "0.73456789");
        assert_eq!(format_float(0.73456789, Some(2)), "0.73");
        assert_eq!(format_float(0.5, Some(1)), "0.50");
        assert_eq!(format_float(0.5, Some(0)), "0.50");
    }

    const MODES: ParameterSpec = ParameterSpec {
        name: std::borrow::Cow::Borrowed("Mode"),
        min: 0.0,
        max: 2.0,
        default: 0.0,
        kind: ParameterKind::Enum(&["Trigger", "Gate", "Toggle"]),
        unit: Unit::None,
    };
    const STEPS: ParameterSpec = ParameterSpec {
        name: std::borrow::Cow::Borrowed("Steps"),
        min: 1.0,
        max: 16.0,
        default: 4.0,
        kind: ParameterKind::Quantised { step: 1.0 },
        unit: Unit::None,
    };

    #[test]
    fn numeric_checks_share_parse_ranges_and_quantisation() {
        for spec in [VOLUME, STREAM_RATE, TRACK_DELAY, STEPS, MODES] {
            for value in [spec.min, spec.default, spec.max] {
                assert_eq!(spec.check(value).unwrap(), value);
            }
            for value in [
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                spec.min - 1.0,
                spec.max + 1.0,
            ] {
                assert!(spec.check(value).is_err(), "{}: {value}", spec.name);
            }
        }
        assert_eq!(STEPS.check(4.4).unwrap(), 4.0);
        assert_eq!(MODES.check(1.4).unwrap(), 1.0);
        assert_eq!(STEPS.check(4.4), STEPS.parse("4.4"));
        assert_eq!(STREAM_RATE.check(50.5).unwrap(), 50.5);
        assert_eq!(VOLUME.check(0.73456789).unwrap(), 0.73456789);
        for value in [f64::NAN, f64::INFINITY, 201.0, 0.0] {
            assert_eq!(
                STREAM_RATE.check(value),
                STREAM_RATE.parse(&value.to_string())
            );
        }
    }

    #[test]
    fn floats_format_with_two_decimals_and_enums_by_item() {
        assert_eq!(TRACK_DELAY.format(0.1234), "0.1234");
        assert_eq!(TRACK_DELAY.format(-12.5), "-12.50");
        assert_eq!(TRACK_DELAY.format(1000.0), "1000.00");
        assert_eq!(TRACK_DELAY.format_with_unit(5.0), "5.00 ms");
        assert_eq!(MODES.format(1.0), "Gate");
        assert_eq!(STEPS.format(4.0), "4");
    }

    #[test]
    fn parse_trims_and_checks_finiteness_and_range() {
        assert_eq!(TRACK_DELAY.parse(" -2.5 ").unwrap(), -2.5);
        for input in ["", "abc", "NaN", "inf", "1000.5", "-1001"] {
            assert!(TRACK_DELAY.parse(input).is_err(), "{input}");
        }
        assert_eq!(TRACK_DELAY.parse("-1000").unwrap(), -1000.0);
    }

    #[test]
    fn parse_snaps_quantised_and_maps_enum_text() {
        assert_eq!(STEPS.parse("4.4").unwrap(), 4.0);
        assert!(STEPS.parse("17").is_err());
        assert_eq!(MODES.parse("toggle").unwrap(), 2.0);
        assert!(MODES.parse("Loop").is_err());
    }

    #[test]
    fn untouched_rounded_display_keeps_model_precision() {
        assert!(OUTPUT_LATENCY.is_untouched_display("0.1235", 0.12345678));
        assert!(!OUTPUT_LATENCY.is_untouched_display("0.13", 0.12345678));
    }

    #[test]
    fn track_and_output_expose_their_stored_values() {
        let mut track = Track::new(TrackId(1), "", 0);
        track.delay_ms = 7.5;
        assert_eq!(track.delay_parameter().value, 7.5);
        assert_eq!(track.delay_parameter().spec.name, "Track Delay");
        let mut output = Track::new_output(TrackId(201), "A", "127.0.0.1", 9000);
        output.delay_ms = -3.0;
        assert_eq!(output.latency_parameter().display(), "-3.00");
    }
}
