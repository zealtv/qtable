//! Content that crosses tracks brings the definitions it plays (target-device design,
//! decision 10). An unresolved ref is adopted: it goes to the first event on the new track
//! with the same path and signature, or to a copy added to that track's first Target. A ref
//! that resolves to an event with another path than the one it carries is adopted too: ids
//! are reused and every set numbers its devices from 1, so a ref alone can name another event.
use super::EditReport;
use crate::model::*;

/// A definition that travels with content from another track, keyed by its ref there.
#[derive(Clone, Debug, PartialEq)]
pub enum Carried {
    Event {
        from: EventRef,
        definition: EventDef,
    },
    Parameter {
        from: ArgumentRef,
        definition: ParameterDef,
    },
}

impl Carried {
    /// The definitions `clip` plays on `track`, to carry with a copy of it.
    pub fn from_clip(track: &Track, clip: &Clip) -> Vec<Carried> {
        clip.lane_refs()
            .into_iter()
            .filter_map(|from| {
                let (_, definition) = track.event_def(&from)?;
                Some(Carried::Event {
                    definition: definition.clone(),
                    from,
                })
            })
            .chain(
                clip.lanes
                    .iter()
                    .filter_map(Lane::envelope)
                    .filter_map(|e| {
                        let definition = parameter_definition(track, &e.target)?;
                        Some(Carried::Parameter {
                            from: e.target.clone(),
                            definition: definition.clone(),
                        })
                    }),
            )
            .collect()
    }
}

/// Rewrite every ref in `clip` that doesn't resolve on `track`, or resolves to another path
/// than its carried one, using the carried definitions. Hits of the refs kept conform to their
/// event's current signature, and the report counts those it converted. An unresolved ref
/// with no carried definition rejects the edit.
pub(crate) fn rehome_clip(
    project: &mut Project,
    track: TrackId,
    clip: &mut Clip,
    carried: &[Carried],
) -> Result<EditReport, ModelError> {
    let device = project.next_device_id();
    let track = project
        .track_mut(track)
        .ok_or_else(|| ModelError("unknown track".into()))?;
    let mut report = EditReport::default();
    let mut adopted: Vec<(EventRef, EventRef)> = Vec::new();
    let mut kept: Vec<(EventRef, Vec<FieldType>)> = Vec::new();
    for reference in clip.lane_refs() {
        let carried = carried.iter().find_map(|c| match c {
            Carried::Event { from, definition } if *from == reference => Some(definition),
            _ => None,
        });
        if let Some((_, current)) = track.event_def(&reference)
            && carried.is_none_or(|c| c.path == current.path)
        {
            kept.push((reference, current.signature().collect()));
            continue;
        }
        let definition = carried
            .ok_or_else(|| ModelError("the clip plays an event this track doesn't have".into()))?
            .clone();
        let new = match track.find_event_like(&definition) {
            Some(new) => new,
            None => track.add_event(definition, || device),
        };
        adopted.push((reference, new));
    }
    let renew = |reference: &mut EventRef| {
        if let Some((_, new)) = adopted.iter().find(|(old, _)| old == reference) {
            *reference = new.clone();
        }
    };
    clip.lanes
        .iter_mut()
        .filter_map(crate::model::Lane::event_mut)
        .for_each(|l| renew(&mut l.definition));
    report.adopted_lanes += adopted.len();
    for event in &mut clip.events {
        if let Some((_, signature)) = kept.iter().find(|(kept, _)| *kept == event.definition) {
            report.converted(usize::from(event.conform(signature)), signature);
        }
        renew(&mut event.definition);
    }
    for envelope in clip.lanes.iter_mut().filter_map(|l| match l {
        Lane::Envelope(e) => Some(e),
        _ => None,
    }) {
        if envelope.target.device == DeviceRef::Mixer {
            continue;
        }
        let source = carried.iter().find_map(|c| match c {
            Carried::Parameter { from, definition } if *from == envelope.target => Some(definition),
            _ => None,
        });
        if source.is_some_and(|p| p.field(&envelope.target.field).is_none()) {
            return Err(ModelError(
                "carried parameter is missing the automated argument".into(),
            ));
        }
        let current = parameter_definition(track, &envelope.target);
        if current.is_some_and(|p| source.is_none_or(|s| s.path == p.path))
            && track.argument_spec(&envelope.target).is_some()
        {
            let new = current.unwrap().field(&envelope.target.field).unwrap();
            let old = source.and_then(|p| p.field(&envelope.target.field));
            super::envelopes::conform_points(envelope, old, new, &mut report);
            continue;
        }
        let source = source.ok_or_else(|| {
            ModelError("the clip automates an argument this track doesn't have".into())
        })?;
        let found = track.targets().find_map(|(d, t)| {
            t.parameters
                .iter()
                .find(|p| p.path == source.path && p.field(&envelope.target.field).is_some())
                .map(|p| (d.id, p.key.clone()))
        });
        let (id, key) = found.unwrap_or_else(|| {
            let d = track.first_target_mut_or_insert(|| device);
            let t = d
                .target_mut()
                .expect("first_target_mut_or_insert returns a Target");
            let key = t.fresh_key(&source.path);
            t.parameters.push(ParameterDef {
                key: key.clone(),
                ..source.clone()
            });
            (d.id, key)
        });
        report.adopted_lanes += 1;
        let old = source.field(&envelope.target.field);
        envelope.target.device = DeviceRef::Chain(id);
        envelope.target.parameter = ParameterKey(key.0);
        let new = parameter_definition(track, &envelope.target)
            .unwrap()
            .field(&envelope.target.field)
            .unwrap();
        super::envelopes::conform_points(envelope, old, new, &mut report);
    }
    retain_unique_lanes(clip, &[], &[], &mut report);
    Ok(report)
}

