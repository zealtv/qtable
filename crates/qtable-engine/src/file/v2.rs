//! Version 2 wire model: groups stored apart from tracks, and the step that merges them
//! into group tracks (2026-10-13).
//! It returns the v3 wire shape; `v3::migrate` takes it to the model.
use super::v5::Track;
use crate::model::*;
use serde::Deserialize;
use std::collections::HashMap;

/// A v2 group id; v2 numbered groups apart from tracks and nothing else names one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
pub struct GroupId(pub u64);

#[derive(Deserialize)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    #[serde(default = "unset_colour")]
    pub colour: Colour,
    #[serde(default, alias = "folded")]
    pub fold_state: bool,
    #[serde(default)]
    pub route: Route,
    #[serde(default)]
    pub mixer: MixerState,
    #[serde(default)]
    pub delay_ms: f64,
    /// Non-empty, adjacent members in project track order.
    pub tracks: Vec<TrackId>,
}

/// v2 tracks read as v3 tracks: every field v3 adds defaults.
#[derive(Deserialize)]
#[serde(default)]
pub struct Project {
    pub tempo: Tempo,
    pub time_signature: TimeSignature,
    pub global_quantisation: GlobalQuantisation,
    pub follow_actions_enabled: bool,
    pub random_seed: u64,
    pub next_colour_index: usize,
    pub outputs: Vec<super::v3::Output>,
    pub tracks: Vec<Track>,
    pub scenes: Vec<Scene>,
    pub groups: Vec<Group>,
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
            groups: Vec::new(),
            main: super::v4::Main::default(),
        }
    }
}

/// Each group becomes a group track just before its first member, with a fresh id above
/// every track id (groups in file order), so a load is reproducible.
pub(super) fn migrate(old: Project) -> super::v3::Project {
    let first_id = old.tracks.iter().map(|t| t.id.0).max().map_or(1, |m| m + 1);
    let mut group_ids = HashMap::new();
    let mut first_member = HashMap::new();
    for (id, group) in (first_id..).map(TrackId).zip(&old.groups) {
        group_ids.insert(group.id, id);
        if let Some(first) = old
            .tracks
            .iter()
            .find(|t| group.tracks.contains(&t.id))
            .map(|t| t.id)
        {
            first_member.insert(first, group);
        }
    }
    let scene_count = old.scenes.len();
    let mut tracks = Vec::with_capacity(old.tracks.len() + old.groups.len());
    for mut track in old.tracks {
        if let Some(group) = first_member.get(&track.id) {
            let mut item = Track::new_group(group_ids[&group.id], &group.name, scene_count);
            item.colour = group.colour;
            item.fold_state = group.fold_state;
            item.route = group.route.clone();
            item.mixer = group.mixer;
            item.delay_ms = group.delay_ms;
            tracks.push(item);
        }
        track.group = old
            .groups
            .iter()
            .find(|g| g.tracks.contains(&track.id))
            .map(|g| group_ids[&g.id]);
        tracks.push(track);
    }
    super::v3::Project {
        tempo: old.tempo,
        time_signature: old.time_signature,
        global_quantisation: old.global_quantisation,
        follow_actions_enabled: old.follow_actions_enabled,
        random_seed: old.random_seed,
        next_colour_index: old.next_colour_index,
        outputs: old.outputs,
        tracks,
        scenes: old.scenes,
        main: old.main,
    }
}
