//! Version 6 wire model, read when loading version 6 files and as the clip shape the older
//! chains (v1–v5) pass along: hits and lanes named their OSC address. Version 7 keeps each
//! address once, on a Target device of the clip's track, and clips point at it (`EventRef`;
//! target-device design, "File format 7", 2026-10-14). Only the structs that changed are
//! mirrored here, under their own names, because RON checks struct names.
use crate::model::{
    self, BeatTime, ClipFollowActions, ClipId, Colour, Device, DeviceId, DeviceKind, EventDef,
    EventRef, FieldType, LaunchMode, LaunchQuantisation, MixerState, OscAddress, OscArgument,
    Route, TargetDevice, TrackId, TrackKind, unset_colour,
};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Event {
    pub position: BeatTime,
    pub address: OscAddress,
    #[serde(default)]
    pub arguments: Vec<OscArgument>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct EventLane {
    pub address: OscAddress,
}

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
    pub lanes: Vec<EventLane>,
    #[serde(default)]
    pub events: Vec<Event>,
}

impl Clip {
    /// Declared lanes, then any other hit address in first-appearance order (v6 rule).
    fn lane_addresses(&self) -> Vec<OscAddress> {
        let mut lanes: Vec<OscAddress> = self.lanes.iter().map(|l| l.address.clone()).collect();
        for event in &self.events {
            if !lanes.contains(&event.address) {
                lanes.push(event.address.clone());
            }
        }
        lanes
    }
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

/// The version 6 track: no device chain.
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

impl Project {
    /// `model::Project::next_strip_id`, for the v4 step that numbers Main.
    pub fn next_strip_id(&self) -> u64 {
        self.tracks
            .iter()
            .chain(&self.outputs)
            .map(|t| t.id.0)
            .chain([self.main.id.0])
            .max()
            .map_or(1, |max| max + 1)
    }

