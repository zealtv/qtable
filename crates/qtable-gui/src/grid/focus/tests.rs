//! Canvas buttons claim the window's focus outline from the same rectangle they hit-test.
use super::super::last_row_tests::{renderer, tracks_layout};
use super::super::test_support;
use super::*;
use crate::app::App;
use crate::grid::HEADER_H;
use crate::messages::{FileMessage, Message};
use iced::advanced::{Layout as WidgetLayout, Shell, layout, widget::Tree};
use iced::{Event, mouse};
use qtable_engine::engine::Engine;
use qtable_engine::model::Project;

fn app() -> App {
    let mut app = App::new(Engine::new(Project::default()).unwrap(), "120".into(), None);
    let _ = crate::update::update(&mut app, Message::Files(FileMessage::New));
    app
}

/// Everything one left press at `point` (relative to the tracks area) publishes.
fn press(app: &App, point: Point) -> Vec<Message> {
    let size = Size::new(800.0, 700.0);
    let renderer = renderer();
    let mut widget = crate::view::view(app);
    let mut tree = Tree::new(widget.as_widget());
    let node =
        widget
            .as_widget_mut()
            .layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
    let origin = tracks_layout(&node).bounds().position();
    let mut messages = vec![];
    widget.as_widget_mut().update(
        &mut tree,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        WidgetLayout::new(&node),
        mouse::Cursor::Available(Point::new(origin.x + point.x, origin.y + point.y)),
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut Shell::new(&mut messages),
        &Rectangle::with_size(size),
    );
    messages
}

fn claims(messages: &[Message]) -> Vec<ControlId> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::Grid(super::super::Action::ControlFocused(id)) => Some(*id),
            _ => None,
        })
        .collect()
}

#[test]
fn a_slot_button_click_claims_its_slot_and_still_acts() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    // A new project's empty slot shows a stop square; pressing it stops the track.
    let square = Point::new(GAP + 10.0, HEADER_H + CELL_H / 2.0);
    let messages = press(&app, square);
    let launch = id("launch", ControlOwner::ClipSlot(track, 0));
    assert_eq!(claims(&messages), [launch]);
    assert!(messages.iter().any(|message| matches!(
        message,
        Message::Grid(super::super::Action::StopSlot(t, 0)) if *t == track
    )));
    for message in messages {
        let _ = crate::update::update(&mut app, message);
    }
    assert!(app.control_focus.has(launch));
    // The slot body is not a button: selecting it leaves the focus where it was.
    let body = press(&app, Point::new(GAP + 60.0, HEADER_H + CELL_H / 2.0));
    assert!(claims(&body).is_empty());
}

#[test]
fn the_outline_rectangle_is_the_rectangle_that_was_hit() {
    let project = crate::files::new_project();
    let mut grid = test_support::grid(&project);
    let track = project.tracks[0].id;
    for (status_row, point, expected) in [
        (
            false,
            Point::new(GAP + 3.0, HEADER_H + 2.0 * (CELL_H + GAP) + 3.0),
            id("launch", ControlOwner::ClipSlot(track, 2)),
        ),
        (
            true,
            Point::new(GAP + 3.0, 5.0),
            id("stop", ControlOwner::Track(track)),
        ),
    ] {
        grid.status_row = status_row;
        assert_eq!(grid.control_at(point), Some(expected));
        let (_, area) = grid
            .buttons()
            .find(|(id, _)| *id == expected)
            .expect("every claimable button has an outline rectangle");
        assert!(area.contains(point), "{area:?} must contain {point:?}");
    }
}

#[test]
fn main_buttons_claim_scene_stop_all_and_follow_actions() {
    let project = crate::files::new_project();
    let master = Master {
        header_h: crate::grid::HEADER_H,
        status_row: true,
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
    assert_eq!(
        master.control_at(Point::new(5.0, 5.0)),
        Some(id("stop-all", ControlOwner::Song))
    );
    let follow = super::super::status_row::follow_area().center();
    assert_eq!(
        master.control_at(follow),
        Some(id("follow-actions", ControlOwner::Song))
    );
    let rows = Master {
        header_h: crate::grid::HEADER_H,
        status_row: false,
        main: &project.main,
        clip: None,
        ..master
    };
    let scene = project.scenes[1].id;
    assert_eq!(
        rows.control_at(Point::new(5.0, HEADER_H + CELL_H + GAP + 5.0)),
        Some(id("launch", ControlOwner::Scene(scene)))
    );
}

#[test]
fn inspector_claims_reach_the_window_focus_and_clip_fields_belong_to_their_slot() {
    use crate::drafts::{ClipField, DraftKey};
    let mut app = app();
    let track = app.project().tracks[0].id;
    let length =
        |slot| ControlId::field(View::Clip, DraftKey::clip(track, slot, ClipField::Length));
    assert_ne!(length(0), length(1));
    let _ = crate::update::update(
        &mut app,
        Message::Inspector(crate::inspector::Message::ControlFocused(length(0))),
    );
    assert!(app.control_focus.has(length(0)) && !app.control_focus.has(length(1)));
}
