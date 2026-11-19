use super::*;
use crate::control_focus::test_registry::TestHelp;
use crate::control_focus::test_registry::TestKey;
use crate::control_focus::{ControlOwner, View};
use iced::{Point, Size, event};
use qtable_engine::model::TrackId;

/// The test message type is the value; a release reports itself as `released(value)`.
fn released(value: f64) -> f64 {
    -(value + 1.)
}
fn host() -> Hosted<impl Fn(f64) -> f64, impl Fn(f64) -> f64, TestKey> {
    Hosted {
        control: Control::new(
            ControlId::action(View::Mixer, "volume", ControlOwner::Track(TrackId(1))),
            TestHelp::Volume,
            Fader {
                value: 0.5,
                level: 0.0,
            },
        ),
        value: 0.5,
        shift: false,
        focused: false,
        theme: crate::control::LIVE_DARK,
        on_change: |value| value,
        on_release: released,
    }
}
#[test]
fn pointer_stays_the_normal_arrow_on_hover_and_during_drag() {
    let mut host = host();
    let bounds = Rectangle::with_size(Size::new(27., 96.));
    for enabled in [true, false] {
        host.control.state.enabled = enabled;
        for drag in [
            None,
            Some(Drag {
                y: Some(50.),
                value: 0.5,
                published: 0.5,
            }),
        ] {
            let state = State {
                drag,
                ..State::default()
            };
            for cursor in [
                mouse::Cursor::Available(bounds.center()),
                mouse::Cursor::Available(Point::new(100., 100.)),
                mouse::Cursor::Unavailable,
            ] {
                assert_eq!(
                    host.mouse_interaction(&state, bounds, cursor),
                    mouse::Interaction::None
                );
            }
        }
    }
}
fn dispatch<F: Fn(f64) -> f64, G: Fn(f64) -> f64>(
    host: &Hosted<F, G, TestKey>,
    state: &mut State<TestKey>,
    event: canvas::Event,
    y: f32,
) -> Option<(Option<f64>, event::Status)> {
    host.update(
        state,
        &event,
        Rectangle::with_size(Size::new(27., 104.)),
        mouse::Cursor::Available(Point::new(20., y)),
    )
    .map(|action| {
        let (message, _, status) = action.into_inner();
        (message, status)
    })
}
fn press<F: Fn(f64) -> f64, G: Fn(f64) -> f64>(
    host: &Hosted<F, G, TestKey>,
    state: &mut State<TestKey>,
    y: f32,
) -> Option<(Option<f64>, event::Status)> {
    let result = dispatch(
        host,
        state,
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        y,
    );
    // The first move only fixes the pointer frame; it never edits.
    assert_eq!(motion(host, state, y), None);
    result
}
fn motion<F: Fn(f64) -> f64, G: Fn(f64) -> f64>(
    host: &Hosted<F, G, TestKey>,
    state: &mut State<TestKey>,
    y: f32,
) -> Option<f64> {
    dispatch(
        host,
        state,
        canvas::Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(20., y),
        }),
        y,
    )
    .and_then(|(message, _)| message)
}
#[test]
fn relative_math_clamps_and_shift_is_four_times_finer() {
    assert!((drag_value(0.5, -10., 100., false) - 0.6).abs() < 1e-7);
    assert!((drag_value(0.5, -10., 100., true) - 0.525).abs() < 1e-7);
    assert_eq!(drag_value(0.5, 1000., 100., false), 0.);
    assert_eq!(drag_value(0.5, -1000., 100., false), 1.);
    assert_eq!(drag_value(0.5, 10., 0., false), 0.5);
}
#[test]
fn pressing_anywhere_never_jumps_and_release_ends_drag() {
    let host = host();
    for y in [1., 100.] {
        let mut state = State::default();
        assert_eq!(
            press(&host, &mut state, y),
            Some((None, event::Status::Captured))
        );
        assert_eq!(motion(&host, &mut state, y), None);
        assert!((motion(&host, &mut state, y - 9.3).unwrap() - 0.6).abs() < 1e-3);
        let result = dispatch(
            &host,
            &mut state,
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            y,
        );
        let (message, status) = result.unwrap();
        assert!((message.unwrap() + 1.6).abs() < 1e-3);
        assert_eq!(status, event::Status::Captured);
        assert!(state.drag.is_none());
        assert_eq!(motion(&host, &mut state, y - 20.), None);
    }
}
#[test]
fn small_motion_accumulates_and_shift_can_change_without_a_jump() {
    let host = host();
    let mut state = State::default();
    press(&host, &mut state, 50.);
    assert_eq!(motion(&host, &mut state, 49.97), None);
    assert!((motion(&host, &mut state, 49.8).unwrap() - (0.5 + 0.2 / 93.)).abs() < 1e-3);
    dispatch(
        &host,
        &mut state,
        canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(
            keyboard::Modifiers::SHIFT,
        )),
        49.8,
    );
    let fine = motion(&host, &mut state, 40.5).unwrap();
    assert!((fine - (0.525 + 0.2 / 93.)).abs() < 1e-3);
}
#[test]
fn endpoint_reversal_is_immediate_and_model_echo_does_not_restart_drag() {
    let mut host = host();
    let mut state = State::default();
    press(&host, &mut state, 50.);
    assert_eq!(motion(&host, &mut state, -100.), Some(1.));
    host.value = 1.;
    assert!((motion(&host, &mut state, -90.7).unwrap() - 0.9).abs() < 1e-3);
}
#[test]
fn reusing_a_canvas_for_another_track_cancels_the_drag() {
    let mut host = host();
    let mut state = State::default();
    press(&host, &mut state, 50.);
    host.control.id = ControlId::action(View::Mixer, "volume", ControlOwner::Track(TrackId(2)));
    assert_eq!(motion(&host, &mut state, 20.), None);
    assert!(state.drag.is_none());
}

