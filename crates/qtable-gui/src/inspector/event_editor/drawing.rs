//! Draw Mode's commits: each finished stroke becomes exactly one `LaneEdit::ReplaceRange`.
use super::*;
use crate::inspector::draw::Replacement;
use qtable_engine::edit::{LaneContent, LaneKey, PointReplacement};
use qtable_engine::model::{EventRef, OscArgument, Project};
use std::ops::Range;

/// A finished stroke, at the clip and revision it was started against.
#[derive(Debug, Clone)]
pub(crate) enum Draw {
    Points {
        lane: PointTarget,
        replacement: Replacement,
    },
    Hits {
        key: ClipKey,
        revision: u64,
        lane: EventRef,
        span: Range<BeatTime>,
        positions: Vec<BeatTime>,
    },
    Erase(HitTarget),
}

fn current(app: &App, key: ClipKey, revision: u64) -> bool {
    app.chrome.detail_open
        && app.selection.inspector_tab == crate::inspector_panel::Tab::Clip
        && active(app, key)
        && revision == app.project_revision()
}

fn replace(app: &mut App, key: ClipKey, range: Range<BeatTime>, lane: (LaneKey, LaneContent)) {
    run_edit(
        app,
        Command::Edit(ProjectEdit::EditLane {
            track: key.track,
            slot: key.slot,
            edit: LaneEdit::ReplaceRange {
                range,
                lanes: vec![lane],
                point_policy: PointReplacement::KeepEdges,
            },
        }),
    );
}

pub(super) fn commit(app: &mut App, draw: Draw) {
    match draw {
        Draw::Points { lane, replacement } => {
            let known = app
                .project()
                .track(lane.key.track)
                .is_some_and(|t| t.argument_spec(&lane.target).is_some());
            if current(app, lane.key, lane.revision) && known {
                replace(
                    app,
                    lane.key,
                    replacement.range,
                    (
                        LaneKey::Envelope(lane.target),
                        LaneContent::Points(replacement.points),
                    ),
                );
            }
        }
        Draw::Hits {
            key,
            revision,
            lane,
            span,
            positions,
        } if current(app, key, revision) => {
            let Some(clip) = clip_owned(app, key) else {
                return;
            };
            let mut hits: Vec<Event> = clip
                .events
                .iter()
                .filter(|e| e.definition == lane && span.contains(&e.position))
                .cloned()
                .collect();
            // Each new hit copies from the nearest earlier hit, the ones just painted included.
            let mut known = clip.events.clone();
            for position in positions {
                let event = Event {
                    position,
                    arguments: new_arguments(app.project(), key.track, &known, &lane, position),
                    definition: lane.clone(),
                };
                known.push(event.clone());
                hits.push(event);
            }
            hits.sort_by_key(|e| e.position);
            replace(
                app,
                key,
                span,
                (LaneKey::Events(lane), LaneContent::Hits(hits)),
            );
        }
        Draw::Erase(target) if hits::valid_target(app, target) => {
            let Some(clip) = clip_owned(app, target.key) else {
                return;
            };
            let Some(gone) = clip.events.get(target.index) else {
                return;
            };
            let (lane, at) = (gone.definition.clone(), gone.position);
            let rest: Vec<Event> = clip
                .events
                .iter()
                .enumerate()
                .filter(|(i, e)| *i != target.index && e.definition == lane && e.position == at)
                .map(|(_, e)| e.clone())
                .collect();
            content::clear(&mut app.clip_view.editor);
            replace(
                app,
                target.key,
                at..BeatTime(at.ticks() + 1),
                (LaneKey::Events(lane), LaneContent::Hits(rest)),
            );
        }
        _ => {}
    }
}

/// The arguments of a new hit at `position`: those of the lane's nearest earlier hit, as Live
/// reuses the last velocity, or else its event's field defaults.
pub(super) fn new_arguments(
    project: &Project,
    track: TrackId,
    events: &[Event],
    lane: &EventRef,
    position: BeatTime,
) -> Vec<OscArgument> {
    events
        .iter()
        .filter(|e| e.definition == *lane && e.position <= position)
        .max_by_key(|e| e.position)
        .map_or_else(
            || {
                project
                    .track(track)
                    .and_then(|t| t.event_def(lane))
                    .map(|(_, d)| d.fields.iter().map(|field| field.default.clone()).collect())
                    .unwrap_or_default()
            },
            |e| e.arguments.clone(),
        )
}

#[cfg(test)]
mod tests;

/// Unattended capture: Draw Mode on, and for `steps` or `hits` a real stroke committed through
/// the ordinary action handler.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture(app: &mut App, scenario: &str) {
    use crate::inspector::draw::{HitStroke, Outcome, Painting};
    use crate::inspector::lane_model::{LaneModel, RowKind};
    use crate::inspector::point_gesture::Keys;
    use iced::Point;
    reconcile(app);
    app.clip_view.editor.draw.on = true;
    let Some((track, slot, id)) = app.clip_view.editor.owner else {
        return;
    };
    let key = ClipKey {
        track,
        slot,
        id,
        epoch: app.clip_view.editor.epoch,
    };
    let Some(clip) = clip_owned(app, key) else {
        return;
    };
    let model = LaneModel::build(app.project(), track, &clip);
    let revision = app.project_revision();
    let draw = match scenario {
        "steps" => model.rows.iter().find_map(|row| {
            let RowKind::Envelope(e) = &row.kind else {
                return None;
            };
            let plot = crate::inspector::point_gesture::Plot {
                left: 0.0,
                width: 400.0,
                top: 0.0,
                height: 100.0,
            };
            let lane = e.lane(plot, clip.length(), 240);
            let keys = Keys::default();
            let at =
                |cell: f32, fraction: f32| Point::new(cell * 25.0 + 5.0, 100.0 * (1.0 - fraction));
            let mut painting = Painting::envelope(&lane, e.threshold, keys, at(0.0, 0.3));
            for (cell, fraction) in [
                (1.0, 0.3),
                (2.0, 0.8),
                (3.0, 0.8),
                (4.0, 0.5),
                (5.0, 0.1),
                (6.0, 0.9),
            ] {
                painting.push(Some(&lane), at(cell, fraction), 0.0, keys);
            }
            let Some(Outcome::Points(replacement)) = painting.finish() else {
                return None;
            };
            Some(Draw::Points {
                lane: PointTarget {
                    key,
                    revision,
                    target: e.target.clone(),
                },
                replacement,
            })
        }),
        "hits" => model.rows.iter().find_map(|row| {
            let RowKind::Event { reference, .. } = &row.kind else {
                return None;
            };
            let existing = clip
                .events
                .iter()
                .filter(|e| &e.definition == reference)
                .map(|e| e.position);
            let stroke = HitStroke::new(240, clip.length(), existing);
            let mut painting = Painting::hits(stroke, 1930.0, None, Point::ORIGIN);
            painting.push(None, Point::new(200.0, 0.0), 3100.0, Keys::default());
            let Some(Outcome::Hits { span, positions }) = painting.finish() else {
                return None;
            };
            Some(Draw::Hits {
                key,
                revision,
                lane: reference.clone(),
                span,
                positions,
            })
        }),
        _ => None,
    };
    if let Some(draw) = draw {
        commit(app, draw);
    }
    app.focus = crate::focus::Focus::Detail;
}
