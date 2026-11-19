//! Canvas-backed Session View grid: track and group track columns, clip slots and clicks.
//! Geometry follows Live 12's Session View: ~20 px rows, ~5.2 rows per track column.
use crate::drafts::ControlId;
#[cfg(test)]
use iced::Point;
use iced::mouse;
use iced::widget::canvas::{self, Geometry, Program};
use iced::{Rectangle, Theme};
use qtable_engine::engine::TrackSnapshot;
use qtable_engine::model::{Clip, Scene, SceneId, Track, TrackId};

pub(crate) mod band;
mod cache;
mod clip_drag;
mod context;
mod detail_open;
mod drag_capture;
mod draw;
mod edge_scroll;
pub(crate) mod scene_follow;
pub(crate) use draw::COLUMN_TINT;
mod feedback;
mod group_column;
pub(crate) mod header;
mod info;
pub(crate) mod layout;
mod navigation;
pub(crate) mod scene_area;
mod scene_drag;
mod status_row;
mod view;

pub(crate) use band::{CellBuilder, Row};
pub(crate) use cache::LayoutCache;
use cache::{GridState, Layout, MasterState};
use layout::{Column, column_at, hit_test_layout, master_hit_test, x_positions};
#[cfg(test)]
use layout::{hit_test, track_columns};
#[cfg(test)]
pub use view::view;
pub(crate) use view::{Extras, NameEditor, RenameEditor};
pub use view::{ViewState, view_with_outputs};
pub(crate) use view::{scroll_columns, scroll_scenes};

/// Shared Session View width for tracks, group tracks, outputs and Main.
/// Fixed for now; a later narrow mode can collapse labels at this geometry boundary.
pub(crate) const COLUMN_W: f32 = 104.0;
pub(super) const TRACK_W: f32 = COLUMN_W;
pub(crate) const TRACK_H: f32 = 20.0;
/// Top of the first clip row: the title row and a gap.
const HEADER_H: f32 = TRACK_H + GAP;
const CELL_H: f32 = 20.0;
pub(crate) const FOOTER_H: f32 = 20.0;
const MASTER_W: f32 = COLUMN_W;
const GAP: f32 = 1.0;
/// Bright core in a retained Live scene-drag recording (2026-11-06).
const SCENE_DROP_LINE: f32 = 3.0;
/// The fold button at the right end of a group track's title.
const FOLD_W: f32 = 20.0;
/// A clip's name starts this far past its launch area and keeps as much from the slot's right edge.
const CLIP_NAME_INSET: f32 = 4.0;
/// A scene's name starts this far past its launch area, leaving room at the right for its number.
const SCENE_NAME_INSET: f32 = 2.0;
const SCENE_NUMBER_ROOM: f32 = 24.0;
/// The scene number's inset from the Main column's right edge.
const SCENE_NUMBER_INSET: f32 = 5.0;
/// The add-track hint in the gap after the last track: its inset and line spacing.
const HINT_INSET: f32 = 10.0;
const HINT_LINE_H: f32 = 16.0;
/// A playing clip's progress bar along the slot's bottom edge.
const PROGRESS_H: f32 = 2.0;
/// Slot outlines: selected, triggered or an active cell; and a ghost or inactive cell.
const SELECTION_OUTLINE: f32 = 2.0;
const THIN_OUTLINE: f32 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub track: TrackId,
    pub slot: usize,
}

