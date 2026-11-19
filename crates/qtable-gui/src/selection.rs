//! Selection and editing mode belong to the UI, not the engine project.
use qtable_engine::model::{DeviceId, SceneId, TrackId};

use crate::{grid, inspector_panel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnOwner {
    Track(TrackId),
    Group(TrackId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnCell {
    pub owner: ColumnOwner,
    pub row: usize,
    pub active: bool,
}

pub(crate) struct SelectionState {
    pub owner: Option<ColumnOwner>,
    pub cell: Option<ColumnCell>,
    pub ghost_clip: Option<grid::Selection>,
    pub clip: Option<grid::Selection>,
    pub scene: Option<SceneId>,
    pub scenes: SceneSelection,
    pub tracks: Vec<TrackId>,
    pub devices: Vec<DeviceId>,
    pub device_anchor: Option<DeviceId>,
    pub anchor: Option<TrackId>,
    pub shift_down: bool,
    /// ⌘ held: a Solo click adds to the solo set instead of replacing it.
    pub command_down: bool,
    pub move_clip: bool,
    pub inspector_tab: inspector_panel::Tab,
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            owner: None,
            cell: None,
            ghost_clip: None,
            clip: None,
            scene: None,
            scenes: SceneSelection::default(),
            tracks: Vec::new(),
            devices: Vec::new(),
            device_anchor: None,
            anchor: None,
            shift_down: false,
            command_down: false,
            move_clip: false,
            inspector_tab: inspector_panel::Tab::Clip,
        }
    }
}

impl SelectionState {
    /// Shared by highlights and header commands. A range selects a group only when
    /// it includes the header's id; selecting its members alone never selects it.
    pub(crate) fn group_selected(
        selected: &[TrackId],
        models: &[qtable_engine::model::Track],
        group: TrackId,
    ) -> bool {
        selected.contains(&group)
            && models
                .iter()
                .any(|track| track.id == group && track.is_group())
    }

    pub(crate) fn select_column(&mut self, owner: ColumnOwner, row: Option<usize>) {
        self.ghost_clip = if row.is_none() && self.owner == Some(owner) {
            self.clip.or(self.ghost_clip)
        } else {
            None
        };
        self.cell = match row {
            Some(row) => Some(ColumnCell {
                owner,
                row,
                active: true,
            }),
            None => self
                .cell
                .filter(|cell| cell.owner == owner)
                .map(|cell| ColumnCell {
                    active: false,
                    ..cell
                }),
        };
        self.owner = Some(owner);
    }

    /// Select a column body or title through the same capability-driven path.
    /// A cell's row belongs to the grid highlight, independently of the Detail tab.
    pub(crate) fn select_column_cell(
        &mut self,
        project: &qtable_engine::model::Project,
        owner: ColumnOwner,
        row: Option<usize>,
    ) {
        use qtable_engine::model::Region;
        let id = match owner {
            ColumnOwner::Track(id) | ColumnOwner::Group(id) => id,
        };
        let Some(caps) = project.track_capabilities(id) else {
            return;
        };
        self.select_column(owner, row);
        self.clip = None;
        self.scene = None;
        self.move_clip = false;
        self.inspector_tab = match caps.region {
            Region::Tracks => inspector_panel::Tab::Track,
            Region::Outputs => inspector_panel::Tab::Output,
            Region::Main => inspector_panel::Tab::Main,
        };
        self.tracks = if caps.foldable {
            if row.is_some() {
                project.descendant_clip_track_ids(id).unwrap_or_default()
            } else {
                vec![id]
            }
        } else if caps.region == Region::Main {
            Vec::new()
        } else {
            vec![id]
        };
        self.anchor = (caps.region != Region::Main).then_some(id);
    }

    pub(crate) fn clear_cell(&mut self) {
        self.cell = None;
        self.ghost_clip = None;
    }

    /// The track the Detail View's Track tab and name field act on: the selected clip's
    /// track, otherwise the selected track or group header.
    pub(crate) fn track(&self) -> Option<TrackId> {
        self.clip.map(|sel| sel.track).or(match self.owner {
            Some(ColumnOwner::Group(group)) => Some(group),
            _ => self.anchor,
        })
    }
}

/// Stable scene identities in project order, independent of the focused scene.
/// The anchor remains fixed while a Shift range grows or shrinks. Scene dragging
/// can carry `members` without reconstructing a selection from row indices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SceneSelection {
    pub members: Vec<SceneId>,
    pub anchor: Option<SceneId>,
    /// Selection before the current Shift range; keeps disjoint Cmd selections.
    range_base: Option<Vec<SceneId>>,
}

impl SceneSelection {
    pub(crate) fn single(&mut self, scene: SceneId) {
        self.members = vec![scene];
        self.anchor = Some(scene);
        self.range_base = None;
    }

    pub(crate) fn click(
        &mut self,
        scenes: &[qtable_engine::model::Scene],
        scene: SceneId,
        shift: bool,
        command: bool,
    ) -> Option<SceneId> {
        let clicked = scenes.iter().position(|s| s.id == scene)?;
        if shift {
            let anchor = self
                .anchor
                .and_then(|id| scenes.iter().position(|s| s.id == id))
                .unwrap_or(clicked);
            let range = &scenes[header_range(anchor, clicked)];
            // Preserve disjoint selections while extending a range; the recording
            // shows additive selection, but not modifiers or post-toggle anchors.
            // Freeze the base so shrinking the range can release its old rows.
            self.members = self
                .range_base
                .get_or_insert_with(|| self.members.clone())
                .clone();
            self.members.extend(range.iter().map(|s| s.id));
            self.anchor.get_or_insert(scene);
        } else if command {
            self.range_base = None;
            if self.members.contains(&scene) {
                self.members.retain(|id| *id != scene);
            } else {
                self.members.push(scene);
            }
            self.anchor = Some(scene);
        } else {
            self.single(scene);
        }
        // Normalize both ordering and duplicates against the current project.
        self.members = scenes
            .iter()
            .filter(|s| self.members.contains(&s.id))
            .map(|s| s.id)
            .collect();
        self.members
            .contains(&scene)
            .then_some(scene)
            .or_else(|| self.members.last().copied())
    }
}

