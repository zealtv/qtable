//! Content clipboard and atomic region edits, shared by keys, menus and both canvases.
use super::*;
use crate::clip_shortcuts::Shortcut;
use crate::inspector::lane_model::{LaneModel, RowKind};
use qtable_engine::edit::{LaneContent, LaneKey, PointReplacement};
use qtable_engine::model::{Axis, curve_after};
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Clipboard {
    pub duration: u64,
    pub lanes: Vec<(LaneKey, LaneContent)>,
    /// Copied from a time range: each envelope's points start with the curve's value at the
    /// range start and end, at `duration`, with its value arriving at the range end.
    pub ranged: bool,
}

fn lane_key(identity: &lanes::Identity) -> LaneKey {
    match identity {
        lanes::Identity::Event(r) => LaneKey::Events(r.clone()),
        lanes::Identity::Envelope(t) => LaneKey::Envelope(t.clone()),
    }
}
fn identity(key: &LaneKey) -> lanes::Identity {
    match key {
        LaneKey::Events(r) => lanes::Identity::Event(r.clone()),
        LaneKey::Envelope(t) => lanes::Identity::Envelope(t.clone()),
    }
}
fn points(clip: &Clip, target: &ArgumentRef) -> Vec<Breakpoint> {
    clip.lanes
        .iter()
        .filter_map(|l| l.envelope())
        .find(|e| e.target == *target)
        .map(|e| e.points.clone())
        .unwrap_or_default()
}

/// Each envelope lane's axis in `clip`, for sampling it at a range edge.
fn axes(app: &App, key: ClipKey, clip: &Clip) -> impl Fn(&ArgumentRef) -> Axis {
    let model = LaneModel::build(app.project(), key.track, clip);
    move |target| {
        model
            .rows
            .iter()
            .find_map(|row| match &row.kind {
                RowKind::Envelope(e) if e.target == *target => Some(e.axis),
                _ => None,
            })
            .unwrap_or(Axis::Continuous)
    }
}

/// What a range edge at `time` meets on a lane: the value arriving from the left, the value
/// leaving to the right, and the curve that leaving value carries to the next point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Edge {
    pub(super) arriving: f64,
    pub(super) leaving: f64,
    pub(super) curve: f32,
}

/// `None` for a lane with no points. A cut curved segment splits there (`curve_after`).
pub(super) fn edge(points: &[Breakpoint], time: BeatTime, axis: Axis) -> Option<Edge> {
    let first = points.first()?;
    let before = points.iter().rfind(|p| p.time < time);
    let mut at = points.iter().filter(|p| p.time == time);
    if let (Some(arrives), Some(leaves)) = (at.clone().next(), at.next_back()) {
        // A Discrete line holds the previous level right up to the point.
        let arriving = match before {
            Some(held) if axis == Axis::Discrete => held.value,
            _ => arrives.value,
        };
        return Some(Edge {
            arriving,
            leaving: leaves.value,
            curve: leaves.curve,
        });
    }
    let after = points.iter().find(|p| p.time > time);
    let (value, curve) = match (before, after) {
        (Some(left), Some(right)) if axis != Axis::Discrete => {
            let fraction = (time.ticks() - left.time.ticks()) as f64
                / (right.time.ticks() - left.time.ticks()) as f64;
            (
                left.towards(right, time.ticks() as f64),
                curve_after(left.curve, fraction),
            )
        }
        (Some(left), _) => (left.value, 0.0),
        (None, _) => (first.value, 0.0),
    };
    let value = if axis == Axis::Stepped {
        value.round()
    } else {
        value
    };
    Some(Edge {
        arriving: value,
        leaving: value,
        curve,
    })
}

/// A time range's envelope content, as Live copies it: the curve's value at the start, the
/// points inside, and the value arriving at the end, so a copy between two points still
/// carries the line between them.
fn range_points(points: &[Breakpoint], range: &Range<BeatTime>, axis: Axis) -> Vec<Breakpoint> {
    let start = range.start.ticks();
    let Some(first) = edge(points, range.start, axis) else {
        return Vec::new();
    };
    let mut copied = vec![Breakpoint {
        time: BeatTime(0),
        value: first.leaving,
        curve: first.curve,
    }];
    copied.extend(
        points
            .iter()
            .filter(|p| p.time > range.start && p.time < range.end)
            .map(|p| Breakpoint {
                time: BeatTime(p.time.ticks() - start),
                ..p.clone()
            }),
    );
    if let Some(last) = edge(points, range.end, axis) {
        copied.push(Breakpoint {
            time: BeatTime(range.end.ticks() - start),
            value: last.arriving,
            curve: 0.0,
        });
    }
    copied
}

