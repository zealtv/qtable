//! Version 3 wire model: outputs stored apart from tracks as flat records, and the step
//! that turns them into output tracks (2026-10-13).
use super::v5::{Destination, Output as OutputComponent, Track};
use crate::model::{Colour, LIVE_CLIP_PALETTE, MixerState, TrackId, UdpMode};
use serde::{Deserialize, Deserializer};

/// A v3 output id; v3 files write `OutputId(n)` where handwritten.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
pub struct OutputId(pub u64);

/// One v3 receiver. Missing presentation fields derive from the name and palette.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(from = "OutputFields")]
pub struct Output {
    pub id: OutputId,
    pub name: String,
    pub colour: Colour,
    pub host: String,
    pub port: u16,
    pub mode: UdpMode,
    pub prefix: String,
    pub enabled: bool,
    pub solo: bool,
    pub latency_ms: f64,
    pub default_for_new_tracks: bool,
}

impl Output {
    pub fn new(id: OutputId, name: impl Into<String>, host: impl Into<String>, port: u16) -> Self {
        let name = name.into();
        Self {
            id,
            name,
            colour: LIVE_CLIP_PALETTE[0],
            host: host.into(),
            port,
            mode: UdpMode::Unicast,
            prefix: String::new(),
            enabled: true,
            solo: false,
            latency_ms: 0.0,
            default_for_new_tracks: false,
        }
    }
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(rename = "Output")]
struct OutputFields {
    id: OutputId,
    name: String,
    host: String,
    port: u16,
    #[serde(default, deserialize_with = "present")]
    colour: Option<Colour>,
    #[serde(default)]
    mode: UdpMode,
    #[serde(default)]
    prefix: String,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
    #[serde(default)]
    solo: bool,
    #[serde(default)]
    latency_ms: f64,
    #[serde(default)]
    default_for_new_tracks: bool,
}

impl From<OutputFields> for Output {
    fn from(fields: OutputFields) -> Self {
        let mut output = Self::new(fields.id, fields.name, fields.host, fields.port);
        if let Some(colour) = fields.colour {
            output.colour = colour;
        }
        output.mode = fields.mode;
        output.prefix = fields.prefix;
        output.enabled = fields.enabled;
        output.solo = fields.solo;
        output.latency_ms = fields.latency_ms;
        output.default_for_new_tracks = fields.default_for_new_tracks;
        output
    }
}

fn present<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

/// v3 tracks read as v4 tracks: the field v4 adds (`output`) defaults to none.
#[derive(Deserialize)]
#[serde(default)]
pub struct Project {
    pub tempo: crate::model::Tempo,
    pub time_signature: crate::model::TimeSignature,
    pub global_quantisation: crate::model::GlobalQuantisation,
    pub follow_actions_enabled: bool,
    pub random_seed: u64,
    pub next_colour_index: usize,
    pub outputs: Vec<Output>,
    pub tracks: Vec<Track>,
    pub scenes: Vec<crate::model::Scene>,
    pub main: super::v4::Main,
}

impl Default for Project {
    fn default() -> Self {
        let new = crate::model::Project::default();
        Self {
            tempo: new.tempo,
            time_signature: new.time_signature,
            global_quantisation: new.global_quantisation,
            follow_actions_enabled: new.follow_actions_enabled,
            random_seed: new.random_seed,
            next_colour_index: new.next_colour_index,
            outputs: Vec::new(),
            tracks: Vec::new(),
            scenes: new.scenes,
            main: super::v4::Main::default(),
        }
    }
}

impl Output {
    /// The output track for this record. Ids keep their number here; loading renumbers any
    /// that collide with a track id (`Project::renumber_colliding_outputs`).
    fn into_track(self) -> Track {
        let mut track: Track =
            crate::model::Track::new_output(TrackId(self.id.0), self.name, "", 0).into();
        track.colour = self.colour;
        track.delay_ms = self.latency_ms;
        track.mixer = MixerState {
            active: self.enabled,
            solo: self.solo,
        };
        // The prefix becomes the track's `path` when the chain ends in `Track::into_model`.
        track.output = Some(OutputComponent {
            destination: Destination {
                host: self.host,
                port: self.port,
                mode: self.mode,
                prefix: self.prefix,
            },
        });
        track
    }
}

pub(super) fn migrate(old: Project) -> super::v4::Project {
    super::v4::Project {
        tempo: old.tempo,
        time_signature: old.time_signature,
        global_quantisation: old.global_quantisation,
        follow_actions_enabled: old.follow_actions_enabled,
        random_seed: old.random_seed,
        next_colour_index: old.next_colour_index,
        outputs: old.outputs.into_iter().map(Output::into_track).collect(),
        tracks: old.tracks,
        scenes: old.scenes,
        main: old.main,
    }
}
