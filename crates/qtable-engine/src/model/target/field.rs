//! One typed argument, with a control independent of its OSC wire type.
use super::*;
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FieldKey(pub String);

impl FieldKey {
    pub fn validate(&self) -> ModelResult {
        require(
            !self.0.is_empty()
                && self.0.bytes().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-'
                }),
            "argument key must be lowercase [a-z0-9_-]+",
        )
    }
    pub fn fresh(fields: &[FieldDef], name: &str) -> Self {
        let base: String = name
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        if base.is_empty() {
            for n in 1.. {
                let key = Self(format!("f{n}"));
                if !fields.iter().any(|f| f.key == key) {
                    return key;
                }
            }
        }
        let mut key = Self(base.clone());
        let mut n = 2;
        while fields.iter().any(|f| f.key == key) {
            key = Self(format!("{base}-{n}"));
            n += 1;
        }
        key
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldType {
    Float,
    Int,
    Bool,
    String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Control {
    Slider,
    Dial,
    NumberBox,
    Toggle,
    Enum,
    Text,
}

impl FieldType {
    /// The range switched on when a numeric argument selects Slider or Dial.
    pub fn default_range(self) -> Option<[f64; 2]> {
        match self {
            Self::Float => Some([0.0, 1.0]),
            Self::Int => Some([0.0, 127.0]),
            Self::Bool | Self::String => None,
        }
    }

    /// The control a new or retyped argument starts with: the first offered, except that an
    /// unranged number starts as a Number box, since Slider and Dial would switch a range on.
    pub fn default_control(self, parameter: bool, ranged: bool) -> Control {
        match self {
            Self::Float | Self::Int if !ranged => Control::NumberBox,
            _ => self.controls(parameter, ranged)[0],
        }
    }
    pub fn controls(self, parameter: bool, ranged: bool) -> &'static [Control] {
        use Control::*;
        match self {
            Self::Float | Self::Int if !ranged => &[Slider, Dial, NumberBox],
            Self::Float | Self::Int => &[Slider, Dial, NumberBox, Toggle, Enum],
            Self::Bool => &[Toggle],
            Self::String if parameter => &[Enum],
            Self::String => &[Text, Enum],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnumOption {
    pub label: String,
    pub value: OscArgument,
}

impl EnumOption {
    pub(crate) fn numbered(value_type: FieldType, number: u32) -> Option<Self> {
        let words = [
            "One",
            "Two",
            "Three",
            "Four",
            "Five",
            "Six",
            "Seven",
            "Eight",
            "Nine",
            "Ten",
            "Eleven",
            "Twelve",
            "Thirteen",
            "Fourteen",
            "Fifteen",
            "Sixteen",
            "Seventeen",
            "Eighteen",
            "Nineteen",
            "Twenty",
        ];
        let label = number
            .checked_sub(1)
            .and_then(|n| words.get(n as usize))
            .map_or_else(|| number.to_string(), |s| (*s).to_owned());
        let value = match value_type {
            FieldType::Float => OscArgument::Float(number as f32),
            FieldType::Int => OscArgument::Int(i32::try_from(number).ok()?),
            FieldType::String => OscArgument::String(label.clone()),
            FieldType::Bool => return None,
        };
        Some(Self { label, value })
    }
    /// First free positive number; option counts bound the search to small exact wire values.
    pub fn next_unused(value_type: FieldType, options: &[Self]) -> Option<Self> {
        (1..=options.len() as u32 + 1)
            .filter_map(|n| Self::numbered(value_type, n))
            .find(|candidate| !options.iter().any(|o| o.value == candidate.value))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    pub key: FieldKey,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub value_type: FieldType,
    pub control: Control,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<EnumOption>,
    pub default: OscArgument,
    /// The current numeric control value (an Enum's option index), held by parameter fields
    /// and by event fields other than Text. The default only seeds new hits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// The current value of a Text event field, which has no numeric control value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub threshold: f64,
}
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl FieldDef {
    /// Maximum parameter arity and maximum size to which edits can grow an event.
    /// Stored legacy events may have more arguments.
    pub const MAX_ARGUMENTS: usize = 16;

    pub fn new(key: FieldKey, value_type: FieldType, parameter: bool) -> Self {
        let range = match value_type {
            FieldType::Float | FieldType::Int if parameter => Some([0.0, 1.0]),
            _ => None,
        };
        let options = if value_type == FieldType::String && parameter {
            (1..=2)
                .map(|n| EnumOption::numbered(value_type, n).unwrap())
                .collect()
        } else {
            vec![]
        };
        let default = options
            .first()
            .map(|o: &EnumOption| o.value.clone())
            .unwrap_or_else(|| value_type.default_argument());
        let mut field = Self {
            key,
            name: String::new(),
            value_type,
            control: value_type.default_control(parameter, range.is_some()),
            range,
            options,
            default,
            value: None,
            text: None,
            threshold: 0.0,
        };
        field.seed_current();
        field
    }
    /// Give the field a current value from its default where it holds none, and drop the
    /// kind of value its control does not use.
    pub fn seed_current(&mut self) {
        if self.control == Control::Text {
            self.value = None;
            if self.text.is_none() {
                self.text = Some(match &self.default {
                    OscArgument::String(v) => v.clone(),
                    _ => String::new(),
                });
            }
        } else {
            self.text = None;
            if self.value.is_none() {
                self.value = Some(self.default_value());
            }
        }
    }
    /// How an unnamed argument at the 0-based `position` reads: `Arg 1`, `Arg 2`, ... The word marks a
    /// position, so it does not read as a value ("1 0.50").
    pub fn position_label(position: usize) -> String {
        format!("Arg {}", position + 1)
    }
    pub fn display_name(&self, position: usize) -> Cow<'_, str> {
        if self.name.is_empty() {
            Cow::Owned(Self::position_label(position))
        } else {
            Cow::Borrowed(&self.name)
        }
    }
    /// The argument a bang sends: the current value, or the default for a field that holds none.
    pub fn current_argument(&self) -> OscArgument {
        if let Some(text) = &self.text {
            return OscArgument::String(text.clone());
        }
        let Some(value) = self.value else {
            return self.default.clone();
        };
        if self.control == Control::Enum {
            return self
                .options
                .get(value.round() as usize)
                .map_or_else(|| self.default.clone(), |o| o.value.clone());
        }
        match self.value_type {
            FieldType::Float => OscArgument::Float(value as f32),
            FieldType::Int => OscArgument::Int(value.round() as i32),
            FieldType::Bool => OscArgument::Bool(value != 0.0),
            FieldType::String => self.default.clone(),
        }
    }
    /// The argument that holds `value`: how a number from a controller becomes the OSC argument
    /// a face edit stores. An Enum takes the option at that index (the default when none is);
    /// a String field has no number, so it keeps its default.
    pub fn argument_at(&self, value: f64) -> OscArgument {
        if self.control == Control::Enum {
            return self
                .options
                .get(value.round().max(0.0) as usize)
                .map_or_else(|| self.default.clone(), |o| o.value.clone());
        }
        match self.value_type {
            FieldType::Float => OscArgument::Float(value as f32),
            FieldType::Int => OscArgument::Int(value.round() as i32),
            FieldType::Bool => OscArgument::Bool(value != 0.0),
            FieldType::String => self.default.clone(),
        }
    }
    /// Whether the argument is a continuous numeric one: a Float or Int under a Slider, Dial or
    /// Number box. A Toggle, Enum or Text, and a Bool or String value, are not.
    fn is_continuous_numeric(&self) -> bool {
        matches!(self.value_type, FieldType::Float | FieldType::Int)
            && matches!(
                self.control,
                Control::Slider | Control::Dial | Control::NumberBox
            )
    }
    /// Whether Threshold can govern this argument: a continuous numeric one. The rest have no
    /// small change to hold back.
    pub fn takes_threshold(&self) -> bool {
        self.is_continuous_numeric()
    }
    /// Whether the argument has a range to set (Range, Min and Max): a continuous numeric one. A
    /// Toggle spans 0–1 and an Enum its options, and a Bool, String or Text has no span, so a
    /// client shows no range controls for them. The stored range stays, so switching back to a
    /// continuous control restores it.
    pub fn takes_range(&self) -> bool {
        self.is_continuous_numeric()
    }
    /// The Threshold in force: the stored one where `takes_threshold`, otherwise 0. The stored
    /// value stays, so switching back to a continuous control restores it. The scheduler and the
    /// inspector's enabled state both read this.
    pub fn effective_threshold(&self) -> f64 {
        if self.takes_threshold() {
            self.threshold
        } else {
            0.0
        }
    }
    pub fn default_value(&self) -> f64 {
        if self.control == Control::Enum {
            return self
                .options
                .iter()
                .position(|o| o.value == self.default)
                .unwrap_or(0) as f64;
        }
        match &self.default {
            OscArgument::Float(v) => f64::from(*v),
            OscArgument::Int(v) => f64::from(*v),
            OscArgument::Bool(v) => f64::from(*v),
            OscArgument::String(_) => 0.0,
        }
    }
    pub fn axis(&self) -> Axis {
        match self.control {
            Control::Toggle | Control::Enum => Axis::Discrete,
            _ if self.value_type == FieldType::Int => Axis::Stepped,
            _ => Axis::Continuous,
        }
    }
    /// `spec` named as the argument reads in the UI, for messages that quote it.
    pub fn spec_named(&self, name: &str) -> ParameterSpec {
        ParameterSpec {
            name: name.to_owned().into(),
            ..self.spec()
        }
    }
    pub fn spec(&self) -> ParameterSpec {
        let [min, max] = match self.control {
            Control::Toggle => [0.0, 1.0],
            Control::Enum => [0.0, self.options.len().saturating_sub(1) as f64],
            _ => self.range.unwrap_or(match self.value_type {
                FieldType::Int => [f64::from(i32::MIN), f64::from(i32::MAX)],
                _ => [-f64::from(f32::MAX), f64::from(f32::MAX)],
            }),
        };
        ParameterSpec {
            name: self.name.clone().into(),
            min,
            max,
            default: self.default_value().clamp(min, max),
            kind: match self.control {
                Control::Toggle => ParameterKind::Enum(&["Off", "On"]),
                Control::Enum => ParameterKind::Quantised { step: 1.0 },
                _ if self.value_type == FieldType::Int => ParameterKind::Quantised { step: 1.0 },
                _ => ParameterKind::Continuous,
            },
            unit: Unit::None,
        }
    }
    pub fn validate_fields(fields: &[Self], parameter: bool) -> ModelResult {
        // Legacy events have no arity cap; AddField bounds new growth.
        require(
            !parameter || (1..=Self::MAX_ARGUMENTS).contains(&fields.len()),
            &format!("parameters need 1–{} arguments", Self::MAX_ARGUMENTS),
        )?;
        let mut keys = HashSet::new();
        for field in fields {
            field.validate(parameter)?;
            require(keys.insert(&field.key), "duplicate argument key")?;
        }
        Ok(())
    }
    pub fn validate(&self, parameter: bool) -> ModelResult {
        self.key.validate()?;
        require(
            self.value_type
                .controls(parameter, self.range.is_some())
                .contains(&self.control),
            "control is not allowed for this argument type and range",
        )?;
        require(
            !parameter
                || self.range.is_some()
                || matches!(self.value_type, FieldType::Bool | FieldType::String),
            "numeric parameter arguments need a range",
        )?;
        require(
            FieldType::of(&self.default) == self.value_type,
            "argument default must match its wire type",
        )?;
        if let OscArgument::Float(v) = self.default {
            require(v.is_finite(), "argument default must be finite")?;
        }
        if let Some([min, max]) = self.range {
            require(
                matches!(self.value_type, FieldType::Float | FieldType::Int),
                "only numeric arguments have ranges",
            )?;
            require(
                min.is_finite() && max.is_finite() && min < max,
                "argument range must be finite with min less than max",
            )?;
            require(
                match self.value_type {
                    FieldType::Float => (min as f32).is_finite() && (max as f32).is_finite(),
                    _ => {
                        min >= f64::from(i32::MIN)
                            && max <= f64::from(i32::MAX)
                            && min.fract() == 0.0
                            && max.fract() == 0.0
                    }
                },
                "argument range must fit its OSC type",
            )?;
        }
        if self.control == Control::Enum {
            require(
                self.options.len() >= if parameter { 2 } else { 1 } && self.options.len() <= 64,
                "Enum needs 2–64 options for parameters or 1–64 for events",
            )?;
            for option in &self.options {
                require(
                    FieldType::of(&option.value) == self.value_type,
                    "Enum option must match its wire type",
                )?;
                if let OscArgument::Float(v) = option.value {
                    require(v.is_finite(), "Enum option must be finite")?;
                }
                if let OscArgument::String(ref v) = option.value {
                    require(*v == option.label, "String Enum label must equal its value")?;
                }
            }
            require(
                !self.options.iter().enumerate().any(|(index, option)| {
                    self.options[..index]
                        .iter()
                        .any(|o| o.value == option.value)
                }),
                "Enum option values must be unique",
            )?;
            require(
                self.options.iter().any(|o| o.value == self.default),
                "Enum default must be an option",
            )?;
        } else {
            require(self.options.is_empty(), "only Enum arguments have options")?;
        }
        if self.control != Control::Text {
            let spec = self.spec();
            let check = |v: f64| -> ModelResult {
                let snapped = spec.check(v).map_err(ModelError)?;
                require(
                    snapped == v,
                    "argument value must be integral for a stepped control",
                )
            };
            let default = self.default_value();
            // Float defaults are already wire values: accept a rounded endpoint.
            let default = if self.value_type == FieldType::Float
                && !matches!(self.control, Control::Enum | Control::Toggle)
                && (default as f32 == spec.min as f32 || default as f32 == spec.max as f32)
            {
                default.clamp(spec.min, spec.max)
            } else {
                default
            };
            check(default)?;
            if let Some(value) = self.value {
                check(value)?;
            }
        }
        require(
            self.value.is_some() == (self.control != Control::Text)
                && self.text.is_some() == (self.control == Control::Text),
            "an argument holds the current value its control uses",
        )?;
        THRESHOLD.check(self.threshold).map_err(ModelError)?;
        require(
            parameter || self.threshold == 0.0,
            "event arguments have no Threshold",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_takes_effect_only_for_a_continuous_numeric_argument() {
        let mut field = FieldDef::new(FieldKey("v".into()), FieldType::Float, true);
        field.threshold = 0.25;
        for control in [Control::Slider, Control::Dial, Control::NumberBox] {
            field.control = control;
            assert_eq!(field.effective_threshold(), 0.25, "{control:?}");
        }
        field.value_type = FieldType::Int;
        assert_eq!(field.effective_threshold(), 0.25);
        for control in [Control::Toggle, Control::Enum, Control::Text] {
            field.control = control;
            assert_eq!(field.effective_threshold(), 0.0, "{control:?}");
        }
        for value_type in [FieldType::Bool, FieldType::String] {
            field.value_type = value_type;
            field.control = Control::NumberBox;
            assert_eq!(field.effective_threshold(), 0.0, "{value_type:?}");
        }
        // The stored value stays, so a return to a continuous control restores it.
        assert_eq!(field.threshold, 0.25);
    }

    #[test]
    fn a_range_belongs_to_a_continuous_numeric_argument_only() {
        let mut field = FieldDef::new(FieldKey("v".into()), FieldType::Float, true);
        field.range = Some([0.0, 10.0]);
        for value_type in [FieldType::Float, FieldType::Int] {
            field.value_type = value_type;
            for control in [Control::Slider, Control::Dial, Control::NumberBox] {
                field.control = control;
                assert!(field.takes_range(), "{value_type:?} {control:?}");
            }
            for control in [Control::Toggle, Control::Enum, Control::Text] {
                field.control = control;
                assert!(!field.takes_range(), "{value_type:?} {control:?}");
            }
        }
        for value_type in [FieldType::Bool, FieldType::String] {
            field.value_type = value_type;
            field.control = Control::NumberBox;
            assert!(!field.takes_range(), "{value_type:?}");
        }
        // The stored range stays for a return to a continuous control.
        assert_eq!(field.range, Some([0.0, 10.0]));
    }

    #[test]
    fn auto_ranges_apply_only_to_numeric_wire_types() {
        assert_eq!(FieldType::Float.default_range(), Some([0.0, 1.0]));
        assert_eq!(FieldType::Int.default_range(), Some([0.0, 127.0]));
        assert_eq!(FieldType::Bool.default_range(), None);
        assert_eq!(FieldType::String.default_range(), None);
    }

    #[test]
    fn parameter_arity_is_bounded_but_legacy_event_arity_is_not() {
        let fields: Vec<_> = (0..=FieldDef::MAX_ARGUMENTS)
            .map(|n| FieldDef::new(FieldKey(format!("f{n}")), FieldType::Float, true))
            .collect();
        assert!(FieldDef::validate_fields(&[], true).is_err());
        assert!(FieldDef::validate_fields(&fields[..1], true).is_ok());
        assert!(FieldDef::validate_fields(&fields[..FieldDef::MAX_ARGUMENTS], true).is_ok());
        assert!(FieldDef::validate_fields(&fields, true).is_err());
        assert!(FieldDef::validate_fields(&[], false).is_ok());
        assert!(FieldDef::validate_fields(&fields, false).is_ok());
    }
}
