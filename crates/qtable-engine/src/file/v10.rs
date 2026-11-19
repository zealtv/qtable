//! Frozen format 10 project container. Track, clip and mapping containers are unchanged in
//! format 11, so this keeps the mapping list and the step to the format-11 container is the
//! identity (the nested-group format of 2026-11-06). The model fields keep their wire
//! shape; nesting is the only thing format 11 widens.
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

/// The one place a pre-11 set meets the format-11 container. Format 10 readers accepted only flat
/// groups, so the set keeps that rule here, however far the current format opens nesting;
/// nothing is rewritten (ids, order, routes, folds, slots and mappings pass through).
pub(super) fn migrate(old: Project) -> Result<super::v11::Project, model::ModelError> {
    model::validate_flat_groups(&old.tracks)?;
    Ok(super::v11::Project {
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format10_keeps_its_wire_shape() {
        assert!(ron::from_str::<Project>("()").is_ok());
        assert!(ron::from_str::<Project>("(mappings:[])").is_ok());
        assert!(ron::from_str::<Project>("(future:true)").is_err());
    }
}