pub(super) fn copy(
    clip: &Clip,
    selection: &content::Selection,
    grid: u64,
    axis: impl Fn(&ArgumentRef) -> Axis,
) -> Clipboard {
    let start = selection.range.start.ticks();
    let duration = if selection.picks.is_some() {
        // Picked hits have no duration. Give a lone pick one grid cell, as Live's note row.
        (selection
            .range
            .end
            .ticks()
            .saturating_sub(start)
            .saturating_sub(1)
            .saturating_add(grid))
        .max(1)
    } else {
        selection.range.end.ticks().saturating_sub(start)
    };
    let lanes = selection
        .lanes
        .iter()
        .map(|id| {
            let value = match id {
                lanes::Identity::Event(r) => LaneContent::Hits(
                    selection
                        .hits
                        .iter()
                        .filter_map(|i| clip.events.get(*i))
                        .filter(|e| e.definition == *r)
                        .map(|e| {
                            let mut e = e.clone();
                            e.position = BeatTime(e.position.ticks() - start);
                            e
                        })
                        .collect(),
                ),
                lanes::Identity::Envelope(t) if selection.picks.is_none() => {
                    LaneContent::Points(range_points(&points(clip, t), &selection.range, axis(t)))
                }
                lanes::Identity::Envelope(t) => {
                    let all = points(clip, t);
                    LaneContent::Points(
                        selection
                            .points
                            .iter()
                            .filter(|(target, _)| target == t)
                            .filter_map(|(_, i)| all.get(*i))
                            .map(|p| {
                                let mut p = p.clone();
                                p.time = BeatTime(p.time.ticks() - start);
                                p
                            })
                            .collect(),
                    )
                }
            };
            (lane_key(id), value)
        })
        .collect();
    Clipboard {
        duration,
        lanes,
        ranged: selection.picks.is_none(),
    }
}

pub(super) fn submit(
    app: &mut App,
    key: ClipKey,
    range: Range<BeatTime>,
    lanes: Vec<(LaneKey, LaneContent)>,
    point_policy: PointReplacement,
    selection: content::Selection,
) {
    if lanes.is_empty() || range.is_empty() {
        return;
    }
    if run_edit(
        app,
        Command::Edit(ProjectEdit::EditLane {
            track: key.track,
            slot: key.slot,
            edit: LaneEdit::ReplaceRange {
                range,
                lanes,
                point_policy,
            },
        }),
    ) {
        content::apply(app, selection);
    }
}

fn point_policy(selection: &content::Selection) -> PointReplacement {
    if selection.picks.is_some() {
        PointReplacement::Exact
    } else {
        PointReplacement::KeepEdges
    }
}

/// Retain all unpicked items, including equal-time siblings. Range clears use the engine's
/// open envelope interior; picked items replace the exact half-open span.
fn deletion(
    clip: &Clip,
    selection: &content::Selection,
) -> (Range<BeatTime>, Vec<(LaneKey, LaneContent)>) {
    let picked = selection.picks.is_some();
    let range = selection.range.clone();
    let lanes = selection
        .lanes
        .iter()
        .map(|id| {
            let data = match id {
                lanes::Identity::Event(r) => LaneContent::Hits(
                    clip.events
                        .iter()
                        .enumerate()
                        .filter(|(i, e)| {
                            e.definition == *r
                                && range.contains(&e.position)
                                && !selection.hits.contains(i)
                        })
                        .map(|(_, e)| e.clone())
                        .collect(),
                ),
                lanes::Identity::Envelope(t) => LaneContent::Points(
                    points(clip, t)
                        .into_iter()
                        .enumerate()
                        .filter(|(i, p)| {
                            point_policy(selection).contains(&range, p.time)
                                && !(if picked {
                                    selection.points.contains(&(t.clone(), *i))
                                } else {
                                    p.time > selection.range.start && p.time < selection.range.end
                                })
                        })
                        .map(|(_, p)| p)
                        .collect(),
                ),
            };
            (lane_key(id), data)
        })
        .collect();
    (range, lanes)
}