/// Whatever the one in-place name editor can be open on besides a clip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RenameTarget {
    Header(TrackId),
    Scene(SceneId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Info(crate::info::Hover),
    /// A canvas button was clicked; it takes the window's focus outline.
    ControlFocused(ControlId),
    LaunchClip(TrackId, usize),
    LaunchGroupSlot(TrackId, usize),
    LaunchScene(SceneId),
    SelectScene(SceneId),
    /// Pointer selection, using the current Shift/Command modifiers.
    ClickScene(SceneId),
    MoveScenes {
        scenes: Vec<SceneId>,
        boundary: usize,
    },
    SelectColumn(crate::selection::ColumnOwner, Option<usize>),
    SelectBlankColumn(crate::selection::ColumnOwner),
    /// Double-click in the gap after the last track: add a track there.
    AddTrack,
    StopTrack(TrackId),
    StopSlot(TrackId, usize),
    StopAll,
    /// Click on Main's global Follow Actions button.
    ToggleFollowActions,
    Select(TrackId, usize),
    /// Double-click on an empty slot: select it and create a clip there.
    CreateClip(TrackId, usize),
    /// Double-click on an existing clip: select it and open the Detail View on it.
    OpenClip(TrackId, usize),
    /// A pointer gesture selects its target and reveals the matching Detail tab.
    OpenDetail(Box<Action>),
    /// Right-click on a slot: select it and open the clip context menu.
    ContextMenu(TrackId, usize),
    /// Right-click on a track title or group header: open its menu.
    HeaderMenu(TrackId),
    /// Right-click on a scene in Main: select it and open its menu.
    SceneMenu(SceneId),
    /// Right-click on empty space in a region (the outputs area): open its Insert menu.
    RegionMenu(qtable_engine::model::Region),
    /// Pointer-only header selection: preserve selected drag sources until release.
    PressHeader(Box<Action>),
    SelectTrack(TrackId, bool),
    /// Click on a group header: select all its tracks (`true` extends the selection, as for tracks).
    SelectGroup(TrackId, bool),
    ToggleGroup(TrackId),
    StopGroup(TrackId),
    /// A dragged track title dropped: before `before` (last when None), in `group` or none.
    PlaceTrack {
        track: TrackId,
        before: Option<TrackId>,
        group: Option<TrackId>,
    },
    PlaceTracks {
        tracks: Vec<TrackId>,
        before: Option<TrackId>,
        group: Option<TrackId>,
    },
    /// A clip body dragged onto an existing track slot.
    DropClip {
        from: Selection,
        to: Selection,
        copy: bool,
    },
    /// Horizontal scroll of the clips (`footer: false`) or of the pinned stop row and band.
    ScrollColumns {
        region: qtable_engine::model::Region,
        footer: bool,
        offset: u32,
    },
    /// Vertical scroll of the scene rows, in px below the pinned header row.
    ScrollScenes {
        offset: u32,
    },
    /// Held-edge scene scrolling requests a scroll; ScrollScenes reports it.
    DragScrollScenes {
        offset: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    ClipLaunch(usize, usize),
    ClipName(usize, usize),
    TrackStop(usize),
    /// A group track's slot: its launch (or stop) square, by group index and row.
    GroupLaunch(usize, usize),
    /// The rest of a group track's slot.
    GroupName(usize, usize),
    /// A group track's stop square in the status row, by group index.
    GroupStop(usize),
    SceneLaunch(usize),
    SceneSelect(usize),
    StopAll,
    FollowToggle,
}

/// Triggered and follow-queued blink: 5 ticks of 33 ms each way, a 0.33 s period.
/// Measured in Live 12.1 on 2026-10-11 (window captures sampled every 33 ms): the
/// follow-queued play icon pulsed every 0.39 s at 120 BPM and 0.33 s at 90 BPM, so
/// Live's pulse is a fixed timer of about 0.33-0.39 s, not tempo-locked.
pub fn blink_phase(tick_count: u64) -> bool {
    (tick_count / 5).is_multiple_of(2)
}
#[derive(Clone)]
struct Cell<'a> {
    clip: Option<&'a Clip>,
    stop_button: bool,
    status: CellStatus,
    /// A pending follow action will launch this slot: its play icon pulses.
    follow_queued: bool,
    selected: bool,
    ghost: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum CellStatus {
    Stopped,
    Playing(f32),
    Triggered,
}

fn cell_status(state: Option<&TrackSnapshot>, slot: usize) -> CellStatus {
    let Some(state) = state else {
        return CellStatus::Stopped;
    };
    if state.triggered.as_ref().is_some_and(|v| v.slot == slot) {
        return CellStatus::Triggered;
    }
    if state.playing_slot == Some(slot) {
        let progress = state.pass_progress.unwrap_or(0.0) as f32;
        return CellStatus::Playing(progress.clamp(0.0, 1.0));
    }
    CellStatus::Stopped
}

/// Live pulses the play icon of the clip a pending follow action will launch
/// (2026-10-11), the playing clip included. A manual trigger keeps its
/// own whole-cell blink instead.
fn follow_queued(state: Option<&TrackSnapshot>, slot: usize, status: CellStatus) -> bool {
    status != CellStatus::Triggered
        && state.is_some_and(|state| state.queued_follow_slot == Some(slot))
}

#[derive(Clone)]
struct Master<'a> {
    header_h: f32,
    status_row: bool,
    main: &'a Track,
    clip: Option<Selection>,
    scenes: &'a [Scene],
    selected_scene: Option<SceneId>,
    selected_scenes: &'a [SceneId],
    owner: Option<crate::selection::ColumnOwner>,
    cell: Option<crate::selection::ColumnCell>,
    last_triggered_scene: Option<SceneId>,
    queued_follow_scene: Option<SceneId>,
    follow_enabled: bool,
    blink_on: bool,
    revision: u64,
    focused: Option<ControlId>,
}

