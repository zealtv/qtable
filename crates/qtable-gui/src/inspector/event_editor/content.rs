//! The content half of the editor selection. Ranges resolve items afresh after edits;
//! individual picks use lane identity, time and equal-time ordinal, never a lasting index.
use super::*;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub lane: lanes::Identity,
    pub time: BeatTime,
    pub ordinal: usize,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Selection {
    pub key: ClipKey,
    pub range: Range<BeatTime>,
    pub lanes: Vec<lanes::Identity>,
    /// None is a time selection (including the empty insert marker); Some is explicit picks.
    pub picks: Option<Vec<Item>>,
    pub hits: Vec<usize>,
    pub points: Vec<(ArgumentRef, usize)>,
}
impl Selection {
    pub(crate) fn time(key: ClipKey, range: Range<BeatTime>, lanes: Vec<lanes::Identity>) -> Self {
        Self {
            key,
            range,
            lanes,
            picks: None,
            hits: vec![],
            points: vec![],
        }
    }
    pub(crate) fn items(clip: &Clip) -> Vec<(Item, usize)> {
        let mut all = vec![];
        let mut ordinals = std::collections::HashMap::new();
        for (i, event) in clip.events.iter().enumerate() {
            let ordinal = ordinals
                .entry((&event.definition, event.position))
                .or_insert(0);
            let occurrence = *ordinal;
            *ordinal += 1;
            all.push((
                Item {
                    lane: lanes::Identity::Event(event.definition.clone()),
                    time: event.position,
                    ordinal: occurrence,
                },
                i,
            ));
        }
        for envelope in clip.lanes.iter().filter_map(|l| l.envelope()) {
            let mut previous = None;
            let mut ordinal = 0;
            for (i, point) in envelope.points.iter().enumerate() {
                ordinal = if previous == Some(point.time) {
                    ordinal + 1
                } else {
                    0
                };
                previous = Some(point.time);
                all.push((
                    Item {
                        lane: lanes::Identity::Envelope(envelope.target.clone()),
                        time: point.time,
                        ordinal,
                    },
                    i,
                ));
            }
        }
        all
    }
    pub(crate) fn resolve(&mut self, clip: &Clip) {
        self.hits.clear();
        self.points.clear();
        let all = Self::items(clip);
        if let Some(picks) = &mut self.picks {
            picks.retain(|p| all.iter().any(|(item, _)| item == p));
            self.lanes.clear();
            for item in picks.iter() {
                if !self.lanes.contains(&item.lane) {
                    self.lanes.push(item.lane.clone());
                }
            }
            if let (Some(start), Some(end)) = (
                picks.iter().map(|i| i.time).min(),
                picks.iter().map(|i| i.time).max(),
            ) {
                self.range = start..BeatTime(end.ticks().saturating_add(1));
            }
        }
        for (item, index) in all {
            let chosen = self.picks.as_ref().map_or_else(
                || self.lanes.contains(&item.lane) && self.range.contains(&item.time),
                |picks| picks.contains(&item),
            );
            if chosen {
                match item.lane {
                    lanes::Identity::Event(_) => self.hits.push(index),
                    lanes::Identity::Envelope(target) => self.points.push((target, index)),
                }
            }
        }
    }
    pub(crate) fn contains(&self, lane: &lanes::Identity, at: BeatTime) -> bool {
        self.picks.is_none()
            && !self.range.is_empty()
            && self.lanes.contains(lane)
            && self.range.contains(&at)
    }
    /// Only a zero-width time selection is an insert marker; picked items are not.
    pub(crate) fn insert_mark(&self) -> Option<BeatTime> {
        (self.picks.is_none() && self.range.is_empty()).then_some(self.range.start)
    }
    pub(crate) fn count(&self) -> usize {
        self.hits.len() + self.points.len()
    }
    pub(crate) fn summary(&self, signature: qtable_engine::model::TimeSignature) -> Option<String> {
        (self.count() != 1
            && (self.count() > 1 || (self.picks.is_none() && !self.range.is_empty())))
        .then(|| {
            format!(
                "{} hits, {} points · {} – {}",
                self.hits.len(),
                self.points.len(),
                super::super::musical_time::format_position(self.range.start, signature),
                super::super::musical_time::format_position(self.range.end, signature)
            )
        })
    }
}

/// A region gesture is routed here for region-edits to commit. It never falls through to
/// a one-point or one-hit edit. Identity and revision guard the eventual command.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Move {
    pub key: ClipKey,
    pub revision: u64,
    pub from: iced::Point,
    pub to: iced::Point,
    pub copy: bool,
    pub moved: bool,
    pub time_scale: f64,
    pub value_scale: f64,
}