pub(super) fn shortcut(app: &mut App, command: Shortcut) {
    if !app.chrome.detail_open
        || !app.clip_view.editor.focused
        || app.pending_field.is_some()
        || app.selection.inspector_tab != crate::inspector_panel::Tab::Clip
    {
        return;
    }
    app.close_overlay();
    reconcile(app);
    let Some(selection) = app.clip_view.editor.content.clone() else {
        return;
    };
    let Some(clip) = clip_owned(app, selection.key) else {
        return;
    };
    match command {
        Shortcut::Copy | Shortcut::Cut if !selection.range.is_empty() => {
            let axis = axes(app, selection.key, &clip);
            app.clip_view.editor.clipboard = Some(copy(
                &clip,
                &selection,
                app.clip_view.editor.grid.ticks(),
                axis,
            ));
            if command == Shortcut::Cut {
                delete(app, &clip, selection);
            }
        }
        Shortcut::Delete => delete(app, &clip, selection),
        Shortcut::Paste => {
            if let Some(copied) = app.clip_view.editor.clipboard.clone() {
                paste(app, selection.key, selection.range.start, copied, true);
            }
        }
        Shortcut::Duplicate if !selection.range.is_empty() => {
            let axis = axes(app, selection.key, &clip);
            let copied = copy(&clip, &selection, app.clip_view.editor.grid.ticks(), axis);
            let at = BeatTime(
                selection
                    .range
                    .start
                    .ticks()
                    .saturating_add(copied.duration),
            );
            paste(app, selection.key, at, copied, false);
        }
        _ => {}
    }
}
fn delete(app: &mut App, clip: &Clip, selection: content::Selection) {
    if selection.range.is_empty() {
        return;
    }
    let policy = point_policy(&selection);
    let (range, lanes) = deletion(clip, &selection);
    let marker = content::Selection::time(
        selection.key,
        selection.range.start..selection.range.start,
        selection.lanes,
    );
    submit(app, selection.key, range, lanes, policy, marker);
}
pub(super) fn paste(app: &mut App, key: ClipKey, at: BeatTime, mut copied: Clipboard, remap: bool) {
    let Some(end) = at.ticks().checked_add(copied.duration).map(BeatTime) else {
        return;
    };
    let events_only = app
        .clip_view
        .editor
        .views
        .choice(app.project(), key.track, key.id)
        .tab
        == super::super::lane_view::EditorTab::Events;
    if events_only {
        copied
            .lanes
            .retain(|(key, _)| matches!(key, LaneKey::Events(_)));
        if copied.lanes.is_empty() {
            app.status.show(
                "Events can only paste event hits",
                crate::status::Severity::Notice,
            );
            return;
        }
    }
    let one = app
        .clip_view
        .editor
        .views
        .choice(app.project(), key.track, key.id)
        .one_lane()
        .cloned();
    let Some(clip) = clip_owned(app, key) else {
        return;
    };
    if one.is_none() {
        copied.lanes.retain(|(key, _)| {
            !clip
                .lanes
                .iter()
                .filter_map(|l| l.envelope())
                .any(|e| e.region.is_some() && *key == LaneKey::Envelope(e.target.clone()))
        });
        if copied.lanes.is_empty() {
            app.status.show(
                "Unlinked lanes can only be pasted in their One lane view",
                crate::status::Severity::Notice,
            );
            return;
        }
    }
    if remap
        && copied.lanes.len() == 1
        && let (Some(target), (LaneKey::Envelope(_), LaneContent::Points(_))) =
            (one, &copied.lanes[0])
    {
        copied.lanes[0].0 = LaneKey::Envelope(target);
    }
    for (_, data) in &mut copied.lanes {
        match data {
            LaneContent::Hits(hits) => {
                for h in hits {
                    h.position = BeatTime(h.position.ticks().saturating_add(at.ticks()));
                }
            }
            LaneContent::Points(points) => {
                for p in points {
                    p.time = BeatTime(p.time.ticks().saturating_add(at.ticks()));
                }
            }
        }
    }
    // A range copy ends on its own last value, so the write reaches one tick past the end and
    // puts back what stood there, as Draw Mode's steps do.
    let reach = copied.ranged
        && copied.lanes.iter().any(|(_, data)| {
            matches!(data, LaneContent::Points(p) if p.last().is_some_and(|p| p.time == end))
        });
    if copied.ranged {
        let axis = axes(app, key, &clip);
        for (lane, data) in &mut copied.lanes {
            match (lane, data) {
                (LaneKey::Envelope(target), LaneContent::Points(incoming)) => {
                    seam(
                        &points(&clip, target),
                        incoming,
                        at..end,
                        axis(target),
                        reach,
                    );
                }
                (LaneKey::Events(r), LaneContent::Hits(hits)) if reach => hits.extend(
                    clip.events
                        .iter()
                        .filter(|e| e.definition == *r && e.position == end)
                        .cloned(),
                ),
                _ => {}
            }
        }
    }
    let selection = content::Selection::time(
        key,
        at..end,
        copied.lanes.iter().map(|(k, _)| identity(k)).collect(),
    );
    let range = if reach {
        at..BeatTime(end.ticks() + 1)
    } else {
        at..end
    };
    submit(
        app,
        key,
        range,
        copied.lanes,
        PointReplacement::KeepEdges,
        selection,
    );
}

