//! What a control drives, by stable ids only: the identity that help, Map Mode and remote
//! control share.
//!
//! A `ControlRef` names one control the way a command would address it, but holds no
//! command: translating a gesture into a `Command` belongs to the remote crate, so the
//! model keeps naming nothing above it. `describe_control` answers what the control is and holds
//! now; `control_address` and `parse_control_address` map a ref to its canonical OSC leaf and back.
//! Nothing here stores a list position, so reordering tracks, scenes or entries never
//! re-points a ref.
use super::*;

mod address;
mod descriptor;
mod facets;
mod mapping;
pub use address::{control_address, parse_control_address};
pub use descriptor::{
    ControlAccess, ControlDescriptor, ControlKind, ControlValue, describe_control, describe_value,
};
pub use facets::{
    ACTIVATOR_WINDOW, Facet, Facets, Limits, TEMPO_RANGE, UNRANGED_FLOAT, UNRANGED_INT,
    control_limits, learn_range,
};
pub use mapping::*;

/// The transport-level controls (`/qtable/transport/…`), after Live's transport and Song class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum TransportControl {
    Play,
    Stop,
    StopAll,
    Tempo,
    Quantisation,
    FollowActions,
    ReEnableAutomation,
}

impl TransportControl {
    pub const ALL: [Self; 7] = [
        Self::Play,
        Self::Stop,
        Self::StopAll,
        Self::Tempo,
        Self::Quantisation,
        Self::FollowActions,
        Self::ReEnableAutomation,
    ];
}

/// The map-only scene navigation strip, after Live's Song.View controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SceneNavigation {
    Launch,
    CancelLaunch,
    Up,
    Down,
    Select,
}

impl SceneNavigation {
    pub const ALL: [Self; 5] = [
        Self::Launch,
        Self::CancelLaunch,
        Self::Up,
        Self::Down,
        Self::Select,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Launch => "Scene Launch",
            Self::CancelLaunch => "Cancel Scene Launch",
            Self::Up => "Scene Up",
            Self::Down => "Scene Down",
            Self::Select => "Scene Select",
        }
    }
}

/// One mappable control. Ids are project-scoped; a clip slot keys on its scene's id, so
/// inserting a scene doesn't re-point it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ControlRef {
    /// A Target parameter's argument, or a Mixer's Volume.
    Argument(FieldRef),
    /// A Target event's Bang.
    Event {
        track: TrackId,
        event: EventRef,
    },
    /// One argument of a Target event: the value the event's face holds and a Bang sends.
    /// Setting it sends nothing; only the Bang does. Format 12.
    EventArgument {
        track: TrackId,
        event: EventRef,
        field: FieldKey,
    },
    /// A slot's launch, on a clip track or a group track.
    ClipSlot {
        track: TrackId,
        scene: SceneId,
    },
    Scene(SceneId),
    TrackSelect(TrackId),
    SceneNavigation(SceneNavigation),
    TrackStop(TrackId),
    TrackActivator(TrackId),
    TrackSolo(TrackId),
    Transport(TransportControl),
}

#[cfg(test)]
mod facets_tests;
#[cfg(test)]
mod mapping_tests;
#[cfg(test)]
mod tests;