impl Program<Action> for Master<'_> {
    type State = MasterState;

    fn update(
        &self,
        state: &mut MasterState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Action>> {
        let hit = cursor.position_in(bounds).and_then(|p| self.button_hit(p));
        let changed = state.feedback.update(event, hit, self.revision);
        if changed {
            state.clear_feedback();
        }
        if let canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) = event
            && !self.status_row
            && let Some(p) = cursor.position_in(bounds)
        {
            if p.y < self.header_h && p.x < MASTER_W {
                return Some(canvas::Action::publish(header::menu(&Column::main(
                    self.main.id,
                ))));
            }
            if let Some(Hit::SceneLaunch(slot) | Hit::SceneSelect(slot)) =
                master_hit_test(p, self.scenes.len(), self.header_h)
            {
                return Some(canvas::Action::publish(Action::SceneMenu(
                    self.scenes.get(slot)?.id,
                )));
            }
        }
        if !matches!(
            event,
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) {
            return changed.then(canvas::Action::request_redraw);
        }
        let p = cursor.position_in(bounds)?;
        let double_click = state.clicks.register(p, crate::gesture_clock::now());
        if p.x < 0.0 || p.x >= MASTER_W {
            return None;
        }
        if !self.status_row
            && (p.y < self.header_h
                || p.y >= self.header_h + self.scenes.len() as f32 * (CELL_H + GAP))
        {
            return Some(canvas::Action::publish(if p.y < self.header_h {
                detail_open::click(
                    header::press(&Column::main(self.main.id), p, 0.0, false, self.revision).0,
                    crate::inspector_panel::Tab::Main,
                    double_click,
                )
            } else {
                Action::SelectBlankColumn(crate::selection::ColumnOwner::Track(self.main.id))
            }));
        }
        let hit = if self.status_row {
            status_row::master_hit(p)
        } else {
            master_hit_test(p, self.scenes.len(), self.header_h)
        }?;
        let action = match hit {
            Hit::SceneLaunch(slot) => Action::LaunchScene(self.scenes.get(slot)?.id),
            Hit::SceneSelect(slot) => detail_open::click(
                Action::ClickScene(self.scenes.get(slot)?.id),
                crate::inspector_panel::Tab::Scene,
                double_click,
            ),
            Hit::StopAll => Action::StopAll,
            Hit::FollowToggle => Action::ToggleFollowActions,
            _ => return None,
        };
        Some(canvas::Action::publish(action))
    }
    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        feedback::interaction(cursor.position_in(bounds).and_then(|p| self.button_hit(p)))
    }

    fn draw(
        &self,
        state: &MasterState,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        state.draw(self, renderer, bounds)
    }
}

