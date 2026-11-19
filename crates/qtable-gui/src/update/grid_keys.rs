//! Keyboard navigation of the Session grid: arrows move a cursor between track titles,
//! clip slots and scenes (Live 12 manual, Accessibility and Keyboard Navigation, 2026-10-11).
use super::{edit, grid_actions};
use crate::app::App;
use crate::edit_actions::execute;
use crate::focus::{Direction, Focus, GridKey};
use crate::grid;
use crate::messages::EditMessage;
use qtable_engine::engine::Command;
use qtable_engine::model::{Project, TrackId};

use grid::layout::{Column, ColumnKey};

/// Where the keyboard cursor sits, as a column or row index into the visible grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cursor {
    Header(usize),
    Slot(usize, usize),
    Scene(usize),
}

pub(super) fn update(app: &mut App, key: GridKey) {
    if app.focus != Focus::Grid {
        return;
    }
    if matches!(key, GridKey::Move(_) | GridKey::Launch) {
        app.control_focus.clear();
    }
    match key {
        GridKey::Move(direction) => step(app, direction),
        GridKey::Reorder(direction) => reorder(app, direction),
        GridKey::Launch => launch(app),
    }
}

fn columns(project: &Project) -> Vec<Column> {
    grid::layout::columns(project, qtable_engine::model::Region::Tracks)
}

fn track_id(column: &Column) -> TrackId {
    let ColumnKey::Track(id) = column.key;
    id
}

fn column_of(columns: &[Column], track: TrackId) -> Option<usize> {
    columns.iter().position(|c| track_id(c) == track)
}

/// The cursor implied by the selection: a clip wins over a scene, a scene over a track title.
fn cursor(app: &App, columns: &[Column]) -> Option<Cursor> {
    let selection = &app.selection;
    if let Some(clip) = selection.clip {
        return Some(Cursor::Slot(column_of(columns, clip.track)?, clip.slot));
    }
    if let Some(scene) = selection.scene {
        return Some(Cursor::Scene(
            app.project().scenes.iter().position(|s| s.id == scene)?,
        ));
    }
    if let Some(cell) = selection.cell
        && cell.active
        && let crate::selection::ColumnOwner::Group(group) = cell.owner
    {
        return Some(Cursor::Slot(column_of(columns, group)?, cell.row));
    }
    if let Some(crate::selection::ColumnOwner::Group(group)) = selection.owner {
        return Some(Cursor::Header(column_of(columns, group)?));
    }
    let track = selection
        .anchor
        .or_else(|| selection.tracks.last().copied())?;
    Some(Cursor::Header(column_of(columns, track)?))
}

fn moved(cursor: Cursor, direction: Direction, columns: usize, scenes: usize) -> Cursor {
    use Direction::*;
    match (cursor, direction) {
        (Cursor::Header(c), Left) => Cursor::Header(c.saturating_sub(1)),
        (Cursor::Header(c), Right) => Cursor::Header((c + 1).min(columns - 1)),
        (Cursor::Header(c), Down) if scenes > 0 => Cursor::Slot(c, 0),
        (Cursor::Slot(c, r), Left) => Cursor::Slot(c.saturating_sub(1), r),
        (Cursor::Slot(c, r), Right) if c + 1 < columns => Cursor::Slot(c + 1, r),
        (Cursor::Slot(_, r), Right) => Cursor::Scene(r),
        (Cursor::Slot(c, 0), Up) => Cursor::Header(c),
        (Cursor::Slot(c, r), Up) => Cursor::Slot(c, r - 1),
        (Cursor::Slot(c, r), Down) => Cursor::Slot(c, (r + 1).min(scenes - 1)),
        (Cursor::Scene(r), Left) => Cursor::Slot(columns - 1, r),
        (Cursor::Scene(r), Up) => Cursor::Scene(r.saturating_sub(1)),
        (Cursor::Scene(r), Down) => Cursor::Scene((r + 1).min(scenes - 1)),
        (cursor, _) => cursor,
    }
}

fn step(app: &mut App, direction: Direction) {
    let columns = columns(app.project());
    let scenes = app.project().scenes.len();
    if columns.is_empty() {
        return;
    }
    // Slot-bearing moves need at least one scene; an empty grid only has title bars.
    let target = match cursor(app, &columns) {
        Some(current) => moved(current, direction, columns.len(), scenes),
        None if scenes > 0 => Cursor::Slot(0, 0),
        None => Cursor::Header(0),
    };
    // Navigating cancels a pending "Move clip…" instead of dropping the clip on the cursor.
    app.selection.move_clip = false;
    match target {
        Cursor::Slot(c, r) => {
            let action = if columns[c].caps.foldable {
                grid::Action::SelectColumn(
                    crate::selection::ColumnOwner::Group(track_id(&columns[c])),
                    Some(r),
                )
            } else {
                grid::Action::Select(track_id(&columns[c]), r)
            };
            grid_actions::update(app, action);
            app.selection.scene = None;
        }
        Cursor::Scene(r) => {
            grid_actions::update(app, grid::Action::SelectScene(app.project().scenes[r].id));
        }
        Cursor::Header(c) => {
            crate::edit_actions::clear_drafts(app);
            let action = if columns[c].caps.foldable {
                grid::Action::SelectGroup(track_id(&columns[c]), false)
            } else {
                grid::Action::SelectTrack(track_id(&columns[c]), false)
            };
            grid_actions::update(app, action);
            app.selection.clip = None;
            app.selection.scene = None;
        }
    }
}

/// Move the selected track left/right or the selected scene up/down, as the removed
/// Track and Scene buttons did.
fn reorder(app: &mut App, direction: Direction) {
    // Tracks move sideways from a slot or title, scenes vertically from a slot or scene.
    let columns = columns(app.project());
    let sideways = matches!(direction, Direction::Left | Direction::Right);
    let applies = match cursor(app, &columns) {
        Some(Cursor::Slot(..)) => true,
        Some(Cursor::Header(_)) => sideways,
        Some(Cursor::Scene(_)) => !sideways,
        None => false,
    };
    if !applies {
        return;
    }
    let delta = match direction {
        Direction::Left | Direction::Up => -1,
        Direction::Right | Direction::Down => 1,
    };
    let message = match direction {
        Direction::Left | Direction::Right => EditMessage::ReorderTrack(delta),
        Direction::Up | Direction::Down => EditMessage::ReorderScene(delta),
    };
    let _ = edit::update(app, message);
}

fn launch(app: &mut App) {
    // A fold changes visibility, not the identity/scope of a retained selection.
    if let Some(selected) = app.selection.clip {
        let track = selected.track;
        let slot = selected.slot;
        let cell = app.project().clip_slot(track, slot);
        // Match a click on the launch button: clips launch, stop-button slots stop.
        if cell.is_some_and(|s| s.clip.is_some() || s.has_stop_button) {
            execute(app, Command::LaunchClip { track, slot });
        }
    } else if let Some(scene) = app.selection.scene {
        execute(app, Command::LaunchScene { scene });
    } else if let Some(cell) = app.selection.cell.filter(|cell| cell.active)
        && let crate::selection::ColumnOwner::Group(group) = cell.owner
    {
        execute(
            app,
            Command::LaunchGroupSlot {
                group,
                slot: cell.row,
            },
        );
    }
}
