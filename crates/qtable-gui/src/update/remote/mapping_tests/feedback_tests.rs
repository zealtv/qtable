//! Slot selection and bang feedback through the ordinary mapping host.
use super::*;

fn key_input(app: &mut App, key: &str, pressed: bool) {
    app.remote.ingress.push(
        Endpoint::Keyboard,
        qtable_remote::Input::Key {
            key: key.into(),
            pressed,
            repeat: false,
        },
    );
    update(app, Action::Wake);
}

#[test]
fn mapped_launches_and_empty_stops_select_the_slot_and_scene_for_midi_and_keys() {
    let mut project = saved_project(9000);
    project.outputs.clear();
    project.tracks[0].route = Route::empty();
    project.scenes.push(Scene {
        id: SceneId(20),
        name: "Stop".into(),
        follow_actions: Default::default(),
    });
    project.tracks[0].slots.push(Default::default());
    let slot_ref = |scene| ControlRef::ClipSlot {
        track: TrackId(1),
        scene: SceneId(scene),
    };
    project.mappings.push(RemoteMapping::new(
        MappingId(100),
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Note(66),
        }),
        slot_ref(20),
        0.0,
        1.0,
    ));
    for (id, key, scene) in [(101, "a", 10), (102, "s", 20)] {
        project.mappings.push(RemoteMapping::new(
            MappingId(id),
            Source::Key(KeySource {
                key: KeyName(key.into()),
            }),
            slot_ref(scene),
            0.0,
            1.0,
        ));
    }
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    let before = app.project().clone();
    for key_mode in [false, true] {
        // Mapping must not accidentally finish the GUI's armed Move Clip operation.
        app.selection.move_clip = true;
        if key_mode {
            key_input(&mut app, "a", true);
        } else {
            input(&mut app, &[[0x90, 60, 100]]);
        }
        assert_slot_selection(&app, 0);
        wait(&mut app, |snapshot| {
            snapshot.tracks[0].playing_slot == Some(0)
        });
        capture(
            &app,
            if key_mode {
                "key-launch"
            } else {
                "midi-launch"
            },
        );
        if key_mode {
            key_input(&mut app, "s", true);
        } else {
            input(&mut app, &[[0x90, 66, 100]]);
        }
        assert_slot_selection(&app, 1);
        wait(&mut app, |snapshot| {
            snapshot.tracks[0].playing_slot.is_none()
        });
        capture(&app, if key_mode { "key-stop" } else { "midi-stop" });
    }
    assert_eq!(
        app.project(),
        &before,
        "selection and launches never edit the Set"
    );
    assert!(!app.is_dirty());
}

fn assert_slot_selection(app: &App, row: usize) {
    use crate::selection::{ColumnCell, ColumnOwner};
    assert_eq!(
        app.selection.clip,
        Some(crate::grid::Selection {
            track: TrackId(1),
            slot: row
        })
    );
    assert_eq!(
        app.selection.cell,
        Some(ColumnCell {
            owner: ColumnOwner::Track(TrackId(1)),
            row,
            active: true
        })
    );
    assert!(!app.selection.move_clip);
}

#[test]
fn mapped_group_slots_select_the_group_cell_and_scene() {
    let mut project = saved_project(9000);
    project.outputs.clear();
    project.tracks[0].route = Route::empty();
    project.scenes.push(Scene {
        id: SceneId(20),
        name: "Stop".into(),
        follow_actions: Default::default(),
    });
    project.tracks[0].slots.push(Default::default());
    project.insert_group(Track::new_group(TrackId(3), "Group", 2), &[TrackId(1)]);
    project.mappings.push(RemoteMapping::new(
        MappingId(100),
        Source::Key(KeySource {
            key: KeyName("g".into()),
        }),
        ControlRef::ClipSlot {
            track: TrackId(3),
            scene: SceneId(10),
        },
        0.0,
        1.0,
    ));
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    key_input(&mut app, "g", true);
    assert_eq!(
        app.selection.cell,
        Some(crate::selection::ColumnCell {
            owner: crate::selection::ColumnOwner::Group(TrackId(3)),
            row: 0,
            active: true
        })
    );
    assert_eq!(app.selection.tracks, [TrackId(1)]);
    assert!(app.selection.clip.is_none());
    wait(&mut app, |snapshot| {
        snapshot
            .tracks
            .iter()
            .any(|track| track.id == TrackId(1) && track.playing_slot.is_some())
    });
    capture(&app, "group-launch");
    crate::map_mode::update(&mut app, crate::map_mode::Action::Toggle);
    capture(&app, "whole-cell-midi-map");
    crate::map_mode::update(&mut app, crate::map_mode::Action::ToggleKey);
    capture(&app, "whole-cell-key-map");
}

