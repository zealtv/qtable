use super::{LayoutCache, Names, OverlayKey, draw};
use crate::grid::{Cell, CellStatus, GAP, Grid, Selection};
use qtable_engine::model::{BeatTime, Clip, ClipId, Project, Track, TrackId};
use std::collections::HashMap;
use std::rc::Rc;

#[test]
fn layout_is_reused_until_an_edit_and_fold_changes_its_columns() {
    let cache = LayoutCache::default();
    let mut project = Project {
        tracks: vec![
            Track::new(TrackId(1), "A", 1),
            Track::new(TrackId(2), "B", 1),
        ],
        ..Project::default()
    };
    {
        let mut group = Track::new_group(TrackId(101), "Group", project.scenes.len());
        group.colour = qtable_engine::model::LIVE_CLIP_PALETTE[0];
        project.insert_group(group, &[TrackId(1), TrackId(2)]);
    }
    let first = cache.get(&project, 0);
    assert!(Rc::ptr_eq(&first, &cache.get(&project, 0)));
    // The group track and its two members.
    assert_eq!(first.columns.len(), 3);
    let group = project.group_tracks().next().unwrap().id;
    project[group].fold_state = true;
    let folded = cache.get(&project, 1);
    assert!(!Rc::ptr_eq(&first, &folded));
    assert_eq!(folded.columns.len(), 1);
    assert_eq!(folded.xs, vec![GAP]);
    assert_eq!(folded.columns[0].leaves, vec![1, 2]);
}

#[test]
fn layout_rebuilds_when_the_track_count_changes_under_a_stale_revision() {
    let cache = LayoutCache::default();
    let mut project = Project::default();
    project.tracks.push(Track::new(TrackId(1), "A", 1));
    let first = cache.get(&project, 7);
    assert_eq!(first.columns.len(), 1);
    project.tracks.push(Track::new(TrackId(2), "B", 1));
    let rebuilt = cache.get(&project, 7);
    assert!(!Rc::ptr_eq(&first, &rebuilt));
    assert_eq!(rebuilt.columns.len(), 2);
}

#[test]
fn overlay_reuses_idle_geometry_but_changes_for_blinks_progress_and_selection() {
    let mut project = Project::default();
    project.tracks.push(Track::new(TrackId(1), "Track", 1));
    project.tracks[0].slots[0].clip = Some(
        Clip::new(
            ClipId(1),
            "Clip",
            BeatTime(3840),
            qtable_engine::model::LIVE_CLIP_PALETTE[0],
        )
        .unwrap(),
    );
    let cache = LayoutCache::default();
    let mut grid = Grid {
        drag: Default::default(),
        drag_viewport: Default::default(),
        drag_h: 400.,
        status_row: false,
        tracks: &project.tracks,
        scenes: &[],
        cells: vec![vec![Cell {
            clip: project.tracks[0].slots[0].clip.as_ref(),
            stop_button: true,
            status: CellStatus::Stopped,
            follow_queued: false,
            selected: false,
            ghost: false,
        }]],
        scene_stop: Vec::new(),
        pending_stop: vec![None],
        blink_on: true,
        selected_tracks: &[],
        revision: 0,
        layout: cache.get(&project, 0),
        shift_down: false,
        selected: None,
        selected_scene: None,
        owner: None,
        cell: None,
        focused: None,
    };
    let stopped = OverlayKey::new(&grid);
    grid.scene_stop = vec![crate::scene_stop_feedback::Row {
        slot: 1,
        burst: true,
        pending_on: false,
    }];
    let burst = OverlayKey::new(&grid);
    assert_ne!(stopped, burst);
    grid.scene_stop[0].burst = false;
    grid.scene_stop[0].pending_on = true;
    let on = OverlayKey::new(&grid);
    assert_ne!(burst, on);
    grid.scene_stop[0].pending_on = false;
    assert_ne!(on, OverlayKey::new(&grid));
    grid.scene_stop.clear();
    grid.blink_on = false;
    assert_eq!(stopped, OverlayKey::new(&grid));
    grid.cells[0][0].status = CellStatus::Triggered;
    let triggered = OverlayKey::new(&grid);
    grid.blink_on = true;
    assert_ne!(triggered, OverlayKey::new(&grid));
    grid.cells[0][0].status = CellStatus::Stopped;
    grid.cells[0][0].follow_queued = true;
    let queued_on = OverlayKey::new(&grid);
    grid.blink_on = false;
    assert_ne!(queued_on, OverlayKey::new(&grid));
    grid.blink_on = true;
    grid.cells[0][0].follow_queued = false;
    grid.cells[0][0].status = CellStatus::Playing(0.25);
    let playing = OverlayKey::new(&grid);
    grid.cells[0][0].status = CellStatus::Playing(0.5);
    assert_ne!(playing, OverlayKey::new(&grid));
    let progressed = OverlayKey::new(&grid);
    grid.selected = Some(Selection {
        track: TrackId(1),
        slot: 0,
    });
    assert_ne!(progressed, OverlayKey::new(&grid));
    let selected = OverlayKey::new(&grid);
    grid.pending_stop[0] = Some(1);
    assert_ne!(selected, OverlayKey::new(&grid));
    let stopping = OverlayKey::new(&grid);
    grid.revision += 1;
    assert_ne!(stopping, OverlayKey::new(&grid));
}