impl SelectionState {
    /// Reconcile external structural edits and routes that leave scene selection.
    pub(crate) fn reconcile_scenes(&mut self, scenes: &[qtable_engine::model::Scene]) {
        let Some(primary) = self.scene else {
            self.scenes = SceneSelection::default();
            return;
        };
        self.scenes.members = scenes
            .iter()
            .filter(|s| self.scenes.members.contains(&s.id))
            .map(|s| s.id)
            .collect();
        if !scenes.iter().any(|s| s.id == primary) {
            self.scene = self.scenes.members.first().copied();
            self.clear_cell();
        } else if !self.scenes.members.contains(&primary) {
            self.scenes.single(primary);
        }
        if let Some(row) = self
            .scene
            .and_then(|id| scenes.iter().position(|s| s.id == id))
            && let Some(cell) = self.cell.as_mut()
        {
            cell.row = row;
        }
        if !self
            .scenes
            .anchor
            .is_some_and(|id| scenes.iter().any(|s| s.id == id))
        {
            self.scenes.anchor = self.scene;
        }
    }
}

impl SelectionState {
    /// Keeps what still resolves in `project` and drops the rest, after the set changed under
    /// the selection (an undo or redo). Scenes and devices reconcile separately.
    pub(crate) fn reconcile_tracks(&mut self, project: &qtable_engine::model::Project) {
        let exists = |id: &TrackId| project.track(*id).is_some();
        self.tracks.retain(exists);
        if self.anchor.as_ref().is_some_and(|id| !exists(id)) {
            self.anchor = None;
        }
        let slot_exists = |sel: &grid::Selection| {
            project
                .track(sel.track)
                .is_some_and(|track| sel.slot < track.slots.len())
        };
        if self.clip.as_ref().is_some_and(|sel| !slot_exists(sel)) {
            self.clip = None;
        }
        if self
            .ghost_clip
            .as_ref()
            .is_some_and(|sel| !slot_exists(sel))
        {
            self.ghost_clip = None;
        }
        let gone = |owner: ColumnOwner| match owner {
            ColumnOwner::Track(id) | ColumnOwner::Group(id) => !exists(&id),
        };
        if self.owner.is_some_and(gone) {
            self.owner = None;
            self.clear_cell();
        }
        if self.cell.is_some_and(|cell| gone(cell.owner)) {
            self.clear_cell();
        }
    }
}

/// Borrowed selection used by all regions' headers and column bodies.
pub(crate) struct SelectionView<'a> {
    pub models: &'a [qtable_engine::model::Track],
    pub tracks: &'a [TrackId],
    pub owner: Option<ColumnOwner>,
    pub cell: Option<ColumnCell>,
    pub clip: Option<grid::Selection>,
    pub scene: Option<SceneId>,
}

impl SelectionView<'_> {
    pub(crate) fn column_selected(&self, col: &grid::layout::Column) -> bool {
        use grid::layout::ColumnKey;
        match col.key {
            // Main's column also holds the scene rows, so a selected scene or clip
            // moves the highlight off its header.
            ColumnKey::Track(id) if col.caps.region == qtable_engine::model::Region::Main => {
                self.owner == Some(ColumnOwner::Track(id))
                    && self.scene.is_none()
                    && self.clip.is_none()
            }
            ColumnKey::Track(id) if col.caps.foldable => {
                SelectionState::group_selected(self.tracks, self.models, id)
            }
            ColumnKey::Track(id) => self.tracks.contains(&id),
        }
    }

    pub(crate) fn active_column(&self, col: &grid::layout::Column) -> bool {
        self.active_owner(col.owner())
    }

    pub(crate) fn active_owner(&self, owner: ColumnOwner) -> bool {
        self.cell
            .is_some_and(|cell| cell.active && cell.owner == owner)
    }
}

#[cfg(test)]
impl SelectionState {
    pub(crate) fn view<'a>(
        &'a self,
        models: &'a [qtable_engine::model::Track],
    ) -> SelectionView<'a> {
        SelectionView {
            models,
            tracks: &self.tracks,
            owner: self.owner,
            cell: self.cell,
            clip: self.clip,
            scene: self.scene,
        }
    }
}

/// Inclusive header selection, shared by Session tracks and clip lanes.
pub(crate) fn header_range(anchor: usize, clicked: usize) -> std::ops::Range<usize> {
    anchor.min(clicked)..anchor.max(clicked) + 1
}

/// Header drag threshold, independent of the orientation of the strip.
pub(crate) fn header_dragging(origin: iced::Point, cursor: iced::Point) -> bool {
    origin.distance(cursor) >= 4.0
}

/// Which side of a header receives an insertion marker.
pub(crate) fn header_boundary(index: usize, position: f32, start: f32, size: f32) -> usize {
    index + usize::from(position >= start + size / 2.0)
}
