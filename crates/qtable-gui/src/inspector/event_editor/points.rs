//! Envelope point edits: a released gesture, Delete on the selected point, and the strip's
//! typed Position, Value and Curve. Each is one `EnvelopeEdit::SetPoints` of the whole list.
use super::*;
use crate::inspector::point_gesture::move_point;
use qtable_engine::model::Axis;

pub(crate) fn valid_point_target(app: &App, target: &PointTarget) -> bool {
    app.chrome.detail_open
        && app.selection.inspector_tab == crate::inspector_panel::Tab::Clip
        && active(app, target.key)
        && target.revision == app.project_revision()
        && points_of(app, target.key, &target.target).is_some()
}

fn points_of<'a>(app: &'a App, key: ClipKey, target: &ArgumentRef) -> Option<&'a [Breakpoint]> {
    clip(app.project(), key.track, key.slot)?
        .lanes
        .iter()
        .find_map(|l| l.envelope().filter(|e| e.target == *target))
        .map(|e| e.points.as_slice())
}

/// The selected envelope lane's selected point, if both still describe the project.
pub(crate) struct Selected {
    pub(crate) lane: PointTarget,
    pub(crate) index: usize,
    pub(crate) points: Vec<Breakpoint>,
}

pub(crate) fn selected(app: &App) -> Option<Selected> {
    let selection = app.clip_view.editor.point.as_ref()?;
    if !valid_point_target(app, &selection.lane) {
        return None;
    }
    let points = points_of(app, selection.lane.key, &selection.lane.target)?;
    (selection.index < points.len()).then(|| Selected {
        lane: selection.lane.clone(),
        index: selection.index,
        points: points.to_vec(),
    })
}

/// Drops the selected point's draft fields, so a different point shows its own values.
pub(crate) fn forget_drafts(app: &mut App, track: TrackId, slot: usize) {
    for field in [
        ClipField::PointPosition,
        ClipField::PointValue,
        ClipField::PointCurve,
    ] {
        app.drafts.forget(DraftKey::clip(track, slot, field));
    }
}

/// Replaces the lane's points in one edit, selects the lane and (when given) a point.
pub(super) fn set_points(
    app: &mut App,
    lane: PointTarget,
    points: Vec<Breakpoint>,
    select: Option<usize>,
    draft: Option<DraftKey>,
) -> bool {
    let PointTarget { key, target, .. } = lane;
    if points_of(app, key, &target).is_some_and(|current| current == points.as_slice()) {
        // A click on a point: select it for the fields, with no edit.
        if let Some(index) = select {
            select_point(app, key, &target, index);
        }
        return false;
    }
    // Editing inside a time selection updates its members; it must not turn the
    // range into a header selection or collapse it to the edited point.
    let region = app
        .clip_view
        .editor
        .content
        .as_ref()
        .filter(|s| {
            s.key == key
                && s.picks.is_none()
                && !s.range.is_empty()
                && s.lanes.contains(&lanes::Identity::Envelope(target.clone()))
                && select.is_none_or(|i| points.get(i).is_some_and(|p| s.range.contains(&p.time)))
        })
        .cloned();
    let edit = ProjectEdit::EditEnvelope {
        track: key.track,
        slot: key.slot,
        edit: EnvelopeEdit::SetPoints {
            target: target.clone(),
            points,
        },
    };
    if crate::edit_actions::run_edit_report(app, Command::Edit(edit), draft).is_none()
        || !active(app, key)
    {
        return false;
    }
    forget_drafts(app, key.track, key.slot);
    if let Some(region) = region {
        content::apply(app, region);
        return true;
    }
    match select {
        Some(index) => select_point(app, key, &target, index),
        None => {
            let editor = &mut app.clip_view.editor;
            editor.selected = None;
            editor.hover = None;
            editor.focused = true;
            editor.point = None;
            editor.select_lane(key, lanes::Identity::Envelope(target.clone()));
        }
    }
    true
}

/// Select point `index` of the lane: its fields fill and the content selection picks it.
fn select_point(app: &mut App, key: ClipKey, target: &ArgumentRef, index: usize) {
    let revision = app.project_revision();
    let editor = &mut app.clip_view.editor;
    editor.selected = None;
    editor.hover = None;
    editor.focused = true;
    editor.point = Some(PointSelection {
        lane: PointTarget {
            key,
            revision,
            target: target.clone(),
        },
        index,
    });
    editor.select_lane(key, lanes::Identity::Envelope(target.clone()));
    if let Some(clip) = clip_owned(app, key)
        && let Some((item, _)) = content::Selection::items(&clip)
            .into_iter()
            .find(|(item, i)| item.lane == lanes::Identity::Envelope(target.clone()) && *i == index)
    {
        content::pick(app, key, item, false);
    }
}

