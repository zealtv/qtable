//! ⌘C and ⌘V on selected columns. The clipboard holds whole `Track`
//! values in the GUI session, like the clip clipboard; the engine's `PasteTracks` makes the
//! copies and the GUI never predicts ids.
use crate::app::App;
use crate::clip_actions::GridClipboard;
use crate::edit_actions::execute_structural_edit;
use crate::grid;
use crate::messages::Message;
use crate::selection::ColumnOwner;
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::{Region, Track, TrackId};
use std::collections::HashSet;

/// Copies the selected columns, in project order, as one family. Main, a selection of
/// nothing, or a column that cannot be copied leaves the clipboard as it was.
pub(crate) fn copy(app: &mut App) {
    let tracks = selected_columns(app);
    if !tracks.is_empty() {
        app.clip_actions.clipboard = Some(GridClipboard::Tracks {
            generation: app.project_generation,
            tracks,
        });
    }
}

/// The selected columns as `Track` values: outputs when an output is selected, otherwise
/// each selected subtree once, hidden descendants included (`Project::closure_tracks`).
fn selected_columns(app: &App) -> Vec<Track> {
    let project = app.project();
    let selected = &app.selection.tracks;
    if let Some(first) = selected.first().and_then(|id| project.track(*id))
        && first.capabilities().region == Region::Outputs
    {
        return project
            .outputs
            .iter()
            .filter(|o| selected.contains(&o.id) && o.capabilities().copyable)
            .cloned()
            .collect();
    }
    project
        .closure_tracks(&selected_ids(app))
        .unwrap_or_default()
}

/// The selected tracks and groups in project order. An explicitly selected group makes its
/// descendants part of the same selection, so callers reduce this to subtree roots.
fn selected_ids(app: &App) -> Vec<TrackId> {
    app.project()
        .tracks
        .iter()
        .filter(|t| t.capabilities().copyable && column_selected(app, t))
        .map(|t| t.id)
        .collect()
}

/// A plain track is selected when it is in the selection; a group track when its own
/// header is the owner or its header id is in the selected range.
fn column_selected(app: &App, track: &Track) -> bool {
    if !track.is_group() {
        return app.selection.tracks.contains(&track.id);
    }
    crate::selection::SelectionState::group_selected(
        &app.selection.tracks,
        &app.project().tracks,
        track.id,
    )
}

/// True when the grid clipboard holds copied tracks, which the header menu's Paste needs.
pub(crate) fn holds_tracks(app: &App) -> bool {
    matches!(app.clip_actions.clipboard, Some(GridClipboard::Tracks { generation, .. }) if generation == app.project_generation)
}

/// A header menu only pastes columns from its own region.
pub(crate) fn can_paste_region(app: &App, region: Region) -> bool {
    holds_tracks(app)
        && matches!(&app.clip_actions.clipboard,
        Some(GridClipboard::Tracks { tracks, .. })
            if tracks.first().is_some_and(|t| t.capabilities().region == region))
}

/// The header menu's Paste: pastes the held tracks, never a clip.
pub(crate) fn paste_held(app: &mut App) {
    if holds_tracks(app) {
        crate::clip_actions::paste(app);
    }
}

/// Pastes before the leftmost selected column of the clipboard's family, else at the end of
/// its region, then selects the copies.
pub(crate) fn paste(app: &mut App, tracks: Vec<Track>) {
    let Some(first) = tracks.first() else {
        return;
    };
    let region = first.capabilities().region;
    let (before, group) = target(app, region);
    submit(app, tracks, before, group);
}

