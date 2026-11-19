//! Normalized volume and its optional independent OSC controller.
use super::*;

/// Mixer Volume's identity: the parameter key and the one field it holds. Spell it through
/// `ParameterRef::volume`, `FieldRef::volume` and `ArgumentRef::volume`, not these literals.
pub(super) const VOLUME_PARAMETER: &str = "volume";
pub(super) const VOLUME_FIELD: &str = "value";

pub const VOLUME: ParameterSpec = ParameterSpec {
    name: std::borrow::Cow::Borrowed("Volume"),
    min: 0.0,
    max: 1.0,
    default: 1.0,
    kind: ParameterKind::Continuous,
    unit: Unit::None,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "MixerWire", into = "MixerWire")]
pub struct MixerDevice {
    pub volume: StoredParameter,
}
impl Default for MixerDevice {
    fn default() -> Self {
        Self {
            volume: StoredParameter {
                value: VOLUME.default,
                output: ParameterOutput {
                    address: "/volume".into(),
                    ..ParameterOutput::default()
                },
            },
        }
    }
}
impl MixerDevice {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn volume_parameter(&self) -> Parameter<'static> {
        Parameter::new(&VOLUME, self.volume.value)
    }
    pub fn mapped_volume(&self) -> Result<f32, MappingError> {
        parameter_value_flow(&VOLUME, self.volume.value, self.volume.output.mapping)
    }
    pub(super) fn validate_with(
        &self,
        address: fn(&str) -> ModelResult,
        stream_rate: ParameterSpec,
    ) -> ModelResult {
        stream_rate
            .check(self.volume.output.stream_rate)
            .map_err(ModelError)?;
        THRESHOLD
            .check(self.volume.output.threshold)
            .map_err(ModelError)?;
        VOLUME
            .check(self.volume.value)
            // Preserve the existing model diagnostic while the spec owns validation.
            .map_err(|_| ModelError("volume must be finite and between 0 and 1".into()))?;
        require(
            !self.volume.output.address.is_empty(),
            "volume path must not be empty",
        )?;
        address(&self.volume.output.address)?;
        for input in [0.0, 1.0] {
            let mapped = self
                .volume
                .output
                .mapping
                .map(input)
                .map_err(|e| ModelError(e.to_string()))?;
            require(
                (mapped as f32).is_finite(),
                "mapped volume must fit a finite OSC float",
            )?;
        }
        Ok(())
    }
}
impl Project {
    /// Mixer conveniences for volume-specific views; routing rules live with parameters.
    pub fn volume_recipients(&self, owner: TrackId) -> Vec<&Track> {
        self.parameter_recipients(&ParameterRef::volume(owner))
    }
    pub fn volume_address(&self, owner: TrackId, output: TrackId) -> Option<String> {
        self.parameter_address(&ParameterRef::volume(owner), output)
    }
    pub fn volume_delay_ms(&self, owner: TrackId, output: TrackId) -> f64 {
        self.parameter_delay_ms(&ParameterRef::volume(owner), output)
    }
}
#[cfg(test)]
mod tests;

// Format 6 remains flat even though runtime parameter storage is shared.
#[derive(Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename = "MixerDevice")]
struct MixerWire {
    volume: f64,
    volume_output: ParameterOutput,
}
impl From<MixerDevice> for MixerWire {
    fn from(device: MixerDevice) -> Self {
        Self {
            volume: device.volume.value,
            volume_output: device.volume.output,
        }
    }
}
impl From<MixerWire> for MixerDevice {
    fn from(wire: MixerWire) -> Self {
        Self {
            volume: StoredParameter {
                value: wire.volume,
                output: wire.volume_output,
            },
        }
    }
}

impl Default for MixerWire {
    fn default() -> Self {
        MixerDevice::default().into()
    }
}