/// Join a pasted range copy to the lane it lands on with a jump pair at each end: the lane's
/// value arriving at the start, then the copy; the copy's last value, then the lane's value
/// leaving the end, shaped like the lane's own segment. `reach` writes the end tick too, so
/// points already there are put back when the copy has none of its own there.
fn seam(
    existing: &[Breakpoint],
    incoming: &mut Vec<Breakpoint>,
    range: Range<BeatTime>,
    axis: Axis,
    reach: bool,
) {
    let ends_here = incoming.last().is_some_and(|p| p.time == range.end);
    if !incoming.is_empty()
        && let Some(start) = edge(existing, range.start, axis)
    {
        incoming.insert(
            0,
            Breakpoint {
                time: range.start,
                value: start.arriving,
                curve: 0.0,
            },
        );
    }
    if ends_here {
        if let Some(end) = edge(existing, range.end, axis) {
            incoming.push(Breakpoint {
                time: range.end,
                value: end.leaving,
                curve: end.curve,
            });
        }
    } else if reach {
        incoming.extend(existing.iter().filter(|p| p.time == range.end).cloned());
    }
}

/// Pixel deltas become content deltas once, shared by preview and release. The earliest
/// selected time limits a leftward move; vertical movement uses each parameter's own scale.
pub(crate) fn deltas(
    selection: &content::Selection,
    gesture: &content::Move,
    grid: Grid,
    command: bool,
) -> (i64, f64) {
    let dx = gesture.to.x - gesture.from.x;
    let dy = gesture.to.y - gesture.from.y;
    let start = selection.range.start.ticks() as f64;
    let step = if command { 1 } else { grid.ticks() } as f64;
    let at = if dx.abs() < super::super::point_gesture::DRAG_THRESHOLD {
        start
    } else {
        ((start + f64::from(dx) * gesture.time_scale) / step).round() * step
    };
    (
        (at.max(0.0) - start).clamp(i64::MIN as f64, i64::MAX as f64) as i64,
        if dy.abs() < super::super::point_gesture::DRAG_THRESHOLD {
            0.0
        } else {
            -f64::from(dy) * gesture.value_scale
        },
    )
}
fn shifted(at: BeatTime, delta: i64) -> BeatTime {
    BeatTime(at.ticks().saturating_add_signed(delta))
}

/// Build one union write, preserving unselected content between the old and new spans.
/// `preview` returns the very same replacement lists the release will send to the engine.
pub(crate) fn move_plan(
    model: &LaneModel,
    clip: &Clip,
    selection: &content::Selection,
    gesture: &content::Move,
    grid: Grid,
    command: bool,
) -> (
    Range<BeatTime>,
    Vec<(LaneKey, LaneContent)>,
    content::Selection,
) {
    plan_move(
        model,
        clip,
        selection,
        deltas(selection, gesture, grid, command),
        gesture.copy,
    )
}

