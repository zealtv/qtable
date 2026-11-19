use super::*;
use iced::Point;

impl Grid<'_> {
    pub(super) fn header_action(&self, p: Point) -> Option<Action> {
        let (ci, col) = super::layout::column_at(&self.layout.columns, &self.layout.xs, p.x)?;
        Some(super::header::press(col, p, self.layout.xs[ci], self.shift_down, self.revision).0)
    }
}

use qtable_engine::engine::{TrackSnapshot, TriggeredSlot};
use qtable_engine::model::{BeatTime, Project};

/// Repeatable headless construction probe; CPU and GPU drawing are measured separately.
#[test]
#[ignore = "manual large-grid performance probe"]
fn large_grid_view_construction_budget() {
    use qtable_engine::engine::Engine;
    use qtable_engine::model::{ClipId, OscAddress, Scene};
    use std::time::Instant;
    let engine = Engine::new(Project::default()).unwrap();
    let snapshot = engine.snapshot();
    let scenes = (0..128)
        .map(|i| Scene {
            id: SceneId(i + 1),
            name: format!("Scene {}", i + 1),
            follow_actions: Default::default(),
        })
        .collect();
    let tracks = (0..64)
        .map(|ti| {
            let mut track = Track::new(TrackId(ti + 1), format!("Track {}", ti + 1), 128);
            let device = qtable_engine::model::DeviceId(ti + 1);
            let events: Vec<_> = (0..16)
                .map(|i| {
                    let cue = OscAddress::Relative("/cue".into());
                    track.hit(BeatTime(i * 240), cue, vec![], || device)
                })
                .collect();
            for (si, slot) in track.slots.iter_mut().enumerate() {
                let mut clip = Clip::new(
                    ClipId(ti * 128 + si as u64 + 1),
                    "Performance clip",
                    BeatTime(3840),
                    qtable_engine::model::LIVE_CLIP_PALETTE[0],
                )
                .unwrap();
                clip.events = events.clone();
                slot.clip = Some(clip);
            }
            track
        })
        .collect();
    let project = Project {
        scenes,
        tracks,
        ..Project::default()
    };
    let cache = LayoutCache::default();
    let started = Instant::now();
    for _ in 0..100 {
        std::hint::black_box(view(
            &project,
            &snapshot,
            ViewState {
                owner: None,
                cell: None,
                ghost: None,
                selected: None,
                selected_scene: None,
                selected_scenes: &[],
                selected_tracks: &[],
                shift_down: false,
                scene_stop: Vec::new(),
                blink_on: true,
                revision: 0,
            },
            &cache,
        ));
    }
    let per_view = started.elapsed() / 100;
    eprintln!("64 × 128 grid, 16 events/clip: {per_view:?} per view construction");
    assert!(
        per_view < std::time::Duration::from_millis(10),
        "view construction exceeded budget: {per_view:?}"
    );
}
#[test]
fn hit_testing_accounts_for_grid_columns_and_rows() {
    assert_eq!(
        hit_test(Point::new(10.0, HEADER_H + 4.0), 1, 2),
        Some(Hit::ClipLaunch(0, 0))
    );
    assert_eq!(
        hit_test(Point::new(60.0, HEADER_H + 4.0), 1, 2),
        Some(Hit::ClipName(0, 0))
    );
    // Past the only track, blank space selects the scene.
    assert_eq!(
        hit_test(Point::new(120.0, HEADER_H + 4.0), 1, 2),
        Some(Hit::SceneSelect(0))
    );
    assert_eq!(
        hit_test(Point::new(150.0, HEADER_H + 4.0), 1, 2),
        Some(Hit::SceneSelect(0))
    );
    assert_eq!(
        hit_test(
            Point::new(10.0, HEADER_H + 2.0 * (CELL_H + GAP) + 2.0),
            1,
            2
        ),
        None
    );
}
/// The group track first, then T1 and T2 (its members) and T3.
fn grouped_tracks(folded: bool) -> Vec<Track> {
    let mut project = qtable_engine::model::Project {
        tracks: (1..=3)
            .map(|i| Track::new(TrackId(i), format!("T{i}"), 2))
            .collect(),
        ..Default::default()
    };
    let mut group = Track::new_group(TrackId(101), "G", 2);
    group.colour = qtable_engine::model::LIVE_CLIP_PALETTE[0];
    group.fold_state = folded;
    project.insert_group(group, &[TrackId(1), TrackId(2)]);
    project.tracks
}

