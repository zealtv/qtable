use super::*;
use crate::selection::ColumnOwner;
use iced::advanced::{Layout, layout, renderer, widget::Tree};
use iced::keyboard::{Key, Modifiers, key::Named};
use qtable_engine::engine::Engine;

fn app() -> App {
    App::new(
        Engine::new(crate::files::new_project()).unwrap(),
        "120".into(),
        None,
    )
}
fn send(app: &mut App, message: Message) {
    let _ = crate::update::update(app, message);
}
fn press(app: &mut App) {
    send(app, Message::Browser(Action::Press(NewDevice::Target)));
}
fn draw<M>(
    mut element: Element<'_, M>,
    geometry: &Geometry,
    size: Size,
    at: Point,
    viewport: Rectangle,
) {
    geometry.borrow_mut().clear();
    let mut renderer = crate::grid::test_renderer();
    let mut tree = Tree::new(&element);
    let node = element
        .as_widget_mut()
        .layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size))
        .move_to(at);
    element.as_widget().draw(
        &tree,
        &mut renderer,
        &iced::Theme::Dark,
        &renderer::Style::default(),
        Layout::new(&node),
        iced::mouse::Cursor::Unavailable,
        &viewport,
    );
}
fn zone(app: &App, target: DropTarget) {
    app.browser.geometry.borrow_mut().insert(
        "test",
        vec![(
            Rectangle::new(Point::new(100.0, 100.0), Size::new(100.0, 100.0)),
            target,
        )],
    );
}
fn drag(app: &mut App, target: DropTarget) {
    app.cursor = Point::ORIGIN;
    press(app);
    zone(app, target);
    send(app, Message::CursorMoved(Point::new(120.0, 120.0)));
    assert!(app.device_drag.as_ref().unwrap().moved);
    assert_eq!(app.device_drag.as_ref().unwrap().over, Some(target));
    send(app, Message::Browser(Action::Release));
}

#[test]
fn double_click_loads_one_empty_target_and_reveals_it() {
    let clock = crate::gesture_clock::test::Clock::new();
    let mut app = app();
    let track = app.project().tracks[0].id;
    send(
        &mut app,
        Message::Grid(crate::grid::Action::Select(track, 0)),
    );
    app.chrome.detail_open = false;
    let revision = app.project_revision();
    clock.advance(std::time::Duration::from_millis(100));
    press(&mut app);
    send(&mut app, Message::Browser(Action::Release));
    assert_eq!(app.project_revision(), revision);
    clock.advance(std::time::Duration::from_millis(100));
    press(&mut app);
    send(&mut app, Message::Browser(Action::Release));
    assert_eq!(app.project_revision(), revision + 1);
    let device = &app.project().track(track).unwrap().devices[0];
    assert_eq!(device.name, "Target");
    assert_eq!(device.kind, NewDevice::Target.build(device.id).kind);
    assert_eq!(app.selection.devices, [device.id]);
    assert_eq!(app.selection.anchor, Some(track));
    assert_eq!(app.selection.inspector_tab, Tab::Track);
    assert!(app.chrome.detail_open);
    assert_eq!(app.focus, Focus::Detail);
}

#[test]
fn no_track_reports_notice_and_browser_keys_respect_text_focus() {
    let mut app = app();
    let revision = app.project_revision();
    send(&mut app, Message::SetFocus(Focus::Browser));
    assert!(app.chrome.browser_open);
    let key = |key, ignored| Message::KeyPressed {
        key: Key::Named(key),
        modifiers: Modifiers::NONE,
        repeat: false,
        ignored,
    };
    send(&mut app, key(Named::Enter, true));
    assert_eq!(app.project_revision(), revision);
    assert_eq!(
        app.status.message(),
        Some("Select a track to load a device")
    );
    let track = app.project().tracks[0].id;
    send(
        &mut app,
        Message::Grid(crate::grid::Action::SelectTrack(track, false)),
    );
    send(&mut app, Message::SetFocus(Focus::Browser));
    app.browser.selected = None;
    send(&mut app, key(Named::ArrowDown, true));
    assert_eq!(app.browser.selected, Some(NewDevice::Target));
    send(&mut app, key(Named::Enter, false));
    assert_eq!(app.project_revision(), revision);
    send(&mut app, key(Named::Enter, true));
    assert_eq!(app.project_revision(), revision + 1);
}

