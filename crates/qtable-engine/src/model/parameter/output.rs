use super::*;

/// When a parameter value reaches its OSC destination. The client decides which edits are
/// drag steps; the engine sends whatever project it is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum SendMode {
    /// Send on every live edit, including each step of a fader drag.
    #[default]
    Continuous,
    /// Send once when a fader drag ends; typed values and resets send at once.
    OnRelease,
    /// Send the current value every output frame while transport plays. Keeps v3.0's wire
    /// name, so v3.0 Stream sets load unchanged and v3.0 reads it as its own Stream.
    #[serde(rename = "Stream")]
    EveryFrame,
}
impl SendMode {
    pub const ALL: [Self; 3] = [Self::Continuous, Self::OnRelease, Self::EveryFrame];
}
impl std::fmt::Display for SendMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Continuous => "Continuous",
            Self::OnRelease => "On Release",
            Self::EveryFrame => "Every Frame",
        })
    }
}

/// Smallest change from the last value sent that is worth sending, in sent OSC units after
/// Mapping (`output-stage-design`, open question 6). 0 sends any change.
pub const THRESHOLD: ParameterSpec = ParameterSpec {
    name: std::borrow::Cow::Borrowed("Threshold"),
    min: 0.0,
    // Any distance between two finite OSC floats.
    max: f32::MAX as f64,
    default: 0.0,
    kind: ParameterKind::Continuous,
    unit: Unit::None,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
// Keep explicit struct names in hand-written format 6 RON compatible too.
#[serde(default, deny_unknown_fields, rename = "VolumeOutput")]
pub struct ParameterOutput {
    pub enabled: bool,
    pub send_mode: SendMode,
    /// Last chosen rate, retained in every send mode. Omit the default for v2 format 6 readers.
    /// Dormant from v3.1, where Every Frame follows its output's Frame Rate; still stored and
    /// validated because v3.0 uses it.
    #[serde(skip_serializing_if = "is_default_stream_rate")]
    pub stream_rate: f64,
    /// Complete relative address, composed like a clip lane's, with no implicit extra segment.
    // Missing paths in the legacy VolumeOutput wire type retain the Mixer default.
    #[serde(default = "legacy_volume_path")]
    pub address: String,
    pub mapping: Mapping,
    /// See [`THRESHOLD`]. Omitted at its default so the set still opens in v3.0.
    #[serde(skip_serializing_if = "is_default_threshold")]
    pub threshold: f64,
}
impl Default for ParameterOutput {
    fn default() -> Self {
        Self {
            enabled: false,
            send_mode: SendMode::default(),
            stream_rate: STREAM_RATE.default,
            address: String::new(),
            mapping: Mapping::default(),
            threshold: THRESHOLD.default,
        }
    }
}
impl ParameterOutput {
    /// Active stream rate; scheduling ignores the stored rate in other modes.
    pub fn stream_rate(&self) -> Option<f64> {
        (self.send_mode == SendMode::EveryFrame).then_some(self.stream_rate)
    }
}

fn is_default_stream_rate(rate: &f64) -> bool {
    *rate == STREAM_RATE.default
}

fn is_default_threshold(threshold: &f64) -> bool {
    *threshold == THRESHOLD.default
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredParameter {
    pub value: f64,
    pub output: ParameterOutput,
}

fn legacy_volume_path() -> String {
    "/volume".into()
}