/// ⌘D on a header selection: copies the selected columns right after the source block as one
/// `PasteTracks` edit and selects the copy. The clipboard stays as it was. The copies join the
/// last selected root's parent, so a nested group or member duplicates beside its source.
pub(crate) fn duplicate(app: &mut App) {
    let tracks = selected_columns(app);
    let Some(last) = tracks.last() else {
        return;
    };
    let project = app.project();
    let (before, group) = if last.capabilities().region == Region::Outputs {
        let at = project.outputs.iter().position(|t| t.id == last.id);
        let next = at.and_then(|at| project.outputs.get(insertion_index(at, at, true)));
        (next.map(|t| t.id), None)
    } else {
        // The block ends where the last selected root's whole subtree ends.
        let Ok(topology) = project.topology() else {
            return;
        };
        let roots = topology.subtree_roots(&selected_ids(app));
        let Some(root) = roots.ok().and_then(|roots| roots.last().copied()) else {
            return;
        };
        let end = topology
            .span(root)
            .map_or(project.tracks.len(), |span| span.end);
        (
            project.tracks.get(end).map(|t| t.id),
            project.track(root).and_then(|t| t.group),
        )
    };
    submit(app, tracks, before, group);
}

/// Submits one `PasteTracks` edit and selects the columns it added.
fn submit(app: &mut App, tracks: Vec<Track>, before: Option<TrackId>, group: Option<TrackId>) {
    let known: HashSet<TrackId> = column_ids(app).into_iter().collect();
    if !execute_structural_edit(
        app,
        Command::Edit(ProjectEdit::PasteTracks {
            tracks,
            before,
            group,
        }),
    ) {
        return;
    }
    let fresh: Vec<TrackId> = column_ids(app)
        .into_iter()
        .filter(|id| !known.contains(id))
        .collect();
    select_pasted(app, &fresh);
}

/// Where a paste goes: `before` and `group` as `PlaceTrack` takes them.
///
// Not yet decided (2026-11-06): the nested Paste destination follows the design's
// recommendation (section 5): copied roots, plain or Group, enter the leftmost selected
// column's own parent, just before that column, whatever level it sits at. A selected hidden
// column pastes at its hidden position; relocation when an ancestor folds is still D2.
fn target(app: &App, region: Region) -> (Option<TrackId>, Option<TrackId>) {
    let project = app.project();
    let selected = &app.selection.tracks;
    if region == Region::Outputs {
        let leftmost = project.outputs.iter().find(|o| selected.contains(&o.id));
        return (leftmost.map(|o| o.id), None);
    }
    let leftmost = project
        .tracks
        .iter()
        .find(|t| column_selected(app, t) || app.selection.clip.is_some_and(|c| c.track == t.id));
    match leftmost {
        Some(target) => (Some(target.id), target.group),
        None => (None, None),
    }
}

/// Every track, group and output id in the project, tracks first, each in project order.
fn column_ids(app: &App) -> Vec<TrackId> {
    let project = app.project();
    project
        .tracks
        .iter()
        .chain(&project.outputs)
        .map(|t| t.id)
        .collect()
}

/// Makes the pasted columns the header selection, anchored at the first. A pasted group
/// selects its explicit header; several pasted roots select those roots only.
fn select_pasted(app: &mut App, fresh: &[TrackId]) {
    let project = app.project();
    let roots: Vec<TrackId> = fresh
        .iter()
        .copied()
        .filter(|id| {
            project
                .track(*id)
                .is_some_and(|t| t.group.is_none_or(|g| !fresh.contains(&g)))
        })
        .collect();
    let Some(&first) = roots.first() else {
        return;
    };
    let is_group = project.track(first).is_some_and(Track::is_group);
    let action = if is_group {
        grid::Action::SelectGroup(first, false)
    } else {
        grid::Action::SelectColumn(ColumnOwner::Track(first), None)
    };
    let _ = crate::update::update(app, Message::Grid(action));
    if !is_group || roots.len() > 1 {
        // Clipboard closure and header selection are separate: effects are expanded on use.
        app.selection.tracks = roots;
    }
    if let Some(&anchor) = app.selection.tracks.first() {
        app.selection.anchor = Some(anchor);
    }
}