impl Move {
    pub(crate) fn follow(
        &mut self,
        event: &iced::widget::canvas::Event,
        bounds: iced::Rectangle,
        cursor: iced::mouse::Cursor,
    ) {
        use iced::{mouse, widget::canvas};
        let pointer = match event {
            canvas::Event::Mouse(mouse::Event::CursorMoved { position }) => Some(*position),
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                cursor.position()
            }
            _ => None,
        };
        if let Some(p) = pointer {
            self.to = iced::Point::new(p.x - bounds.x, p.y - bounds.y);
            self.moved |= self.from.distance(self.to) > super::super::point_gesture::DRAG_THRESHOLD;
        }
    }
}

pub(super) fn apply(app: &mut App, mut selection: Selection) {
    if !active(app, selection.key) {
        return;
    }
    // Point fields are keyed by clip, so a new content pick must not inherit another
    // point's draft (especially a refused value) when the single-point strip reappears.
    points::forget_drafts(app, selection.key.track, selection.key.slot);
    let project = app.engine.project();
    let Some(clip) = clip(project, selection.key.track, selection.key.slot) else {
        return;
    };
    selection.resolve(clip);
    if let Some(at) = selection.insert_mark() {
        let position = super::super::musical_time::format_position(at, project.time_signature);
        app.status.show(
            format!("Insert Mark {}", position.replace(". ", ".")),
            crate::status::Severity::Plain,
        );
    }
    let editor = &mut app.clip_view.editor;
    editor.lanes = None;
    editor.selected = None;
    editor.point = None;
    editor.focused = true;
    editor.hover = None;
    editor.content = Some(selection);
    sync(editor, clip, project);
}

/// Carry visible event selections into Events, leaving hidden envelopes out of edits and fields.
pub(crate) fn events_only(app: &mut App, track: TrackId, slot: usize) {
    points::forget_drafts(app, track, slot);
    let project = app.engine.project();
    let Some(clip) = clip(project, track, slot) else {
        return;
    };
    let editor = &mut app.clip_view.editor;
    if editor.content.is_some() {
        sync(editor, clip, project);
        if editor.content.as_ref().is_some_and(|s| s.lanes.is_empty()) {
            editor.content = None;
        }
    }
    if let Some(selection) = &mut editor.lanes {
        selection
            .items
            .retain(|id| matches!(id, lanes::Identity::Event(_)));
        if let Some(first) = selection.items.first() {
            if !selection.items.contains(&selection.anchor) {
                selection.anchor = first.clone();
            }
        } else {
            editor.lanes = None;
        }
    }
    editor.point = None;
    editor.hover = None;
}

pub(super) fn sync(editor: &mut State, clip: &Clip, project: &qtable_engine::model::Project) {
    let Some(content) = &mut editor.content else {
        return;
    };

    if editor.views.choice(project, content.key.track, clip.id).tab
        == super::super::lane_view::EditorTab::Events
    {
        content
            .lanes
            .retain(|id| matches!(id, lanes::Identity::Event(_)));
        if let Some(picks) = &mut content.picks {
            picks.retain(|p| matches!(p.lane, lanes::Identity::Event(_)));
        }
    }
    let ids = lanes::displayed(project.track(content.key.track), clip);
    content.lanes.retain(|id| match id {
        lanes::Identity::Event(_) => ids.contains(id),
        lanes::Identity::Envelope(t) => project
            .track(content.key.track)
            .is_some_and(|track| track.argument_spec(t).is_some()),
    });
    // Unlinked content has launch-time coordinates in the overview and is editable only
    // in its source-coordinate One lane view, including keyboard range operations.
    if editor
        .views
        .choice(project, content.key.track, clip.id)
        .one_lane()
        .is_none()
    {
        let editable =
            |id: &lanes::Identity| {
                !clip.lanes.iter().filter_map(|l| l.envelope()).any(|e| {
                    e.region.is_some() && *id == lanes::Identity::Envelope(e.target.clone())
                })
            };
        content.lanes.retain(editable);
        if let Some(picks) = &mut content.picks {
            picks.retain(|p| editable(&p.lane));
        }
    }
    content.resolve(clip);
    editor.selected = None;
    editor.point = None;
    editor.lanes = None;
    // These are adapters for the existing strip and single-item commands, not selections.
    if content.count() == 1 {
        if let Some(&index) = content.hits.first() {
            editor.selected = Some((content.key, index));
        }
        if let Some((target, index)) = content.points.first() {
            editor.point = Some(PointSelection {
                lane: PointTarget {
                    key: content.key,
                    revision: editor.revision,
                    target: target.clone(),
                },
                index: *index,
            });
        }
    }
}

