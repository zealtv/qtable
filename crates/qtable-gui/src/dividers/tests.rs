use super::*;

/// Compare the dragged Detail View height, allowing for f32 rounding.
fn assert_height(sizes: &Sizes, expected: f32) {
    let height = sizes.detail_height().expect("a drag has set the height");
    assert!((height - expected).abs() < 0.01, "{height} != {expected}");
}

fn at(x: f32, y: f32) -> Point {
    Point::new(x, y)
}

#[test]
fn defaults_match_the_original_layout() {
    let sizes = Sizes::default();
    assert_eq!(sizes.browser_width(), 220.0);
    assert_eq!(sizes.detail_height(), None, "proportional until dragged");
}

#[test]
fn dragging_the_browser_divider_right_widens_the_browser() {
    let mut sizes = Sizes::default();
    sizes.begin(Divider::Browser, at(224.0, 300.0));
    sizes.drag_to(at(274.0, 310.0));
    assert_eq!(sizes.browser_width(), 270.0);
    assert_eq!(sizes.detail_height(), None);
}

#[test]
fn dragging_the_detail_divider_up_heightens_the_detail_view() {
    let mut sizes = Sizes::default();
    sizes.begin(Divider::Detail, at(500.0, 400.0));
    sizes.drag_to(at(520.0, 350.0));
    // A first drag starts from the default share, 2/5 of the 678 px below the chrome.
    assert_height(&sizes, 271.2 + 50.0);
    assert_eq!(sizes.browser_width(), 220.0);
}

#[test]
fn sizes_stop_at_their_minimums_and_at_the_grid_minimum() {
    let mut sizes = Sizes::default();
    sizes.begin(Divider::Browser, at(224.0, 0.0));
    sizes.drag_to(at(-500.0, 0.0));
    assert_eq!(sizes.browser_width(), MIN_BROWSER_WIDTH);
    sizes.drag_to(at(5000.0, 0.0));
    assert_eq!(sizes.browser_width(), 1024.0 - MIN_GRID_WIDTH);
    sizes.end();
    sizes.begin(Divider::Detail, at(0.0, 400.0));
    sizes.drag_to(at(0.0, 5000.0));
    assert_height(&sizes, MIN_DETAIL_HEIGHT);
    sizes.drag_to(at(0.0, -5000.0));
    assert_height(&sizes, 768.0 - WINDOW_CHROME_HEIGHT - MIN_GRID_HEIGHT);
}

#[test]
fn motion_without_a_drag_changes_nothing() {
    let mut sizes = Sizes::default();
    sizes.drag_to(at(900.0, 10.0));
    assert_eq!(sizes, Sizes::default());
    sizes.begin(Divider::Detail, at(0.0, 400.0));
    sizes.end();
    sizes.drag_to(at(0.0, 100.0));
    assert_eq!(sizes.detail_height(), None);
}

#[test]
fn a_smaller_window_clamps_the_reported_sizes_but_keeps_the_choice() {
    let mut sizes = Sizes::default();
    sizes.begin(Divider::Detail, at(0.0, 400.0));
    sizes.drag_to(at(0.0, 200.0));
    sizes.end();
    assert_height(&sizes, 468.0);
    sizes.resize_window(Size::new(1024.0, 500.0));
    assert_height(&sizes, 500.0 - WINDOW_CHROME_HEIGHT - MIN_GRID_HEIGHT);
    sizes.resize_window(Size::new(1024.0, 768.0));
    assert_height(&sizes, 468.0);
}

#[test]
fn a_window_too_small_for_the_minimums_does_not_invert_the_clamp() {
    let mut sizes = Sizes::default();
    sizes.resize_window(Size::new(200.0, 100.0));
    sizes.begin(Divider::Detail, at(0.0, 50.0));
    sizes.drag_to(at(0.0, 0.0));
    assert_eq!(sizes.browser_width(), MIN_BROWSER_WIDTH);
    assert_height(&sizes, MIN_DETAIL_HEIGHT);
}

