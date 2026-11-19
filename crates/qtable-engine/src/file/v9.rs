//! Frozen format 9 project container. Track/clip containers are unchanged in format 10.
//! The step to format 10 adds an empty mapping list, never machine-local MIDI settings.
use crate::model;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Project {
    pub tempo: model::Tempo,
    pub time_signature: model::TimeSignature,
    pub global_quantisation: model::GlobalQuantisation,
    pub follow_actions_enabled: bool,
    pub random_seed: u64,
    pub next_colour_index: usize,
    pub outputs: Vec<model::Track>,
    pub tracks: Vec<model::Track>,
    pub scenes: Vec<model::Scene>,
    pub main: model::Track,
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
            outputs: new.outputs,
            tracks: new.tracks,
            scenes: new.scenes,
            main: new.main,
        }
    }
}

pub(super) fn migrate(old: Project) -> super::v10::Project {
    super::v10::Project {
        tempo: old.tempo,
        time_signature: old.time_signature,
        global_quantisation: old.global_quantisation,
        follow_actions_enabled: old.follow_actions_enabled,
        random_seed: old.random_seed,
        next_colour_index: old.next_colour_index,
        outputs: old.outputs,
        tracks: old.tracks,
        scenes: old.scenes,
        main: old.main,
        mappings: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format9_rejects_mappings_without_a_version_bump() {
        assert!(ron::from_str::<Project>("()").is_ok());
        assert!(ron::from_str::<Project>("(mappings:[])").is_err());
    }
}