fn plan_move(
    model: &LaneModel,
    clip: &Clip,
    selection: &content::Selection,
    (dt, dv): (i64, f64),
    copy: bool,
) -> (
    Range<BeatTime>,
    Vec<(LaneKey, LaneContent)>,
    content::Selection,
) {
    let dest = shifted(selection.range.start, dt)..shifted(selection.range.end, dt);
    let mut range = selection.range.start.min(dest.start)..selection.range.end.max(dest.end);
    // Preserve the established time-range move rule; picked items need no margin.
    if selection.picks.is_none() {
        range.start = BeatTime(range.start.ticks().saturating_sub(1));
    }
    let data = selection
        .lanes
        .iter()
        .map(|id| {
            let data = match id {
                lanes::Identity::Event(r) => {
                    let mut hits: Vec<_> = clip
                        .events
                        .iter()
                        .enumerate()
                        .filter(|(i, e)| {
                            e.definition == *r
                                && range.contains(&e.position)
                                && (copy || !selection.hits.contains(i))
                                && !dest.contains(&e.position)
                        })
                        .map(|(_, e)| e.clone())
                        .collect();
                    hits.extend(
                        selection
                            .hits
                            .iter()
                            .filter_map(|i| clip.events.get(*i))
                            .filter(|e| e.definition == *r)
                            .map(|e| {
                                let mut e = e.clone();
                                e.position = shifted(e.position, dt);
                                e
                            }),
                    );
                    hits.sort_by_key(|e| e.position);
                    LaneContent::Hits(hits)
                }
                lanes::Identity::Envelope(t) => {
                    let all = points(clip, t);
                    let Some(row) = model.rows.iter().find_map(|r| match &r.kind {
                        RowKind::Envelope(e) if e.target == *t => Some(e),
                        _ => None,
                    }) else {
                        return (lane_key(id), LaneContent::Points(vec![]));
                    };
                    let chosen: Vec<_> = all
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| selection.points.contains(&(t.clone(), *i)))
                        .collect();
                    let delta = if chosen.is_empty() {
                        0.0
                    } else {
                        let lo = chosen
                            .iter()
                            .map(|(_, p)| p.value)
                            .fold(f64::INFINITY, f64::min);
                        let hi = chosen
                            .iter()
                            .map(|(_, p)| p.value)
                            .fold(f64::NEG_INFINITY, f64::max);
                        (dv * (row.max - row.min)).clamp(row.min - lo, row.max - hi)
                    };
                    let lane = row.lane(
                        super::super::point_gesture::Plot {
                            left: 0.0,
                            width: 1.0,
                            top: 0.0,
                            height: 1.0,
                        },
                        clip.end,
                        1,
                    );
                    let mut result: Vec<_> = all
                        .iter()
                        .enumerate()
                        .filter(|(i, p)| {
                            point_policy(selection).contains(&range, p.time)
                                && (copy || !selection.points.contains(&(t.clone(), *i)))
                                && !(p.time > dest.start && p.time < dest.end)
                        })
                        .map(|(_, p)| p.clone())
                        .collect();
                    result.extend(chosen.into_iter().map(|(_, p)| {
                        let mut p = p.clone();
                        p.time = shifted(p.time, dt);
                        p.value = lane.quantise(p.value + delta);
                        p
                    }));
                    result.sort_by_key(|p| p.time);
                    LaneContent::Points(result)
                }
            };
            (lane_key(id), data)
        })
        .collect();
    let mut next = content::Selection::time(selection.key, dest.clone(), selection.lanes.clone());
    if let Some(picks) = &selection.picks {
        next.picks = Some(
            picks
                .iter()
                .map(|pick| {
                    let time = shifted(pick.time, dt);
                    let preceding = picks
                        .iter()
                        .filter(|p| {
                            p.lane == pick.lane && p.time == pick.time && p.ordinal < pick.ordinal
                        })
                        .count();
                    // Destination boundary points survive and sort before the moved picks. Event
                    // destinations are half-open and are overwritten, so their ordinal starts at zero.
                    let retained = match &pick.lane {
                        lanes::Identity::Event(_) => 0,
                        lanes::Identity::Envelope(t) => points(clip, t)
                            .iter()
                            .enumerate()
                            .filter(|(i, p)| {
                                p.time == time
                                    && !(p.time > dest.start && p.time < dest.end)
                                    && (copy || !selection.points.contains(&(t.clone(), *i)))
                            })
                            .count(),
                    };
                    content::Item {
                        lane: pick.lane.clone(),
                        time,
                        ordinal: retained + preceding,
                    }
                })
                .collect(),
        );
    }
    (range, data, next)
}