#[test]
fn drops_load_only_on_tracks_with_clips() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::CreateGroup {
            tracks: vec![track],
        }),
    );
    let group = app.project().group_tracks().next().unwrap().id;
    send(
        &mut app,
        Message::Outputs(crate::messages::OutputMessage::AddOutput),
    );
    let output = app.project().outputs[0].id;
    let main = app.project().main.id;
    drag(&mut app, DropTarget::Column(track));
    assert_eq!(app.selection.inspector_tab, Tab::Track);
    assert_eq!(app.selection.anchor, Some(track));
    for id in [group, output, main] {
        for target in [
            DropTarget::Column(id),
            DropTarget::Chain {
                track: id,
                before: None,
            },
        ] {
            let revision = app.project_revision();
            app.cursor = Point::ORIGIN;
            press(&mut app);
            zone(&app, target);
            send(&mut app, Message::CursorMoved(Point::new(120.0, 120.0)));
            assert!(app.device_drag.as_ref().unwrap().over.is_none());
            assert!(app.device_drag.as_ref().unwrap().refused);
            assert!(!accepts(&app, NewDevice::Target, target));
            send(&mut app, Message::Browser(Action::Release));
            assert_eq!(app.project_revision(), revision);
            assert!(app.project().track(id).unwrap().devices.is_empty());
        }
        send(
            &mut app,
            Message::Grid(crate::grid::Action::SelectColumn(
                if id == group {
                    ColumnOwner::Group(id)
                } else {
                    ColumnOwner::Track(id)
                },
                None,
            )),
        );
        update(&mut app, Action::Load);
        assert!(
            app.status
                .message()
                .unwrap()
                .contains("Targets require a Track")
        );
    }
}

#[test]
fn chain_drop_inserts_at_marker_and_cancelled_or_short_drags_do_nothing() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    drag(&mut app, DropTarget::Column(track));
    let first = app.selection.devices[0];
    drag(
        &mut app,
        DropTarget::Chain {
            track,
            before: Some(first),
        },
    );
    let second = app.selection.devices[0];
    assert_eq!(
        app.project()
            .track(track)
            .unwrap()
            .devices
            .iter()
            .map(|d| d.id)
            .collect::<Vec<_>>(),
        [second, first]
    );
    let revision = app.project_revision();
    for cancel in [
        Message::CancelField,
        Message::CommitField,
        Message::ToggleRegion(crate::chrome::Region::Browser),
    ] {
        app.cursor = Point::ORIGIN;
        press(&mut app);
        send(&mut app, Message::CursorMoved(Point::new(120.0, 120.0)));
        send(&mut app, cancel);
        send(&mut app, Message::Browser(Action::Release));
        assert_eq!(app.project_revision(), revision);
        assert!(app.device_drag.is_none());
    }
    app.cursor = Point::ORIGIN;
    press(&mut app);
    zone(&app, DropTarget::Column(track));
    send(&mut app, Message::CursorMoved(Point::new(2.0, 0.0)));
    send(&mut app, Message::Browser(Action::Release));
    assert_eq!(app.project_revision(), revision);
    app.browser.last_press = None;
    press(&mut app);
    send(&mut app, Message::CursorMoved(Point::new(300.0, 300.0)));
    send(&mut app, Message::Browser(Action::Release));
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn rendered_columns_clip_hit_testing_and_follow_scrolled_coordinates() {
    let app = app();
    let cols = crate::grid::layout::columns(app.project(), qtable_engine::model::Region::Tracks);
    let track = app.project().tracks[0].id;
    let content: Element<'_, Message> = Space::new().width(400).height(300).into();
    let geometry = app.browser.geometry.clone();
    let element = columns(
        content,
        "tracks",
        Some(geometry.clone()),
        &cols,
        &[20.0],
        None,
    );
    let viewport = Rectangle::new(Point::new(50.0, 40.0), Size::new(70.0, 100.0));
    draw(
        element,
        &geometry,
        Size::new(400.0, 300.0),
        Point::new(-10.0, 20.0),
        viewport,
    );
    assert_eq!(
        hit(&geometry, Point::new(60.0, 60.0)),
        Some(DropTarget::Column(track))
    );
    assert_eq!(hit(&geometry, Point::new(30.0, 60.0)), None);
    assert_eq!(hit(&geometry, Point::new(60.0, 160.0)), None);
}

