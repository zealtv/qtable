//! A grid over a project with no playback, for tests of hit testing, hover help and clicks.
use super::{Cell, CellStatus, Grid, LayoutCache, Master};
use qtable_engine::model::{Project, Scene, Track};

pub(super) fn grid(project: &Project) -> Grid<'_> {
    Grid {
        drag: Default::default(),
        drag_viewport: Default::default(),
        drag_h: 400.,
        status_row: false,
        tracks: &project.tracks,
        scenes: &project.scenes,
        cells: project
            .tracks
            .iter()
            .map(|t| {
                t.slots
                    .iter()
                    .map(|s| Cell {
                        clip: s.clip.as_ref(),
                        stop_button: s.has_stop_button,
                        status: CellStatus::Stopped,
                        follow_queued: false,
                        selected: false,
                        ghost: false,
                    })
                    .collect()
            })
            .collect(),
        scene_stop: Vec::new(),
        pending_stop: vec![None; project.tracks.len()],
        blink_on: false,
        selected_tracks: &[],
        revision: 0,
        layout: LayoutCache::default().get(project, 0),
        shift_down: false,
        selected: None,
        selected_scene: None,
        owner: None,
        cell: None,
        focused: None,
    }
}

pub(super) fn master<'a>(main: &'a Track, scenes: &'a [Scene]) -> Master<'a> {
    Master {
        header_h: crate::grid::HEADER_H,
        status_row: false,
        main,
        clip: None,
        scenes,
        selected_scene: None,
        selected_scenes: &[],
        owner: None,
        cell: None,
        last_triggered_scene: None,
        queued_follow_scene: None,
        follow_enabled: true,
        blink_on: false,
        revision: 0,
        focused: None,
    }
}
