//! Version 4 stored Main had only global delay and no destination.
use super::v5::Track;
use super::v6;
use crate::model::{self, TrackId};
use serde::Deserialize;
#[derive(Default, Deserialize)]
#[serde(default)]
pub struct Main {
    pub delay_ms: f64,
}
#[derive(Deserialize)]
#[serde(default)]
pub struct Project {
    pub tempo: crate::model::Tempo,
    pub time_signature: crate::model::TimeSignature,
    pub global_quantisation: crate::model::GlobalQuantisation,
    pub follow_actions_enabled: bool,
    pub random_seed: u64,
    pub next_colour_index: usize,
    pub outputs: Vec<Track>,
    pub tracks: Vec<Track>,
    pub scenes: Vec<crate::model::Scene>,
    pub main: Main,
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
            main: Main::default(),
        }
    }
}

pub(super) fn migrate(old: Project) -> v6::Project {
    let mut project = v6::Project {
        tempo: old.tempo,
        time_signature: old.time_signature,
        global_quantisation: old.global_quantisation,
        follow_actions_enabled: old.follow_actions_enabled,
        random_seed: old.random_seed,
        next_colour_index: old.next_colour_index,
        tracks: old.tracks.into_iter().map(Track::into_v6).collect(),
        outputs: old.outputs.into_iter().map(Track::into_v6).collect(),
        scenes: old.scenes,
        main: v6::Track::shell(model::Track::new_unset_main(TrackId(0))),
    };
    project.renumber_colliding_outputs();
    project.main.id = TrackId(project.next_strip_id());
    project.main.delay_ms = old.main.delay_ms;
    project
}
