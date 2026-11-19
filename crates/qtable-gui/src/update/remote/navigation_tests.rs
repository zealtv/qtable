//! Navigation through learned MIDI/key bindings and the real owner update path.
use super::*;
use crate::{Message, grid, map_mode, selection::ColumnOwner};
use qtable_engine::{engine::Engine, model::*};
use qtable_remote::{Endpoint, Input};
use std::time::{Duration, Instant};

fn project() -> Project {
    let mut project = Project {
        tracks: vec![
            Track::new(TrackId(1), "One", 12),
            Track::new(TrackId(2), "Two", 12),
        ],
        scenes: (1..=12)
            .map(|id| Scene {
                id: SceneId(id),
                name: String::new(),
                follow_actions: Default::default(),
            })
            .collect(),
        ..Project::default()
    };
    project.insert_group(Track::new_group(TrackId(3), "Group", 12), &[TrackId(1)]);
    project
}

fn engine_with_udp_marker(mut project: Project, scene: SceneId, port: u16) -> Engine {
    let row = project.scenes.iter().position(|s| s.id == scene).unwrap();
    let track = project
        .tracks
        .iter_mut()
        .find(|t| t.id == TrackId(1))
        .unwrap();
    track.route = Route::single(TrackId(1001));
    let mut clip = Clip::new(ClipId(1), "Marker", BeatTime(960), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.looping = false;
    clip.events.push(track.hit(
        BeatTime::ZERO,
        OscAddress::Absolute("/receipt/owner".into()),
        vec![],
        || DeviceId(1),
    ));
    track.slots[row].clip = Some(clip);
    project.outputs.push(Track::new_output(
        TrackId(1001),
        "Receiver",
        "127.0.0.1",
        port,
    ));
    let engine = Engine::with_grid_lead(project, Duration::ZERO).unwrap();
    // IP literals resolve synchronously when the worker is created.
    assert!(engine.snapshot().unresolved_outputs.is_empty());
    engine
}

fn send(app: &mut App, inputs: impl IntoIterator<Item = Input>) {
    for input in inputs {
        app.remote.ingress.push(Endpoint::Keyboard, input);
    }
    let _ = crate::update::update(app, Message::Remote(Action::Wake));
}

fn press(index: usize, mode: map_mode::Mode) -> [Input; 2] {
    match mode {
        map_mode::Mode::Midi => [
            Input::Midi(vec![0x90, 60 + index as u8, 100].into()),
            Input::Midi(vec![0x80, 60 + index as u8, 0].into()),
        ],
        map_mode::Mode::Key => [
            Input::Key {
                key: ((b'a' + index as u8) as char).to_string(),
                pressed: true,
                repeat: false,
            },
            Input::Key {
                key: ((b'a' + index as u8) as char).to_string(),
                pressed: false,
                repeat: false,
            },
        ],
    }
}

fn learned(mode: map_mode::Mode) -> App {
    let mut app = App::new(Engine::new(project()).unwrap(), "120".into(), None);
    map_mode::update(
        &mut app,
        if mode == map_mode::Mode::Midi {
            map_mode::Action::Toggle
        } else {
            map_mode::Action::ToggleKey
        },
    );
    let controls = [
        ControlRef::TrackSelect(TrackId(2)),
        ControlRef::TrackSelect(TrackId(3)),
    ]
    .into_iter()
    .chain(SceneNavigation::ALL.map(ControlRef::SceneNavigation));
    for (index, control) in controls.enumerate() {
        map_mode::update(&mut app, map_mode::Action::Pick(control.clone()));
        let before = app.selection.owner;
        send(&mut app, press(index, mode));
        assert_eq!(app.selection.owner, before, "learning operates nothing");
        assert!(
            app.project()
                .mappings
                .iter()
                .any(|row| row.control == control)
        );
    }
    assert_eq!(app.project().mappings.len(), 7);
    map_mode::update(
        &mut app,
        if mode == map_mode::Mode::Midi {
            map_mode::Action::Toggle
        } else {
            map_mode::Action::ToggleKey
        },
    );
    app
}

fn wait_scene(app: &mut App, expected: Option<SceneId>) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while app.engine.snapshot().last_triggered_scene != expected {
        assert!(
            Instant::now() < deadline,
            "scene command not published: {expected:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn navigation_learns_and_operates_from_both_midi_and_keys_without_editing_the_set() {
    for mode in [map_mode::Mode::Midi, map_mode::Mode::Key] {
        let mut app = learned(mode);
        let revision = app.project_revision();
        send(&mut app, press(0, mode));
        assert_eq!(app.selection.owner, Some(ColumnOwner::Track(TrackId(2))));
        assert_eq!(app.selection.tracks, [TrackId(2)]);
        send(&mut app, press(1, mode));
        assert_eq!(app.selection.owner, Some(ColumnOwner::Group(TrackId(3))));
        // TrackSelect(3) selects the Group header itself. Its member (1) is an
        // affected descendant, not an explicit header selection.
        assert_eq!(app.selection.tracks, [TrackId(3)]);
        super::super::grid_actions::update(&mut app, grid::Action::Select(TrackId(1), 3));
        // Down then launch in one drain must launch scene 5, not the previous scene 4.
        send(&mut app, press(5, mode).into_iter().chain(press(2, mode)));
        // Live's Select Next Scene on Launch: the highlight moves down at once.
        assert_eq!(app.selection.scene, Some(SceneId(6)));
        assert_eq!(app.selection.clip, None);
        wait_scene(&mut app, Some(SceneId(5)));
        send(&mut app, press(3, mode));
        wait_scene(&mut app, None);
        send(&mut app, press(4, mode));
        assert_eq!(app.selection.scene, Some(SceneId(5)));
        send(&mut app, press(6, mode));
        assert_eq!(
            app.selection.scene,
            Some(SceneId(6)),
            "selector notes/keys advance a scene"
        );
        for _ in 0..20 {
            send(&mut app, press(4, mode));
        }
        assert_eq!(
            app.selection.scene,
            Some(SceneId(1)),
            "Up clamps at the start"
        );
        for _ in 0..20 {
            send(&mut app, press(5, mode));
        }
        assert_eq!(
            app.selection.scene,
            Some(SceneId(12)),
            "Down clamps at the end"
        );
        send(&mut app, press(6, mode));
        assert_eq!(app.selection.scene, Some(SceneId(1)), "selector keys wrap");
        assert_eq!(
            app.project_revision(),
            revision,
            "navigation is session state"
        );
    }
}

#[test]
fn absolute_and_endless_scene_selectors_reach_bounds_and_follow_the_highlight() {
    let mut app = App::new(Engine::new(project()).unwrap(), "120".into(), None);
    let control = ControlRef::SceneNavigation(SceneNavigation::Select);
    let mapping = RemoteMapping::new(
        MappingId(1),
        Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Cc(1),
        }),
        control,
        1.0,
        12.0,
    );
    crate::edit_actions::execute_edit(
        &mut app,
        Command::Edit(qtable_engine::edit::ProjectEdit::EditMapping(
            qtable_engine::edit::MappingEdit::Add {
                source: mapping.source,
                control: mapping.control,
                min: mapping.min,
                max: mapping.max,
            },
        )),
    );
    let midi = |value| Input::Midi(vec![0xb0, 1, value].into());
    app.scene_follow
        .viewport()
        .set((100.0, crate::grid::layout::header_height(&[])));
    send(&mut app, [midi(127)]);
    assert_eq!(app.selection.scene, Some(SceneId(12)));
    let _ = crate::update::update(&mut app, Message::Tick);
    assert!(
        crate::grid::scene_follow::follow(
            &app.scene_follow,
            None,
            app.selection.scene,
            app.selection.cell,
            &app.project().scenes
        )
        .units()
            == 0,
        "the host tick already followed this selection"
    );
    send(&mut app, [midi(0)]);
    assert_eq!(app.selection.scene, Some(SceneId(1)));
    map_mode::update(
        &mut app,
        map_mode::Action::SetMode(
            MappingId(1),
            SourceMode::Relative(RelativeEncoding::TwosComplementLinear),
        ),
    );
    send(&mut app, [midi(3), midi(127)]);
    assert_eq!(
        app.selection.scene,
        Some(SceneId(3)),
        "all relative samples accumulate in order"
    );
    send(&mut app, [midi(126)]);
    assert_eq!(app.selection.scene, Some(SceneId(1)));
}

