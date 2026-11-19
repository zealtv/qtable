//! Shared atomic content write for regions, drawing and later recording.
use super::{EditReport, LaneContent, LaneKey, PointReplacement, lane_list};
use crate::model::*;
use std::{collections::HashSet, ops::Range};

pub(super) fn apply(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    range: Range<BeatTime>,
    lanes: Vec<(LaneKey, LaneContent)>,
    point_policy: PointReplacement,
) -> Result<EditReport, ModelError> {
    if range.start >= range.end {
        return Err(ModelError("replacement range must be nonempty".into()));
    }
    let mut seen = HashSet::new();
    for (key, content) in &lanes {
        if !seen.insert(key) {
            return Err(ModelError("replacement names a lane more than once".into()));
        }
        match (key, content) {
            (LaneKey::Events(reference), LaneContent::Hits(hits)) => {
                if hits
                    .iter()
                    .any(|h| h.definition != *reference || !range.contains(&h.position))
                {
                    return Err(ModelError(
                        "replacement hits must match their lane and lie inside the range".into(),
                    ));
                }
                if !hits.windows(2).all(|w| w[0].position <= w[1].position) {
                    return Err(ModelError("replacement hits must be sorted".into()));
                }
            }
            (LaneKey::Envelope(target), LaneContent::Points(points)) => {
                if points.iter().any(|p| !range.contains(&p.time)) {
                    return Err(ModelError(
                        "replacement points must lie inside the range".into(),
                    ));
                }
                if !points.windows(2).all(|w| w[0].time <= w[1].time) {
                    return Err(ModelError("replacement points must be sorted".into()));
                }
                // Validate incoming points before coalescing: an overwritten jump member
                // must not hide invalid values, curves or an oversized incoming jump.
                AutomationEnvelope::validate_points(points)?;
                if let Some(track) = project.track(track) {
                    track.validate_envelope_values(target, points)?;
                }
            }
            _ => {
                return Err(ModelError(
                    "replacement content must match the lane kind".into(),
                ));
            }
        }
    }
    let clip = lane_list::get_clip(project, track, slot)?;
    // Clearing the last derived hit must leave its lane in place.
    lane_list::declare_events(clip);
    for (key, content) in lanes {
        match (key, content) {
            (LaneKey::Events(definition), LaneContent::Hits(hits)) => {
                if !clip.lane_refs().contains(&definition) {
                    clip.lanes.push(Lane::Events(EventLane {
                        definition: definition.clone(),
                    }));
                }
                clip.events
                    .retain(|h| h.definition != definition || !range.contains(&h.position));
                clip.events.extend(hits);
            }
            (LaneKey::Envelope(target), LaneContent::Points(points)) => {
                let index = match clip
                    .lanes
                    .iter()
                    .position(|l| l.envelope().is_some_and(|e| e.target == target))
                {
                    Some(index) => index,
                    None => {
                        clip.lanes.push(Lane::Envelope(AutomationEnvelope {
                            region: None,
                            target,
                            points: vec![],
                        }));
                        clip.lanes.len() - 1
                    }
                };
                let Lane::Envelope(envelope) = &mut clip.lanes[index] else {
                    unreachable!()
                };
                point_policy.replace_points(&mut envelope.points, &range, points);
            }
            _ => unreachable!("kinds checked above"),
        }
    }
    clip.events.sort_by_key(|h| h.position);
    // edit::apply validates the whole candidate (refs, signatures, values and curves)
    // before publishing it. No definition adoption or value conformance here.
    Ok(EditReport::default())
}

#[cfg(test)]
mod tests;
