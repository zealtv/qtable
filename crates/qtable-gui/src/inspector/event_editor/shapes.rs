//! Insert Shape and Simplify Envelope on the clip editor's content selection.
//!
//! A shape goes in as a generated range copy pasted by `regions::paste`, so it is joined to the
//! envelope as any pasted range is and reaches the engine as one `ReplaceRange`: one revision,
//! one undo step, on however many lanes. Simplify submits one `ReplaceRange` over the lanes it
//! thinned. The curve maths lives in the engine (`shape_points`, `simplify_points`); this file
//! chooses the lanes and the span and hands the result on. The context menu and the canvases
//! reach it through `Action::InsertShape` and `Action::Simplify`.
use super::regions::{self, Clipboard};
use super::*;
use crate::inspector::draw::OFF_CELL;
use crate::inspector::lane_model::{EnvelopeRow, LaneModel, RowKind};
use crate::status::Severity;
use qtable_engine::edit::{LaneContent, LaneKey, PointReplacement};
use qtable_engine::model::{
    Axis, Link, MIN_SHAPE_SPAN, Shape, shape_points, simplify_points, simplify_tolerance,
};
use std::ops::Range;
use std::rc::Rc;

/// Where a shape goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Place {
    /// The content selection's time range, on every selected lane that takes shapes.
    Selection,
    /// One grid step from this time (1/16 with Grid Off), on this one lane, whatever the
    /// selection holds. The step starts exactly at the time and is not snapped again.
    Cell(BeatTime, lanes::Identity),
}

/// The envelope row of `target`: the clip's own lane, or else a fresh one for an argument
/// the clip has no lane for yet. `None` when the track no longer defines the argument.
fn row_of(
    app: &App,
    model: &LaneModel,
    key: ClipKey,
    target: &ArgumentRef,
) -> Option<Rc<EnvelopeRow>> {
    model
        .rows
        .iter()
        .find_map(|row| match &row.kind {
            RowKind::Envelope(e) if e.target == *target => Some(Rc::clone(e)),
            _ => None,
        })
        .or_else(|| {
            app.project()
                .track(key.track)?
                .argument_spec(target)
                .map(|_| Rc::new(EnvelopeRow::of_argument(app.project(), key.track, target)))
        })
}

/// The argument's stored value, the one its dotted line shows.
fn stored_value(app: &App, row: &EnvelopeRow) -> f64 {
    app.project()
        .field(&row.field)
        .map_or(row.min, |field| field.value)
}

/// What the lane arrives with at `start` and leaves with at `end`; for a lane with no points,
/// the argument's stored value.
fn link(app: &App, row: &EnvelopeRow, start: BeatTime, end: BeatTime) -> Link {
    let stored = stored_value(app, row);
    Link {
        before: regions::edge(&row.points, start, row.axis).map_or(stored, |e| e.arriving),
        after: regions::edge(&row.points, end, row.axis).map_or(stored, |e| e.leaving),
    }
}

/// On a lane with no points the curve outside the span is the stored value (the dotted line), so
/// the shape is entered from it and left back to it, with a jump at each boundary, as on a lane
/// with points (`regions::seam`) and as Live does. An end that already meets the stored value
/// needs no jump.
fn hold_stored_value(points: &mut Vec<Breakpoint>, stored: f64, span: u64, axis: Axis) {
    let stored = if axis == Axis::Stepped {
        stored.round()
    } else {
        stored
    };
    let held = |time: u64| Breakpoint {
        time: BeatTime(time),
        value: stored,
        curve: 0.0,
    };
    if points.first().is_some_and(|p| p.value != stored) {
        points.insert(0, held(0));
    }
    if points.last().is_some_and(|p| p.value != stored) {
        points.push(held(span));
    }
}

/// Insert `shape` at `place`. The caller has checked the clip and the project revision.
pub(super) fn insert(app: &mut App, key: ClipKey, shape: Shape, place: Place) {
    let Some(clip) = clip_owned(app, key) else {
        return;
    };
    let cell = matches!(place, Place::Cell(..));
    let (start, span, ids) = match place {
        Place::Selection => {
            let Some(selection) = app
                .clip_view
                .editor
                .content
                .as_ref()
                .filter(|s| s.key == key && s.picks.is_none() && !s.range.is_empty())
            else {
                return;
            };
            let (start, end) = (selection.range.start, selection.range.end);
            (start, end.ticks() - start.ticks(), selection.lanes.clone())
        }
        Place::Cell(time, lane) => {
            let step = match app.clip_view.editor.grid.ticks() {
                1 => OFF_CELL,
                cell => cell,
            };
            // Like Draw Mode's strokes, a step stops at the editor's extent: the end of what
            // the lanes show, which is not the clip's loop length when the loop starts later
            // than the beginning or the content runs past it.
            let extent =
                crate::inspector::clip_markers::extent(&clip, app.project().time_signature);
            let end = time.ticks().saturating_add(step).min(extent.ticks());
            (time, end.saturating_sub(time.ticks()), vec![lane])
        }
    };
    if span < MIN_SHAPE_SPAN {
        // A grid step has no selection to be too short: say what is wrong with where it starts.
        let message = if cell {
            "Too close to the clip's end for a shape"
        } else {
            "Selection too short for a shape"
        };
        app.status.show(message, Severity::Notice);
        return;
    }
    let end = BeatTime(start.ticks() + span);
    let model = LaneModel::build(app.project(), key.track, &clip);
    let lanes: Vec<_> = ids
        .iter()
        .filter_map(|id| match id {
            lanes::Identity::Envelope(target) => row_of(app, &model, key, target),
            lanes::Identity::Event(_) => None,
        })
        // Choice and switch lanes hold a value; they take no shape.
        .filter(|row| row.axis != Axis::Discrete)
        .map(|row| {
            let mut points = shape_points(
                shape,
                span,
                row.min,
                row.max,
                row.axis,
                link(app, &row, start, end),
            );
            if row.points.is_empty() {
                hold_stored_value(&mut points, stored_value(app, &row), span, row.axis);
            }
            (
                LaneKey::Envelope(row.target.clone()),
                LaneContent::Points(points),
            )
        })
        .collect();
    if lanes.is_empty() {
        return;
    }
    let shape = Clipboard {
        duration: span,
        lanes,
        ranged: true,
    };
    regions::paste(app, key, start, shape, false);
}

