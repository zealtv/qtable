//! Layout and geometry live across views; playback only redraws the overlay.
use super::{CellStatus, Column, Grid, Master, Selection, x_positions};

use super::{draw, feedback, status_row};
use iced::widget::canvas::{self, Geometry};
use iced::{Point, Rectangle};
use qtable_engine::model::{Project, SceneId, TrackId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

pub(super) struct Layout {
    pub header_h: f32,
    pub columns: Vec<Column>,
    pub xs: Vec<f32>,
}

impl Layout {
    /// Where the last column ends: the start of the gap that holds the add-track hint.
    pub(super) fn end(&self) -> f32 {
        match (self.xs.last(), self.columns.last()) {
            (Some(x), Some(col)) => x + col.width,
            _ => super::GAP,
        }
    }
}

#[derive(Default)]
pub(crate) struct LayoutCache {
    current: RefCell<Option<(LayoutKey, Rc<Layout>)>>,
}

/// The engine revision plus the shape the layout indexes into. The shape is a
/// backstop: if a revision is ever missed, a changed track or group count
/// still forces a rebuild instead of out-of-bounds indexing at draw time.
#[derive(PartialEq, Eq)]
struct LayoutKey {
    revision: u64,
    tracks: usize,
}

impl LayoutCache {
    pub(super) fn get(&self, project: &Project, revision: u64) -> Rc<Layout> {
        let key = LayoutKey {
            revision,
            tracks: project.tracks.len(),
        };
        let mut current = self.current.borrow_mut();
        if let Some((cached, layout)) = current.as_ref()
            && *cached == key
        {
            return Rc::clone(layout);
        }
        let columns = super::layout::columns(project, qtable_engine::model::Region::Tracks);
        let layout = Rc::new(Layout {
            header_h: super::layout::header_height(&columns),
            xs: x_positions(&columns),
            columns,
        });
        *current = Some((key, Rc::clone(&layout)));
        layout
    }
}

/// The fitted-name map starts over once it holds this many times the labels
/// the last frame drew. Entries never go stale, since the key holds everything
/// `draw::fit` reads, so the bound only limits memory from names edited away.
const FITTED_PER_DRAWN_LABEL: usize = 4;
/// Floor for the bound, so a near-empty frame does not make it thrash.
const MIN_FITTED_NAMES: usize = 256;

#[derive(Default)]
pub(super) struct Names {
    fitted: RefCell<Fitted>,
}

#[derive(Default)]
struct Fitted {
    by_text: HashMap<String, HashMap<u32, String>>,
    entries: usize,
    /// `fit` calls so far in the frame being drawn, and in the one before it.
    drawn: usize,
    last_drawn: usize,
}

impl Fitted {
    fn limit(&self) -> usize {
        (self.last_drawn * FITTED_PER_DRAWN_LABEL).max(MIN_FITTED_NAMES)
    }
}

impl Names {
    /// Call once before each draw, so the bound follows the visible label count.
    pub fn begin_frame(&self) {
        let mut fitted = self.fitted.borrow_mut();
        fitted.last_drawn = std::mem::take(&mut fitted.drawn);
    }

    pub fn fit(&self, text: &str, width: f32) -> String {
        let mut fitted = self.fitted.borrow_mut();
        fitted.drawn += 1;
        if fitted.entries >= fitted.limit() {
            fitted.by_text.clear();
            fitted.entries = 0;
        }
        let Fitted {
            by_text, entries, ..
        } = &mut *fitted;
        let widths = by_text.entry(text.to_owned()).or_default();
        widths
            .entry(width.to_bits())
            .or_insert_with(|| {
                *entries += 1;
                draw::fit(text, width)
            })
            .clone()
    }
}

#[derive(Default)]
pub(super) struct GridState {
    pub feedback: feedback::Feedback,
    base: canvas::Cache,
    key: RefCell<Option<BaseKey>>,
    overlay: canvas::Cache,
    overlay_key: RefCell<Option<OverlayKey>>,
    names: Names,
    clicks: Clicks,
    /// A clip body held down, which becomes a drag once the cursor travels.
    pub clip_drag: Option<super::clip_drag::ClipDrag>,
    pub modifiers: iced::keyboard::Modifiers,
}

/// Presses closer than this many logical pixels, and within [`DOUBLE_CLICK_WINDOW`], chain.
const DOUBLE_CLICK_RADIUS: f32 = 6.0;
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(300);

/// The last left press; `presses` counts the chain (1 single, 2 double, 3 triple, then 2 again,
/// as iced's own click detector counts).
#[derive(Clone, Copy)]
struct Click {
    position: Point,
    time: Instant,
    presses: u8,
}

#[derive(Default)]
pub(super) struct Clicks(Option<Click>);

impl GridState {
    pub fn reset_click(&mut self) {
        self.clicks.0 = None;
    }

    /// Records a left press made at `time` and reports whether it completes a double-click.
    /// The caller passes the time so tests can use fixed instants.
    pub fn register_click(&mut self, position: Point, time: Instant) -> bool {
        self.clicks.register(position, time)
    }
}

impl Clicks {
    pub fn register(&mut self, position: Point, time: Instant) -> bool {
        let presses = match self.0 {
            Some(last)
                if last.position.distance(position) < DOUBLE_CLICK_RADIUS
                    && time > last.time
                    && time - last.time <= DOUBLE_CLICK_WINDOW =>
            {
                if last.presses == 2 {
                    3
                } else {
                    2
                }
            }
            _ => 1,
        };
        self.0 = Some(Click {
            position,
            time,
            presses,
        });
        presses == 2
    }
}

struct BaseKey {
    revision: u64,
    selected_tracks: Vec<TrackId>,
    selected: Option<Selection>,
    selected_scene: Option<SceneId>,
    owner: Option<crate::selection::ColumnOwner>,
    cell: Option<crate::selection::ColumnCell>,
}

#[derive(Debug, PartialEq)]
struct OverlayKey {
    drag: Option<super::header::ColumnDrag>,
    revision: u64,
    selected: Option<Selection>,
    selected_tracks: Vec<TrackId>,
    ghost_cells: Vec<(usize, usize)>,
    owner: Option<crate::selection::ColumnOwner>,
    cell: Option<crate::selection::ColumnCell>,
    active_cells: Vec<(usize, usize, CellStatus, bool)>,
    pending_stop: Vec<Option<usize>>,
    scene_stop: Vec<crate::scene_stop_feedback::Row>,
    blink_on: bool,
    focused: Option<crate::drafts::ControlId>,
}

#[cfg(test)]
mod tests;

impl OverlayKey {
    fn new(grid: &Grid<'_>) -> Self {
        let active_cells: Vec<_> = grid
            .cells
            .iter()
            .enumerate()
            .flat_map(|(track, cells)| {
                cells.iter().enumerate().filter_map(move |(slot, cell)| {
                    (cell.status != CellStatus::Stopped || cell.follow_queued).then_some((
                        track,
                        slot,
                        cell.status,
                        cell.follow_queued,
                    ))
                })
            })
            .collect();
        let blinking = grid.pending_stop.iter().any(Option::is_some)
            || active_cells.iter().any(|(_, _, status, follow_queued)| {
                *status == CellStatus::Triggered || *follow_queued
            });
        Self {
            drag: grid.drag.get(),
            revision: grid.revision,
            selected: grid.selected,
            selected_tracks: grid.selected_tracks.to_vec(),
            ghost_cells: grid
                .cells
                .iter()
                .enumerate()
                .flat_map(|(t, cells)| {
                    cells
                        .iter()
                        .enumerate()
                        .filter_map(move |(s, c)| c.ghost.then_some((t, s)))
                })
                .collect(),
            owner: grid.owner,
            cell: grid.cell,
            active_cells,
            pending_stop: grid.pending_stop.clone(),
            scene_stop: grid.scene_stop.clone(),
            blink_on: blinking && grid.blink_on,
            focused: grid.focused,
        }
    }
}

impl GridState {
    pub fn clear_feedback(&self) {
        self.overlay.clear();
    }

    pub fn draw(
        &self,
        grid: &Grid<'_>,
        renderer: &iced::Renderer,
        bounds: Rectangle,
    ) -> Vec<Geometry> {
        let mut key = self.key.borrow_mut();
        let changed = key.as_ref().is_none_or(|key| {
            key.revision != grid.revision
                || key.selected_tracks != grid.selected_tracks
                || key.selected != grid.selected
                || key.selected_scene != grid.selected_scene
                || key.owner != grid.owner
                || key.cell != grid.cell
        });
        if changed {
            self.base.clear();
            *key = Some(BaseKey {
                revision: grid.revision,
                selected_tracks: grid.selected_tracks.to_vec(),
                selected: grid.selected,
                selected_scene: grid.selected_scene,
                owner: grid.owner,
                cell: grid.cell,
            });
        }
        let next_overlay = OverlayKey::new(grid);
        let mut overlay_key = self.overlay_key.borrow_mut();
        if overlay_key.as_ref() != Some(&next_overlay) {
            self.overlay.clear();
            *overlay_key = Some(next_overlay);
        }
        vec![
            self.base.draw(renderer, bounds.size(), |frame| {
                self.names.begin_frame();
                if !grid.status_row {
                    draw::grid_base(grid, frame, &self.names);
                }
            }),
            self.overlay.draw(renderer, bounds.size(), |frame| {
                if grid.status_row {
                    status_row::tracks(grid, frame);
                } else {
                    draw::grid_overlay(grid, frame);
                }
                feedback::grid(grid, self.feedback, frame);
                grid.draw_focus(frame);
                if let Some(drag) = self.clip_drag.as_ref()
                    && let Some(to) = grid.clip_drop(drag, drag.cursor)
                {
                    super::clip_drag::draw(
                        frame,
                        grid,
                        to,
                        super::clip_drag::copy_modifier(self.modifiers),
                    );
                }
                if !grid.status_row
                    && let Some(drop) = grid.drag.get().and_then(|drag| grid.pending_drop(&drag))
                {
                    super::header::draw_drop(frame, &grid.layout.columns, &grid.layout.xs, &drop);
                }
            }),
        ]
    }
}

#[derive(Default)]
pub(super) struct MasterState {
    pub clicks: Clicks,
    pub feedback: feedback::Feedback,
    overlay: canvas::Cache,
    geometry: canvas::Cache,
    key: RefCell<Option<MasterKey>>,
    /// Focus only touches the overlay, so it is keyed apart from the geometry.
    focused: std::cell::Cell<Option<crate::drafts::ControlId>>,
    names: Names,
}

#[derive(Debug, PartialEq)]
struct MasterKey {
    revision: u64,
    selected_scene: Option<SceneId>,
    selected_scenes: Vec<SceneId>,
    owner: Option<crate::selection::ColumnOwner>,
    cell: Option<crate::selection::ColumnCell>,
    last_triggered_scene: Option<SceneId>,
    queued_follow_scene: Option<SceneId>,
    follow_enabled: bool,
    blink_on: bool,
}

impl MasterKey {
    fn new(master: &Master<'_>) -> Self {
        Self {
            revision: master.revision,
            selected_scene: master.selected_scene,
            selected_scenes: master.selected_scenes.to_vec(),
            owner: master.owner,
            cell: master.cell,
            last_triggered_scene: master.last_triggered_scene,
            queued_follow_scene: master.queued_follow_scene,
            follow_enabled: master.follow_enabled,
            blink_on: master.queued_follow_scene.is_some() && master.blink_on,
        }
    }
}

impl MasterState {
    pub fn clear_feedback(&self) {
        self.overlay.clear();
    }

    pub fn draw(
        &self,
        master: &Master<'_>,
        renderer: &iced::Renderer,
        bounds: Rectangle,
    ) -> Vec<Geometry> {
        let next = MasterKey::new(master);
        let mut key = self.key.borrow_mut();
        if key.as_ref() != Some(&next) {
            self.geometry.clear();
            self.overlay.clear();
            *key = Some(next);
        }
        if self.focused.replace(master.focused) != master.focused {
            self.overlay.clear();
        }
        vec![
            self.geometry.draw(renderer, bounds.size(), |frame| {
                self.names.begin_frame();
                if master.status_row {
                    status_row::master(frame, master);
                } else {
                    draw::master(master, frame, &self.names);
                }
            }),
            self.overlay.draw(renderer, bounds.size(), |frame| {
                feedback::master(master, self.feedback, frame);
                master.draw_focus(frame);
            }),
        ]
    }
}
