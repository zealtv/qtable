//! Track-like edits for the clip's one mixed lane list. Content adoption stays in rehome.
use super::{Carried, EditReport, rehome};
use crate::model::*;
use std::ops::Range;

fn err(message: &str) -> ModelError {
    ModelError(message.into())
}

pub(super) fn get_clip(
    project: &mut Project,
    track: TrackId,
    slot: usize,
) -> Result<&mut Clip, ModelError> {
    project
        .clip_mut(track, slot)
        .ok_or_else(|| err("clip not found"))
}

/// Old clips can have hits without declared lanes. Preserve their first-appearance
/// order after the declared mixed list before applying an indexed list edit.
pub(super) fn declare_events(clip: &mut Clip) {
    for definition in clip.lane_refs() {
        if !clip
            .lanes
            .iter()
            .any(|l| l.event().is_some_and(|l| l.definition == definition))
        {
            clip.lanes.push(Lane::Events(EventLane { definition }));
        }
    }
}

pub(super) fn move_lanes(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    from: Range<usize>,
    to: usize,
) -> Result<EditReport, ModelError> {
    let clip = get_clip(project, track, slot)?;
    declare_events(clip);
    super::move_range(&mut clip.lanes, from, to)?;
    Ok(EditReport::default())
}

pub(super) fn remove(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    mut indices: Vec<usize>,
) -> Result<EditReport, ModelError> {
    let clip = get_clip(project, track, slot)?;
    declare_events(clip);
    indices.sort_unstable();
    indices.dedup();
    if indices.iter().any(|i| *i >= clip.lanes.len()) {
        return Err(err("lane index out of range"));
    }
    let mut report = EditReport::default();
    let hits = clip.events.len();
    for index in indices.iter().rev() {
        match clip.lanes.remove(*index) {
            Lane::Events(l) => clip.events.retain(|e| e.definition != l.definition),
            Lane::Envelope(e) => report.removed_points += e.points.len(),
        }
    }
    report.removed(indices.len(), hits - clip.events.len());
    Ok(report)
}

pub(super) fn paste(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    at: usize,
    lanes: Vec<Lane>,
    hits: Vec<Event>,
    carried: Vec<Carried>,
) -> Result<EditReport, ModelError> {
    let mut target = get_clip(project, track, slot)?.clone();
    declare_events(&mut target);
    if at > target.lanes.len() {
        return Err(err("lane index out of range"));
    }
    if lanes.is_empty() {
        return Err(err("nothing to paste"));
    }
    if hits.iter().any(|hit| {
        !lanes
            .iter()
            .any(|l| l.event().is_some_and(|l| l.definition == hit.definition))
    }) {
        return Err(err("pasted hit has no copied event lane"));
    }
    // A temporary clip lets lane copies use exactly the same adoption, conformance
    // and within-copy duplicate resolution as whole clip copies.
    let mut copy = target.clone();
    copy.lanes = lanes;
    copy.events = hits;
    let mut report = rehome::rehome_clip(project, track, &mut copy, &carried)?;
    // Match clip-paste resolution: one event lane with distinct merged hits; first envelope
    // wins. Existing destination envelopes have priority regardless of insertion index.
    rehome::retain_unique_lanes(&mut copy, &target.lanes, &target.events, &mut report);
    if report.removed_lanes + report.removed_hits + report.removed_points > 0 {
        report.removed_in_clips = 1;
    }
    target.lanes.splice(at..at, copy.lanes);
    target.events.extend(copy.events);
    target.events.sort_by_key(|e| e.position);
    *get_clip(project, track, slot)? = target;
    Ok(report)
}

#[cfg(test)]
mod tests;