/// The lanes Simplify would thin and the time range that limits it, from the content
/// selection (or else the lanes a header selection names).
struct Scope {
    /// A time selection's range; `None` takes each envelope whole.
    time: Option<Range<BeatTime>>,
    rows: Vec<Rc<EnvelopeRow>>,
    selection: Option<content::Selection>,
}

fn scope(app: &App, key: ClipKey) -> Scope {
    let editor = &app.clip_view.editor;
    let selection = editor.content.clone().filter(|s| s.key == key);
    let ids: Vec<lanes::Identity> = match (&selection, editor.lanes.as_ref()) {
        (Some(selection), _) => selection.lanes.clone(),
        (None, Some(headers)) if headers.key == key => headers.items.clone(),
        _ => vec![],
    };
    let time = selection
        .as_ref()
        .filter(|s| s.picks.is_none() && !s.range.is_empty())
        .map(|s| s.range.clone());
    let one_lane = editor
        .views
        .choice(app.project(), key.track, key.id)
        .one_lane()
        .is_some();
    let rows = clip_owned(app, key)
        .map(|clip| {
            let model = LaneModel::build(app.project(), key.track, &clip);
            ids.iter()
                .filter_map(|id| match id {
                    lanes::Identity::Envelope(target) => row_of(app, &model, key, target),
                    lanes::Identity::Event(_) => None,
                })
                // An unlinked lane has its own timeline: only its One lane view edits it.
                .filter(|row| row.region.is_none() || one_lane)
                .collect()
        })
        .unwrap_or_default();
    Scope {
        time,
        rows,
        selection,
    }
}

/// Whether Simplify has anything to look at: some lane it reaches holds three points or more.
pub(crate) fn can_simplify(app: &App) -> bool {
    let Some((track, slot, id)) = app.clip_view.editor.owner else {
        return false;
    };
    let key = ClipKey {
        track,
        slot,
        id,
        epoch: app.clip_view.editor.epoch,
    };
    scope(app, key).rows.iter().any(|row| row.points.len() >= 3)
}

/// Thin the envelopes the selection names: the points strictly inside a time selection, or
/// else every point but each lane's first and last. One `ReplaceRange` (`Exact`) over the
/// lanes that lost points, and the selection stays. The status bar counts those lanes'
/// points before and after. The caller has checked the clip and the
/// project revision.
pub(super) fn simplify(app: &mut App, key: ClipKey) {
    let scope = scope(app, key);
    let whole = BeatTime(0)..BeatTime(u64::MAX);
    let inside = scope.time.clone().unwrap_or(whole);
    let mut changed = Vec::new();
    for row in &scope.rows {
        let tolerance = simplify_tolerance(row.threshold, row.min, row.max, row.axis);
        let thinned = simplify_points(&row.points, inside.clone(), tolerance, row.axis);
        if thinned.len() < row.points.len() {
            changed.push((row, thinned));
        }
    }
    let extent = changed
        .iter()
        .filter_map(|(row, _)| Some((row.points.first()?.time, row.points.last()?.time)))
        .fold(None, |all: Option<(BeatTime, BeatTime)>, (first, last)| {
            Some(all.map_or((first, last), |(a, b)| (a.min(first), b.max(last))))
        });
    let Some((first, last)) = extent else {
        app.status.show("Nothing to simplify", Severity::Notice);
        return;
    };
    // A time selection replaces exactly its range; otherwise the range covers every point of
    // the thinned lanes.
    let range = scope
        .time
        .clone()
        .unwrap_or(first..BeatTime(last.ticks().saturating_add(1)));
    let (mut before, mut after) = (0, 0);
    let lanes: Vec<_> = changed
        .into_iter()
        .map(|(row, thinned)| {
            before += row.points.len();
            after += thinned.len();
            let kept: Vec<_> = thinned
                .into_iter()
                .filter(|p| range.contains(&p.time))
                .collect();
            (
                LaneKey::Envelope(row.target.clone()),
                LaneContent::Points(kept),
            )
        })
        .collect();
    let selection = scope.selection.unwrap_or_else(|| {
        content::Selection::time(
            key,
            range.start..range.start,
            scope
                .rows
                .iter()
                .map(|row| lanes::Identity::Envelope(row.target.clone()))
                .collect(),
        )
    });
    let revision = app.project_revision();
    regions::submit(app, key, range, lanes, PointReplacement::Exact, selection);
    if app.project_revision() != revision {
        app.status.show(
            format!("Simplified {before} points to {after}"),
            Severity::Plain,
        );
    }
}

#[cfg(test)]
mod tests;
