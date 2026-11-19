//! Frozen format 11 project container. Format 12 adds only to the mapping types (the `Gate` mode,
//! the `ValueScaling` takeover, a defaulted `sensitivity` and the `EventArgument` control), all
//! additive, so this keeps the project's fields as format 11 wrote them and the step to the
//! current model is the identity: nothing is renumbered, reordered or rewritten, and a set
//! saved again differs only by its version line.
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
    pub mappings: Vec<model::RemoteMapping>,
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
            mappings: new.mappings,
        }
    }
}

/// The one place a format-11 set meets the current model.
pub(super) fn migrate(old: Project) -> model::Project {
    model::Project {
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
        mappings: old.mappings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format11_keeps_its_wire_shape() {
        assert!(ron::from_str::<Project>("()").is_ok());
        assert!(ron::from_str::<Project>("(mappings:[])").is_ok());
        assert!(ron::from_str::<Project>("(future:true)").is_err());
    }
}
