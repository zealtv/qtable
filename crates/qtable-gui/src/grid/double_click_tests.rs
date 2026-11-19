//! Double-click detection and the empty-slot clip creation it triggers.
use super::cache::GridState;
use super::last_row_tests::{press_sequence, renderer};
use super::*;
use crate::app::App;
use crate::messages::{FileMessage, Message};
use iced::Size;
use qtable_engine::engine::Engine;
use qtable_engine::model::{BeatTime, Colour, Project};
use std::time::{Duration, Instant};

fn app() -> App {
    let mut app = App::new(Engine::new(Project::default()).unwrap(), "120".into(), None);
    let _ = crate::update::update(&mut app, Message::Files(FileMessage::New));
    app
}

#[test]
fn second_press_in_place_is_a_double_click_and_a_far_press_is_not() {
    let t0 = Instant::now();
    let after = |ms| t0 + Duration::from_millis(ms);
    let mut state = GridState::default();
    let here = Point::new(40.0, 40.0);
    assert!(!state.register_click(here, t0));
    assert!(state.register_click(here, after(100)));
    // A third quick press is a triple click, not another create.
    assert!(!state.register_click(here, after(200)));
    let mut state = GridState::default();
    assert!(!state.register_click(here, t0));
    assert!(!state.register_click(Point::new(200.0, 40.0), after(100)));
    // A slow second press starts a new chain.
    let mut state = GridState::default();
    assert!(!state.register_click(here, t0));
    assert!(!state.register_click(here, after(400)));
}

#[test]
fn create_clip_action_selects_the_slot_and_makes_a_track_coloured_one_bar_clip() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    app.chrome.detail_open = false;
    let track_colour: Colour = app.project().tracks[0].colour;
    let _ = crate::update::update(&mut app, Message::Grid(Action::CreateClip(track, 1)));
    let clip = app.project().tracks[0].slots[1]
        .clip
        .as_ref()
        .expect("clip created");
    assert!(app.chrome.detail_open);
    assert_eq!(clip.colour, track_colour);
    assert_eq!(clip.length(), BeatTime::from_beats(4).unwrap());
    assert!(app.project().tracks[0].slots[0].clip.is_none());
    assert_eq!(app.selection.clip, Some(Selection { track, slot: 1 }));
}

#[test]
fn create_clip_on_an_occupied_slot_reports_the_error_and_keeps_the_clip() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let _ = crate::update::update(&mut app, Message::Grid(Action::CreateClip(track, 0)));
    let before = app.project().tracks[0].slots[0].clip.clone();
    app.chrome.detail_open = false;
    let _ = crate::update::update(&mut app, Message::Grid(Action::CreateClip(track, 0)));
    assert_eq!(app.project().tracks[0].slots[0].clip, before);
    assert!(app.status.message().is_some());
    assert!(!app.chrome.detail_open);
}

const SIZE: Size = Size::new(800.0, 700.0);

/// A point in the tracks area, inside track `track` and slot `slot`; `square` picks the
/// stop/launch square at the cell's left rather than the body.
fn cell_point(track: usize, slot: usize, square: bool) -> Point {
    let dx = if square { 10.0 } else { 60.0 };
    Point::new(
        GAP + track as f32 * (TRACK_W + GAP) + dx,
        HEADER_H + slot as f32 * (CELL_H + GAP) + CELL_H / 2.0,
    )
}

#[test]
fn double_press_on_an_empty_slot_body_creates_even_with_a_stop_button() {
    let app = app();
    let track = app.project().tracks[0].id;
    // A new project's empty slots carry a stop button; Live creates on the body anyway.
    assert!(app.project().tracks[0].slots[0].has_stop_button);
    let presses = press_sequence(&app, &renderer(), SIZE, cell_point(0, 0, false), 2);
    assert_eq!(
        presses,
        vec![
            vec![Action::Select(track, 0)],
            vec![Action::CreateClip(track, 0)]
        ]
    );
}

#[test]
fn double_press_on_the_stop_square_keeps_stopping_the_track() {
    let app = app();
    let track = app.project().tracks[0].id;
    let presses = press_sequence(&app, &renderer(), SIZE, cell_point(0, 0, true), 2);
    assert_eq!(
        presses,
        vec![
            vec![Action::StopSlot(track, 0)],
            vec![Action::StopSlot(track, 0)]
        ]
    );
}

#[test]
fn double_press_on_a_clip_body_selects_then_opens_the_detail_view() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let _ = crate::update::update(&mut app, Message::Grid(Action::CreateClip(track, 0)));
    let presses = press_sequence(&app, &renderer(), SIZE, cell_point(0, 0, false), 2);
    assert_eq!(
        presses,
        vec![
            vec![Action::Select(track, 0)],
            vec![Action::OpenClip(track, 0)]
        ]
    );
}

#[test]
fn add_track_action_appends_a_track_and_the_gap_starts_after_the_last_column() {
    let mut app = app();
    let before = app.project().tracks.len();
    let _ = crate::update::update(&mut app, Message::Grid(Action::AddTrack));
    assert_eq!(app.project().tracks.len(), before + 1);
    let layout = app.grid_layout.get(app.project(), 0);
    let last = layout.columns.len() - 1;
    assert_eq!(layout.end(), layout.xs[last] + layout.columns[last].width);
}