#[test]
fn rendered_empty_and_populated_chains_exclude_the_mixer() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let geometry = app.browser.geometry.clone();
    for count in [0, 1] {
        if count == 1 {
            crate::edit_actions::run_edit(
                &mut app,
                Command::Edit(ProjectEdit::AddDevice {
                    track,
                    kind: NewDevice::Target,
                    before: None,
                }),
            );
        }
        let element = crate::device_chain::view(
            &app,
            app.project().track(track).unwrap(),
            crate::control_focus::View::Track,
        );
        draw(
            element,
            &geometry,
            Size::new(900.0, 300.0),
            Point::ORIGIN,
            Rectangle::new(Point::ORIGIN, Size::new(900.0, 300.0)),
        );
        let before = app
            .project()
            .track(track)
            .unwrap()
            .devices
            .first()
            .map(|d| d.id);
        assert_eq!(
            hit(&geometry, Point::new(20.0, 50.0)),
            Some(DropTarget::Chain { track, before })
        );
        // The Mixer, last in the chain, is no drop target.
        let target = app.project().track(track).unwrap().devices.first();
        let target = target.map_or(0.0, |d| crate::device_chain::width(d) + 8.0);
        let mixer_x = 4.0 + target + 20.0;
        let mixer_x = if count == 0 {
            900.0 - 4.0 - crate::mixer_device::width(app.project().track(track).unwrap()) + 20.0
        } else {
            mixer_x
        };
        assert_eq!(hit(&geometry, Point::new(mixer_x, 50.0)), None);
        if count == 1 {
            assert_eq!(
                hit(&geometry, Point::new(4.0 + 400.0, 50.0)),
                Some(DropTarget::Chain {
                    track,
                    before: None
                })
            );
        }
    }
}

#[test]
fn window_geometry_registers_all_regions_and_drops_hidden_columns() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    send(
        &mut app,
        Message::Outputs(crate::messages::OutputMessage::AddOutput),
    );
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::CreateGroup {
            tracks: vec![track],
        }),
    );
    let group = app.project().group_tracks().next().unwrap().id;
    let output = app.project().outputs[0].id;
    let main = app.project().main.id;
    let geometry = app.browser.geometry.clone();
    let size = Size::new(1100.0, 900.0);
    let viewport = Rectangle::new(Point::ORIGIN, size);
    draw(
        crate::view::view(&app),
        &geometry,
        size,
        Point::ORIGIN,
        viewport,
    );
    let targets = || {
        geometry
            .borrow()
            .values()
            .flatten()
            .map(|(_, t)| *t)
            .collect::<Vec<_>>()
    };
    for id in [track, group, output, main] {
        assert!(targets().contains(&DropTarget::Column(id)));
    }
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::SetGroupFolded {
            group,
            folded: true,
        }),
    );
    app.chrome.outputs_open = false;
    draw(
        crate::view::view(&app),
        &geometry,
        size,
        Point::ORIGIN,
        viewport,
    );
    assert!(!targets().contains(&DropTarget::Column(track)));
    assert!(!targets().contains(&DropTarget::Column(output)));
    assert!(targets().contains(&DropTarget::Column(group)));
    assert!(targets().contains(&DropTarget::Column(main)));
}

#[test]
fn no_track_double_click_and_project_replacement_cannot_load_a_stale_drag() {
    let clock = crate::gesture_clock::test::Clock::new();
    let mut app = app();
    let revision = app.project_revision();
    clock.advance(std::time::Duration::from_millis(100));
    press(&mut app);
    send(&mut app, Message::Browser(Action::Release));
    clock.advance(std::time::Duration::from_millis(100));
    press(&mut app);
    assert_eq!(
        app.status.message(),
        Some("Select a track to load a device")
    );
    assert_eq!(app.project_revision(), revision);
    let track = app.project().tracks[0].id;
    zone(&app, DropTarget::Column(track));
    clock.advance(std::time::Duration::from_millis(100));
    press(&mut app);
    send(&mut app, Message::CursorMoved(Point::new(120.0, 120.0)));
    app.reset_for_project();
    send(&mut app, Message::Browser(Action::Release));
    assert_eq!(app.project_revision(), revision);
    assert!(app.device_drag.is_none());
    assert!(app.browser.geometry.borrow().is_empty());
}