/// A first point for an argument that has no lane: the same checks as `valid_point_target`,
/// but the lane must be absent, and the argument must still resolve on its device.
pub(crate) fn valid_start(app: &App, target: &PointTarget) -> bool {
    app.chrome.detail_open
        && app.selection.inspector_tab == crate::inspector_panel::Tab::Clip
        && active(app, target.key)
        && target.revision == app.project_revision()
        && points_of(app, target.key, &target.target).is_none()
        && app
            .project()
            .track(target.key.track)
            .is_some_and(|t| t.argument_spec(&target.target).is_some())
}

/// Creates the lane and gives it its first points: two edits with today's API,
/// `EnvelopeEdit::Add`, then `SetPoints`. An empty lane is a valid state (design section 1),
/// so if the second edit is refused the first stays: an empty lane, never a corrupt one, and
/// the refusal reaches the status bar.
pub(super) fn start_envelope(
    app: &mut App,
    lane: PointTarget,
    points: Vec<Breakpoint>,
    select: Option<usize>,
) {
    let PointTarget { key, target, .. } = lane;
    let added = run_edit(
        app,
        Command::Edit(ProjectEdit::EditEnvelope {
            track: key.track,
            slot: key.slot,
            edit: EnvelopeEdit::Add {
                target: target.clone(),
            },
        }),
    );
    if !added || !active(app, key) {
        return;
    }
    let lane = PointTarget {
        key,
        revision: app.project_revision(),
        target: target.clone(),
    };
    // Even if the points are refused, the new lane is the one selected.
    let editor = &mut app.clip_view.editor;
    editor.selected = None;
    editor.select_lane(key, lanes::Identity::Envelope(target));
    set_points(app, lane, points, select, None);
}

/// Delete or Backspace removes the selected point. `false` when none is selected, so the
/// shortcut falls through to hits.
/// A typed Position, Value or Curve for the selected point. A rejected entry stays in its
/// field with the engine's or the parser's message (shown in the status bar by the caller).
pub(crate) fn commit_text(
    app: &mut App,
    track: TrackId,
    slot: usize,
    field: ClipField,
    text: &str,
) {
    let key = DraftKey::clip(track, slot, field);
    let Some(Selected {
        lane,
        index,
        mut points,
    }) = selected(app).filter(|s| (s.lane.key.track, s.lane.key.slot) == (track, slot))
    else {
        app.drafts.forget(key);
        return;
    };
    let Some(owner) = app.project().track(track) else {
        return;
    };
    let (Some(spec), Some(axis)) = (
        owner.argument_spec(&lane.target),
        owner.argument_axis(&lane.target),
    ) else {
        return;
    };
    let Some(length) = clip(app.project(), track, slot)
        .map(|c| super::super::clip_markers::extent(c, app.project().time_signature))
    else {
        return;
    };
    let mut at = index;
    match field {
        ClipField::PointPosition => {
            match crate::inspector::musical_time::parse_position(text, app.project().time_signature)
            {
                // Typed past the visible axis, a position lands on its end, as a drag does.
                Ok(time) => {
                    (points, at) = move_point(&points, index, time.min(length), points[index].value)
                }
                Err(error) => {
                    app.drafts.errors.insert(key, error);
                    return;
                }
            }
        }
        ClipField::PointValue => match spec.parse(text) {
            Ok(value) => points[index].value = value,
            Err(error) => {
                app.drafts.errors.insert(key, error);
                return;
            }
        },
        ClipField::PointCurve => {
            let last = index + 1 == points.len();
            match text.trim().parse::<f32>() {
                _ if axis == Axis::Discrete || last => return,
                Ok(curve) if (-1.0..=1.0).contains(&curve) => points[index].curve = curve,
                _ => {
                    app.drafts
                        .errors
                        .insert(key, "Curve must be a number from -1 to 1".into());
                    return;
                }
            }
        }
        _ => return,
    }
    if !set_points(app, lane, points, Some(at), Some(key)) {
        // Unchanged text, or a refused edit: an unchanged one drops its draft to show the model.
        if !app.drafts.errors.contains_key(&key) {
            app.drafts.forget(key);
        }
    }
}

/// A level chosen in the Value dropdown or toggle.
pub(crate) fn choose_value(app: &mut App, track: TrackId, slot: usize, value: f64) {
    let Some(Selected {
        lane,
        index,
        mut points,
    }) = selected(app).filter(|s| (s.lane.key.track, s.lane.key.slot) == (track, slot))
    else {
        return;
    };
    points[index].value = value;
    set_points(app, lane, points, Some(index), None);
}