#[test]
fn an_encoder_spun_past_either_end_owes_nothing_in_the_real_host() {
    use qtable_engine::edit::{MappingEdit, ProjectEdit};
    let mut app = App::new(Engine::new(project()).unwrap(), "120".into(), None);
    let source = Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::Cc(1),
    });
    let edit = |app: &mut App, edit| {
        crate::edit_actions::execute_edit(app, Command::Edit(ProjectEdit::EditMapping(edit)));
    };
    edit(
        &mut app,
        MappingEdit::Add {
            source,
            control: ControlRef::SceneNavigation(SceneNavigation::Select),
            min: 1.0,
            max: 12.0,
        },
    );
    map_mode::update(
        &mut app,
        map_mode::Action::SetMode(
            MappingId(1),
            SourceMode::Relative(RelativeEncoding::TwosComplement),
        ),
    );
    edit(
        &mut app,
        MappingEdit::SetSensitivity {
            mapping: MappingId(1),
            sensitivity: 0.25,
        },
    );
    let click = |app: &mut App, wire: u8| send(app, [Input::Midi(vec![0xb0, 1, wire].into())]);
    let (first, last) = (SceneId(1), SceneId(12));
    // At the last scene: three clicks on owe nothing, so the fourth click back moves.
    crate::update::grid_actions::update(&mut app, grid::Action::SelectScene(last));
    for wire in [1, 1, 1, 127, 127, 127] {
        click(&mut app, wire);
        assert_eq!(app.selection.scene, Some(last));
    }
    click(&mut app, 127);
    assert_eq!(app.selection.scene, Some(SceneId(11)));
    // The mirror, at the first scene.
    crate::update::grid_actions::update(&mut app, grid::Action::SelectScene(first));
    for wire in [127, 127, 127, 1, 1, 1] {
        click(&mut app, wire);
        assert_eq!(app.selection.scene, Some(first));
    }
    click(&mut app, 1);
    assert_eq!(app.selection.scene, Some(SceneId(2)));
}

