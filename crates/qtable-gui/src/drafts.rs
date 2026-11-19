//! Typed field identities keep UI drafts separate from the authoritative model.
use std::collections::HashMap;

use crate::control::mapping::MappingField;
use crate::control_focus::ControlKey;
use qtable_engine::model::{DeviceId, SceneId, TrackId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ClipField {
    Name,
    Length,
    Start,
    End,
    Position,
    FollowTime,
    ChanceA,
    ChanceB,
    PercentA,
    PercentB,
    Multiplier,
    EventArgument {
        event: usize,
        argument: usize,
    },
    /// The selected envelope point's position, value and curve. The point is the editor's
    /// selection, so its drafts are dropped whenever that selection changes.
    PointPosition,
    PointValue,
    PointCurve,
    LaneMarker(crate::inspector::lane_view::region::Field),
}

/// A field of the selected entry of a Target device, or of the Target itself (`Path`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TargetField {
    Path,
    Name,
    EntryPath,
    Definition(usize, DefinitionField),
    Face(bool, usize, usize),
}

/// A property of one ordered field in the selected Target entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DefinitionField {
    Name,
    Min,
    Max,
    Default,
    Threshold,
    OptionLabel(usize),
    OptionValue(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SceneField {
    Name,
    FollowTime,
    ChanceA,
    ChanceB,
    PercentA,
    PercentB,
}

/// A track's own fields, after the LOM's Track: its name, delay and path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TrackField {
    Name,
    Delay,
    Path,
}

/// A field of a track's Mixer device (`mixer_device`): its volume and the volume's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum MixerField {
    Volume,
    Threshold,
    Address,
    Mapping(MappingField),
}

/// A setting of an output track (`output_detail`): its endpoint, latency and stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum OutputField {
    Host,
    Port,
    Latency,
    FrameRate,
    Budget,
    Refresh,
}

/// A field as a domain plus a field. The domain names the panel or LOM object that owns the
/// field's commit, so adding a field touches only that domain's field enum and its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DraftKey {
    Tempo,
    Clip {
        track: TrackId,
        slot: usize,
        field: ClipField,
    },
    Scene {
        scene: SceneId,
        field: SceneField,
    },
    Target(DeviceId, TargetField),
    Track(TrackId, TrackField),
    Mixer(TrackId, MixerField),
    Output(TrackId, OutputField),
    /// A cell of the mapping list: a row's Min, Max, Mode or Takeover.
    Mapping(qtable_engine::model::MappingId, crate::map_mode::Cell),
}

impl DraftKey {
    pub(crate) fn clip(track: TrackId, slot: usize, field: ClipField) -> Self {
        Self::Clip { track, slot, field }
    }

    pub(crate) fn scene(scene: SceneId, field: SceneField) -> Self {
        Self::Scene { scene, field }
    }

    pub(crate) fn input_id(self) -> iced::widget::Id {
        iced::widget::Id::from(format!("field:{self:?}"))
    }

    fn is_inspector(self) -> bool {
        matches!(
            self,
            Self::Clip { .. }
                | Self::Scene { .. }
                | Self::Track(_, TrackField::Name)
                | Self::Mixer(..)
        )
    }
}

/// The control layer names a text input by the key it is given, and never reads the key.
impl ControlKey for DraftKey {
    type Help = crate::info::Id;

    fn input_id(self) -> iced::widget::Id {
        DraftKey::input_id(self)
    }
}

// The control layer's types as the app uses them, keyed by `DraftKey`.
pub(crate) type ControlId = crate::control_focus::ControlId<DraftKey>;
pub(crate) type ControlFocus = crate::control_focus::ControlFocus<DraftKey>;
pub(crate) type Control<K> = crate::control::Control<K, DraftKey>;
pub(crate) type Controls<'a> = crate::control::Controls<'a, DraftKey>;

#[derive(Default)]
pub(crate) struct Drafts {
    pub values: HashMap<DraftKey, String>,
    pub errors: HashMap<DraftKey, String>,
}

impl Drafts {
    /// Drops the draft and error of one field, once its edit has landed.
    pub(crate) fn forget(&mut self, key: DraftKey) {
        self.values.remove(&key);
        self.errors.remove(&key);
    }

    /// Preserve Routes-tab drafts across inspector selection/structural changes.
    pub(crate) fn clear_inspector(&mut self) {
        self.values.retain(|key, _| !key.is_inspector());
        self.errors.retain(|key, _| !key.is_inspector());
    }
}

/// A rejected draft wins; otherwise expose an invalid stored value until editing starts.
/// Deriving this from the model keeps load errors visible after selection changes/Escape.
pub(crate) fn field_error(
    values: &HashMap<DraftKey, String>,
    errors: &HashMap<DraftKey, String>,
    key: DraftKey,
    validation: Result<(), qtable_engine::model::ModelError>,
) -> Option<String> {
    errors.get(&key).cloned().or_else(|| {
        if values.contains_key(&key) {
            None
        } else {
            validation.err().map(|error| error.to_string())
        }
    })
}

impl Drafts {
    pub(crate) fn address_error(
        &self,
        key: DraftKey,
        validation: Result<(), qtable_engine::model::ModelError>,
    ) -> Option<String> {
        field_error(&self.values, &self.errors, key, validation)
    }
}