    /// `model::Project::renumber_colliding_outputs`, for the v4 step, which runs it before
    /// numbering Main.
    pub fn renumber_colliding_outputs(&mut self) {
        let track_ids: HashSet<u64> = self.tracks.iter().map(|t| t.id.0).collect();
        let mut next = self.next_strip_id();
        let mut moved = Vec::new();
        for output in &mut self.outputs {
            if track_ids.contains(&output.id.0) {
                moved.push((output.id, TrackId(next)));
                output.id = TrackId(next);
                next += 1;
            }
        }
        for track in &mut self.tracks {
            if let Route::Outputs(ids) = &mut track.route {
                for id in ids.iter_mut() {
                    if let Some((_, new)) = moved.iter().find(|(old, _)| old == id) {
                        *id = *new;
                    }
                }
            }
        }
    }
}

/// One event definition found on a track: its address and its hits' argument types.
type Found = (OscAddress, Vec<FieldType>);

fn signature(event: &Event) -> Vec<FieldType> {
    event.arguments.iter().map(FieldType::of).collect()
}

fn signature_matches(event: &Event, signature: &[FieldType]) -> bool {
    event.arguments.len() == signature.len()
        && event
            .arguments
            .iter()
            .zip(signature)
            .all(|(a, t)| FieldType::of(a) == *t)
}

/// A v6 project as the current model: each clip track's lanes become one Target, so every
/// composed address stays byte-identical (target-device design, "`v6::migrate`").
pub(super) fn migrate(old: Project) -> super::v7::Project {
    let mut next_device = 1;
    let mut track = |track: Track| into_model(track, &mut next_device);
    let tracks = old.tracks.into_iter().map(&mut track).collect();
    let outputs = old.outputs.into_iter().map(&mut track).collect();
    let main = track(old.main);
    super::v7::Project {
        tempo: old.tempo,
        time_signature: old.time_signature,
        global_quantisation: old.global_quantisation,
        follow_actions_enabled: old.follow_actions_enabled,
        random_seed: old.random_seed,
        next_colour_index: old.next_colour_index,
        outputs,
        tracks,
        scenes: old.scenes,
        main,
    }
}

fn into_model(track: Track, next_device: &mut u64) -> super::v7::Track {
    let clips = || track.slots.iter().filter_map(|s| s.clip.as_ref());
    // 1. Events from hits, by address and signature, in slot then hit order.
    let mut found: Vec<Found> = Vec::new();
    for event in clips().flat_map(|c| &c.events) {
        let key = (event.address.clone(), signature(event));
        if !found.contains(&key) {
            found.push(key);
        }
    }
    // 2. An empty lane adds an event only for an address no hit has defined.
    for address in clips().flat_map(Clip::lane_addresses) {
        if !found.iter().any(|(a, _)| *a == address) {
            found.push((address, Vec::new()));
        }
    }
    let device = DeviceId(*next_device);
    let mut target = TargetDevice::default();
    let mut refs = Vec::with_capacity(found.len());
    // Names stay empty: `Track::event_label` tells same-path definitions apart from their
    // current path and signature, so a later path edit can't leave a stale name behind.
    // Each field's default comes from the event's first hit in slot then hit order, so a
    // trigger fires the values its hits already hold; an event with no hit keeps type defaults.
    for (address, signature) in &found {
        let first_hit = clips()
            .flat_map(|c| &c.events)
            .find(|e| e.address == *address && signature_matches(e, signature));
        let key = target.fresh_key(address);
        target.events.push(EventDef {
            key: key.clone(),
            name: String::new(),
            path: address.clone(),
            fields: signature
                .iter()
                .enumerate()
                .map(|(i, kind)| {
                    let mut field =
                        model::FieldDef::new(model::FieldKey(format!("f{}", i + 1)), *kind, false);
                    if let Some(hit) = first_hit {
                        field.default = hit.arguments[i].clone();
                        field.value = None;
                        field.text = None;
                        field.seed_current();
                    }
                    field
                })
                .collect(),
        });
        refs.push(EventRef { device, key });
    }
    let devices = if found.is_empty() {
        Vec::new()
    } else {
        *next_device += 1;
        vec![Device {
            id: device,
            name: "Target".into(),
            active: true,
            expanded: false,
            kind: DeviceKind::Target(target),
        }]
    };
    let reference = |key: &Found| refs[found.iter().position(|f| f == key).expect("found")].clone();
    let slots = track
        .slots
        .iter()
        .map(|slot| super::v7::Slot {
            has_stop_button: slot.has_stop_button,
            clip: slot.clip.as_ref().map(|clip| {
                let lanes = clip
                    .lane_addresses()
                    .into_iter()
                    .flat_map(|address| {
                        // One lane per definition the clip's hits on this address use, in
                        // definition order; a lane with no hits takes the first definition.
                        let used: Vec<&Found> = found
                            .iter()
                            .filter(|f| f.0 == address)
                            .filter(|f| {
                                clip.events
                                    .iter()
                                    .any(|e| e.address == f.0 && signature(e) == f.1)
                            })
                            .collect();
                        let used = if used.is_empty() {
                            found.iter().filter(|f| f.0 == address).take(1).collect()
                        } else {
                            used
                        };
                        used.into_iter()
                            .map(|f| model::EventLane {
                                definition: reference(f),
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect();
                super::v7::Clip {
                    id: clip.id,
                    name: clip.name.clone(),
                    colour: clip.colour,
                    length: clip.length,
                    looping: clip.looping,
                    quantisation: clip.quantisation,
                    launch_mode: clip.launch_mode,
                    legato: clip.legato,
                    follow_actions: clip.follow_actions.clone(),
                    lanes,
                    events: clip
                        .events
                        .iter()
                        .map(|event| model::Event {
                            position: event.position,
                            definition: reference(&(event.address.clone(), signature(event))),
                            arguments: event.arguments.clone(),
                        })
                        .collect(),
                }
            }),
        })
        .collect();
    super::v7::Track {
        id: track.id,
        kind: track.kind,
        name: track.name,
        route: track.route,
        colour: track.colour,
        delay_ms: track.delay_ms,
        mixer: track.mixer,
        mixer_device: track.mixer_device,
        devices,
        group: track.group,
        path: track.path,
        fold_state: track.fold_state,
        slots,
        output: track.output,
    }
}
