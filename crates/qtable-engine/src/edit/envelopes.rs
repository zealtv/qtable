//! Envelope edits and definition conformance, shared by every command client.
use super::EditReport;
use crate::model::*;

#[derive(Clone, Debug, PartialEq)]
pub enum EnvelopeEdit {
    Add {
        target: ArgumentRef,
    },
    Remove {
        target: ArgumentRef,
    },
    /// Move to an index in the complete mixed lane list.
    Move {
        target: ArgumentRef,
        to: usize,
    },
    /// Replace points atomically; stable argument identity survives lane reordering.
    SetPoints {
        target: ArgumentRef,
        points: Vec<Breakpoint>,
    },
}

pub(super) fn apply(
    project: &mut Project,
    track: TrackId,
    slot: usize,
    edit: EnvelopeEdit,
) -> Result<EditReport, ModelError> {
    let clip = project
        .track_mut(track)
        .and_then(|t| t.slots.get_mut(slot))
        .and_then(|s| s.clip.as_mut())
        .ok_or_else(|| ModelError("clip not found".into()))?;
    let target = match &edit {
        EnvelopeEdit::Add { target }
        | EnvelopeEdit::Remove { target }
        | EnvelopeEdit::SetPoints { target, .. }
        | EnvelopeEdit::Move { target, .. } => target,
    };
    let index = clip
        .lanes
        .iter()
        .position(|l| l.envelope().is_some_and(|e| e.target == *target));
    if matches!(
        edit,
        EnvelopeEdit::Remove { .. } | EnvelopeEdit::Move { .. }
    ) {
        let index = index.ok_or_else(|| ModelError("envelope not found".into()))?;
        return match edit {
            EnvelopeEdit::Remove { .. } => {
                super::lane_list::remove(project, track, slot, vec![index])
            }
            EnvelopeEdit::Move { to, .. } => {
                super::lane_list::move_lanes(project, track, slot, index..index + 1, to)
            }
            _ => unreachable!(),
        };
    }
    match edit {
        EnvelopeEdit::Add { target } => {
            if index.is_some() {
                return Err(ModelError(
                    "another lane already automates that argument".into(),
                ));
            }
            clip.lanes.push(Lane::Envelope(AutomationEnvelope {
                region: None,
                target,
                points: vec![],
            }));
        }
        EnvelopeEdit::Remove { .. } | EnvelopeEdit::Move { .. } => unreachable!("handled above"),
        EnvelopeEdit::SetPoints { points, .. } => {
            let Lane::Envelope(envelope) =
                &mut clip.lanes[index.ok_or_else(|| ModelError("envelope not found".into()))?]
            else {
                unreachable!()
            };
            envelope.points = points;
        }
    }
    Ok(EditReport::default())
}

fn definition<'a>(track: &'a Track, target: &ArgumentRef) -> Option<&'a FieldDef> {
    Some(track.argument(target)?.2)
}

/// Called only for definition edits/deletions; ordinary point edits must fail validation,
/// rather than silently conforming invalid user input.
pub(super) fn conform(before: &Project, project: &mut Project, report: &mut EditReport) {
    for track in &mut project.tracks {
        let Some(old_track) = before.track(track.id) else {
            continue;
        };
        let rules: Vec<_> = track
            .slots
            .iter()
            .filter_map(|s| s.clip.as_ref())
            .flat_map(|c| c.lanes.iter().filter_map(Lane::envelope))
            .map(|e| {
                (
                    e.target.clone(),
                    track.argument_spec(&e.target),
                    definition(old_track, &e.target).cloned(),
                    definition(track, &e.target).cloned(),
                )
            })
            .collect();
        for clip in track.slots.iter_mut().filter_map(|s| s.clip.as_mut()) {
            let mut removed = 0;
            clip.lanes.retain_mut(|lane| {
                let Lane::Envelope(e) = lane else { return true };
                let (_, spec, old, new) = rules
                    .iter()
                    .find(|(t, ..)| *t == e.target)
                    .expect("collected above");
                let Some(_spec) = spec else {
                    removed += 1;
                    return false;
                };
                if old == new {
                    return true;
                }
                if let Some(new) = new {
                    conform_points(e, old.as_ref(), new, report);
                }
                true
            });
            report.removed(removed, 0);
        }
    }
}

/// Definition conversion shared by edits and cross-track adoption.
pub(super) fn conform_points(
    envelope: &mut AutomationEnvelope,
    old: Option<&FieldDef>,
    new: &FieldDef,
    report: &mut EditReport,
) {
    if old.is_some_and(|old| {
        old.value_type == new.value_type
            && old.control == new.control
            && old.range == new.range
            && old.options == new.options
    }) {
        return;
    }
    let spec = new.spec();
    for point in &mut envelope.points {
        let before = point.clone();
        let value = if let Some(old) =
            old.filter(|o| o.control == Control::Enum && new.control == Control::Enum)
        {
            old.options
                .get(point.value as usize)
                .and_then(|o| new.options.iter().position(|n| n.value == o.value))
                .map_or_else(|| new.default_value(), |i| i as f64)
        } else if new.control == Control::Toggle
            && old.is_some_and(|o| o.control != Control::Toggle)
        {
            let old = old.unwrap().spec();
            f64::from(point.value >= old.min + (old.max - old.min) / 2.0)
        } else {
            point.value
        };
        point.value = spec
            .check(value.clamp(spec.min, spec.max))
            .expect("clamped domain");
        if new.axis() != Axis::Continuous {
            point.curve = 0.0;
        }
        report.converted_points += usize::from(*point != before);
    }
}