#[test]
fn losing_window_focus_cancels_drag_and_shift() {
    let host = host();
    let mut state = State::default();
    dispatch(
        &host,
        &mut state,
        canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(
            keyboard::Modifiers::SHIFT,
        )),
        50.,
    );
    press(&host, &mut state, 50.);
    dispatch(
        &host,
        &mut state,
        canvas::Event::Window(iced::window::Event::Unfocused),
        50.,
    );
    assert!(state.drag.is_none());
    assert_eq!(state.modifiers, keyboard::Modifiers::NONE);
    assert_eq!(motion(&host, &mut state, 20.), None);
}

#[test]
fn losing_focus_mid_drag_releases_the_value_so_the_receiver_can_settle() {
    let host = host();
    let mut state = State::default();
    press(&host, &mut state, 50.);
    assert!(motion(&host, &mut state, 40.7).is_some());
    let (message, _) = dispatch(
        &host,
        &mut state,
        canvas::Event::Window(iced::window::Event::Unfocused),
        40.7,
    )
    .unwrap();
    assert!((message.unwrap() + 1.6).abs() < 1e-3);
}

#[test]
fn release_reports_the_final_value_once_even_when_nothing_moved() {
    let host = host();
    let mut state = State::default();
    press(&host, &mut state, 50.);
    let (message, _) = dispatch(
        &host,
        &mut state,
        canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        50.,
    )
    .unwrap();
    assert_eq!(message, Some(released(0.5)));
}

#[test]
fn a_drag_to_the_end_publishes_the_end_even_within_a_thousandth() {
    let mut host = host();
    host.value = 0.9995;
    let mut state = State::default();
    press(&host, &mut state, 50.);
    dispatch(
        &host,
        &mut state,
        canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(
            keyboard::Modifiers::SHIFT,
        )),
        50.,
    );
    assert_eq!(motion(&host, &mut state, 40.), Some(1.));
}

#[test]
fn release_publishes_the_unpublished_remainder_rounded() {
    let host = host();
    let mut state = State::default();
    press(&host, &mut state, 50.);
    state.drag.as_mut().unwrap().value = 0.7234;
    state.drag.as_mut().unwrap().published = 0.72;
    let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    let (message, status) = dispatch(&host, &mut state, release, 50.).unwrap();
    assert_eq!(
        (message, status),
        (Some(released(0.723)), event::Status::Captured)
    );
    assert!(state.drag.is_none());
    assert_eq!(published_value(0.7234567), 0.723);
}

#[test]
fn press_and_move_share_a_frame_and_held_shift_counts_at_press() {
    let mut host = host();
    host.shift = true;
    let mut state = State::default();
    // A scrolled host would report a translated press y; only raw moves matter.
    press(&host, &mut state, 50.);
    let fine = motion(&host, &mut state, 40.5).unwrap();
    assert!((fine - (0.5 + 0.25 * 9.5 / 93.)).abs() < 1e-3);
}

#[test]
fn main_drag_publishes_through_the_mixer_edit_router() {
    use crate::{app::App, messages::Message, mixer_band::MixerMessage};
    use qtable_engine::{engine::Engine, model::Project};
    let mut app = App::new(Engine::new(Project::default()).unwrap(), "120".into(), None);
    let main = app.project().main.id;
    let mut control = host().control;
    control.id = ControlId::action(View::Mixer, "volume", ControlOwner::Track(main));
    control.kind.value = 1.0;
    let host = Hosted {
        control,
        value: 1.0,
        shift: false,
        focused: false,
        theme: crate::control::LIVE_DARK,
        on_change: |value| Message::Mixer(MixerMessage::DragVolume(main, value)),
        on_release: |value| Message::Mixer(MixerMessage::ReleaseVolume(main, value)),
    };
    let mut state = State::default();
    let bounds = Rectangle::with_size(Size::new(27., 96.));
    let cursor = mouse::Cursor::Available(Point::new(20., 50.));
    let action = host
        .update(
            &mut state,
            &canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            cursor,
        )
        .unwrap();
    assert!(action.into_inner().0.is_none(), "a press never jumps");
    for y in [50., 70.] {
        let action = host
            .update(
                &mut state,
                &canvas::Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(20., y),
                }),
                bounds,
                cursor,
            )
            .unwrap();
        if let Some(message) = action.into_inner().0 {
            let _ = crate::update::update(&mut app, message);
        }
    }
    assert!((app.project().main.mixer_device.volume.value - (1.0 - 20.0 / 85.0)).abs() < 0.001);
    assert!(app.is_dirty());
}