#[derive(Clone)]
struct Grid<'a> {
    drag: header::Gesture,
    drag_viewport: crate::control::viewport::Viewport,
    drag_h: f32,
    status_row: bool,
    tracks: &'a [Track],
    scenes: &'a [Scene],
    cells: Vec<Vec<Cell<'a>>>,
    pending_stop: Vec<Option<usize>>,
    scene_stop: Vec<crate::scene_stop_feedback::Row>,
    blink_on: bool,
    selected_tracks: &'a [TrackId],
    revision: u64,
    layout: std::rc::Rc<Layout>,
    shift_down: bool,
    selected: Option<Selection>,
    selected_scene: Option<SceneId>,
    owner: Option<crate::selection::ColumnOwner>,
    cell: Option<crate::selection::ColumnCell>,
    /// The window's focused control, outlined when it is one of this canvas's buttons.
    focused: Option<ControlId>,
}

/// A group slot's launch or stop square acts on the whole group's row, never one member's clip.
fn group_slot_action(
    group: TrackId,
    tracks: &[Track],
    members: &[usize],
    row: usize,
) -> Option<Action> {
    let mut active = false;
    for &member in members {
        let slot = tracks.get(member)?.slots.get(row)?;
        active |= slot.clip.is_some() || slot.has_stop_button;
    }
    if active {
        Some(Action::LaunchGroupSlot(group, row))
    } else {
        Some(Action::SelectColumn(
            crate::selection::ColumnOwner::Group(group),
            Some(row),
        ))
    }
}

impl Grid<'_> {
    /// The group track column of group `gi`.
    fn group_column(&self, gi: usize) -> Option<&Column> {
        self.layout
            .columns
            .iter()
            .find(|c| c.group_track() == Some(gi))
    }
}

impl Grid<'_> {
    pub(super) fn selection_view(&self) -> crate::selection::SelectionView<'_> {
        crate::selection::SelectionView {
            models: self.tracks,
            tracks: self.selected_tracks,
            owner: self.owner,
            cell: self.cell,
            clip: self.selected,
            scene: self.selected_scene,
        }
    }

    pub(super) fn column_selected(&self, col: &Column) -> bool {
        self.selection_view().column_selected(col)
    }
}