pub(super) fn commit_move(app: &mut App, gesture: content::Move) {
    if !active(app, gesture.key) || gesture.revision != app.project_revision() || !gesture.moved {
        return;
    }
    let Some(selection) = app
        .clip_view
        .editor
        .content
        .clone()
        .filter(|s| s.key == gesture.key)
    else {
        return;
    };
    if deltas(
        &selection,
        &gesture,
        app.clip_view.editor.grid,
        app.clip_view.editor.command_down,
    ) == (0, 0.0)
        && !gesture.copy
    {
        return;
    }
    let Some(clip) = clip_owned(app, gesture.key) else {
        return;
    };
    let model = LaneModel::build(app.project(), gesture.key.track, &clip);
    let (range, lanes, next) = move_plan(
        &model,
        &clip,
        &selection,
        &gesture,
        app.clip_view.editor.grid,
        app.clip_view.editor.command_down,
    );
    submit(
        app,
        gesture.key,
        range,
        lanes,
        point_policy(&selection),
        next,
    );
}

/// Single-hit legacy canvas messages join the same transaction path.
pub(super) fn move_hit(app: &mut App, target: HitTarget, at: BeatTime, copy: bool) {
    if !valid_target(app, target) {
        return;
    }
    let Some(clip) = clip_owned(app, target.key) else {
        return;
    };
    app.clip_view
        .editor
        .select_hit(&clip, target.key, Some(target.index));
    let from = clip.events[target.index].position.ticks();
    let selection = app.clip_view.editor.content.clone().unwrap();
    let dt = (i128::from(at.ticks()) - i128::from(from))
        .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64;
    let model = LaneModel::build(app.project(), target.key.track, &clip);
    let (range, lanes, next) = plan_move(&model, &clip, &selection, (dt, 0.0), copy);
    submit(
        app,
        target.key,
        range,
        lanes,
        point_policy(&selection),
        next,
    );
}

/// Apply the planned transaction to presentation copies only. Stale gestures never preview.
pub(crate) fn preview(
    model: &LaneModel,
    clip: &Clip,
    selection: &content::Selection,
    gesture: &content::Move,
    grid: Grid,
    command: bool,
) -> (Clip, LaneModel, content::Selection) {
    let (range, data, mut next) = move_plan(model, clip, selection, gesture, grid, command);
    let mut clip = clip.clone();
    for (key, data) in data {
        match (key, data) {
            (LaneKey::Events(r), LaneContent::Hits(hits)) => {
                clip.events
                    .retain(|e| e.definition != r || !range.contains(&e.position));
                clip.events.extend(hits);
            }
            (LaneKey::Envelope(t), LaneContent::Points(points)) => {
                if let Some(e) = clip
                    .lanes
                    .iter_mut()
                    .filter_map(|l| match l {
                        qtable_engine::model::Lane::Envelope(e) => Some(e),
                        _ => None,
                    })
                    .find(|e| e.target == t)
                {
                    point_policy(selection).replace_points(&mut e.points, &range, points);
                }
            }
            _ => unreachable!(),
        }
    }
    clip.events.sort_by_key(|e| e.position);
    next.resolve(&clip);
    let mut model = model.clone();
    for row in &mut model.rows {
        if let RowKind::Envelope(e) = &mut row.kind {
            std::rc::Rc::make_mut(e).points = points(&clip, &e.target);
        }
    }
    (clip, model, next)
}

#[cfg(test)]
mod tests;