pub(super) fn pick(app: &mut App, key: ClipKey, item: Item, toggle: bool) {
    let Some(clip) = clip_owned(app, key) else {
        return;
    };
    let mut picks = if toggle {
        app.clip_view
            .editor
            .content
            .as_ref()
            .filter(|s| s.key == key)
            .map(|s| {
                s.picks.clone().unwrap_or_else(|| {
                    Selection::items(&clip)
                        .into_iter()
                        .filter(|(i, _)| s.lanes.contains(&i.lane) && s.range.contains(&i.time))
                        .map(|(i, _)| i)
                        .collect()
                })
            })
            .unwrap_or_default()
    } else {
        vec![]
    };
    if let Some(index) = picks.iter().position(|i| i == &item) {
        picks.remove(index);
    } else {
        picks.push(item.clone());
    }
    let mut selection = Selection::time(key, item.time..item.time, vec![item.lane]);
    if !picks.is_empty() {
        selection.picks = Some(picks);
    }
    apply(app, selection);
}

pub(crate) fn clear(editor: &mut State) {
    editor.content = None;
    editor.lanes = None;
    editor.selected = None;
    editor.point = None;
    editor.hover = None;
}

pub(crate) fn keyboard(
    app: &mut App,
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
    repeat: bool,
    ignored: bool,
) -> bool {
    use iced::keyboard::{Key, key::Named};
    if !ignored
        || app.focus != crate::focus::Focus::Detail
        || !app.clip_view.editor.focused
        || app.pending_field.is_some()
        || !app.chrome.detail_open
        || app.selection.inspector_tab != crate::inspector_panel::Tab::Clip
    {
        return false;
    }
    match key.as_ref() {
        Key::Named(named @ (Named::ArrowLeft | Named::ArrowRight))
            if modifiers == iced::keyboard::Modifiers::ALT =>
        {
            super::super::clip_markers::nudge(app, named == Named::ArrowRight)
        }
        Key::Named(named @ (Named::ArrowLeft | Named::ArrowRight | Named::Home | Named::End))
            if modifiers.is_empty() =>
        {
            move_insert(app, named)
        }
        Key::Named(Named::Escape) => {
            clear(&mut app.clip_view.editor);
            true
        }
        Key::Character("a" | "A") if modifiers == iced::keyboard::Modifiers::COMMAND => {
            if repeat {
                return true;
            }
            let Some((track, slot, id)) = app.clip_view.editor.owner else {
                return false;
            };
            let key = ClipKey {
                track,
                slot,
                id,
                epoch: app.clip_view.editor.epoch,
            };
            let Some(clip) = clip_owned(app, key) else {
                return false;
            };
            let lanes = app
                .clip_view
                .editor
                .views
                .choice(app.project(), track, id)
                .visible_lanes(app.project().track(track), &clip);
            apply(
                app,
                Selection::time(
                    key,
                    BeatTime::ZERO
                        ..BeatTime(
                            super::super::clip_markers::extent(&clip, app.project().time_signature)
                                .ticks()
                                .saturating_add(1),
                        ),
                    lanes,
                ),
            );
            true
        }
        _ => false,
    }
}

/// Move only the empty time selection. Item picks, ranges, headers and typed drafts
/// retain their keys. Preserve the clicked lane; with no content, use the current view.
fn move_insert(app: &mut App, named: iced::keyboard::key::Named) -> bool {
    use iced::keyboard::key::Named;
    let editor = &app.clip_view.editor;
    if editor
        .content
        .as_ref()
        .is_some_and(|s| s.insert_mark().is_none())
        || editor.lanes.as_ref().is_some_and(|s| s.headers)
        || editor.selected.is_some()
        || editor.point.is_some()
    {
        return false;
    }
    let Some((track, slot, id)) = editor.owner else {
        return false;
    };
    let key = ClipKey {
        track,
        slot,
        id,
        epoch: editor.epoch,
    };
    let Some(clip) = clip(app.project(), track, slot) else {
        return false;
    };
    let at = editor
        .content
        .as_ref()
        .and_then(Selection::insert_mark)
        .unwrap_or(clip.start);
    let step = editor.grid.ticks();
    let position = match named {
        Named::ArrowLeft => BeatTime(at.ticks().saturating_sub(step)),
        Named::ArrowRight => BeatTime(
            at.ticks()
                .saturating_add(step)
                .min(clip.end.max(clip.loop_end).ticks()),
        ),
        Named::Home => clip.start,
        Named::End => clip.end,
        _ => return false,
    };
    let lanes = editor
        .content
        .as_ref()
        .map(|s| s.lanes.clone())
        .or_else(|| editor.lanes.as_ref().map(|s| s.items.clone()))
        .unwrap_or_else(|| {
            editor
                .views
                .choice(app.project(), track, id)
                .visible_lanes(app.project().track(track), clip)
                .into_iter()
                .take(1)
                .collect()
        });
    apply(app, Selection::time(key, position..position, lanes));
    true
}

