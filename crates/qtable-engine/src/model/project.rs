//! Editable project, track, clip and follow-action data.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FollowAction {
    #[default]
    NoAction,
    Stop,
    PlayAgain,
    Previous,
    Next,
    First,
    Last,
    Any,
    Other,
    /// Stable row reference: for clips, the slot in this scene on the same track.
    Jump(SceneId),
}

/// Relative weights, not percentages; zero disables that branch. Both zero
/// weights are retained as editable data (selection behaviour belongs to playback).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FollowActions {
    pub enabled: bool,
    pub action_a: FollowAction,
    pub action_b: FollowAction,
    pub chance_a: u32,
    pub chance_b: u32,
}

impl Default for FollowActions {
    fn default() -> Self {
        Self {
            enabled: false,
            action_a: FollowAction::NoAction,
            action_b: FollowAction::NoAction,
            chance_a: 1,
            chance_b: 0,
        }
    }
}

/// An exact positive ratio of clip lengths for linked follow timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedFollowMultiplier")]
pub struct FollowMultiplier {
    numerator: u32,
    denominator: u32,
}

impl FollowMultiplier {
    pub fn new(numerator: u32, denominator: u32) -> ModelResult<Self> {
        require(
            numerator > 0 && denominator > 0,
            "follow multiplier must be positive",
        )?;
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    pub const fn denominator(self) -> u32 {
        self.denominator
    }
}

impl Default for FollowMultiplier {
    fn default() -> Self {
        Self {
            numerator: 1,
            denominator: 1,
        }
    }
}

/// Both settings persist when switching between linked and unlinked modes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClipFollowActions {
    pub actions: FollowActions,
    pub linked: bool,
    pub multiplier: FollowMultiplier,
    pub time: BeatTime,
}