#[test]
fn update_messages_drive_a_drag_and_any_release_ends_it() {
    use crate::{app::App, files::new_project, messages::Message, update::update};
    use qtable_engine::engine::Engine;
    let mut app = App::new(Engine::new(new_project()).unwrap(), "120".into(), None);
    let _ = update(&mut app, Message::CursorMoved(at(224.0, 400.0)));
    let _ = update(&mut app, Message::BeginDrag(Divider::Browser));
    let _ = update(&mut app, Message::CursorMoved(at(264.0, 400.0)));
    assert_eq!(app.dividers.browser_width(), 260.0);
    let _ = update(&mut app, Message::CheckFieldFocus);
    let _ = update(&mut app, Message::CursorMoved(at(400.0, 400.0)));
    assert_eq!(app.dividers.browser_width(), 260.0);
    assert!(!app.is_dirty(), "sizes are session state, not the set");
    let _ = update(&mut app, Message::WindowResized(Size::new(500.0, 700.0)));
    assert_eq!(app.dividers.browser_width(), 500.0 - MIN_GRID_WIDTH);
}

#[test]
fn device_height_shrinks_for_the_grid_without_changing_the_clip_choice() {
    let mut sizes = Sizes::default();
    sizes.begin(Divider::Detail, at(0.0, 400.0));
    sizes.drag_to(at(0.0, 200.0));
    sizes.end();
    assert_eq!(sizes.shown_detail_height(true), Some(238.0));
    assert_height(&sizes, 468.0);
    sizes.resize_window(Size::new(1024.0, 400.0));
    assert_eq!(sizes.shown_detail_height(true), Some(100.0));
    sizes.resize_window(Size::new(1024.0, 200.0));
    assert_eq!(sizes.shown_detail_height(true), Some(0.0));
    sizes.resize_window(Size::new(1024.0, 768.0));
    assert_eq!(sizes.shown_detail_height(true), Some(238.0));
    assert_height(&sizes, 468.0);
}

#[test]
fn selecting_devices_cancels_a_clip_drag_and_restores_its_height_on_return() {
    use crate::{app::App, grid, inspector_panel::Tab, update::update};
    use qtable_engine::engine::Engine;
    let mut app = App::new(
        Engine::new(crate::files::new_project()).unwrap(),
        String::new(),
        None,
    );
    let track = app.project().tracks[0].id;
    app.selection.inspector_tab = Tab::Clip;
    let _ = update(&mut app, Message::CursorMoved(at(500.0, 400.0)));
    let _ = update(&mut app, Message::BeginDrag(Divider::Detail));
    let _ = update(&mut app, Message::CursorMoved(at(500.0, 350.0)));
    assert_height(&app.dividers, 321.2);
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(track, false)),
    );
    assert_eq!(app.selection.inspector_tab, Tab::Track);
    let _ = update(&mut app, Message::CursorMoved(at(500.0, 200.0)));
    let _ = update(&mut app, Message::BeginDrag(Divider::Detail));
    let _ = update(&mut app, Message::CursorMoved(at(500.0, 100.0)));
    assert_height(&app.dividers, 321.2);
    assert_eq!(app.dividers.shown_detail_height(true), Some(238.0));
    let _ = update(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    assert_eq!(app.selection.inspector_tab, Tab::Clip);
    assert_eq!(
        app.dividers.shown_detail_height(false),
        app.dividers.detail_height()
    );
    assert_height(&app.dividers, 321.2);
    assert!(!app.is_dirty());
}

#[test]
fn a_short_window_keeps_the_grid_minimum_even_at_the_default_share() {
    let mut sizes = Sizes::default();
    assert_eq!(sizes.shown_detail_height(false), None);
    sizes.resize_window(Size::new(1024.0, 430.0));
    assert_eq!(
        sizes.shown_detail_height(false),
        Some(430.0 - WINDOW_CHROME_HEIGHT - MIN_GRID_HEIGHT)
    );
    // The Detail View's own minimum still holds in a very short window.
    sizes.resize_window(Size::new(1024.0, 400.0));
    assert_eq!(sizes.shown_detail_height(false), Some(MIN_DETAIL_HEIGHT));
    sizes.resize_window(Size::new(1024.0, 768.0));
    assert_eq!(sizes.shown_detail_height(false), None);
}

#[test]
fn the_shortest_window_holds_the_grid_minimum_above_the_detail_minimum() {
    assert_eq!(MIN_WINDOW_HEIGHT, 90.0 + 210.0 + 120.0);
    let mut sizes = Sizes::default();
    sizes.resize_window(Size::new(1024.0, MIN_WINDOW_HEIGHT));
    assert_eq!(sizes.shown_detail_height(false), Some(MIN_DETAIL_HEIGHT));
}