#[test]
fn mapped_bangs_flash_for_midi_and_keys_without_selecting_the_device() {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = saved_project(receiver.addr().port());
    let control = ControlRef::Event {
        track: TrackId(1),
        event: EventRef {
            device: DeviceId(5),
            key: EntryKey("ping".into()),
        },
    };
    project.mappings.push(RemoteMapping::new(
        MappingId(100),
        Source::Key(KeySource {
            key: KeyName("b".into()),
        }),
        control.clone(),
        0.0,
        1.0,
    ));
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    let _ = crate::update::update(
        &mut app,
        Message::Grid(crate::grid::Action::Select(TrackId(1), 0)),
    );
    app.chrome.detail_open = true;
    app.selection.inspector_tab = crate::inspector_panel::Tab::Track;
    let ControlRef::Event { track, event } = control else {
        unreachable!()
    };
    capture(&app, "bang-idle");
    assert!(!app.event_feedback.lit(track, &event, Instant::now()));
    input(&mut app, &[[0xb0, 5, 64]]);
    assert!(app.event_feedback.lit(track, &event, Instant::now()));
    assert!(
        !app.event_feedback
            .lit(track, &event, Instant::now() + Duration::from_secs(1))
    );
    receiver
        .wait_for_messages(1, Duration::from_secs(1))
        .unwrap();
    key_input(&mut app, "b", true);
    assert!(app.event_feedback.lit(track, &event, Instant::now()));
    receiver
        .wait_for_messages(1, Duration::from_secs(1))
        .unwrap();
    assert!(app.selection.devices.is_empty());
    assert!(!app.is_dirty());
    // The capture redraw happens on its own thread budget; hold the same flash state
    // into the future so this optional screenshot is independent of render duration.
    if std::env::var_os("QTABLE_TEST_SLOT_CAPTURE_DIR").is_some() {
        app.event_feedback.fired(
            track,
            event.clone(),
            Instant::now() + Duration::from_secs(60),
        );
        capture(&app, "bang-fired");
    }
    app.reset_for_project();
    assert!(!app.event_feedback.lit(track, &event, Instant::now()));
}

/// Optional evidence from the real window composition, rendered without desktop input.
fn capture(app: &App, name: &str) {
    use iced::advanced::{Layout, Shell, layout, renderer::Headless, widget::Tree};
    use iced::{Event, Point, Rectangle, Size, mouse};
    let Some(directory) = std::env::var_os("QTABLE_TEST_SLOT_CAPTURE_DIR") else {
        return;
    };
    let size = Size::new(1280.0, 800.0);
    let mut element = crate::view::view(app);
    let mut tree = Tree::new(&element);
    let mut renderer = crate::grid::test_renderer();
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, size),
    );
    element.as_widget_mut().update(
        &mut tree,
        &Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
        Layout::new(&node),
        mouse::Cursor::Available(Point::new(1279.0, 799.0)),
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut Shell::new(&mut Vec::new()),
        &Rectangle::with_size(size),
    );
    element.as_widget().draw(
        &tree,
        &mut renderer,
        &iced::Theme::Dark,
        &iced::advanced::renderer::Style::default(),
        Layout::new(&node),
        mouse::Cursor::Unavailable,
        &Rectangle::with_size(size),
    );
    let pixels = renderer.screenshot(Size::new(1280, 800), 1.0, app.theme.surface_background);
    std::fs::write(
        std::path::Path::new(&directory).join(format!("{name}.rgba")),
        pixels,
    )
    .unwrap();
}
