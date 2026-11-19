//! Editable project data, independent of playback and the project-file format.
//!
//! Public fields allow command clients to assemble edits; call [`Project::validate`]
//! before accepting an edit or loading it into playback. IDs survive reordering.

use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;
use std::fmt;

/// One beat is a quarter note. 960 ticks exactly represent common straight,
/// dotted and triplet note values; finer input must be rounded at the boundary.
pub const TICKS_PER_BEAT: u64 = 960;

/// Maximum absolute track delay in milliseconds (inclusive).
/// qtable's safety limit bounds delay-driven speculative planning to one second;
/// decided 2026-10-11.
pub const MAX_TRACK_DELAY_MS: f64 = 1000.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelError(pub String);

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for ModelError {}

type ModelResult<T = ()> = Result<T, ModelError>;

fn require(condition: bool, message: &str) -> ModelResult {
    if condition {
        Ok(())
    } else {
        Err(ModelError(message.to_owned()))
    }
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub u64);
    };
}

id_type!(TrackId);

id_type!(SceneId);
id_type!(ClipId);
id_type!(DeviceId);
id_type!(MappingId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Colour {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Colour {
    /// Black marks a colour nobody chose (a missing field, an old file). Live
    /// never shows black, so loading replaces it from the palette.
    pub const UNSET: Colour = Colour {
        red: 0,
        green: 0,
        blue: 0,
    };

    pub fn is_unset(self) -> bool {
        self == Self::UNSET
    }
}

pub(crate) fn unset_colour() -> Colour {
    Colour::UNSET
}

mod address;
mod ancestors;
mod capabilities;
mod colour;
mod delay;
mod device;
mod envelope;
pub use envelope::*;
mod grouping;
mod main_track;
mod mixer;
mod mixer_device;
mod osc;
mod output_checks;
mod output_stage;
mod parameter;
mod path;
mod project;
mod quantisation;
pub mod region;
mod remote;
mod shape;
mod simplify;
mod strip_ids;
mod target;
mod time;
mod topology;
mod track_facts;
mod track_kind;
mod validate;
pub use address::{PathLevel, resolve_address};
pub use ancestors::{Ancestor, Ancestors};
pub use capabilities::{Capabilities, Region};
pub use colour::LIVE_CLIP_PALETTE;
pub use device::*;
pub use main_track::Main;
pub use mixer::{MixerState, SoloLook};
pub use mixer_device::{MixerDevice, VOLUME};
use mixer_device::{VOLUME_FIELD, VOLUME_PARAMETER};
pub use osc::*;
pub use output_stage::{BUDGET, EVERY_FRAME_RATE_NOTICE, FRAME_RATE, OutputStage, REFRESH};
pub use parameter::{
    DeviceRef, FieldRef, FieldView, Mapping, MappingError, OUTPUT_LATENCY, Parameter, ParameterKey,
    ParameterKind, ParameterOutput, ParameterRef, ParameterSpec, ParameterView, STREAM_RATE,
    SendMode, StoredParameter, THRESHOLD, TRACK_DELAY, Unit, format_float, parameter_value_flow,
};
pub use path::{Path, lead_with_slash};
pub use project::*;
pub use quantisation::*;
pub use remote::{
    ACTIVATOR_WINDOW, ControlAccess, ControlDescriptor, ControlKind, ControlRef, ControlValue,
    DEFAULT_SENSITIVITY, Facet, Facets, KeyName, KeySource, Limits, MidiMessage, MidiSource,
    RelativeEncoding, RemoteMapping, SENSITIVITY_RANGE, SceneNavigation, Source, SourceMode,
    TEMPO_RANGE, Takeover, TransportControl, UNRANGED_FLOAT, UNRANGED_INT, control_address,
    control_limits, describe_control, describe_value, learn_range, parse_control_address,
};
pub use shape::{Link, MIN_SHAPE_SPAN, Shape, shape_points};
pub use simplify::{simplify_points, simplify_tolerance, thin};
pub use target::*;
pub use time::{BeatTime, Tempo, TimeSignature};
pub use topology::{SiblingRun, Topology};
pub use track_facts::TrackFacts;
pub use track_kind::TrackKind;
mod address_validation;
use address_validation::validate_stored_address;
pub use address_validation::{validate_address, validate_address_part};
pub(crate) use validate::{validate_flat_groups, validate_route};
#[cfg(test)]
mod group_shape_tests;
#[cfg(test)]
mod mixer_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod topology_tests;
#[cfg(test)]
mod track_facts_tests;
