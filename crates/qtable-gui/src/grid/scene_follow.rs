//! Keeps the selected scene in view as the window shortens, as Live's slot grid does
//! (it scrolled to keep scene 4 visible when the window shortened, as checked 2026-11-02).
//! The track header row is pinned above the scrolling rows, so the rows' viewport shows slot rows
//! from the scroll offset down: row `k` is fully visible when `k * (CELL_H + GAP)` is at or below the
//! offset and its bottom is within `offset + viewport - header height`.
use super::{CELL_H, GAP, Selection};
use crate::messages::Message;
use iced::Task;
use qtable_engine::model::{Scene, SceneId};
use std::cell::Cell;
use std::rc::Rc;

/// The scene rows' viewport and pinned header heights in px, written by the grid's view.
pub(crate) type Viewport = Rc<Cell<(f32, f32)>>;

/// What the update loop last saw of the scene rows: their scroll offset and what it already
/// scrolled for.
#[derive(Debug, Default)]
pub(crate) struct SceneFollow {
    viewport: Viewport,
    offset: Cell<f32>,
    handled: Cell<(Option<usize>, u32, u32)>,
}

impl SceneFollow {
    pub(crate) fn viewport(&self) -> Viewport {
        Rc::clone(&self.viewport)
    }

    /// The user scrolled the rows to `offset`.
    pub(crate) fn scrolled(&self, offset: f32) {
        self.offset.set(offset);
    }
}

/// The scroll offset that brings `row` fully into view, or `None` when it already is.
pub(crate) fn offset_to_show(row: usize, offset: f32, viewport: f32, header_h: f32) -> Option<f32> {
    crate::control::viewport::reveal(
        row as f32 * (CELL_H + GAP),
        CELL_H,
        offset,
        viewport - header_h,
    )
}

/// Scroll the rows when the selected scene is out of view, once per change of selection or
/// viewport height, so wheel scrolling elsewhere is left alone.
pub(crate) fn follow(
    state: &SceneFollow,
    clip: Option<Selection>,
    scene: Option<SceneId>,
    cell: Option<crate::selection::ColumnCell>,
    scenes: &[Scene],
) -> Task<Message> {
    let (viewport, header_h) = state.viewport.get();
    if viewport <= header_h {
        return Task::none();
    }
    let row = super::scene_area::selected_row(clip, scene, cell, scenes);
    let key = (row, viewport.round() as u32, header_h.round() as u32);
    if state.handled.get() == key {
        return Task::none();
    }
    state.handled.set(key);
    let Some(offset) =
        row.and_then(|row| offset_to_show(row, state.offset.get(), viewport, header_h))
    else {
        return Task::none();
    };
    state.offset.set(offset);
    super::scroll_scenes(offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::HEADER_H;

    const STEP: f32 = CELL_H + GAP;

    #[test]
    fn a_visible_row_stays_put() {
        // Header plus three rows.
        let viewport = HEADER_H + 3.0 * STEP;
        for row in 0..3 {
            assert_eq!(
                offset_to_show(row, 0.0, viewport, HEADER_H),
                None,
                "row {row}"
            );
        }
    }

    #[test]
    fn a_row_below_scrolls_up_to_the_bottom_edge() {
        let viewport = HEADER_H + 3.0 * STEP;
        // Row 3 needs rows 1 to 3 showing: offset one step.
        let offset = offset_to_show(3, 0.0, viewport, HEADER_H).unwrap();
        assert!((offset - (STEP - GAP)).abs() < 0.01, "{offset}");
        assert_eq!(offset_to_show(3, offset, viewport, HEADER_H), None);
    }

    #[test]
    fn a_row_above_scrolls_down_to_the_top_edge() {
        let viewport = HEADER_H + 3.0 * STEP;
        assert_eq!(
            offset_to_show(1, 5.0 * STEP, viewport, HEADER_H),
            Some(STEP)
        );
    }

    #[test]
    fn it_follows_once_per_change_and_leaves_wheel_scrolling_alone() {
        let scenes: Vec<Scene> = (1..=8)
            .map(|id| Scene {
                id: SceneId(id),
                name: String::new(),
                follow_actions: Default::default(),
            })
            .collect();
        let state = SceneFollow::default();
        state.viewport.set((HEADER_H + 3.0 * STEP, HEADER_H));
        let selected = Some(SceneId(5));
        let _ = follow(&state, None, selected, None, &scenes);
        assert!(state.offset.get() > 0.0, "scrolled to scene 5");
        let there = state.offset.get();
        // The user wheels elsewhere; nothing changed, so nothing snaps back.
        state.scrolled(0.0);
        let _ = follow(&state, None, selected, None, &scenes);
        assert_eq!(state.offset.get(), 0.0);
        // A taller viewport counts as a change and finds the row visible.
        state.viewport.set((HEADER_H + 8.0 * STEP, HEADER_H));
        let _ = follow(&state, None, selected, None, &scenes);
        assert_eq!(state.offset.get(), 0.0);
        assert!(there > 0.0);
    }
}
