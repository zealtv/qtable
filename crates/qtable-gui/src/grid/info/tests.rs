use super::*;
use crate::grid::FOLD_W;
use crate::grid::test_support::grid;
use crate::grid::{CELL_H, GAP, HEADER_H};
use qtable_engine::model::{
    BeatTime, Clip, ClipId, ControlRef, Project, Scene, SceneId, Track, TrackId, TransportControl,
    describe_control,
};

fn project() -> Project {
    let mut tracks = vec![
        Track::new(TrackId(1), "One", 3),
        Track::new(TrackId(2), "Two", 3),
    ];
    tracks[0].slots[0].clip = Some(
        Clip::new(
            ClipId(1),
            "Cue",
            BeatTime(3840),
            qtable_engine::model::LIVE_CLIP_PALETTE[0],
        )
        .unwrap(),
    );
    tracks[0].slots[2].has_stop_button = false;
    Project {
        tracks,
        scenes: (1..=3)
            .map(|id| Scene {
                id: SceneId(id),
                name: "Scene".into(),
                follow_actions: Default::default(),
            })
            .collect(),
        ..Project::default()
    }
}

#[test]
fn help_distinguishes_launch_stop_name_empty_slot_and_pinned_status_row() {
    let project = project();
    let mut grid = grid(&project);
    let at = |slot, x| Point::new(x, HEADER_H + slot as f32 * (CELL_H + GAP) + 4.0);
    assert_eq!(grid.info_at(Point::new(40.0, 4.0)), Some(Id::TrackHeader));
    assert_eq!(grid.info_at(at(0, 4.0)), Some(Id::ClipLaunch));
    assert_eq!(grid.info_at(at(0, 40.0)), Some(Id::ClipSlot));
    assert_eq!(grid.info_at(at(1, 4.0)), Some(Id::SlotStop));
    assert_eq!(grid.info_at(at(1, 40.0)), Some(Id::EmptySlot));
    assert_eq!(grid.info_at(at(2, 4.0)), Some(Id::EmptySlot));
    assert_eq!(grid.info_at(at(3, 4.0)), None);
    assert_eq!(grid.info_at(at(0, 300.0)), Some(Id::SceneSelect));
    grid.status_row = true;
    assert_eq!(grid.info_at(Point::new(4.0, 4.0)), Some(Id::TrackStop));
    assert_eq!(grid.info_at(Point::new(40.0, 4.0)), None);
}

#[test]
fn group_controls_and_folded_slots_report_their_group_scope() {
    let mut project = project();
    {
        let mut group = Track::new_group(TrackId(101), "Group", project.scenes.len());
        group.colour = qtable_engine::model::LIVE_CLIP_PALETTE[0];
        project.insert_group(group, &[TrackId(1), TrackId(2)]);
    }
    // The group track is the first column, its members follow.
    let mut unfolded = grid(&project);
    assert_eq!(
        unfolded.info_at(Point::new(5.0, 4.0)),
        Some(Id::GroupHeader)
    );
    assert_eq!(
        unfolded.info_at(unfolded.layout.columns[0].fold(1.0).center()),
        Some(Id::GroupFold)
    );
    let second = unfolded.layout.xs[1] + 40.0;
    assert_eq!(
        unfolded.info_at(Point::new(second, 4.0)),
        Some(Id::TrackHeader)
    );
    let y = unfolded.layout.header_h + 4.0;
    assert_eq!(unfolded.info_at(Point::new(4.0, y)), Some(Id::GroupLaunch));
    assert_eq!(unfolded.info_at(Point::new(40.0, y)), Some(Id::GroupSlot));
    assert_eq!(
        unfolded.info_at(Point::new(unfolded.layout.xs[1] + 4.0, y)),
        Some(Id::ClipLaunch)
    );
    unfolded.status_row = true;
    assert_eq!(unfolded.info_at(Point::new(4.0, 4.0)), Some(Id::GroupStop));
    let group = project.group_tracks().next().unwrap().id;
    project[group].fold_state = true;
    let mut folded = grid(&project);
    let y = folded.layout.header_h + 4.0;
    assert_eq!(folded.layout.columns.len(), 1);
    assert_eq!(folded.info_at(Point::new(4.0, y)), Some(Id::GroupLaunch));
    assert_eq!(folded.info_at(Point::new(40.0, y)), Some(Id::GroupSlot));
    assert_eq!(folded.info_at(Point::new(45.0, 4.0)), Some(Id::GroupHeader));
    folded.status_row = true;
    assert_eq!(folded.info_at(Point::new(4.0, 4.0)), Some(Id::GroupStop));
}

