//! Stop feedback stays scoped to the fired scene/slot and follows engine pending state.
use super::*;
use iced::advanced::{Layout as WidgetLayout, layout, renderer::Headless, widget::Tree};
use iced::{Color, Size};
use qtable_engine::model::{BeatTime, ClipId};

fn pixels(grid: Grid<'_>) -> Vec<u8> {
    let size = Size::new(120.0, 240.0);
    let mut widget = grid.with_help(size.width, size.height);
    let mut renderer = test_renderer();
    let mut tree = Tree::new(&widget);
    let node =
        widget
            .as_widget_mut()
            .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
    widget.as_widget().draw(
        &tree,
        &mut renderer,
        &iced::Theme::Dark,
        &iced::advanced::renderer::Style::default(),
        WidgetLayout::new(&node),
        mouse::Cursor::Unavailable,
        &Rectangle::with_size(size),
    );
    renderer.screenshot(Size::new(120, 240), 1.0, Color::BLACK)
}

#[test]
fn only_the_fired_stop_square_blinks_and_the_playing_triangle_stays_green() {
    let mut project = crate::files::new_project();
    project.tracks[0].slots[0].clip = Some(
        Clip::new(
            ClipId(1),
            "Playing",
            BeatTime(3840),
            qtable_engine::model::LIVE_CLIP_PALETTE[0],
        )
        .unwrap(),
    );
    let mut grid = test_support::grid(&project);
    grid.cells[0][0].status = CellStatus::Playing(0.25);
    grid.pending_stop[0] = Some(1);
    let pixel = |image: &[u8], x: usize, y: usize| -> [u8; 4] {
        image[(y * 120 + x) * 4..(y * 120 + x) * 4 + 4]
            .try_into()
            .unwrap()
    };
    for selected in [false, true] {
        grid.cells[0][1].selected = selected;
        grid.blink_on = true;
        let on = pixels(grid.clone());
        grid.blink_on = false;
        let off = pixels(grid.clone());
        let x = (GAP + CELL_H / 2.0) as usize;
        let y = (HEADER_H + CELL_H + GAP + CELL_H / 2.0) as usize;
        assert_eq!(pixel(&on, x, y), draw::GREEN.into_rgba8());
        assert_eq!(
            pixel(&off, x, y),
            crate::control::theme::DARK_INK.into_rgba8()
        );
        // Every changed pixel lies inside the fired slot's launch square.
        for y in 0..240 {
            for x in 0..120 {
                if pixel(&on, x, y) != pixel(&off, x, y) {
                    assert!((GAP..GAP + CELL_H).contains(&(x as f32)));
                    assert!(
                        (HEADER_H + CELL_H + GAP..HEADER_H + 2.0 * CELL_H + GAP)
                            .contains(&(y as f32))
                    );
                }
            }
        }
        // In particular, the playing icon is still green in both phases.
        let play_y = (HEADER_H + CELL_H / 2.0) as usize;
        assert_eq!(pixel(&on, x, play_y), draw::GREEN.into_rgba8());
        assert_eq!(pixel(&off, x, play_y), draw::GREEN.into_rgba8());
    }
    grid.status_row = true;
    grid.blink_on = true;
    let footer_on = pixels(grid.clone());
    grid.blink_on = false;
    assert_eq!(footer_on, pixels(grid.clone()));
    grid.status_row = false;
    grid.pending_stop[0] = None;
    let landed = pixels(grid.clone());
    grid.blink_on = true;
    assert_eq!(landed, pixels(grid));
}

