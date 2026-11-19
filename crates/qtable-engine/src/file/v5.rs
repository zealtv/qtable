//! Version 5 wire model, read when loading version 5 files and as the track shape the older
//! chains (v2–v4) pass along: address prefixes lived in three places (`Destination.prefix`,
//! a group's `group_prefix`, `VolumeOutput.prefix`). Version 6 gives every track one `path`
//! and the volume controller an `address` (2026-10-13).
//! Only the structs that changed are mirrored here.
use super::v6::{self, Slot};
use crate::model::{
    self, Colour, Mapping, MixerState, Route, TrackId, TrackKind, UdpMode, unset_colour,
};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Destination {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub mode: UdpMode,
    #[serde(default)]
    pub prefix: String,
}

/// The output component; `default_for_new_tracks` and short tags in older files are ignored.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Output {
    pub destination: Destination,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VolumeOutput {
    pub enabled: bool,
    pub prefix: String,
    pub mapping: Mapping,
}
impl Default for VolumeOutput {
    fn default() -> Self {
        model::ParameterOutput::default().into()
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MixerDevice {
    pub volume: f64,
    pub volume_output: VolumeOutput,
}
impl Default for MixerDevice {
    fn default() -> Self {
        model::MixerDevice::default().into()
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Track {
    pub id: TrackId,
    #[serde(default)]
    pub kind: TrackKind,
    pub name: String,
    #[serde(default)]
    pub route: Route,
    #[serde(default = "unset_colour")]
    pub colour: Colour,
    #[serde(default)]
    pub delay_ms: f64,
    #[serde(default)]
    pub mixer: MixerState,
    #[serde(default)]
    pub mixer_device: Box<MixerDevice>,
    #[serde(default)]
    pub group: Option<TrackId>,
    #[serde(default)]
    pub group_prefix: String,
    #[serde(default)]
    pub fold_state: bool,
    pub slots: Vec<Slot>,
    #[serde(default)]
    pub output: Option<Output>,
}

#[derive(Deserialize)]
#[serde(default)]
pub struct Project {
    pub tempo: model::Tempo,
    pub time_signature: model::TimeSignature,
    pub global_quantisation: model::GlobalQuantisation,
    pub follow_actions_enabled: bool,
    pub random_seed: u64,
    pub next_colour_index: usize,
    pub outputs: Vec<Track>,
    pub tracks: Vec<Track>,
    pub scenes: Vec<model::Scene>,
    pub main: Track,
}

impl Default for Project {
    fn default() -> Self {
        let new = model::Project::default();
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
            main: new.main.into(),
        }
    }
}

impl Track {
    pub fn new_group(id: TrackId, name: impl Into<String>, scene_count: usize) -> Self {
        model::Track::new_group(id, name, scene_count).into()
    }

    /// The version 6 track: the group's prefix, or an output's or Main's destination prefix,
    /// becomes the track's `path`. Nothing is rejected here; `Project::validate` checks it.
    pub fn into_v6(self) -> v6::Track {
        let mut destination_prefix = String::new();
        let output = self.output.map(|output| {
            destination_prefix = output.destination.prefix;
            model::Output {
                destination: model::Destination {
                    host: output.destination.host,
                    port: output.destination.port,
                    mode: output.destination.mode,
                },
                stage: model::OutputStage::default(),
            }
        });
        let path = if destination_prefix.is_empty() {
            self.group_prefix
        } else {
            destination_prefix
        };
        v6::Track {
            id: self.id,
            kind: self.kind,
            name: self.name,
            route: self.route,
            colour: self.colour,
            delay_ms: self.delay_ms,
            mixer: self.mixer,
            mixer_device: Box::new((*self.mixer_device).into()),
            group: self.group,
            path: model::Path::unchecked(path),
            fold_state: self.fold_state,
            slots: self.slots,
            output,
        }
    }
}

/// Only for clip-less tracks (defaults, new groups and outputs); see `v6::Track::shell`.
impl From<model::Track> for Track {
    fn from(track: model::Track) -> Self {
        let is_destination = track.output.is_some();
        let path = track.path.as_str().to_owned();
        Self {
            id: track.id,
            kind: track.kind,
            name: track.name,
            route: track.route,
            colour: track.colour,
            delay_ms: track.delay_ms,
            mixer: track.mixer,
            mixer_device: Box::new((*track.mixer_device).into()),
            group: track.group,
            group_prefix: if is_destination {
                String::new()
            } else {
                path.clone()
            },
            fold_state: track.fold_state,
            slots: track
                .slots
                .into_iter()
                .map(|slot| {
                    debug_assert!(slot.clip.is_none());
                    Slot {
                        clip: None,
                        has_stop_button: slot.has_stop_button,
                    }
                })
                .collect(),
            output: track.output.map(|output| Output {
                destination: Destination {
                    host: output.destination.host,
                    port: output.destination.port,
                    mode: output.destination.mode,
                    prefix: path,
                },
            }),
        }
    }
}

impl From<model::ParameterOutput> for VolumeOutput {
    fn from(v: model::ParameterOutput) -> Self {
        Self {
            enabled: v.enabled,
            prefix: v.address,
            mapping: v.mapping,
        }
    }
}
impl From<VolumeOutput> for model::ParameterOutput {
    fn from(v: VolumeOutput) -> Self {
        Self {
            enabled: v.enabled,
            address: v.prefix,
            mapping: v.mapping,
            ..Self::default()
        }
    }
}
impl From<model::MixerDevice> for MixerDevice {
    fn from(d: model::MixerDevice) -> Self {
        Self {
            volume: d.volume.value,
            volume_output: d.volume.output.into(),
        }
    }
}
impl From<MixerDevice> for model::MixerDevice {
    fn from(d: MixerDevice) -> Self {
        Self {
            volume: model::StoredParameter {
                value: d.volume,
                output: d.volume_output.into(),
            },
        }
    }
}

/// A v5 file's project in the version 6 shape: Main keeps its destination prefix as its path,
/// so every composed address stays as it was.
pub(super) fn migrate(old: Project) -> v6::Project {
    v6::Project {
        tempo: old.tempo,
        time_signature: old.time_signature,
        global_quantisation: old.global_quantisation,
        follow_actions_enabled: old.follow_actions_enabled,
        random_seed: old.random_seed,
        next_colour_index: old.next_colour_index,
        outputs: old.outputs.into_iter().map(Track::into_v6).collect(),
        tracks: old.tracks.into_iter().map(Track::into_v6).collect(),
        scenes: old.scenes,
        main: old.main.into_v6(),
    }
}