#[test]
fn main_canvas_routes_scene_launch_labels_and_stop_all() {
    let project = project();
    let mut master = Master {
        header_h: crate::grid::HEADER_H,
        status_row: false,
        main: &project.main,
        clip: None,
        scenes: &project.scenes,
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
    let y = HEADER_H + 4.0;
    assert_eq!(master.info_at(Point::new(4.0, y)), Some(Id::SceneLaunch));
    assert_eq!(master.info_at(Point::new(40.0, y)), Some(Id::SceneSelect));
    assert_eq!(master.info_at(Point::new(4.0, y + 200.0)), None);
    master.status_row = true;
    assert_eq!(master.info_at(Point::new(4.0, 4.0)), Some(Id::StopAll));
    assert_eq!(master.info_at(Point::new(40.0, 4.0)), None);
}

#[test]
fn hits_declare_the_stable_ids_of_what_they_drive() {
    let project = project();
    let mut grid = grid(&project);
    let at = |slot, x| Point::new(x, HEADER_H + slot as f32 * (CELL_H + GAP) + 4.0);
    let drives = |grid: &Grid<'_>, point| grid.identity_at(point).and_then(|i| i.drives);
    // A clip's launch square, an empty slot's stop square: both drive the slot, by scene id.
    for (slot, scene) in [(0, 1), (1, 2)] {
        let slot_ref = ControlRef::ClipSlot {
            track: TrackId(1),
            scene: SceneId(scene),
        };
        assert_eq!(drives(&grid, at(slot, 4.0)), Some(slot_ref.clone()));
        assert_eq!(drives(&grid, at(slot, 40.0)), Some(slot_ref.clone()));
        assert!(describe_control(&project, &slot_ref).is_some());
    }
    // Stopless empty slots drive nothing; track headers select the track.
    for point in [at(2, 4.0), at(2, 40.0)] {
        assert_eq!(drives(&grid, point), None);
    }
    assert_eq!(
        drives(&grid, Point::new(40.0, 4.0)),
        Some(ControlRef::TrackSelect(TrackId(1)))
    );
    let second = grid.layout.xs[1] + 4.0;
    assert_eq!(
        drives(&grid, at(1, second)),
        Some(ControlRef::ClipSlot {
            track: TrackId(2),
            scene: SceneId(2)
        })
    );
    grid.status_row = true;
    assert_eq!(
        drives(&grid, Point::new(4.0, 4.0)),
        Some(ControlRef::TrackStop(TrackId(1)))
    );
}

#[test]
fn map_mode_picks_the_whole_slot_including_group_names_without_operating_it() {
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    use iced::{Event, mouse};
    use std::collections::HashMap;

    #[derive(Debug)]
    enum Message {
        Grid(Action),
        Pick(ControlRef),
    }
    let mut project = project();
    project.insert_group(
        Track::new_group(TrackId(101), "Group", project.scenes.len()),
        &[TrackId(1), TrackId(2)],
    );
    // No clip or stop in the group's third row: it has no launch target.
    project[TrackId(2)].slots[2].has_stop_button = false;
    for folded in [false, true] {
        project[TrackId(101)].fold_state = folded;
        let grid = grid(&project);
        let regions = grid.declared_regions();
        for (rect, control) in &regions {
            assert_eq!(rect.height, CELL_H);
            let ci = grid.layout.xs.iter().position(|x| *x == rect.x).unwrap();
            let col = &grid.layout.columns[ci];
            let fold = if matches!(control, ControlRef::TrackSelect(_)) && col.caps.foldable {
                FOLD_W
            } else {
                0.0
            };
            assert_eq!(rect.width, col.width - fold);
            assert_eq!(
                grid.identity_at(rect.center()).unwrap().drives.as_ref(),
                Some(control)
            );
        }
        let size = Size::new(600.0, 200.0);
        let mut element = crate::control::map_layer::map_layer(
            grid.with_help(size.width, size.height).map(Message::Grid),
            true,
            crate::control::LIVE_DARK,
            crate::control::LIVE_DARK.learn_midi,
            HashMap::new(),
            Message::Pick,
        );
        let renderer = crate::grid::test_renderer();
        let mut tree = Tree::new(&element);
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        // Left, middle and far right of both launch and empty-stop cells use the same id.
        for (rect, control) in regions {
            for x in [rect.x + 2.0, rect.center_x(), rect.x + rect.width - 2.0] {
                let mut messages = Vec::new();
                let mut shell = Shell::new(&mut messages);
                element.as_widget_mut().update(
                    &mut tree,
                    &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    Layout::new(&node),
                    mouse::Cursor::Available(Point::new(x, rect.center_y())),
                    &renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut shell,
                    &Rectangle::with_size(size),
                );
                assert!(shell.is_event_captured());
                assert!(
                    matches!(&messages[..], [Message::Pick(picked)] if *picked == control),
                    "{messages:?}"
                );
                assert!(!messages.iter().any(
                    |m| matches!(m, Message::Grid(action) if !matches!(action, Action::Info(_)))
                ));
            }
        }
    }
}

#[test]
fn the_main_column_declares_scene_launch_stop_all_and_follow_actions() {
    use crate::grid::{MASTER_W, status_row};
    let scenes = project().scenes;
    let main = Track::new_main(TrackId(0));
    let mut master = crate::grid::test_support::master(&main, &scenes);
    let drives = |master: &Master<'_>, point| master.identity_at(point).and_then(|i| i.drives);
    assert_eq!(
        drives(&master, Point::new(4.0, HEADER_H + 4.0)),
        Some(ControlRef::Scene(SceneId(1)))
    );
    assert_eq!(drives(&master, Point::new(40.0, HEADER_H + 4.0)), None);
    master.status_row = true;
    assert_eq!(
        drives(&master, Point::new(4.0, 4.0)),
        Some(ControlRef::Transport(TransportControl::StopAll))
    );
    let follow = status_row::follow_area();
    assert!(follow.x < MASTER_W);
    assert_eq!(
        drives(&master, follow.center()),
        Some(ControlRef::Transport(TransportControl::FollowActions))
    );
}

#[test]
fn nested_slot_identities_follow_leaf_content_at_every_fold_geometry() {
    for folded in [false, true] {
        let mut p = crate::grid::nested_header_tests::project();
        p.tracks[0].fold_state = folded;
        // All real leaf stops off: header flags alone must not expose a mapping.
        for track in &mut p.tracks {
            if track.capabilities().holds_clips {
                for slot in &mut track.slots {
                    slot.has_stop_button = false;
                }
            }
        }
        let g = grid(&p);
        let declared = g.declared_regions();
        for row in 0..4 {
            let reference = ControlRef::ClipSlot {
                track: TrackId(1),
                scene: p.scenes[row].id,
            };
            let (area, _) = declared
                .iter()
                .find(|(_, control)| *control == reference)
                .unwrap();
            assert_eq!(
                area.y,
                g.layout.header_h + row as f32 * (crate::grid::CELL_H + GAP)
            );
            assert_eq!(
                g.identity_at(area.center()).unwrap().drives,
                Some(reference.clone())
            );
            assert!(describe_control(&p, &reference).is_some());
            assert_eq!(
                g.button_hit(Point::new(area.x + 4.0, area.center_y())),
                Some(crate::grid::Hit::GroupLaunch(0, row))
            );
        }
        assert!(!declared.iter().any(|(_, control)| *control
            == ControlRef::ClipSlot {
                track: TrackId(1),
                scene: p.scenes[4].id
            }));
    }
}
