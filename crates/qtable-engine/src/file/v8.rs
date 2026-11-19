//! Frozen format 8 containers. Upgrade adds clip markers and linked lane regions.
use crate::model::{self, *};
use serde::Deserialize;
fn yes() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Clip {
    pub id: ClipId,
    pub name: String,
    #[serde(default = "unset_colour")]
    pub colour: Colour,
    pub length: BeatTime,
    #[serde(default = "yes")]
    pub looping: bool,
    #[serde(default)]
    pub quantisation: LaunchQuantisation,
    #[serde(default)]
    pub launch_mode: LaunchMode,
    #[serde(default)]
    pub legato: bool,
    #[serde(default)]
    pub follow_actions: ClipFollowActions,
    #[serde(default)]
    pub lanes: Vec<Lane>,
    #[serde(default)]
    pub events: Vec<model::Event>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default)]
pub struct Slot {
    pub clip: Option<Clip>,
    #[serde(default = "yes", alias = "stop_button")]
    pub has_stop_button: bool,
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            clip: None,
            has_stop_button: true,
        }
    }
}

/// Frozen format 8 track container.
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
    pub mixer_device: Box<model::MixerDevice>,
    #[serde(default)]
    pub group: Option<TrackId>,
    #[serde(default)]
    pub path: model::Path,
    #[serde(default)]
    pub fold_state: bool,
    #[serde(default)]
    pub devices: Vec<model::Device>,
    pub slots: Vec<Slot>,
    #[serde(default)]
    pub output: Option<model::Output>,
}

impl Track {
    /// A clip-less model track (a default, a new group or output) in the wire shape. Older
    /// chains build tracks this way and then set their wire slots.
    pub fn shell(track: model::Track) -> Self {
        debug_assert!(track.slots.iter().all(|s| s.clip.is_none()));
        Self {
            id: track.id,
            kind: track.kind,
            name: track.name,
            route: track.route,
            colour: track.colour,
            delay_ms: track.delay_ms,
            mixer: track.mixer,
            mixer_device: track.mixer_device,
            group: track.group,
            path: track.path,
            fold_state: track.fold_state,
            devices: track.devices,
            slots: track
                .slots
                .into_iter()
                .map(|s| Slot {
                    clip: None,
                    has_stop_button: s.has_stop_button,
                })
                .collect(),
            output: track.output,
        }
    }
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
            main: Track::shell(new.main),
        }
    }
}

pub(super) fn migrate(old: Project) -> super::v9::Project {
    super::v9::Project {
        tempo: old.tempo,
        time_signature: old.time_signature,
        global_quantisation: old.global_quantisation,
        follow_actions_enabled: old.follow_actions_enabled,
        random_seed: old.random_seed,
        next_colour_index: old.next_colour_index,
        scenes: old.scenes,
        tracks: old.tracks.into_iter().map(track).collect(),
        outputs: old.outputs.into_iter().map(track).collect(),
        main: track(old.main),
    }
}
fn track(old: Track) -> model::Track {
    model::Track {
        id: old.id,
        kind: old.kind,
        name: old.name,
        route: old.route,
        colour: old.colour,
        delay_ms: old.delay_ms,
        mixer: old.mixer,
        mixer_device: old.mixer_device,
        devices: old.devices,
        group: old.group,
        path: old.path,
        fold_state: old.fold_state,
        output: old.output,
        slots: old
            .slots
            .into_iter()
            .map(|slot| model::Slot {
                has_stop_button: slot.has_stop_button,
                clip: slot.clip.map(|c| model::Clip {
                    id: c.id,
                    name: c.name,
                    colour: c.colour,
                    start: BeatTime::ZERO,
                    end: c.length,
                    loop_start: BeatTime::ZERO,
                    loop_end: c.length,
                    looping: c.looping,
                    quantisation: c.quantisation,
                    launch_mode: c.launch_mode,
                    legato: c.legato,
                    follow_actions: c.follow_actions,
                    events: c.events,
                    lanes: c.lanes.into_iter().map(lane).collect(),
                }),
            })
            .collect(),
    }
}

// These types must keep rejecting fields unknown to format 8 (the B7 guard).
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Lane {
    Events(model::EventLane),
    Envelope(AutomationEnvelope),
}
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationEnvelope {
    pub target: ArgumentRef,
    pub points: Vec<Breakpoint>,
}
fn lane(old: Lane) -> model::Lane {
    match old {
        Lane::Events(e) => model::Lane::Events(e),
        Lane::Envelope(e) => model::Lane::Envelope(model::AutomationEnvelope {
            target: e.target,
            points: e.points,
            region: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v8_guard_rejects_a_region_without_a_version_bump() {
        let source = "(target:(device:Mixer,parameter:\"volume\",field:(\"value\")),points:[])";
        assert!(ron::from_str::<AutomationEnvelope>(source).is_ok());
        let bad = source.replacen("points:", "region:None,points:", 1);
        assert!(ron::from_str::<AutomationEnvelope>(&bad).is_err());
        assert!(ron::from_str::<Lane>(&format!("Envelope({bad})")).is_err());
    }
}
