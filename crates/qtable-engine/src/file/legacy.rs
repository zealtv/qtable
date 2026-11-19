//! Version 1 wire model, retained only for project-file migration.
use super::v6::Slot;
use crate::model::*;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
pub struct DestinationId(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
pub struct TargetId(u64);
#[derive(Deserialize)]
pub struct Destination {
    pub id: DestinationId,
    #[serde(rename = "name")]
    pub _name: String,
    /// Hostname or IP; resolution belongs to the sender.
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub mode: UdpMode,
}
#[derive(Deserialize)]
pub struct Target {
    pub id: TargetId,
    pub name: String,
    pub destination: DestinationId,
    /// Empty for a generic receiver, otherwise slash-led with no trailing slash.
    #[serde(default)]
    pub prefix: String,
}
#[derive(Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    #[serde(default)]
    pub target: Option<TargetId>,
    #[serde(default = "crate::model::unset_colour")]
    pub colour: Colour,
    /// Signed send offset in milliseconds, within ±MAX_TRACK_DELAY_MS.
    #[serde(default)]
    pub delay_ms: f64,
    /// Index is the scene row, including empty slots.
    pub slots: Vec<Slot>,
}
#[derive(Deserialize)]
pub struct Group {
    pub id: super::v2::GroupId,
    pub name: String,
    #[serde(default = "crate::model::unset_colour")]
    pub colour: Colour,
    #[serde(default)]
    pub folded: bool,
    #[serde(default)]
    pub default_target: Option<TargetId>,
    /// Non-empty, adjacent members in project track order.
    pub tracks: Vec<TrackId>,
}
#[derive(Deserialize)]
#[serde(default)]
pub struct Project {
    pub tempo: Tempo,
    pub time_signature: TimeSignature,
    pub global_quantisation: GlobalQuantisation,
    pub follow_actions_enabled: bool,
    /// Seed retained with the model; the playback engine owns the random generator.
    pub random_seed: u64,
    /// Next swatch for a new track/group; serde defaults old project files to zero.
    pub next_colour_index: usize,
    pub destinations: Vec<Destination>,
    pub targets: Vec<Target>,
    pub tracks: Vec<Track>,
    pub scenes: Vec<Scene>,
    pub groups: Vec<Group>,
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
            destinations: Vec::new(),
            targets: Vec::new(),
            tracks: Vec::new(),
            scenes: Vec::new(),
            groups: Vec::new(),
        }
    }
}

pub(super) fn migrate(old: Project) -> Result<super::v2::Project, ModelError> {
    let mut outputs = Vec::new();
    let mut destination_ids = std::collections::HashSet::new();
    for d in &old.destinations {
        if !destination_ids.insert(d.id)
            || d.host.trim().is_empty()
            || d.host.contains('\0')
            || d.port == 0
        {
            return Err(ModelError("invalid legacy destination".into()));
        }
    }
    for target in old.targets {
        let destination = old
            .destinations
            .iter()
            .find(|d| d.id == target.destination)
            .ok_or_else(|| ModelError("target references missing destination".into()))?;
        let mut output = super::v3::Output::new(
            super::v3::OutputId(target.id.0),
            target.name,
            destination.host.clone(),
            destination.port,
        );
        output.colour = LIVE_CLIP_PALETTE[outputs.len() % LIVE_CLIP_PALETTE.len()];
        output.mode = destination.mode;
        output.prefix = target.prefix;
        outputs.push(output);
    }
    Ok(super::v2::Project {
        tempo: old.tempo,
        time_signature: old.time_signature,
        global_quantisation: old.global_quantisation,
        follow_actions_enabled: old.follow_actions_enabled,
        random_seed: old.random_seed,
        next_colour_index: old.next_colour_index,
        outputs,
        main: Default::default(),
        scenes: old.scenes,
        tracks: old
            .tracks
            .into_iter()
            .map(|t| {
                let mut track: super::v5::Track = crate::model::Track {
                    mixer_device: Box::default(),
                    devices: Vec::new(),
                    mixer: Default::default(),
                    kind: Default::default(),
                    group: None,
                    path: Default::default(),
                    fold_state: false,
                    id: t.id,
                    name: t.name,
                    route: t
                        .target
                        .map_or(Route::Group, |id| Route::Outputs(vec![TrackId(id.0)])),
                    colour: t.colour,
                    delay_ms: t.delay_ms,
                    slots: Vec::new(),
                    output: None,
                }
                .into();
                track.slots = t.slots;
                track
            })
            .collect(),
        groups: old
            .groups
            .into_iter()
            .map(|g| super::v2::Group {
                mixer: Default::default(),
                delay_ms: 0.0,
                id: g.id,
                name: g.name,
                colour: g.colour,
                fold_state: g.folded,
                route: Route::Outputs(
                    g.default_target
                        .into_iter()
                        .map(|id| TrackId(id.0))
                        .collect(),
                ),
                tracks: g.tracks,
            })
            .collect(),
    })
}