#[test]
fn a_direct_stop_preserves_selection_and_publishes_the_fired_slot() {
    use crate::{app::App, messages::Message};
    use qtable_engine::engine::Engine;
    use std::time::{Duration, Instant};
    let mut app = App::new(
        Engine::new(crate::files::new_project()).unwrap(),
        String::new(),
        None,
    );
    let track = app.project().tracks[0].id;
    let _ = crate::update::update(&mut app, Message::Grid(Action::CreateClip(track, 0)));
    let _ = crate::update::update(&mut app, Message::Grid(Action::LaunchClip(track, 0)));
    let until = Instant::now() + Duration::from_secs(1);
    while app.engine.snapshot().tracks[0].playing_slot != Some(0) {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
    // Request inside the bar, rather than on the immediate beat-zero boundary.
    std::thread::sleep(Duration::from_millis(10));
    let cell = app.selection.cell;
    let scene = app.selection.scene;
    let _ = crate::update::update(&mut app, Message::Grid(Action::StopSlot(track, 1)));
    assert_eq!(app.selection.clip, Some(Selection { track, slot: 0 }));
    assert_eq!(app.selection.cell, cell);
    assert_eq!(app.selection.scene, scene);
    let until = Instant::now() + Duration::from_secs(1);
    loop {
        let snapshot = app.engine.snapshot();
        if snapshot.pending_stop_slots == vec![(track, 1)] {
            assert_eq!(snapshot.tracks[0].playing_slot, Some(0));
            break;
        }
        assert!(
            Instant::now() < until,
            "slot stop never reached the snapshot"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// V37-R2: Enter on an empty stop slot must retain the same source as a square click.
#[test]
fn review_v37_r2_keyboard_stop_publishes_the_fired_slot() {
    use crate::{
        app::App,
        focus::{Focus, GridKey},
        messages::Message,
    };
    use qtable_engine::engine::Engine;
    use std::time::{Duration, Instant};
    let mut app = App::new(
        Engine::new(crate::files::new_project()).unwrap(),
        String::new(),
        None,
    );
    let track = app.project().tracks[0].id;
    let _ = crate::update::update(&mut app, Message::Grid(Action::CreateClip(track, 0)));
    let _ = crate::update::update(&mut app, Message::Grid(Action::LaunchClip(track, 0)));
    let until = Instant::now() + Duration::from_secs(1);
    while app.engine.snapshot().tracks[0].playing_slot != Some(0) {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
    }
    std::thread::sleep(Duration::from_millis(10));
    let _ = crate::update::update(&mut app, Message::Grid(Action::Select(track, 1)));
    app.focus = Focus::Grid;
    let _ = crate::update::update(&mut app, Message::GridKey(GridKey::Launch));
    let until = Instant::now() + Duration::from_secs(1);
    loop {
        let snapshot = app.engine.snapshot();
        if !snapshot.pending_stops.is_empty() {
            assert_eq!(
                snapshot.pending_stop_slots,
                vec![(track, 1)],
                "V37-R2: keyboard stop has a boundary but no slot to flash"
            );
            break;
        }
        assert!(Instant::now() < until, "keyboard stop was not published");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn scene_stop_feedback_reaches_nested_groups_and_only_pending_descendants_keep_pulsing() {
    use crate::{app::App, scene_stop_feedback};
    use qtable_engine::engine::Engine;
    use std::time::Instant;
    for (scenario, burst, on) in [
        ("scene-stop-burst", true, true),
        ("scene-stop-on", false, true),
        ("scene-stop-off", false, false),
        ("scene-stop-landed", false, false),
    ] {
        let mut app = App::new(
            Engine::new(crate::files::new_project()).unwrap(),
            "120".into(),
            None,
        );
        assert!(scene_stop_feedback::capture_session(&mut app, scenario));
        let mut grid = test_support::grid(app.project());
        grid.pending_stop = app
            .project()
            .tracks
            .iter()
            .map(|t| {
                app.snapshot
                    .pending_stop_slots
                    .iter()
                    .find_map(|(id, slot)| (*id == t.id).then_some(*slot))
            })
            .collect();
        grid.scene_stop =
            app.scene_stop_feedback
                .rows(app.project(), &app.snapshot, Instant::now());
        let landed = scenario == "scene-stop-landed";
        assert!(app.snapshot.playing, "scene stop keeps transport running");
        for col in &grid.layout.columns {
            if col.group_track().is_some() {
                assert_eq!(grid.stop_feedback(&col.leaves, 1), (!landed).then_some(on));
                assert!(group_column::summary(&grid, col, 1).stop_button);
            }
        }
        assert_eq!(app.project().group_tracks().count(), 2);
        let leaves: Vec<_> = app
            .project()
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.capabilities().holds_clips)
            .map(|(i, _)| i)
            .collect();
        // Active leaf keeps pulsing, inactive leaf gets only the initial burst.
        assert_eq!(grid.stop_feedback(&[leaves[0]], 1), (!landed).then_some(on));
        assert_eq!(grid.stop_feedback(&[leaves[1]], 1), burst.then_some(true));
        assert!(!grid.cells[leaves[2]][1].stop_button);
        assert_eq!(grid.stop_feedback(&[leaves[0]], 2), None);
        if !landed {
            assert_eq!(app.snapshot.pending_stop_slots.len(), 1);
            assert_eq!(
                app.snapshot
                    .tracks
                    .iter()
                    .find(|s| s.id == app.project().tracks[leaves[0]].id)
                    .unwrap()
                    .playing_slot,
                Some(0)
            );
        }
    }
}
