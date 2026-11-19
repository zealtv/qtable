//! What each enabled parameter wants sent to each recipient: address, mapped value, send mode,
//! Threshold and delay. The output stages do the sending, on frames.
use super::output_stage::slot::Settings;
use super::*;
use crate::model::{DeviceRef, OscArgument, ParameterRef, Track, parameter_value_flow};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Control {
    pub target: ParameterRef,
    pub output: TrackId,
    pub address: String,
    /// The value in its receiver's OSC type and units.
    pub send: SendValues,
    pub settings: Settings,
}

pub(super) fn controls(project: &Project) -> Vec<Control> {
    project
        .tracks
        .iter()
        .chain(&project.outputs)
        .chain([&project.main])
        .flat_map(Track::parameters)
        .flat_map(|target| {
            project
                .sending_parameter_recipients(&target)
                .into_iter()
                .filter_map(move |output| {
                    let parameter = project.parameter(&target)?;
                    Some(Control {
                        target: target.clone(),
                        output: output.id,
                        address: project.parameter_address(&target, output.id)?,
                        send: send_values(&target, &parameter)?,
                        settings: Settings {
                            mode: parameter.output.send_mode,
                            thresholds: parameter
                                .fields
                                .iter()
                                .map(|field| field.threshold)
                                .collect(),
                            delay_ms: project.parameter_delay_ms(&target, output.id),
                        },
                    })
                })
        })
        .collect()
}

/// The one wire conversion boundary for manual and sampled parameter values.
pub(super) fn send_values(
    target: &ParameterRef,
    parameter: &crate::model::ParameterView<'_>,
) -> Option<SendValues> {
    let template = SendTemplate::new(target, parameter);
    Some(SendValues(
        template
            .convert(parameter.fields.iter().map(|f| f.value))?
            .into(),
    ))
}

/// Prepared at sync: enum strings and specs never need rebuilding on a frame.
#[derive(Clone)]
pub(super) struct SendTemplate {
    fields: Arc<[WireField]>,
}
#[derive(Clone)]
struct WireField {
    spec: crate::model::ParameterSpec,
    conversion: Conversion,
}
#[derive(Clone)]
enum Conversion {
    Mixer(crate::model::Mapping),
    Enum(Arc<[SendValue]>),
    Numeric(crate::model::FieldType),
}
impl SendTemplate {
    pub fn new(target: &ParameterRef, parameter: &crate::model::ParameterView<'_>) -> Self {
        Self {
            fields: parameter
                .fields
                .iter()
                .map(|field| WireField {
                    spec: field.spec.clone().into_owned(),
                    conversion: if target.device == DeviceRef::Mixer {
                        Conversion::Mixer(parameter.output.mapping)
                    } else if field.control == crate::model::Control::Enum {
                        Conversion::Enum(
                            field
                                .options
                                .iter()
                                .map(|o| SendValue::from(o.value.clone()))
                                .collect(),
                        )
                    } else {
                        Conversion::Numeric(field.value_type)
                    },
                })
                .collect(),
        }
    }

    pub fn checked(&self, index: usize, value: f64) -> Option<f64> {
        let spec = &self.fields[index].spec;
        spec.check(value.clamp(spec.min, spec.max)).ok()
    }

    pub fn convert(&self, values: impl IntoIterator<Item = f64>) -> Option<Vec<SendValue>> {
        self.fields
            .iter()
            .zip(values)
            .map(|(field, value)| field.convert(value))
            .collect()
    }

    pub fn sample(&self, values: impl IntoIterator<Item = f64>) -> Option<SendValues> {
        let mut result = std::array::from_fn(|_| SendValue::Bool(false));
        for (out, (field, value)) in result.iter_mut().zip(self.fields.iter().zip(values)) {
            *out = field.convert(value)?;
        }
        Some(SendValues(ValueStorage::Sampled {
            values: result,
            len: self.fields.len(),
        }))
    }
}
impl WireField {
    fn convert(&self, value: f64) -> Option<SendValue> {
        use crate::model::FieldType;
        Some(match &self.conversion {
            Conversion::Mixer(mapping) => {
                SendValue::Float(parameter_value_flow(&self.spec, value, *mapping).ok()?)
            }
            Conversion::Enum(options) => options.get(value.round() as usize)?.clone(),
            Conversion::Numeric(kind) => match kind {
                FieldType::Float => SendValue::Float(value as f32),
                FieldType::Int => SendValue::Int(value.round() as i32),
                FieldType::Bool => SendValue::Bool(value != 0.0),
                FieldType::String => unreachable!("String parameters use Enum"),
            },
        })
    }
}