/// Capture-only fixture for the paste screenshots: the route fixture's Rhythm group, its
/// outputs and a top-level Bass track, with clips so a pasted copy shows what came along.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture_project() -> qtable_engine::model::Project {
    use qtable_engine::model::{BeatTime, Clip, ClipId, LIVE_CLIP_PALETTE, Route};
    let mut project = crate::output_band::capture_project();
    let mut bass = Track::new(TrackId(3), "Bass", project.scenes.len());
    bass.colour = LIVE_CLIP_PALETTE[20];
    bass.route = Route::single(TrackId(202));
    project.tracks.push(bass);
    let mut next = 1;
    for (track, slots) in [
        (TrackId(1), [0, 2]),
        (TrackId(2), [1, 3]),
        (TrackId(3), [0, 4]),
    ] {
        for slot in slots {
            let colour = project[track].colour;
            let name = format!("{} {}", project[track].name, slot + 1);
            let clip = Clip::new(ClipId(next), name, BeatTime(3840), colour);
            project[track].slots[slot].clip = clip.ok();
            next += 1;
        }
    }
    project
}

/// True when a paste capture wants `capture_project` rather than the one-track default.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture_wants_fixture() -> bool {
    capture_mode().is_some()
}

#[cfg(any(test, feature = "dev-hooks"))]
fn capture_mode() -> Option<&'static str> {
    let track = std::env::var("QTABLE_CAPTURE_TRACK").ok();
    let output = std::env::var("QTABLE_CAPTURE_OUTPUT").ok();
    match (track.as_deref(), output.as_deref()) {
        (Some("paste" | "duplicate"), _) => Some("track"),
        (Some("menu-paste"), _) => Some("menu"),
        (Some("group-paste" | "group-duplicate"), _) => Some("group"),
        (_, Some("paste" | "duplicate")) => Some("output"),
        _ => None,
    }
}

/// Capture-only: `QTABLE_CAPTURE_TRACK=paste`, `group-paste` or `QTABLE_CAPTURE_OUTPUT=paste`
/// select the first top-level track, group or output, then run ⌘C and ⌘V through the
/// shortcut router, so the screenshot shows a paste with no desktop input. The `duplicate`
/// and `group-duplicate` values run ⌘D instead.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture(app: &mut App) {
    let project = app.project();
    let action = match capture_mode() {
        Some("track" | "menu") => project
            .tracks
            .iter()
            .find(|t| !t.is_group() && t.group.is_none())
            .map(|t| grid::Action::SelectTrack(t.id, false)),
        Some("group") => project
            .group_tracks()
            .next()
            .map(|g| grid::Action::SelectGroup(g.id, false)),
        Some("output") => project
            .outputs
            .first()
            .map(|o| grid::Action::SelectColumn(ColumnOwner::Track(o.id), None)),
        _ => None,
    };
    let Some(action) = action else {
        return;
    };
    let _ = crate::update::update(app, Message::Grid(action));
    app.focus = crate::focus::Focus::Grid;
    if capture_mode() == Some("menu") {
        let _ = crate::clip_shortcuts::update(app, crate::clip_shortcuts::Shortcut::Copy);
        let Some(ColumnOwner::Track(id)) = app.selection.owner else {
            return;
        };
        app.cursor = iced::Point::new(420.0, 80.0);
        let _ = crate::update::update(app, Message::Grid(grid::Action::HeaderMenu(id)));
        return;
    }
    let duplicate = [
        std::env::var("QTABLE_CAPTURE_TRACK").ok(),
        std::env::var("QTABLE_CAPTURE_OUTPUT").ok(),
    ]
    .iter()
    .flatten()
    .any(|v| v.ends_with("duplicate"));
    let shortcuts = if duplicate {
        vec![crate::clip_shortcuts::Shortcut::Duplicate]
    } else {
        vec![
            crate::clip_shortcuts::Shortcut::Copy,
            crate::clip_shortcuts::Shortcut::Paste,
        ]
    };
    for shortcut in shortcuts {
        let _ = crate::clip_shortcuts::update(app, shortcut);
    }
}

#[cfg(test)]
mod nested_tests;
#[cfg(test)]
mod tests;

/// Paste goes before a header selection; Duplicate goes immediately after its block.
/// Shared with the clip's horizontal lane headers.
pub(crate) fn insertion_index(first: usize, last: usize, duplicate: bool) -> usize {
    if duplicate { last + 1 } else { first }
}