#[test]
fn scene_launch_stops_at_the_last_scene_and_keys_wrap_over_every_scene() {
    let mut app = learned(map_mode::Mode::Key);
    // The selector is mapping 7; its stored Min and Max play no part.
    let count = app.project().scenes.len();
    let first = {
        send(&mut app, press(6, map_mode::Mode::Key));
        app.selection.scene
    };
    let mut seen = vec![first];
    for _ in 1..count {
        send(&mut app, press(6, map_mode::Mode::Key));
        seen.push(app.selection.scene);
    }
    let mut all = seen.clone();
    all.sort_by_key(|scene| scene.map(|id| id.0));
    all.dedup();
    assert_eq!(all.len(), count, "one pass visits every scene");
    send(&mut app, press(6, map_mode::Mode::Key));
    assert_eq!(app.selection.scene, first, "the cycle wraps");
    // Down to the end, then Scene Launch launches the last scene and stays on it.
    for _ in 0..count {
        send(&mut app, press(5, map_mode::Mode::Key));
    }
    let last = app.project().scenes.last().unwrap().id;
    assert_eq!(app.selection.scene, Some(last));
    send(&mut app, press(2, map_mode::Mode::Key));
    assert_eq!(app.selection.scene, Some(last));
    wait_scene(&mut app, Some(last));
}

#[test]
fn selected_scene_translation_keeps_midi_and_key_receipt_before_owner_work() {
    use qtable_engine::scheduler::SchedulerEvent;
    for mode in [map_mode::Mode::Midi, map_mode::Mode::Key] {
        let receiver = qtable_test_support::TestReceiver::bind().unwrap();
        let mut app = learned(mode);
        // Keep the learned bindings, but observe the command diagnostics directly.
        app.engine =
            engine_with_udp_marker(app.project().clone(), SceneId(5), receiver.addr().port());
        super::super::grid_actions::update(&mut app, grid::Action::SelectScene(SceneId(4)));
        let before = Instant::now();
        for input in press(5, mode).into_iter().chain(press(2, mode)) {
            app.remote.ingress.push(Endpoint::Keyboard, input);
        }
        let after_receipt = Instant::now();
        super::update(&mut app, Action::Wake);
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let event = app
                .engine
                .scheduler_events()
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
            if let SchedulerEvent::LaunchAdmission {
                received, sequence, ..
            } = event
            {
                assert!((before..=after_receipt).contains(&received));
                assert!(sequence.is_some());
                break;
            }
        }
        wait_scene(&mut app, Some(SceneId(5)));
        assert_eq!(app.selection.scene, Some(SceneId(6)));
        assert!(app.command_context.is_none());
        let messages = receiver
            .wait_for_messages(1, Duration::from_secs(2))
            .unwrap();
        assert_eq!(messages[0].1.addr, "/receipt/owner");
    }
}

#[test]
fn semantic_gui_action_stamps_a_fresh_receipt_and_restores_context() {
    use qtable_engine::{engine::Receipt, scheduler::SchedulerEvent};
    let receiver = qtable_test_support::TestReceiver::bind().unwrap();
    let mut app = App::new(
        engine_with_udp_marker(project(), SceneId(4), receiver.addr().port()),
        "120".into(),
        None,
    );
    let previous = app.engine.command_context(Receipt {
        received_at: Instant::now() - Duration::from_secs(1),
        sequence: Some(99),
    });
    app.command_context = Some(previous);
    let before = Instant::now();
    let _ = crate::update::update(
        &mut app,
        Message::Grid(grid::Action::LaunchScene(SceneId(4))),
    );
    let after = Instant::now();
    assert_eq!(app.command_context.unwrap().receipt, previous.receipt);
    loop {
        let event = app
            .engine
            .scheduler_events()
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        assert!(!matches!(event, SchedulerEvent::Error(_)), "{event:?}");
        if let SchedulerEvent::LaunchAdmission {
            received, sequence, ..
        } = event
        {
            assert!((before..=after).contains(&received));
            assert_eq!(sequence, None);
            break;
        }
    }
    wait_scene(&mut app, Some(SceneId(4)));
    let messages = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(messages[0].1.addr, "/receipt/owner");
}