/// Wire values stay typed through frame coalescing, including delayed edits.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum SendValue {
    Float(f32),
    Int(i32),
    Bool(bool),
    String(Arc<str>),
}
impl From<OscArgument> for SendValue {
    fn from(value: OscArgument) -> Self {
        match value {
            OscArgument::Float(v) => Self::Float(v),
            OscArgument::Int(v) => Self::Int(v),
            OscArgument::Bool(v) => Self::Bool(v),
            OscArgument::String(v) => Self::String(v.into()),
        }
    }
}
impl SendValue {
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            _ => self == other,
        }
    }
    fn distance(&self, other: &Self) -> f64 {
        match (self, other) {
            (Self::Float(a), Self::Float(b)) => (f64::from(*a) - f64::from(*b)).abs(),
            (Self::Int(a), Self::Int(b)) => (f64::from(*a) - f64::from(*b)).abs(),
            (Self::Bool(a), Self::Bool(b)) => {
                if a == b {
                    0.0
                } else {
                    1.0
                }
            }
            _ => f64::INFINITY,
        }
    }
    fn argument(&self) -> rosc::OscType {
        match self {
            Self::Float(v) => rosc::OscType::Float(*v),
            Self::Int(v) => rosc::OscType::Int(*v),
            Self::Bool(v) => rosc::OscType::Bool(*v),
            Self::String(v) => rosc::OscType::String(v.to_string()),
        }
    }
}

/// Manual edits share snapshots; frame samples use bounded inline storage so retaining
/// a sample (including last-sent and delayed histories) never allocates or aliases mutation.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SendValues(pub ValueStorage);
// Boxing the bounded sample would reintroduce the per-frame allocation this storage avoids.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub(super) enum ValueStorage {
    Shared(Arc<[SendValue]>),
    Sampled {
        values: [SendValue; crate::model::FieldDef::MAX_ARGUMENTS],
        len: usize,
    },
}
impl std::ops::Deref for ValueStorage {
    type Target = [SendValue];
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Shared(values) => values,
            Self::Sampled { values, len } => &values[..*len],
        }
    }
}
impl PartialEq for ValueStorage {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}
impl From<Vec<SendValue>> for ValueStorage {
    fn from(values: Vec<SendValue>) -> Self {
        Self::Shared(values.into())
    }
}
impl From<&[SendValue]> for ValueStorage {
    fn from(values: &[SendValue]) -> Self {
        Self::Shared(values.into())
    }
}
#[cfg(test)]
impl ValueStorage {
    pub fn shared(&self) -> &Arc<[SendValue]> {
        match self {
            Self::Shared(values) => values,
            _ => panic!("expected shared manual values"),
        }
    }
}
impl From<SendValue> for SendValues {
    fn from(value: SendValue) -> Self {
        Self(ValueStorage::Shared(Arc::from([value])))
    }
}
impl From<f32> for SendValues {
    fn from(value: f32) -> Self {
        SendValue::Float(value).into()
    }
}
impl SendValues {
    pub fn same(&self, other: &Self) -> bool {
        self.0.len() == other.0.len() && self.0.iter().zip(other.0.iter()).all(|(a, b)| a.same(b))
    }
    pub fn holds(&self, last: &Self, thresholds: &[f64]) -> bool {
        !self.same(last)
            && self.0.len() == last.0.len()
            && self.0.len() == thresholds.len()
            && self
                .0
                .iter()
                .zip(last.0.iter())
                .zip(thresholds)
                .all(|((a, b), threshold)| {
                    std::mem::discriminant(a) == std::mem::discriminant(b)
                        && (a.same(b) || a.distance(b) < *threshold)
                })
    }
    pub fn arguments(&self) -> Vec<rosc::OscType> {
        self.0.iter().map(SendValue::argument).collect()
    }
}
