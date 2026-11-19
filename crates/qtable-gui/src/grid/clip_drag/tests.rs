use super::*;
use crate::grid::{GAP, TRACK_W, cache::GridState, test_support};
use iced::widget::canvas::Program;
use qtable_engine::model::{
    BeatTime, Clip, LIVE_CLIP_PALETTE, Project, Scene, SceneId, Track, TrackId,
};

fn project() -> Project {
    let mut p = Project {
        scenes: (1..=2)
            .map(|id| Scene {
                id: SceneId(id),
                name: String::new(),
                follow_actions: Default::default(),
            })
            .collect(),
        tracks: vec![
            Track::new(TrackId(1), "A", 2),
            Track::new(TrackId(2), "B", 2),
        ],
        ..Project::default()
    };
    p.tracks[0].slots[0].clip = Some(
        Clip::new(
            ClipId(1),
            "Source",
            BeatTime::from_beats(4).unwrap(),
            LIVE_CLIP_PALETTE[0],
        )
        .unwrap(),
    );
    p.tracks[1].slots[1].clip = Some(
        Clip::new(
            ClipId(2),
            "Target",
            BeatTime::from_beats(4).unwrap(),
            LIVE_CLIP_PALETTE[0],
        )
        .unwrap(),
    );
    p.tracks[1].slots[1].has_stop_button = false;
    p
}
fn at(column: usize, row: usize) -> Point {
    Point::new(
        GAP + column as f32 * (TRACK_W + GAP) + 45.0,
        draw::row_y(row) + 10.0,
    )
}
fn event(grid: &Grid<'_>, state: &mut GridState, e: canvas::Event, at: Point) -> Option<Action> {
    grid.update(
        state,
        &e,
        Rectangle::new(Point::ORIGIN, Size::new(500.0, 300.0)),
        mouse::Cursor::Available(at),
    )
    .and_then(|a| a.into_inner().0)
}
fn press(grid: &Grid<'_>, state: &mut GridState) {
    assert_eq!(
        event(
            grid,
            state,
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            at(0, 0)
        ),
        Some(Action::Select(TrackId(1), 0))
    );
}
fn release(grid: &Grid<'_>, state: &mut GridState, at: Point) -> Option<Action> {
    event(
        grid,
        state,
        canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        at,
    )
}
#[test]
fn threshold_latches_and_release_uses_current_position() {
    let p = project();
    let grid = test_support::grid(&p);
    let mut state = GridState::default();
    press(&grid, &mut state);
    let origin = at(0, 0);
    state
        .clip_drag
        .as_mut()
        .unwrap()
        .move_to(Point::new(origin.x + 3.9, origin.y));
    assert!(
        grid.clip_drop(state.clip_drag.as_ref().unwrap(), at(1, 1))
            .is_none()
    );
    state
        .clip_drag
        .as_mut()
        .unwrap()
        .move_to(Point::new(origin.x + 4.0, origin.y));
    state.clip_drag.as_mut().unwrap().move_to(origin);
    assert!(state.clip_drag.as_ref().unwrap().started);
    assert_eq!(
        release(&grid, &mut state, at(1, 1)),
        Some(Action::DropClip {
            from: Selection {
                track: TrackId(1),
                slot: 0
            },
            to: Selection {
                track: TrackId(2),
                slot: 1
            },
            copy: false
        })
    );
    assert!(state.clip_drag.is_none());
    press(&grid, &mut state);
    state.clip_drag.as_mut().unwrap().move_to(at(1, 1));
    assert_eq!(release(&grid, &mut state, Point::new(600.0, 40.0)), None);
    assert!(state.clip_drag.is_none());
}
#[test]
fn invalid_targets_and_replaced_source_cancel() {
    let mut p = project();
    {
        let mut group = Track::new_group(TrackId(101), "G", p.scenes.len());
        group.colour = LIVE_CLIP_PALETTE[0];
        group.fold_state = true;
        p.insert_group(group, &[TrackId(2)]);
    }
    let grid = test_support::grid(&p);
    let mut drag = ClipDrag::new(
        Selection {
            track: TrackId(1),
            slot: 0,
        },
        ClipId(1),
        at(0, 0),
    );
    drag.move_to(at(1, 1));
    for pos in [
        at(1, 1),
        at(2, 1),
        at(0, 2),
        Point::new(30.0, 10.0),
        Point::new(0.0, 40.0),
        at(0, 0),
    ] {
        assert!(grid.clip_drop(&drag, pos).is_none());
    }
    let mut status = grid.clone();
    status.status_row = true;
    assert!(status.clip_drop(&drag, at(0, 1)).is_none());
    p.tracks[0].slots[0].clip.as_mut().unwrap().id = ClipId(3);
    assert!(test_support::grid(&p).clip_drop(&drag, at(0, 1)).is_none());
}
#[test]
fn modifier_copy_reaches_engine_and_preserves_slots() {
    let p = project();
    let grid = test_support::grid(&p);
    let mut state = GridState::default();
    press(&grid, &mut state);
    let modifier = if cfg!(target_os = "windows") {
        keyboard::Modifiers::CTRL
    } else {
        keyboard::Modifiers::ALT
    };
    event(
        &grid,
        &mut state,
        canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(modifier)),
        at(1, 1),
    );
    let action = release(&grid, &mut state, at(1, 1)).unwrap();
    let engine = qtable_engine::engine::Engine::new(p.clone()).unwrap();
    let mut app = crate::app::App::new(engine, "120".into(), None);
    let _ = crate::update::update(&mut app, crate::messages::Message::Grid(action));
    let copied = app.project().tracks[1].slots[1].clip.as_ref().unwrap();
    assert_ne!(copied.id, ClipId(1));
    assert_eq!(copied.name, "Source");
    assert_eq!(
        app.project().tracks[0].slots[0].clip,
        p.tracks[0].slots[0].clip
    );
    assert!(!app.project().tracks[1].slots[1].has_stop_button);
    assert_eq!(
        app.selection.clip,
        Some(Selection {
            track: TrackId(2),
            slot: 1
        })
    );
    let _ = crate::update::update(
        &mut app,
        crate::messages::Message::Grid(Action::DropClip {
            from: Selection {
                track: TrackId(1),
                slot: 0,
            },
            to: Selection {
                track: TrackId(2),
                slot: 1,
            },
            copy: false,
        }),
    );
    assert!(app.project().tracks[0].slots[0].clip.is_none());
    assert_eq!(
        app.project().tracks[1].slots[1].clip.as_ref().unwrap().id,
        ClipId(1)
    );
    assert_eq!(app.project().scenes.len(), 2);
}

#[test]
fn clicks_launch_buttons_empty_slots_and_cancel_do_not_drop() {
    let p = project();
    let grid = test_support::grid(&p);
    let mut state = GridState::default();
    press(&grid, &mut state);
    assert_eq!(release(&grid, &mut state, at(0, 0)), None);
    assert!(state.clip_drag.is_none());
    event(
        &grid,
        &mut state,
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Point::new(GAP + 5.0, draw::row_y(0) + 10.0),
    );
    assert!(state.clip_drag.is_none());
    event(
        &grid,
        &mut state,
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        at(0, 1),
    );
    assert!(state.clip_drag.is_none());
    press(&grid, &mut state);
    let mut target = at(0, 1);
    target.x = GAP + 5.0; // An empty slot's stop square is a valid drop region.
    assert!(matches!(
        release(&grid, &mut state, target),
        Some(Action::DropClip { copy: false, .. })
    ));
    press(&grid, &mut state);
    event(
        &grid,
        &mut state,
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        at(1, 1),
    );
    assert!(state.clip_drag.is_none());
    assert_eq!(release(&grid, &mut state, at(1, 1)), None);
}
