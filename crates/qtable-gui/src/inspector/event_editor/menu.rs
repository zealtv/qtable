//! The content context menus: what a right-click settles in the selection, and what each row
//! of the menu then does. A right-click first selects as a left click would, then opens the
//! menu of what was clicked; a row runs the path its key or button already uses, so a row and
//! its shortcut cannot drift apart.
use super::*;
use crate::clip_shortcuts::Shortcut;
use crate::context_menu::{MenuTarget, OpenMenu};
use crate::overlay::Overlay;
use qtable_engine::model::Shape;

/// What a canvas found under a right-click, in the lane the pointer was over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Under {
    /// The envelope point at this index of the lane.
    Point(usize),
    /// The hit at this index of the clip's events.
    Hit(usize),
    /// Lane space: empty, on the line or in a segment band, at the time a left click would
    /// put the insert marker (the nearest grid line; ⌘ the exact tick).
    Space(BeatTime),
}

/// A right-click as a canvas reports it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Click {
    pub(crate) key: ClipKey,
    pub(crate) revision: u64,
    pub(crate) lane: lanes::Identity,
    pub(crate) under: Under,
}

/// Which menu opens, once the click has settled the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Clicked {
    Point,
    Hit,
    Space,
    TimeSelection,
}

/// The menu's target: the clip and project revision it opened against, the lane clicked, what
/// was clicked and its time (the item's, the insert marker's or the selection's start).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContentTarget {
    pub(crate) key: ClipKey,
    pub(crate) revision: u64,
    pub(crate) lane: lanes::Identity,
    pub(crate) clicked: Clicked,
    pub(crate) at: BeatTime,
}

/// What a row of a content menu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContentCommand {
    /// Cut, Copy, Paste, Duplicate and Delete: the keys' own path.
    Key(Shortcut),
    /// Clear Envelope: remove the clicked lane's envelope from this clip.
    ClearEnvelope,
    Shape(Shape),
    Simplify,
}

/// Whether the menu's clip, editor epoch and project revision still stand.
pub(crate) fn valid_content(app: &App, target: &ContentTarget) -> bool {
    app.chrome.detail_open
        && app.selection.inspector_tab == crate::inspector_panel::Tab::Clip
        && active(app, target.key)
        && target.revision == app.project_revision()
}

/// Settle the selection as a left click would, then open the menu of what was clicked.
pub(super) fn open(app: &mut App, click: Click) -> Option<ContentTarget> {
    let Click {
        key,
        revision,
        lane,
        under,
    } = click;
    if !(app.chrome.detail_open
        && app.selection.inspector_tab == crate::inspector_panel::Tab::Clip
        && active(app, key)
        && revision == app.project_revision())
    {
        return None;
    }
    let clip = clip_owned(app, key)?;
    let (clicked, at) = match under {
        Under::Point(index) => {
            let time = point_time(&clip, &lane, index)?;
            settle_on_item(app, key, &clip, &lane, index, time, Clicked::Point)
        }
        Under::Hit(index) => {
            let event = clip.events.get(index)?;
            let time = event.position;
            settle_on_item(app, key, &clip, &lane, index, time, Clicked::Hit)
        }
        Under::Space(time) => {
            let inside = app
                .clip_view
                .editor
                .content
                .as_ref()
                .filter(|s| s.key == key)
                .filter(|s| s.contains(&lane, time))
                .map(|s| s.range.start);
            match inside {
                Some(start) => (Clicked::TimeSelection, start),
                None => {
                    content::apply(
                        app,
                        content::Selection::time(key, time..time, vec![lane.clone()]),
                    );
                    (Clicked::Space, time)
                }
            }
        }
    };
    let target = ContentTarget {
        key,
        revision,
        lane,
        clicked,
        at,
    };
    // The value readout beside the pointer would draw across the menu.
    app.clip_view.editor.hover = None;
    app.open_overlay(Overlay::Context(OpenMenu {
        at: app.cursor,
        target: MenuTarget::Content(target.clone()),
    }));
    Some(target)
}

/// The time of point `index` on an envelope lane.
fn point_time(clip: &Clip, lane: &lanes::Identity, index: usize) -> Option<BeatTime> {
    let lanes::Identity::Envelope(target) = lane else {
        return None;
    };
    clip.lanes
        .iter()
        .filter_map(|l| l.envelope())
        .find(|e| e.target == *target)
        .and_then(|e| e.points.get(index))
        .map(|p| p.time)
}

/// A click on a point or a hit. Inside the time selection it keeps that selection; on one of
/// the picked items it keeps all the picks; anywhere else the item alone becomes the pick.
fn settle_on_item(
    app: &mut App,
    key: ClipKey,
    clip: &Clip,
    lane: &lanes::Identity,
    index: usize,
    time: BeatTime,
    clicked: Clicked,
) -> (Clicked, BeatTime) {
    let editor = &app.clip_view.editor;
    let selection = editor.content.as_ref().filter(|s| s.key == key);
    if let Some(selection) = selection.filter(|s| s.contains(lane, time)) {
        return (Clicked::TimeSelection, selection.range.start);
    }
    let picked = selection.is_some_and(|s| {
        s.picks.is_some()
            && match lane {
                lanes::Identity::Event(_) => s.hits.contains(&index),
                lanes::Identity::Envelope(target) => s.points.contains(&(target.clone(), index)),
            }
    });
    if !picked
        && let Some((item, _)) = content::Selection::items(clip)
            .into_iter()
            .find(|(item, i)| item.lane == *lane && *i == index)
    {
        content::pick(app, key, item, false);
    }
    (clicked, time)
}

/// Run one row of a content menu, after closing the menu.
pub(super) fn command(app: &mut App, target: ContentTarget, command: ContentCommand) {
    app.close_overlay();
    if !valid_content(app, &target) {
        return;
    }
    app.clip_view.editor.focused = true;
    let ContentTarget {
        key,
        revision,
        lane,
        clicked,
        at,
    } = target;
    match command {
        ContentCommand::Key(shortcut) => {
            regions::shortcut(app, shortcut);
            reconcile(app);
        }
        ContentCommand::Shape(shape) => {
            let place = match clicked {
                Clicked::TimeSelection => shapes::Place::Selection,
                _ => shapes::Place::Cell(at, lane),
            };
            handle(app, Action::InsertShape(key, revision, shape, place));
        }
        ContentCommand::Simplify => handle(app, Action::Simplify(key, revision)),
        ContentCommand::ClearEnvelope => {
            let lanes::Identity::Envelope(argument) = lane else {
                return;
            };
            if run_edit(
                app,
                Command::Edit(ProjectEdit::EditEnvelope {
                    track: key.track,
                    slot: key.slot,
                    edit: EnvelopeEdit::Remove { target: argument },
                }),
            ) {
                content::clear(&mut app.clip_view.editor);
                reconcile(app);
            }
        }
    }
}

#[cfg(test)]
mod tests;