#[test]
fn group_track_column_leads_its_members_and_folding_leaves_only_it() {
    let tracks = grouped_tracks(false);
    let cols = track_columns(&tracks);
    let shape = |cols: &[Column]| {
        cols.iter()
            .map(|c| {
                (
                    c.caps.foldable,
                    c.leaves.clone(),
                    c.number.unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>()
    };
    // Live numbers the group track too: 1 Group, 2 T1, 3 T2, 4 T3.
    assert_eq!(
        shape(&cols),
        vec![
            (true, vec![1, 2], 1),
            (false, vec![1], 2),
            (false, vec![2], 3),
            (false, vec![3], 4),
        ]
    );
    assert!(cols.iter().all(|c| c.width == TRACK_W));
    let row = layout::header_height(&cols) + 4.0;
    assert_eq!(
        hit_test_layout(Point::new(10.0, row), &cols, 2),
        Some(Hit::GroupLaunch(0, 0))
    );
    assert_eq!(
        hit_test_layout(Point::new(60.0, row), &cols, 2),
        Some(Hit::GroupName(0, 0))
    );
    let member = x_positions(&cols)[1];
    assert_eq!(
        hit_test_layout(Point::new(member + 4.0, row), &cols, 2),
        Some(Hit::ClipLaunch(1, 0))
    );
    let tracks = grouped_tracks(true);
    let cols = track_columns(&tracks);
    let row = layout::header_height(&cols) + 4.0;
    assert_eq!(
        shape(&cols),
        vec![(true, vec![1, 2], 1), (false, vec![3], 4)]
    );
    assert_eq!(
        hit_test_layout(Point::new(GAP + TRACK_W + GAP + 4.0, row), &cols, 2),
        Some(Hit::ClipLaunch(3, 0))
    );
}

#[test]
fn group_title_selects_the_group_and_its_fold_button_folds() {
    let project = qtable_engine::model::Project {
        tracks: grouped_tracks(false),
        ..Default::default()
    };
    let grid = super::test_support::grid(&project);
    let at = |x| grid.header_action(Point::new(x, TRACK_H / 2.0));
    assert_eq!(at(20.0), Some(Action::SelectGroup(TrackId(101), false)));
    assert_eq!(
        at(TRACK_W - FOLD_W - 1.0),
        Some(Action::SelectGroup(TrackId(101), false))
    );
    assert_eq!(at(TRACK_W - 8.0), Some(Action::ToggleGroup(TrackId(101))));
    assert_eq!(
        at(GAP + TRACK_W + GAP + 20.0),
        Some(Action::SelectTrack(TrackId(1), false))
    );
}

#[test]
fn master_program_maps_canvas_clicks_to_scene_ids_and_rejects_blank_space() {
    use qtable_engine::model::Scene;
    let scenes = vec![Scene {
        id: SceneId(42),
        name: "Scene".into(),
        follow_actions: Default::default(),
    }];
    let main_track = qtable_engine::model::Track::new_main(qtable_engine::model::TrackId(0));
    let bounds = Rectangle::new(Point::new(250.0, 100.0), iced::Size::new(MASTER_W, 160.0));
    let event = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    {
        let master = Master {
            header_h: crate::grid::HEADER_H,
            status_row: false,
            main: &main_track,
            clip: None,
            scenes: &scenes,
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
        };
        let click = |x, y| {
            master
                .update(
                    &mut MasterState::default(),
                    &event,
                    bounds,
                    mouse::Cursor::Available(Point::new(bounds.x + x, bounds.y + y)),
                )
                .and_then(|action| action.into_inner().0)
        };
        let top = HEADER_H;
        assert_eq!(
            click(4.0, top + 4.0),
            Some(Action::LaunchScene(SceneId(42)))
        );
        assert_eq!(
            click(40.0, top + 4.0),
            Some(Action::ClickScene(SceneId(42)))
        );
        assert_eq!(
            click(4.0, top + CELL_H + GAP + 4.0),
            Some(Action::SelectBlankColumn(
                crate::selection::ColumnOwner::Track(main_track.id)
            ))
        );
        assert_eq!(click(MASTER_W + 1.0, top + 4.0), None);
        assert_eq!(
            click(4.0, top - 1.0),
            Some(Action::SelectColumn(
                crate::selection::ColumnOwner::Track(main_track.id),
                None
            ))
        );
        assert_eq!(click(4.0, top + CELL_H), None);
        assert_eq!(
            click(4.0, top + 2.0 * (CELL_H + GAP)),
            Some(Action::SelectBlankColumn(
                crate::selection::ColumnOwner::Track(main_track.id)
            ))
        );
    }
}
#[test]
fn snapshot_status_prefers_triggered_then_reports_play_progress() {
    let state = TrackSnapshot {
        empty_route: false,
        id: TrackId(1),
        playing_slot: Some(0),
        triggered: Some(TriggeredSlot {
            slot: 1,
            launch_beat: BeatTime(960),
        }),
        play_position_beats: Some(2.0),
        pass_progress: Some(0.5),
        next_follow_beat: None,
        queued_follow_slot: None,
    };
    assert_eq!(cell_status(Some(&state), 1), CellStatus::Triggered);
    assert_eq!(cell_status(Some(&state), 0), CellStatus::Playing(0.5));
    // The grid draws the engine's pass progress as published, whatever the content position.
    let offset = TrackSnapshot {
        play_position_beats: Some(6.0),
        ..state
    };
    assert_eq!(cell_status(Some(&offset), 0), CellStatus::Playing(0.5));
    assert_eq!(cell_status(None, 0), CellStatus::Stopped);
}
#[test]
fn queued_follow_pulses_the_icon_not_the_cell() {
    let mut state = TrackSnapshot {
        empty_route: false,
        id: TrackId(1),
        playing_slot: Some(0),
        triggered: None,
        play_position_beats: Some(1.0),
        pass_progress: Some(0.25),
        next_follow_beat: None,
        queued_follow_slot: Some(2),
    };
    let cell = |state: &TrackSnapshot, slot: usize| {
        let status = cell_status(Some(state), slot);
        (status, follow_queued(Some(state), slot, status))
    };
    // Another slot queued: it looks stopped, with a pulsing icon.
    assert_eq!(cell(&state, 2), (CellStatus::Stopped, true));
    assert_eq!(cell(&state, 1), (CellStatus::Stopped, false));
    assert_eq!(cell(&state, 0), (CellStatus::Playing(0.25), false));
    // The playing clip queued (Play Again): play position stays, icon pulses.
    state.queued_follow_slot = Some(0);
    assert_eq!(cell(&state, 0), (CellStatus::Playing(0.25), true));
    // A manual trigger keeps its whole-cell blink and no icon pulse.
    state.triggered = Some(TriggeredSlot {
        slot: 2,
        launch_beat: BeatTime(960),
    });
    state.queued_follow_slot = Some(2);
    assert_eq!(cell(&state, 2), (CellStatus::Triggered, false));
    // No follow action queued: nothing pulses.
    state.triggered = None;
    state.queued_follow_slot = None;
    assert_eq!(cell(&state, 0), (CellStatus::Playing(0.25), false));
    assert!(!follow_queued(None, 0, CellStatus::Stopped));
}
#[test]
fn pulsing_icon_alternates_green_and_black_only_when_follow_queued() {
    use super::draw::pulsing_icon;
    let mut cell = Cell {
        clip: None,
        stop_button: false,
        status: CellStatus::Stopped,
        follow_queued: true,
        selected: false,
        ghost: false,
    };
    let green = pulsing_icon(&cell, true).unwrap();
    let dark = pulsing_icon(&cell, false).unwrap();
    assert_eq!(green, iced::Color::from_rgb8(0, 211, 141));
    assert_eq!(dark, iced::Color::BLACK);
    cell.follow_queued = false;
    assert_eq!(pulsing_icon(&cell, true), None);
    assert_eq!(pulsing_icon(&cell, false), None);
}
#[test]
fn blink_phase_toggles_every_five_ticks() {
    assert!(blink_phase(0));
    assert!(blink_phase(4));
    assert!(!blink_phase(5));
    assert!(!blink_phase(9));
    assert!(blink_phase(10));
}
#[test]
fn scene_names_show_only_when_not_default() {
    use super::draw::scene_is_named;
    assert!(scene_is_named("Intro"));
    assert!(!scene_is_named("Scene 3"));
    assert!(!scene_is_named(""));
    assert!(!scene_is_named("  "));
    assert!(scene_is_named("Scene two"));
}
#[test]
fn short_grid_selects_scenes_in_blank_space() {
    // Blank space selects scenes without exposing track controls.
    let cols = track_columns(&[Track::new(TrackId(1), String::from("T"), 2)]);
    let at = |x: f32, y: f32| hit_test_layout(Point::new(x, y), &cols, 2);
    let clip_row = HEADER_H + 4.0;
    let stop_row = HEADER_H + 2.0 * (CELL_H + GAP) + 2.0;
    for x in [106.0, 120.0, 150.0, 179.0] {
        assert_eq!(
            at(x, clip_row),
            Some(Hit::SceneSelect(0)),
            "clip row at x={x}"
        );
        assert_eq!(at(x, stop_row), None, "stop row at x={x}");
    }
    // The track itself still answers at its right edge.
    assert_eq!(
        at(100.0, clip_row),
        Some(Hit::ClipName(TrackId(1).0 as usize - 1, 0))
    );
}
#[test]
fn wide_grid_maps_tracks_and_selects_scenes_between_and_after_them() {
    let tracks: Vec<Track> = (1..=3)
        .map(|i| Track::new(TrackId(i), String::from("T"), 2))
        .collect();
    let cols = track_columns(&tracks);
    let at = |x: f32, y: f32| hit_test_layout(Point::new(x, y), &cols, 2);
    let clip_row = HEADER_H + 4.0;
    // Second track's launch square starts after the first track plus its gap.
    let second = GAP + TRACK_W + GAP;
    assert_eq!(at(second + 4.0, clip_row), Some(Hit::ClipLaunch(1, 0)));
    // Gap between two tracks selects the scene.
    assert_eq!(at(GAP + TRACK_W + 0.5, clip_row), Some(Hit::SceneSelect(0)));
    // Blank space beyond the final track selects the scene too.
    let grid_end = GAP + 3.0 * (TRACK_W + GAP);
    // grid_end includes the trailing gap, so the last track's edge is GAP before it.
    assert_eq!(
        at(grid_end - GAP - 1.0, clip_row),
        Some(Hit::ClipName(2, 0))
    );
    assert_eq!(at(grid_end - GAP, clip_row), Some(Hit::SceneSelect(0)));
    assert_eq!(at(grid_end, clip_row), Some(Hit::SceneSelect(0)));
    assert_eq!(at(grid_end + 40.0, clip_row), Some(Hit::SceneSelect(0)));
    assert_eq!(at(grid_end + MASTER_W, clip_row), Some(Hit::SceneSelect(0)));
}
#[test]
fn master_column_hits_scenes_and_rejects_space_below_them() {
    let row = |slot: usize| HEADER_H + slot as f32 * (CELL_H + GAP) + 4.0;
    assert_eq!(
        master_hit_test(Point::new(4.0, row(0)), 2, HEADER_H),
        Some(Hit::SceneLaunch(0))
    );
    assert_eq!(
        master_hit_test(Point::new(40.0, row(1)), 2, HEADER_H),
        Some(Hit::SceneSelect(1))
    );
    let stop_all = HEADER_H + 2.0 * (CELL_H + GAP) + 2.0;
    assert_eq!(
        master_hit_test(Point::new(4.0, stop_all), 2, HEADER_H),
        None
    );
    // Outside the master's own width, or above its first row, is nothing.
    assert_eq!(
        master_hit_test(Point::new(MASTER_W + 1.0, row(0)), 2, HEADER_H),
        None
    );
    assert_eq!(master_hit_test(Point::new(4.0, 2.0), 2, HEADER_H), None);
}

#[test]
fn group_slot_launches_whole_group_and_empty_controls_follow_member_flags() {
    let mut tracks = vec![
        Track::new(TrackId(1), "one", 1),
        Track::new(TrackId(2), "two", 1),
    ];
    tracks[1].slots[0].clip = Some(
        Clip::new(
            qtable_engine::model::ClipId(1),
            "clip",
            qtable_engine::model::BeatTime(960),
            qtable_engine::model::LIVE_CLIP_PALETTE[0],
        )
        .unwrap(),
    );
    let members = [0, 1];
    assert_eq!(
        group_slot_action(TrackId(107), &tracks, &members, 0),
        Some(Action::LaunchGroupSlot(TrackId(107), 0))
    );
    tracks[1].slots[0].clip = None;
    assert_eq!(
        group_slot_action(TrackId(107), &tracks, &members, 0),
        Some(Action::LaunchGroupSlot(TrackId(107), 0))
    );
    for track in &mut tracks {
        track.slots[0].has_stop_button = false;
    }
    assert_eq!(
        group_slot_action(TrackId(107), &tracks, &members, 0),
        Some(Action::SelectColumn(
            crate::selection::ColumnOwner::Group(TrackId(107)),
            Some(0)
        ))
    );
}
