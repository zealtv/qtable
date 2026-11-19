//! Right-click on a slot publishes the context-menu action; elsewhere it does nothing.
use super::last_row_tests::{press_button_sequence, renderer};
use super::*;
use crate::app::App;
use crate::messages::{FileMessage, Message};
use iced::Size;
use qtable_engine::engine::Engine;
use qtable_engine::model::Project;

const SIZE: Size = Size::new(800.0, 750.0);

fn app() -> App {
    let mut app = App::new(Engine::new(Project::default()).unwrap(), "120".into(), None);
    let _ = crate::update::update(&mut app, Message::Files(FileMessage::New));
    app
}

fn right_click(app: &App, point: Point) -> Vec<Action> {
    press_button_sequence(app, &renderer(), SIZE, point, mouse::Button::Right, 1)
        .into_iter()
        .flatten()
        .collect()
}

fn cell_point(slot: usize, square: bool) -> Point {
    Point::new(
        GAP + if square { 10.0 } else { 60.0 },
        HEADER_H + slot as f32 * (CELL_H + GAP) + CELL_H / 2.0,
    )
}

#[test]
fn right_click_on_an_empty_slot_body_or_square_opens_the_menu() {
    let app = app();
    let track = app.project().tracks[0].id;
    assert_eq!(
        right_click(&app, cell_point(1, false)),
        vec![Action::ContextMenu(track, 1)]
    );
    assert_eq!(
        right_click(&app, cell_point(1, true)),
        vec![Action::ContextMenu(track, 1)]
    );
}

#[test]
fn right_click_on_a_clip_opens_the_menu_without_launching() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let _ = crate::update::update(&mut app, Message::Grid(Action::CreateClip(track, 0)));
    assert_eq!(
        right_click(&app, cell_point(0, true)),
        vec![Action::ContextMenu(track, 0)]
    );
}

#[test]
fn right_click_on_a_track_title_opens_the_track_menu() {
    let app = app();
    let track = app.project().tracks[0].id;
    assert_eq!(
        right_click(&app, Point::new(GAP + 60.0, TRACK_H / 2.0)),
        vec![Action::HeaderMenu(track)]
    );
}

/// A new project's track plus a second one, grouped.
fn grouped_app() -> App {
    let mut app = app();
    let _ = crate::update::update(
        &mut app,
        Message::Edit(crate::messages::EditMessage::AddTrack),
    );
    let tracks = app.project().tracks.iter().map(|t| t.id).collect();
    app.engine
        .execute(qtable_engine::engine::Command::Edit(
            qtable_engine::edit::ProjectEdit::CreateGroup { tracks },
        ))
        .expect("group created");
    app
}

#[test]
fn right_click_on_a_group_track_title_opens_the_group_menu_and_a_member_title_the_track_menu() {
    let app = grouped_app();
    let group = app.project().group_tracks().next().unwrap().id;
    let first = app.project().clip_tracks().next().unwrap().id;
    assert_eq!(
        right_click(&app, Point::new(GAP + 10.0, TRACK_H / 2.0)),
        vec![Action::HeaderMenu(group)]
    );
    let title = Point::new(GAP + TRACK_W + GAP + 10.0, TRACK_H / 2.0);
    assert_eq!(right_click(&app, title), vec![Action::HeaderMenu(first)]);
}