impl Program<Action> for Grid<'_> {
    type State = GridState;
    fn update(
        &self,
        state: &mut GridState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Action>> {
        let hit = cursor.position_in(bounds).and_then(|p| self.button_hit(p));
        let changed = state.feedback.update(event, hit, self.revision);
        if changed {
            state.clear_feedback();
        }
        if let Some(action) = self.clip_drag_update(state, event, cursor, bounds) {
            return Some(action);
        }
        if !self.status_row
            && let Some(action) = header::shared_drag_update(
                (self.tracks, self.revision),
                &self.layout.columns,
                &self.layout.xs,
                &self.drag,
                event,
                cursor,
                header::Surface {
                    bounds: Rectangle {
                        height: self.drag_h.max(bounds.height),
                        ..bounds
                    },
                    visible: self.drag_viewport.get(),
                    pinned: bounds.height <= self.layout.header_h,
                },
            )
        {
            state.clear_feedback();
            return Some(action);
        }
        if let canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) = event {
            let p = cursor.position_in(bounds)?;
            return self.context_action(p).map(canvas::Action::publish);
        }
        if !matches!(
            event,
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) {
            return changed.then(canvas::Action::request_redraw);
        }
        let p = cursor.position_in(bounds)?;
        let double_click = state.register_click(p, crate::gesture_clock::now());
        let cols = &self.layout.columns;
        if !self.status_row && double_click && p.x >= self.layout.end() {
            return Some(canvas::Action::publish(Action::AddTrack));
        }
        if !self.status_row && p.y < self.layout.header_h {
            let (ci, col) = column_at(cols, &self.layout.xs, p.x)?;
            let (action, drag) =
                header::press(col, p, self.layout.xs[ci], self.shift_down, self.revision);
            self.drag.set(drag);
            return Some(canvas::Action::publish(
                if matches!(action, Action::ToggleGroup(_)) {
                    action
                } else {
                    detail_open::click(
                        Action::PressHeader(Box::new(action)),
                        crate::inspector_panel::Tab::Track,
                        double_click,
                    )
                },
            ));
        }
        if !self.status_row
            && p.y >= self.layout.header_h + self.scenes.len() as f32 * (CELL_H + GAP)
            && let Some((_, col)) = column_at(cols, &self.layout.xs, p.x)
        {
            let owner = if let Some(gi) = col.group_track() {
                crate::selection::ColumnOwner::Group(self.tracks.get(gi)?.id)
            } else {
                crate::selection::ColumnOwner::Track(self.tracks.get(col.track()?)?.id)
            };
            return Some(canvas::Action::publish(Action::SelectBlankColumn(owner)));
        }
        let hit = if self.status_row {
            status_row::track_hit(p, cols)
        } else {
            hit_test_layout(p, cols, self.scenes.len())
        }?;
        let action = match hit {
            Hit::ClipLaunch(t, s) => {
                let track = self.tracks.get(t)?.id;
                let cell = self.cells.get(t)?.get(s)?;
                if cell.clip.is_some() {
                    Action::LaunchClip(track, s)
                } else if cell.stop_button {
                    Action::StopSlot(track, s)
                } else if double_click {
                    Action::CreateClip(track, s)
                } else {
                    Action::Select(track, s)
                }
            }
            Hit::ClipName(t, s) => {
                let track = self.tracks.get(t)?.id;
                let has_clip = self.cells.get(t)?.get(s)?.clip.is_some();
                if has_clip {
                    state.clip_drag = Some(clip_drag::ClipDrag::new(
                        Selection { track, slot: s },
                        self.tracks.get(t)?.slots.get(s)?.clip.as_ref()?.id,
                        p,
                    ));
                }
                if has_clip {
                    detail_open::click(
                        Action::Select(track, s),
                        crate::inspector_panel::Tab::Clip,
                        double_click,
                    )
                } else if double_click {
                    // Live: a double click on an empty slot's body creates a clip, stop
                    // button or not; only the stop square itself (ClipLaunch) stops.
                    Action::CreateClip(track, s)
                } else {
                    Action::Select(track, s)
                }
            }
            Hit::GroupLaunch(gi, s) => {
                let members = &self.group_column(gi)?.leaves;
                group_slot_action(self.tracks.get(gi)?.id, self.tracks, members, s)?
            }
            Hit::GroupName(gi, s) => Action::SelectColumn(
                crate::selection::ColumnOwner::Group(self.tracks.get(gi)?.id),
                Some(s),
            ),
            Hit::TrackStop(t) => Action::StopTrack(self.tracks.get(t)?.id),
            Hit::GroupStop(gi) => Action::StopGroup(self.tracks.get(gi)?.id),
            Hit::SceneLaunch(s) => Action::LaunchScene(self.scenes.get(s)?.id),
            Hit::SceneSelect(s) => Action::SelectScene(self.scenes.get(s)?.id),
            Hit::StopAll => Action::StopAll,
            Hit::FollowToggle => return None,
        };
        Some(canvas::Action::publish(action))
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        feedback::interaction(cursor.position_in(bounds).and_then(|p| self.button_hit(p)))
    }

    fn draw(
        &self,
        state: &GridState,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        state.draw(self, renderer, bounds)
    }
}

mod focus;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod stopping_tests;

#[cfg(test)]
mod group_slot_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod last_row_tests;

#[cfg(test)]
mod status_row_tests;

#[cfg(test)]
mod scene_scroll_tests;

#[cfg(test)]
mod double_click_tests;

#[cfg(test)]
mod context_tests;

#[cfg(test)]
mod selection_tests;

#[cfg(test)]
pub(crate) use last_row_tests::renderer as test_renderer;

#[cfg(test)]
mod scene_selection_tests;

#[cfg(test)]
mod detail_open_tests;

#[cfg(test)]
mod nested_header_tests;

#[cfg(test)]
mod nested_selection_tests;

#[cfg(test)]
mod autoscroll_tests;
impl Grid<'_> {
    /// None leaves the usual stop square; Some draws the shared feedback phase.
    fn stop_feedback(&self, tracks: &[usize], row: usize) -> Option<bool> {
        let pending = tracks.iter().any(|&ti| self.pending_stop[ti] == Some(row));
        if let Some(phase) = self.scene_stop.iter().find(|phase| phase.slot == row) {
            if phase.burst {
                return Some(true);
            }
            if pending {
                return Some(phase.pending_on);
            }
        }
        pending.then_some(self.blink_on)
    }
}