#[test]
fn a_double_click_publishes_the_default_as_a_release_and_starts_no_drag() {
    let clock = crate::gesture_clock::test::Clock::new();
    let mut host = host();
    host.control = host.control.default_to(1.0);
    let mut state = State::default();
    let (message, _) = press(&host, &mut state, 50.0).unwrap();
    assert_eq!(message, None);
    let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    assert_eq!(
        dispatch(&host, &mut state, release.clone(), 50.0).and_then(|(m, _)| m),
        Some(released(0.5))
    );
    clock.advance(std::time::Duration::from_millis(100));
    let (message, _) = dispatch(
        &host,
        &mut state,
        canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        50.0,
    )
    .unwrap();
    assert_eq!(message, Some(released(1.0)));
    // No drag follows: motion and release publish nothing.
    assert_eq!(motion(&host, &mut state, 20.0), None);
    assert!(dispatch(&host, &mut state, release, 20.0).is_none());
}

#[test]
fn held_fader_keys_use_the_same_engine_gesture_and_capture_grid_navigation() {
    use crate::{app::App, messages::Message, mixer_band::MixerMessage};
    use qtable_engine::{
        engine::Engine,
        model::{Project, SendMode},
    };
    for mode in [SendMode::Continuous, SendMode::OnRelease] {
        let mut project = Project::default();
        project.main.mixer_device.volume.output.enabled = true;
        project.main.mixer_device.volume.output.send_mode = mode;
        let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
        let main = app.project().main.id;
        let mut control = host().control;
        control.id = ControlId::action(View::Mixer, "volume", ControlOwner::Track(main));
        let mut host = Hosted {
            control,
            value: 1.0,
            shift: false,
            focused: false,
            theme: crate::control::LIVE_DARK,
            on_change: |v| Message::Mixer(MixerMessage::DragVolume(main, v)),
            on_release: |v| Message::Mixer(MixerMessage::ReleaseVolume(main, v)),
        };
        let mut state = State::default();
        let bounds = Rectangle::with_size(Size::new(27., 96.));
        let cursor = mouse::Cursor::Unavailable;
        let down = |repeat| {
            canvas::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::ArrowDown),
                modified_key: keyboard::Key::Named(keyboard::key::Named::ArrowDown),
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers: keyboard::Modifiers::NONE,
                text: None,
                repeat,
            })
        };
        assert!(
            host.update(&mut state, &down(false), bounds, cursor)
                .is_none()
        );
        host.focused = true;
        let revision = app.project_revision();
        for repeat in [false, true] {
            let (message, _, status) = host
                .update(&mut state, &down(repeat), bounds, cursor)
                .unwrap()
                .into_inner();
            assert_eq!(status, event::Status::Captured);
            let _ = crate::update::update(&mut app, message.unwrap());
            let _ = crate::update::update(
                &mut app,
                Message::KeyPressed {
                    key: keyboard::Key::Named(keyboard::key::Named::ArrowDown),
                    modifiers: keyboard::Modifiers::NONE,
                    repeat,
                    ignored: false,
                },
            );
            host.value = crate::mixer_device::shown_volume(&app, &app.project().main);
        }
        assert_eq!(host.value, 0.98);
        assert_eq!(
            app.project_revision() - revision,
            if mode == SendMode::OnRelease { 0 } else { 2 }
        );
        let blur = canvas::Event::Window(iced::window::Event::Unfocused);
        let message = host
            .update(&mut state, &blur, bounds, cursor)
            .unwrap()
            .into_inner()
            .0
            .unwrap();
        let _ = crate::update::update(&mut app, message);
        assert_eq!(app.project().main.mixer_device.volume.value, 0.98);
        assert_eq!(
            app.project_revision() - revision,
            if mode == SendMode::OnRelease { 1 } else { 2 }
        );
        assert!(app.parameter_gesture.is_empty());
        assert!(
            host.update(&mut state, &blur, bounds, cursor)
                .unwrap()
                .into_inner()
                .0
                .is_none()
        );
    }
}