impl Default for ClipFollowActions {
    fn default() -> Self {
        Self {
            actions: FollowActions::default(),
            linked: true,
            multiplier: FollowMultiplier::default(),
            time: BeatTime(4 * TICKS_PER_BEAT),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SceneFollowActions {
    pub actions: FollowActions,
    pub time: BeatTime,
}

impl Default for SceneFollowActions {
    fn default() -> Self {
        Self {
            actions: FollowActions::default(),
            time: BeatTime(4 * TICKS_PER_BEAT),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    pub id: ClipId,
    pub name: String,
    #[serde(default = "unset_colour")]
    pub colour: Colour,
    /// LOM start_marker/end_marker and the independent loop brace, in content beats.
    pub start: BeatTime,
    pub end: BeatTime,
    pub loop_start: BeatTime,
    pub loop_end: BeatTime,
    #[serde(default = "default_looping")]
    pub looping: bool,
    #[serde(default)]
    pub quantisation: LaunchQuantisation,
    /// LOM `Clip.launch_mode`. Only Trigger exists in v1.
    #[serde(default, skip_serializing_if = "LaunchMode::is_trigger")]
    pub launch_mode: LaunchMode,
    /// LOM `Clip.legato`: on launch, take over the play position of the clip
    /// that was playing in the track instead of starting from the beginning.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub legato: bool,
    #[serde(default)]
    pub follow_actions: ClipFollowActions,
    /// Declared lane order, including lanes with no events yet. Events played
    /// without a lane still get one; see [`Clip::lane_refs`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lanes: Vec<Lane>,
    /// Equal-time events are allowed and retain their insertion order.
    #[serde(default)]
    pub events: Vec<Event>,
}

/// How a launch button press acts. Gate, Toggle and Repeat wait for v3.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaunchMode {
    #[default]
    Trigger,
}

impl LaunchMode {
    fn is_trigger(&self) -> bool {
        *self == Self::Trigger
    }
}

fn default_looping() -> bool {
    true
}

impl Clip {
    pub fn new(
        id: ClipId,
        name: impl Into<String>,
        length: BeatTime,
        colour: Colour,
    ) -> ModelResult<Self> {
        require(length > BeatTime::ZERO, "clip length must be positive")?;
        Ok(Self {
            id,
            name: name.into(),
            colour,
            start: BeatTime::ZERO,
            end: length,
            loop_start: BeatTime::ZERO,
            loop_end: length,
            looping: true,
            quantisation: LaunchQuantisation::Global,
            launch_mode: LaunchMode::Trigger,
            legato: false,
            follow_actions: ClipFollowActions::default(),
            lanes: Vec::new(),
            events: Vec::new(),
        })
    }

    /// Marker values shared with an unlinked envelope's region.
    pub fn region(&self) -> region::Region {
        region::Region {
            start: self.start,
            end: self.end,
            loop_start: self.loop_start,
            loop_end: self.loop_end,
            looping: self.looping,
        }
    }

    /// LOM length: loop length with Loop on, otherwise End minus Start.
    pub fn length(&self) -> BeatTime {
        self.region().length()
    }

    /// The editor's lanes: declared lanes in order, then any other event a hit plays in
    /// first-appearance order, so a clip needs no lane declared for each of its hits.
    pub fn lane_refs(&self) -> Vec<EventRef> {
        let mut lanes: Vec<EventRef> = self
            .lanes
            .iter()
            .filter_map(Lane::event)
            .map(|l| l.definition.clone())
            .collect();
        for event in &self.events {
            if !lanes.contains(&event.definition) {
                lanes.push(event.definition.clone());
            }
        }
        lanes
    }

    /// The checks that need no track; `Track::validate_clip` checks the refs.
    pub fn validate(&self) -> ModelResult {
        self.region().validate()?;
        if let LaunchQuantisation::Value(value) = self.quantisation {
            value.validate()?;
        }
        for envelope in self.lanes.iter().filter_map(Lane::envelope) {
            envelope.validate()?;
        }
        for event in &self.events {
            for arg in &event.arguments {
                if let OscArgument::String(value) = arg {
                    require(!value.contains('\0'), "OSC string must not contain NUL")?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Slot {
    pub clip: Option<Clip>,
    #[serde(default = "default_has_stop_button", alias = "stop_button")]
    pub has_stop_button: bool,
}

fn default_has_stop_button() -> bool {
    true
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            clip: None,
            has_stop_button: true,
        }
    }
}

impl Slot {
    /// Clearing a clip never removes its slot or changes its stop-button flag.
    pub fn clear(&mut self) -> Option<Clip> {
        self.clip.take()
    }
}

/// A clip track or a group track. A group track precedes its members in
/// `Project.tracks`, holds no clips (its slots stay empty) and carries the group's
/// route, mixer, delay and fold state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub id: TrackId,
    #[serde(default, skip_serializing_if = "TrackKind::is_track")]
    pub kind: TrackKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Route::is_group")]
    pub route: Route,
    #[serde(default = "unset_colour")]
    pub colour: Colour,
    /// Signed send offset in milliseconds, within ±MAX_TRACK_DELAY_MS.
    #[serde(default)]
    pub delay_ms: f64,
    /// Activator and Solo (`model::mixer`).
    #[serde(default)]
    pub mixer: MixerState,
    /// Independent volume controller; boxed to keep Track-bearing command enums compact.
    #[serde(default, skip_serializing_if = "MixerDevice::is_default")]
    pub mixer_device: Box<MixerDevice>,
    /// LOM `Track.devices`, in category order; the Mixer stays separate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<Device>,
    /// LOM `group_track`: the group track this track belongs to; None at top level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<TrackId>,
    /// OSC path this track adds to its descendants' relative addresses, and to its own as a
    /// source (`model::address`).
    #[serde(default, skip_serializing_if = "Path::is_empty")]
    pub path: Path,
    /// LOM `fold_state`; only a group track may set it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fold_state: bool,
    /// Index is the scene row, including empty slots. Empty for an output.
    pub slots: Vec<Slot>,
    /// Destination component: required for Output, optional for Main.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Output>,
}

impl Track {
    pub fn new(id: TrackId, name: impl Into<String>, scene_count: usize) -> Self {
        Self {
            id,
            kind: TrackKind::Track,
            name: name.into(),
            route: Route::empty(),
            colour: LIVE_CLIP_PALETTE[0],
            delay_ms: 0.0,
            mixer: MixerState::default(),
            mixer_device: Box::default(),
            devices: Vec::new(),
            group: None,
            path: Path::default(),
            fold_state: false,
            slots: vec![Slot::default(); scene_count],
            output: None,
        }
    }

    pub fn new_group(id: TrackId, name: impl Into<String>, scene_count: usize) -> Self {
        Self {
            kind: TrackKind::Group,
            ..Self::new(id, name, scene_count)
        }
    }

    pub fn is_group(&self) -> bool {
        self.kind == TrackKind::Group
    }

    /// What this track can do, by its kind.
    pub fn capabilities(&self) -> Capabilities {
        self.kind.capabilities()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub id: SceneId,
    pub name: String,
    #[serde(default)]
    pub follow_actions: SceneFollowActions,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Project {
    pub tempo: Tempo,
    pub time_signature: TimeSignature,
    pub global_quantisation: GlobalQuantisation,
    pub follow_actions_enabled: bool,
    /// Seed retained with the model; the playback engine owns the random generator.
    pub random_seed: u64,
    /// Next swatch for a new track/group; serde defaults old project files to zero.
    pub next_colour_index: usize,
    /// Output tracks (kind Output), in their own region right of the tracks, like Live's
    /// `return_tracks`. Ids share one counter with `tracks`.
    pub outputs: Vec<Track>,
    pub tracks: Vec<Track>,
    pub scenes: Vec<Scene>,
    /// Main is a Track kept apart from the clip and output lists.
    pub main: Track,
    /// Remote bindings in list order. Sources and controls may occur in several rows.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mappings: Vec<RemoteMapping>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            tempo: Tempo::default(),
            time_signature: TimeSignature::default(),
            global_quantisation: GlobalQuantisation::default(),
            follow_actions_enabled: true,
            random_seed: 0,
            next_colour_index: 0,
            outputs: Vec::new(),
            tracks: Vec::new(),
            scenes: Vec::new(),
            main: Track::new_main(TrackId(0)),
            mappings: Vec::new(),
        }
    }
}

impl Project {
    /// Group inheritance is explicit; an empty route is silent and valid.
    pub fn resolved_route(&self, id: TrackId) -> &[TrackId] {
        let Some(track) = self.tracks.iter().find(|t| t.id == id) else {
            return &[];
        };
        if let Route::Outputs(ids) = &track.route {
            return ids;
        }
        self.ancestors(id)
            .filter_map(Ancestor::group)
            .find_map(|group| match &group.route {
                Route::Outputs(ids) => Some(ids.as_slice()),
                Route::Group => None,
            })
            .unwrap_or(&[])
    }
    /// The output track with this id; Main is not one (`destination` finds both).
    pub fn output(&self, id: TrackId) -> Option<&Track> {
        self.outputs.iter().find(|o| o.id == id)
    }
    /// The destination-capable track with this id: Main or an output.
    pub fn destination(&self, id: TrackId) -> Option<&Track> {
        self.destinations().find(|o| o.id == id)
    }
    /// All destination-capable tracks, Main first, including an unset Main.
    pub fn destinations(&self) -> impl Iterator<Item = &Track> {
        std::iter::once(&self.main).chain(&self.outputs)
    }
    /// Destinations that have a host: Main is left out while unset.
    pub fn configured_destinations(&self) -> impl Iterator<Item = &Track> {
        self.destinations().filter(|t| t.destination().is_some())
    }
    pub fn routed_outputs(&self, id: TrackId) -> impl Iterator<Item = &Track> {
        self.resolved_route(id)
            .iter()
            .filter_map(|id| self.destination(*id))
            .filter(|track| track.destination().is_some())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename = "FollowMultiplier")]
struct UncheckedFollowMultiplier {
    numerator: u32,
    denominator: u32,
}
impl TryFrom<UncheckedFollowMultiplier> for FollowMultiplier {
    type Error = ModelError;
    fn try_from(value: UncheckedFollowMultiplier) -> Result<Self, Self::Error> {
        Self::new(value.numerator, value.denominator)
    }
}