/// Capture-only (`tools/gui-capture.sh --show-points-*`): replays scripted gestures through
/// the real `point_gesture` and the real `SetPoints` handler, so a still screenshot shows
/// what each gesture leaves behind. A drag in flight and its tooltip need the pointer and
/// are covered by tests instead.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture(app: &mut App, scenario: &str) {
    use crate::inspector::point_gesture::{Gesture, Keys, Lane, Plot};
    use iced::Point;
    reconcile(app);
    let Some((track, slot, id)) = app.clip_view.editor.owner else {
        return;
    };
    let key = ClipKey {
        track,
        slot,
        id,
        epoch: app.clip_view.editor.epoch,
    };
    let none = Keys::default();
    let alt = Keys { alt: true, ..none };
    // Press at `from`, move to `to`, release: the lane of `parameter` as the gesture sees it.
    let run = |app: &mut App, parameter: &str, keys: Keys, from: (u64, f64), to: (f32, f32)| {
        let Some(clip) = clip(app.project(), track, slot) else {
            return;
        };
        let Some(envelope) = clip
            .lanes
            .iter()
            .filter_map(|l| l.envelope())
            .find(|e| e.target.parameter.0 == parameter)
        else {
            return;
        };
        let Some(owner) = app.project().track(track) else {
            return;
        };
        let (Some(spec), Some(axis)) = (
            owner.argument_spec(&envelope.target),
            owner.argument_axis(&envelope.target),
        ) else {
            return;
        };
        let lane = Lane::new(
            Plot {
                left: 0.0,
                width: 400.0,
                top: 0.0,
                height: 36.0,
            },
            (clip.length(), axis, spec.min, spec.max, 240),
            &envelope.target,
            &envelope.points,
        );
        let start = Point::new(
            lane.x(qtable_engine::model::BeatTime(from.0)),
            lane.y(from.1),
        );
        let end = Point::new(start.x + to.0, start.y + to.1);
        let target = PointTarget {
            key,
            revision: app.project_revision(),
            target: envelope.target.clone(),
        };
        let selected = app
            .clip_view
            .editor
            .point
            .as_ref()
            .filter(|p| p.lane.target == target.target)
            .map(|p| p.index);
        let Some(mut gesture) = Gesture::press(&lane, start, keys, selected) else {
            return;
        };
        gesture.drag(&lane, end, keys);
        if let Some(commit) = gesture.release(&lane, end, keys) {
            handle(app, Action::SetPoints(target, commit.points, commit.select));
        }
    };
    let select = |app: &mut App, parameter: &str, index: usize| {
        let lane = clip(app.project(), track, slot)
            .and_then(|c| {
                c.lanes
                    .iter()
                    .filter_map(|l| l.envelope())
                    .find(|e| e.target.parameter.0 == parameter)
            })
            .map(|e| PointTarget {
                key,
                revision: app.project_revision(),
                target: e.target.clone(),
            });
        if let Some(lane) = lane {
            let editor = &mut app.clip_view.editor;
            editor.select_lane(key, lanes::Identity::Envelope(lane.target.clone()));
            editor.point = Some(PointSelection { lane, index });
            editor.selected = None;

            editor.focused = true;
        }
    };
    match scenario {
        "gestures" => {
            // Steps: drag the segment from 1 to 6 up two levels. Position y: click empty space
            // for its first point. Opacity: drag its 0.9 point past the 0.3 point, which goes.
            run(app, "steps", none, (960, 3.5), (0.0, -9.0));
            run(app, "position", none, (1920, 70.0), (0.0, 0.0));
            run(app, "opacity", none, (1200, 0.9), (150.0, 0.0));
        }
        "curvature" => {
            // Alt-drag the rising first segment up, the falling last one up: both bend upward.
            run(app, "opacity", alt, (600, 0.55), (0.0, -14.0));
            run(app, "opacity", alt, (3120, 0.45), (0.0, -10.0));
            select(app, "opacity", 1);
        }
        "discrete" => {
            // Mode (an Enum): the first point up two levels. Strobe (a Toggle): the 1.0 point
            // down a level. Its Value is a dropdown or toggle and its Curve disabled.
            run(app, "mode", none, (0, 0.0), (0.0, -24.0));
            run(app, "strobe", none, (960, 1.0), (0.0, 20.0));
            select(app, "mode", 1);
        }
        "typed" => {
            select(app, "opacity", 1);
            let draft = |field| DraftKey::clip(track, slot, field);
            app.drafts
                .values
                .insert(draft(ClipField::PointPosition), "1. 3. 1".into());
            let value = draft(ClipField::PointValue);
            app.drafts.values.insert(value, "1.5".into());
            let _ = crate::update::update(
                app,
                crate::messages::Message::Inspector(crate::inspector::Message::PointText(
                    track,
                    slot,
                    ClipField::PointValue,
                    "1.5".into(),
                )),
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
