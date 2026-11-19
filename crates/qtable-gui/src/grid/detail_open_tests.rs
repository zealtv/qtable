//! Real widget presses, releases and view rebuilds with a deterministic gesture clock.
use super::*;
use crate::{app::App, inspector_panel::Tab, messages::Message};
use iced::advanced::{Layout as WidgetLayout, Shell, layout, widget::Tree};
use iced::{Event, Size};
use qtable_engine::engine::Engine;
use std::time::Duration;

#[derive(Clone, Copy)]
enum Target {
    Track,
    Clip,
    Scene,
    Main,
    Output,
}

fn press(app: &mut App, tree: &mut Tree, target: Target, button_down: bool) {
    let renderer = last_row_tests::renderer();
    let size = Size::new(1280.0, 900.0);
    let messages = {
        let mut widget = crate::view::view(app);
        tree.diff(widget.as_widget());
        let node =
            widget
                .as_widget_mut()
                .layout(tree, &renderer, &layout::Limits::new(Size::ZERO, size));
        let tracks = last_row_tests::tracks_layout(&node).bounds();
        let main = last_row_tests::main_layout(&node).bounds();
        let point = match target {
            Target::Track => Point::new(tracks.x + 60.0, tracks.y + TRACK_H / 2.0),
            Target::Clip => Point::new(tracks.x + 60.0, tracks.y + HEADER_H + CELL_H / 2.0),
            Target::Scene => Point::new(main.x + 60.0, main.y + HEADER_H + CELL_H / 2.0),
            Target::Main => Point::new(main.x + 60.0, main.y + TRACK_H / 2.0),
            Target::Output => Point::new(
                main.x - 6.0 - crate::outputs::width(app) + 60.0,
                main.y + TRACK_H / 2.0,
            ),
        };
        let mut messages = vec![];
        widget.as_widget_mut().update(
            tree,
            &Event::Mouse(if button_down {
                mouse::Event::ButtonPressed(mouse::Button::Left)
            } else {
                mouse::Event::ButtonReleased(mouse::Button::Left)
            }),
            WidgetLayout::new(&node),
            mouse::Cursor::Available(point),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
        messages
    };
    for message in messages {
        let _ = crate::update::update(app, message);
    }
}

fn app_and_tree() -> (App, Tree) {
    let mut app = App::new(
        Engine::new(crate::files::new_project()).unwrap(),
        String::new(),
        None,
    );
    app.chrome.detail_open = false;
    let tree = Tree::new(crate::view::view(&app).as_widget());
    (app, tree)
}

fn click(
    app: &mut App,
    tree: &mut Tree,
    clock: &crate::gesture_clock::test::Clock,
    target: Target,
) {
    clock.advance(Duration::from_millis(100));
    press(app, tree, target, true);
    press(app, tree, target, false);
}

#[test]
fn title_single_click_selects_and_double_click_reveals_track_without_editing() {
    let clock = crate::gesture_clock::test::Clock::new();
    let (mut app, mut tree) = app_and_tree();
    let track = app.project().tracks[0].id;
    let revision = app.project_revision();
    click(&mut app, &mut tree, &clock, Target::Track);
    assert!(!app.chrome.detail_open);
    assert_eq!(app.selection.track(), Some(track));
    clock.advance(Duration::from_millis(400));
    click(&mut app, &mut tree, &clock, Target::Track);
    assert!(!app.chrome.detail_open, "slow second click only selects");
    click(&mut app, &mut tree, &clock, Target::Track);
    assert!(app.chrome.detail_open);
    assert_eq!(app.selection.inspector_tab, Tab::Track);
    assert_eq!(app.project_revision(), revision);
    assert!(!app.engine.snapshot().playing);
}

#[test]
fn empty_slot_double_click_creates_and_reveals_then_open_panel_follows_selection() {
    let clock = crate::gesture_clock::test::Clock::new();
    let (mut app, mut tree) = app_and_tree();
    let track = app.project().tracks[0].id;
    click(&mut app, &mut tree, &clock, Target::Clip);
    assert!(!app.chrome.detail_open);
    assert!(app.project().tracks[0].clip(0).is_none());
    click(&mut app, &mut tree, &clock, Target::Clip);
    assert!(app.project().tracks[0].clip(0).is_some());
    assert!(app.chrome.detail_open);
    assert_eq!(app.selection.clip, Some(Selection { track, slot: 0 }));
    assert_eq!(app.selection.inspector_tab, Tab::Clip);
    let revision = app.project_revision();
    click(&mut app, &mut tree, &clock, Target::Track);
    assert!(app.chrome.detail_open);
    assert_eq!(app.selection.inspector_tab, Tab::Track);
    click(&mut app, &mut tree, &clock, Target::Clip);
    assert!(app.chrome.detail_open);
    assert_eq!(app.selection.inspector_tab, Tab::Clip);
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn existing_clip_single_click_stays_collapsed_and_double_click_opens() {
    let clock = crate::gesture_clock::test::Clock::new();
    let (mut app, mut tree) = app_and_tree();
    let track = app.project().tracks[0].id;
    crate::edit_actions::create_clip(&mut app, track, 0);
    let revision = app.project_revision();
    click(&mut app, &mut tree, &clock, Target::Clip);
    assert!(!app.chrome.detail_open);
    assert_eq!(app.selection.clip, Some(Selection { track, slot: 0 }));
    click(&mut app, &mut tree, &clock, Target::Clip);
    assert!(app.chrome.detail_open);
    assert_eq!(app.selection.inspector_tab, Tab::Clip);
    assert_eq!(app.project_revision(), revision);
    assert!(!app.engine.snapshot().playing);
}

#[test]
fn scene_and_main_single_click_selects_and_double_click_opens_without_editing() {
    for (target, tab) in [(Target::Scene, Tab::Scene), (Target::Main, Tab::Main)] {
        let clock = crate::gesture_clock::test::Clock::new();
        let (mut app, mut tree) = app_and_tree();
        let revision = app.project_revision();
        click(&mut app, &mut tree, &clock, target);
        assert!(!app.chrome.detail_open);
        assert_eq!(app.selection.inspector_tab, tab);
        clock.advance(Duration::from_millis(400));
        click(&mut app, &mut tree, &clock, target);
        assert!(!app.chrome.detail_open, "slow second click only selects");
        click(&mut app, &mut tree, &clock, target);
        assert!(app.chrome.detail_open);
        assert_eq!(app.selection.inspector_tab, tab);
        assert_eq!(app.project_revision(), revision);
        assert!(!app.engine.snapshot().playing);
    }
}

#[test]
fn output_title_double_click_opens_its_existing_output_tab() {
    let clock = crate::gesture_clock::test::Clock::new();
    let (mut app, mut tree) = app_and_tree();
    let _ = crate::update::update(
        &mut app,
        Message::Outputs(crate::messages::OutputMessage::AddOutput),
    );
    click(&mut app, &mut tree, &clock, Target::Output);
    assert!(!app.chrome.detail_open);
    click(&mut app, &mut tree, &clock, Target::Output);
    assert!(app.chrome.detail_open);
    assert_eq!(app.selection.inspector_tab, Tab::Output);
}