/// Shared background drag in both canvases. Lane coverage accumulates every row crossed,
/// including rows crossed before the pointer doubles back.
#[derive(Clone)]
pub(crate) struct Drag {
    pub origin: iced::Point,
    pub pointer: iced::Point,
    pub start: BeatTime,
    pub moved: bool,
    pub selection: Selection,
}
impl Drag {
    pub(crate) fn new(
        key: ClipKey,
        at: iced::Point,
        time: BeatTime,
        lane: lanes::Identity,
    ) -> Self {
        Self {
            origin: at,
            pointer: at,
            start: time,
            moved: false,
            selection: Selection::time(key, time..time, vec![lane]),
        }
    }
    pub(crate) fn update(
        &mut self,
        at: iced::Point,
        time: BeatTime,
        lanes: Vec<lanes::Identity>,
    ) -> Selection {
        self.pointer = at;
        self.moved |= self.origin.distance(at) > super::super::point_gesture::DRAG_THRESHOLD;
        if self.moved {
            self.selection.range = self.start.min(time)..self.start.max(time);
            for lane in lanes {
                if !self.selection.lanes.contains(&lane) {
                    self.selection.lanes.push(lane);
                }
            }
        }
        self.selection.clone()
    }
}

/// Unattended capture: use the selection gesture and the ordinary action handler.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture(app: &mut App) {
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
    let Some(clip) = clip_owned(app, key) else {
        return;
    };
    let lanes = app
        .clip_view
        .editor
        .views
        .choice(app.project(), track, id)
        .visible_lanes(app.project().track(track), &clip);
    let Some(first) = lanes.first().cloned() else {
        return;
    };
    let mut drag = Drag::new(key, iced::Point::ORIGIN, BeatTime(480), first);
    let selection = drag.update(iced::Point::new(100.0, 80.0), BeatTime(2880), lanes);
    handle(app, Action::Content(selection.clone()));
    app.focus = crate::focus::Focus::Detail;
    if let Ok(scenario) = std::env::var("QTABLE_CAPTURE_REGION") {
        use crate::clip_shortcuts::Shortcut;
        match scenario.as_str() {
            "delete" => {
                super::shortcut(app, Shortcut::Delete);
                handle(app, Action::Content(selection));
            }
            "paste" => {
                super::shortcut(app, Shortcut::Copy);
                handle(
                    app,
                    Action::Content(Selection::time(
                        key,
                        BeatTime(3840)..BeatTime(3840),
                        selection.lanes,
                    )),
                );
                super::shortcut(app, Shortcut::Paste);
            }
            "duplicate" => super::shortcut(app, Shortcut::Duplicate),
            // range-copy-samples-edges: a 16th inside a ramp, duplicated three times.
            "sawtooth" => {
                handle(
                    app,
                    Action::Content(Selection::time(
                        key,
                        BeatTime(2640)..BeatTime(2880),
                        selection.lanes,
                    )),
                );
                for _ in 0..3 {
                    super::shortcut(app, Shortcut::Duplicate);
                }
            }
            _ => {}
        }
    }
}

/// Capture the three marker states using ordinary content and transport actions. Playing
/// scenarios use gui-capture's existing --show-lanes launch (real scheduler positions).
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture_markers(app: &mut App, scenario: &str) {
    if scenario == "stopped-again" {
        std::thread::sleep(std::time::Duration::from_millis(700));
    }
    app.snapshot = app.engine.snapshot();
    reconcile(app);
    if scenario == "stopped-again" {
        let _ = crate::update::update(app, crate::messages::Message::Stop);
    }
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
    let Some(lane) = lanes::displayed(app.project().track(track), &clip)
        .first()
        .cloned()
    else {
        return;
    };
    let at = if scenario == "stopped-again" {
        BeatTime(2880)
    } else {
        BeatTime(960)
    };
    handle(
        app,
        Action::Content(Selection::time(key, at..at, vec![lane])),
    );
    app.focus = crate::focus::Focus::Detail;
    if scenario == "playing" {
        // Pressing Play takes keyboard ownership away from the editor, leaving the triangle.
        app.clip_view.editor.focused = false;
    }
}

/// Native capture activation can issue a field-commit event after the startup click.
/// Hold only the scripted editor focus; playback still comes from real snapshots.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn hold_marker_capture(app: &mut App) {
    if let Ok(mode) = std::env::var("QTABLE_CAPTURE_MARKERS") {
        app.clip_view.editor.focused = mode != "playing";
        app.focus = crate::focus::Focus::Detail;
    }
}

#[cfg(test)]
mod tests;
