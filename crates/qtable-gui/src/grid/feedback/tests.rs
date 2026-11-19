use super::*;
use crate::grid::{HEADER_H, TRACK_W};
use qtable_engine::model::{BeatTime, Clip, ClipId, Project, Scene, SceneId, Track, TrackId};

#[test]
fn hover_press_drag_release_leave_and_revision_have_distinct_states() {
    let a = Hit::ClipLaunch(0, 0);
    let b = Hit::TrackStop(0);
    let mut feedback = Feedback::default();
    let moved = canvas::Event::Mouse(mouse::Event::CursorMoved {
        position: Point::ORIGIN,
    });
    let press = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    assert!(feedback.update(&moved, Some(a), 0));
    assert_eq!(feedback.visible(0), Some((a, false)));
    assert!(!feedback.update(&moved, Some(a), 0));
    assert!(feedback.update(&press, Some(a), 0));
    assert_eq!(feedback.visible(0), Some((a, true)));
    feedback.update(&moved, Some(b), 0);
    assert_eq!(feedback.visible(0), Some((b, false)));
    feedback.update(&moved, Some(a), 0);
    assert_eq!(feedback.visible(0), Some((a, true)));
    feedback.update(&release, None, 0);
    feedback.update(&moved, Some(a), 0);
    assert_eq!(feedback.visible(0), Some((a, false)));
    feedback.update(&press, Some(a), 0);
    feedback.update(&canvas::Event::Mouse(mouse::Event::CursorLeft), None, 0);
    assert_eq!(feedback.visible(0), None);
    feedback.update(&moved, Some(a), 0);
    assert_eq!(feedback.visible(1), None);
    feedback.update(&moved, Some(a), 1);
    assert_eq!(feedback.visible(1), Some((a, false)));
}

#[test]
fn grid_button_hits_match_drawn_controls_including_group_slots() {
    let mut project = Project {
        tracks: vec![
            Track::new(TrackId(1), "one", 2),
            Track::new(TrackId(2), "two", 2),
        ],
        scenes: (0..2)
            .map(|i| Scene {
                id: SceneId(i),
                name: String::new(),
                follow_actions: Default::default(),
            })
            .collect(),
        ..Project::default()
    };
    project.tracks[0].slots[0].clip = Some(
        Clip::new(
            ClipId(1),
            "clip",
            BeatTime(960),
            qtable_engine::model::LIVE_CLIP_PALETTE[0],
        )
        .unwrap(),
    );
    project.tracks[0].slots[1].has_stop_button = false;
    {
        let mut group = Track::new_group(TrackId(101), "group", project.scenes.len());
        group.colour = qtable_engine::model::LIVE_CLIP_PALETTE[0];
        group.fold_state = true;
        project.insert_group(group, &[TrackId(1), TrackId(2)]);
    }
    for folded in [false, true] {
        let group = project.group_tracks().next().unwrap().id;
        project[group].fold_state = folded;
        let mut grid = crate::grid::test_support::grid(&project);
        let top = grid.layout.header_h;
        let member = GAP + TRACK_W + GAP;
        // The group slot answers for the whole row: a clip, or a member's stop button.
        assert_eq!(
            grid.button_hit(Point::new(GAP + 4.0, top + 4.0)),
            Some(Hit::GroupLaunch(0, 0))
        );
        assert_eq!(
            grid.button_hit(Point::new(GAP + 4.0, top + CELL_H + GAP + 4.0)),
            Some(Hit::GroupLaunch(0, 1))
        );
        assert_eq!(
            grid.button_hit(Point::new(member + 4.0, top + 4.0)),
            (!folded).then_some(Hit::ClipLaunch(1, 0))
        );
        assert_eq!(
            grid.button_hit(Point::new(member + 4.0, top + CELL_H + GAP + 4.0)),
            None,
            "no clip and no stop button"
        );
        for point in [
            Point::new(30.0, top + 4.0),
            Point::new(4.0, top + CELL_H),
            Point::new(4.0, top - 1.0),
            Point::new(-1.0, top),
            Point::new(4.0, top + 2.0 * (CELL_H + GAP)),
        ] {
            assert_eq!(grid.button_hit(point), None);
        }
        grid.status_row = true;
        assert_eq!(
            grid.button_hit(Point::new(GAP + 4.0, GAP + 4.0)),
            Some(Hit::GroupStop(0))
        );
        assert_eq!(
            grid.button_hit(Point::new(member + 4.0, GAP + 4.0)),
            (!folded).then_some(Hit::TrackStop(1))
        );
        assert_eq!(grid.button_hit(Point::new(30.0, 4.0)), None);
    }
}

#[test]
fn master_feedback_only_hits_scene_triangles_and_stop_all() {
    let scenes = [Scene {
        id: SceneId(1),
        name: String::new(),
        follow_actions: Default::default(),
    }];
    let main_track = qtable_engine::model::Track::new_main(qtable_engine::model::TrackId(0));
    {
        let mut master = Master {
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
        let y = HEADER_H;
        assert_eq!(
            master.button_hit(Point::new(4.0, y + 4.0)),
            Some(Hit::SceneLaunch(0))
        );
        for point in [
            Point::new(CELL_H, y + 4.0),
            Point::new(4.0, y + CELL_H),
            Point::new(4.0, y - 1.0),
            Point::new(-1.0, y),
        ] {
            assert_eq!(master.button_hit(point), None);
        }
        master.status_row = true;
        assert_eq!(
            master.button_hit(Point::new(4.0, GAP + 4.0)),
            Some(Hit::StopAll)
        );
        assert_eq!(master.button_hit(Point::new(30.0, 4.0)), None);
        assert_eq!(interaction(Some(Hit::StopAll)), mouse::Interaction::Pointer);
        assert_eq!(interaction(None), mouse::Interaction::default());
    }
}