#[test]
fn cached_names_keep_the_original_fitting_for_unicode_and_different_widths() {
    let names = Names::default();
    for text in ["", "A long clip name", "日本語 🎵 clip"] {
        for width in [0.0, 10.0, 70.0, 104.0] {
            let expected = draw::fit(text, width);
            assert_eq!(names.fit(text, width), expected);
            assert_eq!(names.fit(text, width), expected);
        }
    }
}

#[test]
fn cached_names_persist_across_frames_and_the_map_stays_bounded() {
    let names = Names::default();
    let labels: Vec<String> = (0..1000).map(|i| format!("Clip {i}")).collect();
    for _ in 0..2 {
        names.begin_frame();
        for label in &labels {
            names.fit(label, 40.0);
        }
    }
    // Once the bound follows the 1000 labels drawn, a repeat frame fits nothing new.
    let entries = names.fitted.borrow().entries;
    assert_eq!(entries, labels.len());
    names.begin_frame();
    for label in &labels {
        names.fit(label, 40.0);
    }
    assert_eq!(names.fitted.borrow().entries, entries);
    // Renamed clips pile up, but only to the bound, and the count stays truthful.
    for i in 0..10_000 {
        let text = format!("Renamed {i}");
        assert_eq!(names.fit(&text, 40.0), draw::fit(&text, 40.0));
        assert!(names.fitted.borrow().entries <= 4 * labels.len());
    }
    let recounted: usize = names
        .fitted
        .borrow()
        .by_text
        .values()
        .map(HashMap::len)
        .sum();
    assert_eq!(recounted, names.fitted.borrow().entries);
}

/// Manual probe: the per-edit cost of naming every clip of a 64 × 128 grid,
/// with the map emptied first (the old clear-on-revision) and kept (now).
#[test]
#[ignore = "manual large-grid performance probe"]
fn fitting_every_clip_name_after_an_edit_cold_versus_kept() {
    use std::time::Instant;
    let labels: Vec<String> = (0..64 * 128)
        .map(|i| format!("Performance clip {i}"))
        .collect();
    let pass = |names: &Names| {
        let started = Instant::now();
        for label in &labels {
            std::hint::black_box(names.fit(label, 104.0));
        }
        started.elapsed()
    };
    let names = Names::default();
    // The first frames fill the map and let the bound learn the label count.
    names.begin_frame();
    let cold = pass(&names);
    names.begin_frame();
    pass(&names);
    names.begin_frame();
    let kept = pass(&names);
    eprintln!("8192 labels per edit: {cold:?} re-fitted (old), {kept:?} kept (new)");
}

#[test]
fn scene_follow_pulse_changes_icon_and_cache_only_while_queued() {
    use super::MasterKey;
    use crate::grid::Master;
    use qtable_engine::model::SceneId;
    let source = SceneId(1);
    let destination = SceneId(2);
    let main_track = qtable_engine::model::Track::new_main(qtable_engine::model::TrackId(0));
    let mut master = Master {
        header_h: crate::grid::HEADER_H,
        status_row: false,
        main: &main_track,
        clip: None,
        scenes: &[],
        selected_scene: None,
        selected_scenes: &[],
        owner: None,
        cell: None,
        last_triggered_scene: Some(source),
        queued_follow_scene: None,
        follow_enabled: true,
        blink_on: false,
        revision: 0,
        focused: None,
    };
    let idle = MasterKey::new(&master);
    master.blink_on = true;
    assert_eq!(idle, MasterKey::new(&master));
    let source_green = draw::scene_icon(&master, source);
    let idle_ink = draw::scene_icon(&master, destination);
    master.queued_follow_scene = Some(destination);
    assert_eq!(draw::scene_icon(&master, destination), source_green);
    let queued_on = MasterKey::new(&master);
    master.blink_on = false;
    assert_ne!(queued_on, MasterKey::new(&master));
    assert_eq!(
        draw::scene_icon(&master, destination),
        crate::control::theme::DARK_INK
    );
    assert_eq!(draw::scene_icon(&master, source), source_green);
    // Play Again pulses the same scene, overriding its steady green indicator.
    master.queued_follow_scene = Some(source);
    assert_eq!(
        draw::scene_icon(&master, source),
        crate::control::theme::DARK_INK
    );
    master.queued_follow_scene = None;
    assert_eq!(draw::scene_icon(&master, destination), idle_ink);
    assert_eq!(idle, MasterKey::new(&master));
}

#[test]
fn scene_selection_members_invalidate_the_main_cache_without_a_project_edit() {
    let project = qtable_engine::model::Project::default();
    let mut master = crate::grid::test_support::master(&project.main, &project.scenes);
    let before = super::MasterKey::new(&master);
    master.selected_scenes = &[qtable_engine::model::SceneId(1)];
    assert_ne!(before, super::MasterKey::new(&master));
    let single = super::MasterKey::new(&master);
    master.selected_scenes = &[
        qtable_engine::model::SceneId(1),
        qtable_engine::model::SceneId(2),
    ];
    assert_ne!(single, super::MasterKey::new(&master));
}