/// One lane per event or argument. Coalesced event lanes retain distinct hits;
/// the first envelope wins. Existing destination content takes priority.
pub(super) fn retain_unique_lanes(
    clip: &mut Clip,
    existing: &[Lane],
    existing_hits: &[Event],
    report: &mut EditReport,
) {
    let mut events: Vec<_> = existing
        .iter()
        .filter_map(Lane::event)
        .map(|l| l.definition.clone())
        .collect();
    let mut arguments: Vec<_> = existing
        .iter()
        .filter_map(Lane::envelope)
        .map(|e| e.target.clone())
        .collect();
    let mut coalesced_events = Vec::new();
    clip.lanes.retain(|lane| {
        let duplicate = match lane {
            Lane::Events(l) => {
                let duplicate = events.contains(&l.definition);
                events.push(l.definition.clone());
                if duplicate {
                    coalesced_events.push(l.definition.clone());
                }
                duplicate
            }
            Lane::Envelope(e) => {
                let duplicate = arguments.contains(&e.target);
                arguments.push(e.target.clone());
                if duplicate {
                    report.removed_points += e.points.len();
                }
                duplicate
            }
        };
        report.removed_lanes += usize::from(duplicate);
        !duplicate
    });
    // Compare after adoption/conformance, when distinct source refs may name the
    // same destination event. Preserve existing hits, even deliberate repeats.
    if !coalesced_events.is_empty() {
        let mut kept = existing_hits.to_vec();
        clip.events.retain(|hit| {
            if !coalesced_events.contains(&hit.definition) {
                return true;
            }
            if kept.contains(hit) {
                report.removed_hits += 1;
                false
            } else {
                kept.push(hit.clone());
                true
            }
        });
    }
    if report.removed_lanes + report.removed_hits > 0 {
        report.removed_in_clips = 1;
    }
}

fn parameter_definition<'a>(track: &'a Track, target: &ArgumentRef) -> Option<&'a ParameterDef> {
    Some(track.argument_parameter(target)?.1)
}
