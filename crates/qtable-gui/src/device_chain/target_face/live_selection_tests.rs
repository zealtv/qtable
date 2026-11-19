//! Panel composition regressions: retain the real widget tree while applying every message
//! and rebuilding the view between input events, as iced does during a live interaction.
use super::*;
use crate::control_focus::View;
use crate::drafts::ControlId;
use iced::advanced::{Layout, Shell, layout, widget::Tree};
use iced::{Event, Point, Rectangle, Size, keyboard, mouse};
use qtable_engine::model::{Control, ParameterRef, Route, SendMode, TrackId};
use qtable_test_support::TestReceiver;
use std::time::Duration;

const SIZE: Size = Size::new(1000.0, 900.0);

#[path = "number_drag_tests.rs"]
mod number_drag;

struct Interaction {
    clock: crate::gesture_clock::test::Clock,
    tree: Tree,
    renderer: iced::Renderer,
    scroll_x: f32,
    captured: bool,
}

impl Interaction {
    fn new() -> Self {
        Self {
            clock: crate::gesture_clock::test::Clock::new(),
            tree: Tree::empty(),
            renderer: crate::grid::test_renderer(),
            scroll_x: 0.0,
            captured: false,
        }
    }

    fn bounds(app: &App, id: DeviceId, control: ControlId) -> Rectangle {
        let mut widget = super::super::super::frame::device(
            app,
            app.project().device(id).unwrap().1,
            View::Track,
        );
        crate::control_focus::test_registry::rectangles_in(&mut widget, SIZE)
            .into_iter()
            .find(|(id, _)| *id == control)
            .unwrap()
            .1
    }

    fn step(&mut self, app: &mut App, id: DeviceId, event: Event, point: Point) -> Vec<Message> {
        if let Event::Keyboard(keyboard::Event::KeyPressed { modifiers, .. }) = &event {
            self.step(
                app,
                id,
                Event::Keyboard(keyboard::Event::ModifiersChanged(*modifiers)),
                point,
            );
        }
        self.clock.advance(Duration::from_millis(10));
        let mut widget = super::super::super::frame::device(
            app,
            app.project().device(id).unwrap().1,
            View::Track,
        );
        // Like the chain's horizontal scroll area: cursor coordinates are translated
        // into the content, while CursorMoved's position remains in window coordinates.
        if self.scroll_x > 0.0 {
            widget = iced::widget::scrollable(iced::widget::row![
                iced::widget::Space::new().width(self.scroll_x),
                widget,
                iced::widget::Space::new().width(SIZE.width),
            ])
            .horizontal()
            .id("test-chain-scroll")
            .width(SIZE.width)
            .into();
        }
        self.tree.diff(&widget);
        let node = widget.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, SIZE),
        );
        if self.scroll_x > 0.0 {
            widget.as_widget_mut().operate(
                &mut self.tree,
                Layout::new(&node),
                &self.renderer,
                &mut iced::advanced::widget::operation::scrollable::scroll_to::<()>(
                    iced::widget::Id::new("test-chain-scroll"),
                    iced::widget::scrollable::AbsoluteOffset {
                        x: Some(self.scroll_x),
                        y: None,
                    },
                ),
            );
        }
        let mut messages = vec![];
        let mut shell = Shell::new(&mut messages);
        // Dropdown choices live in iced's overlay, whose state must also survive selection.
        let viewport = Rectangle::with_size(SIZE);
        let mut overlay = widget.as_widget_mut().overlay(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &viewport,
            iced::Vector::ZERO,
        );
        if let Some(overlay) = overlay.as_mut() {
            let node = overlay.as_overlay_mut().layout(&self.renderer, SIZE);
            overlay.as_overlay_mut().update(
                &event,
                Layout::new(&node),
                mouse::Cursor::Available(point),
                &self.renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
            );
        }
        drop(overlay);
        if !shell.is_event_captured() {
            widget.as_widget_mut().update(
                &mut self.tree,
                &event,
                Layout::new(&node),
                mouse::Cursor::Available(point),
                &self.renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &Rectangle::with_size(SIZE),
            );
        }
        self.captured = shell.is_event_captured();
        drop(widget);
        let tasks: Vec<_> = messages
            .iter()
            .cloned()
            .map(|message| crate::update::update(app, message))
            .collect();
        for task in tasks {
            self.task(app, id, task);
        }
        messages
    }

    /// Apply the focus operation issued by the exact-entry task to this retained tree.
    fn operate(
        &mut self,
        app: &App,
        id: DeviceId,
        operation: &mut dyn iced::advanced::widget::Operation,
    ) {
        let mut widget = super::super::super::frame::device(
            app,
            app.project().device(id).unwrap().1,
            View::Track,
        );
        self.tree.diff(&widget);
        let node = widget.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, SIZE),
        );
        widget.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            operation,
        );
    }

    /// Run the actual task returned by update, including chained operations and their
    /// replies. This avoids reconstructing the production focus handoff in the test.
    fn task(&mut self, app: &mut App, id: DeviceId, task: iced::Task<Message>) {
        use iced::advanced::widget::operation::Outcome;
        use iced::futures::{FutureExt, StreamExt};
        let mut pending = vec![task];
        while let Some(task) = pending.pop() {
            let Some(mut stream) = iced_runtime::task::into_stream(task) else {
                continue;
            };
            while let Some(action) = stream
                .next()
                .now_or_never()
                .expect("widget tasks must complete without external I/O")
            {
                match action {
                    iced_runtime::Action::Widget(mut operation) => loop {
                        self.operate(app, id, operation.as_mut());
                        match operation.finish() {
                            Outcome::Chain(next) => operation = next,
                            _ => break,
                        }
                    },
                    iced_runtime::Action::Output(message) => {
                        pending.push(crate::update::update(app, message));
                    }
                    other => panic!("unexpected action in a widget task: {other:?}"),
                }
            }
        }
    }

    /// Deliver the asynchronous focus query's reply after the click reached the input.
    fn check_exact(&mut self, app: &mut App, id: DeviceId) {
        use iced::advanced::widget::{Operation, operation::Focusable};
        struct Query(bool);
        impl Operation for Query {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn focusable(
                &mut self,
                id: Option<&iced::widget::Id>,
                _: Rectangle,
                state: &mut dyn Focusable,
            ) {
                if id == Some(&iced::widget::Id::new("target-face-exact")) {
                    self.0 |= state.is_focused();
                }
            }
        }
        let mut query = Query(false);
        self.operate(app, id, &mut query);
        if let Some((field, text)) = app.device_chain.face.exact.clone() {
            let generation = app.project_generation;
            act(
                app,
                Action::ExactFocusChecked(field, text, generation, query.0),
            );
        }
    }

    /// Optional tour evidence from the same retained tree used by the interaction test.
    fn capture(&mut self, app: &App, id: DeviceId, name: &str) {
        use iced::advanced::renderer::Headless;
        let Some(directory) = std::env::var_os("QTABLE_TEST_FACE_CAPTURE_DIR") else {
            return;
        };
        let mut widget = super::super::super::frame::device(
            app,
            app.project().device(id).unwrap().1,
            View::Track,
        );
        self.tree.diff(&widget);
        let node = widget.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, SIZE),
        );
        self.renderer = crate::grid::test_renderer();
        widget.as_widget().draw(
            &self.tree,
            &mut self.renderer,
            &iced::Theme::Dark,
            &iced::advanced::renderer::Style::default(),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &Rectangle::with_size(SIZE),
        );
        let pixels =
            self.renderer
                .screenshot(Size::new(1000, 220), 1.0, app.theme.detail_view_background);
        std::fs::write(
            std::path::PathBuf::from(directory).join(format!("{name}.rgba")),
            pixels,
        )
        .unwrap();
    }

    fn press(&mut self, app: &mut App, id: DeviceId, point: Point) {
        self.step(app, id, button(true), point);
    }

    fn release(&mut self, app: &mut App, id: DeviceId, point: Point) {
        self.step(app, id, button(false), point);
    }
}

fn button(press: bool) -> Event {
    Event::Mouse(if press {
        mouse::Event::ButtonPressed(mouse::Button::Left)
    } else {
        mouse::Event::ButtonReleased(mouse::Button::Left)
    })
}

fn key(key: keyboard::Key, text: Option<&str>, modifiers: keyboard::Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    })
}

fn control(id: DeviceId, parameter: bool) -> ControlId {
    ControlId::field(
        View::Track,
        DraftKey::Target(id, TargetField::Face(parameter, 0, 0)),
    )
}

/// One control in the first entry, with another entry selected (or no selection, collapsed).
fn fixture(kind: Control, parameter: bool, expanded: bool) -> (App, DeviceId, FaceField) {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, parameter));
    let entry = selected(&app, id).unwrap();
    if !parameter {
        definition(
            &mut app,
            id,
            TargetEdit::AddField {
                entry: entry.clone(),
                value_type: FieldType::Float,
            },
        );
    }
    let field = face_field(&app, id, parameter, 0, 0);
    let value_type = match kind {
        Control::Text | Control::Enum => FieldType::String,
        Control::Toggle => FieldType::Bool,
        _ => FieldType::Float,
    };
    for edit in [
        TargetEdit::SetFieldType {
            entry: entry.clone(),
            field: field.field.clone(),
            value_type,
        },
        TargetEdit::SetFieldControl {
            entry,
            field: field.field.clone(),
            control: kind,
        },
    ] {
        definition(&mut app, id, edit);
    }
    act(&mut app, Action::Add(id, !parameter));
    if !expanded {
        act(&mut app, Action::Fold(id));
        app.device_chain.entry.clear();
        app.device_chain.field.clear();
    }
    (app, id, field)
}

fn receiver(app: &mut App) -> TestReceiver {
    let receiver = TestReceiver::bind().unwrap();
    let mut project = app.project().clone();
    project.outputs = vec![Track::new_output(
        TrackId(999),
        "Test",
        "127.0.0.1",
        receiver.addr().port(),
    )];
    project.tracks[0].route = Route::single(TrackId(999));
    app.engine = Engine::new(project).unwrap();
    receiver
}

fn no_send(receiver: &TestReceiver) {
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(80))
            .is_err()
    );
}

fn one_send(receiver: &TestReceiver) {
    receiver
        .wait_for_messages(1, Duration::from_secs(1))
        .unwrap();
    no_send(receiver);
}

fn selection(app: &App, id: DeviceId, field: &FaceField, expanded: bool) {
    if expanded {
        assert_eq!(
            edited_field(app, id),
            Some((field.entry.clone(), field.field.clone()))
        );
    } else {
        assert!(selected(app, id).is_none());
    }
}

#[test]
fn slider_and_dial_continue_across_selection_and_every_view_rebuild() {
    for kind in [Control::Slider, Control::Dial] {
        for expanded in [true, false] {
            for mode in [SendMode::Continuous, SendMode::OnRelease] {
                for blur in [false, true] {
                    let (mut app, id, field) = fixture(kind, true, expanded);
                    // Edit without selecting the control's entry in the editor.
                    run(
                        &mut app,
                        edit(
                            id,
                            TargetEdit::SetParameterSendMode {
                                key: field.entry.clone(),
                                mode,
                            },
                        ),
                    );
                    let receiver = receiver(&mut app);
                    let control = control(id, true);
                    let bounds = Interaction::bounds(&app, id, control);
                    let start = bounds.center();
                    let mut interaction = Interaction::new();
                    let capture = expanded && mode == SendMode::OnRelease && !blur;
                    let name = if kind == Control::Dial {
                        "dial"
                    } else {
                        "slider"
                    };
                    if capture {
                        interaction.capture(&app, id, &format!("{name}-before"));
                    }
                    interaction.press(&mut app, id, start);
                    if capture {
                        interaction.capture(&app, id, &format!("{name}-press"));
                    }
                    selection(&app, id, &field, expanded);
                    assert!(app.control_focus.has(control));
                    let reference = qtable_engine::model::FieldRef {
                        parameter: ParameterRef {
                            track: app.project().device(id).unwrap().0.id,
                            device: qtable_engine::model::DeviceRef::Chain(id),
                            key: qtable_engine::model::ParameterKey(field.entry.0.clone()),
                        },
                        field: field.field.clone(),
                    };
                    let value = |app: &App| app.project().field(&reference).unwrap().value;
                    assert_eq!(value(&app), 0.0);
                    no_send(&receiver);
                    for step in [1.0, 2.0] {
                        let point = if kind == Control::Dial {
                            Point::new(start.x, start.y - 10.0 * step)
                        } else {
                            Point::new(bounds.x + bounds.width * (0.5 + 0.1 * step), start.y)
                        };
                        let messages = interaction.step(
                            &mut app,
                            id,
                            Event::Mouse(mouse::Event::CursorMoved { position: point }),
                            point,
                        );
                        assert_eq!(
                            messages
                                .iter()
                                .filter(|m| matches!(
                                    m,
                                    Message::Device(crate::device_chain::Action::Target(
                                        Action::Value(_, _, false)
                                    ))
                                ))
                                .count(),
                            1,
                            "{kind:?} must retain its drag through selection"
                        );
                        assert!(app.control_focus.has(control));
                        if capture {
                            interaction.capture(&app, id, &format!("{name}-drag-{step}"));
                        }
                        if mode == SendMode::OnRelease {
                            assert_eq!(value(&app), 0.0);
                            no_send(&receiver);
                        } else {
                            one_send(&receiver);
                        }
                    }
                    interaction.step(
                        &mut app,
                        id,
                        if blur {
                            Event::Window(iced::window::Event::Unfocused)
                        } else {
                            button(false)
                        },
                        start,
                    );
                    if capture {
                        interaction.capture(&app, id, &format!("{name}-release"));
                    }
                    let expected = 0.2;
                    assert!((value(&app) - expected).abs() < 0.001);
                    if mode == SendMode::OnRelease {
                        one_send(&receiver);
                    } else {
                        no_send(&receiver);
                    }
                    // A second release cannot commit/send again.
                    interaction.release(&mut app, id, start);
                    no_send(&receiver);
                    // The selected control is still live on its next gesture.
                    interaction.press(&mut app, id, start);
                    interaction.step(
                        &mut app,
                        id,
                        Event::Mouse(mouse::Event::CursorMoved {
                            position: Point::new(bounds.x + bounds.width * 0.8, start.y - 10.0),
                        }),
                        Point::new(bounds.x + bounds.width * 0.8, start.y - 10.0),
                    );
                    interaction.release(&mut app, id, start);
                    assert_ne!(value(&app), expected);
                    assert!(app.control_focus.has(control));
                }
            }
        }
    }
}

#[test]
fn double_click_reset_survives_selection_rebuilds_on_active_and_inactive_faces() {
    for kind in [Control::Slider, Control::Dial, Control::NumberBox] {
        for expanded in [true, false] {
            for active in [true, false] {
                let (mut app, id, field) = fixture(kind, true, expanded);
                let reference = qtable_engine::model::FieldRef {
                    parameter: ParameterRef {
                        track: app.project().device(id).unwrap().0.id,
                        device: qtable_engine::model::DeviceRef::Chain(id),
                        key: qtable_engine::model::ParameterKey(field.entry.0.clone()),
                    },
                    field: field.field.clone(),
                };
                run(
                    &mut app,
                    edit(
                        id,
                        TargetEdit::SetParameterSendMode {
                            key: field.entry.clone(),
                            mode: SendMode::OnRelease,
                        },
                    ),
                );
                run(
                    &mut app,
                    ProjectEdit::SetDevicesActive {
                        devices: vec![id],
                        active,
                    },
                );
                run(
                    &mut app,
                    ProjectEdit::SetParameter {
                        target: reference.parameter.clone(),
                        values: vec![(reference.field.clone(), 0.8)],
                    },
                );
                let control = control(id, true);
                let point = Interaction::bounds(&app, id, control).center();
                let mut interaction = Interaction::new();
                interaction.press(&mut app, id, point);
                interaction.release(&mut app, id, point);
                selection(&app, id, &field, expanded);
                let revision = app.project_revision();
                // The second press must retain the click history through entry selection,
                // focus styling and the first release's model/view refresh.
                interaction.press(&mut app, id, point);
                assert_eq!(app.project().field(&reference).unwrap().value, 0.0);
                assert_eq!(app.project_revision(), revision + 1, "{kind:?}");
                assert!(app.parameter_gesture.is_empty());
                assert!(app.control_focus.has(control));
                interaction.release(&mut app, id, point);
                assert_eq!(app.project_revision(), revision + 1);
                assert!(app.parameter_gesture.is_empty());
            }
        }
    }
}

#[test]
fn number_and_text_keep_native_input_focus_and_typing_across_entry_selection() {
    for kind in [Control::NumberBox, Control::Text] {
        for expanded in [true, false] {
            let parameter = kind != Control::Text;
            let (mut app, id, field) = fixture(kind, parameter, expanded);
            let receiver = receiver(&mut app);
            let control = control(id, parameter);
            let point = Interaction::bounds(&app, id, control).center();
            let mut interaction = Interaction::new();
            interaction.press(&mut app, id, point);
            interaction.release(&mut app, id, point);
            selection(&app, id, &field, expanded);
            // Native selection and text events, with no synthetic focus task: losing input
            // state on the first selection would make these keystrokes disappear.
            interaction.step(
                &mut app,
                id,
                key(
                    keyboard::Key::Character("a".into()),
                    None,
                    keyboard::Modifiers::COMMAND,
                ),
                point,
            );
            let text = if kind == Control::Text {
                "hello"
            } else {
                "0.25"
            };
            for ch in text.chars() {
                let ch = ch.to_string();
                interaction.step(
                    &mut app,
                    id,
                    key(
                        keyboard::Key::Character(ch.clone().into()),
                        Some(&ch),
                        keyboard::Modifiers::NONE,
                    ),
                    point,
                );
                assert!(app.control_focus.has(control));
            }
            assert_eq!(app.device_chain.face.exact.as_ref().unwrap().1, text);
            no_send(&receiver);
            interaction.step(
                &mut app,
                id,
                key(
                    keyboard::Key::Named(keyboard::key::Named::Enter),
                    None,
                    keyboard::Modifiers::NONE,
                ),
                point,
            );
            assert!(app.device_chain.face.exact.is_none());
            let (_, target) = target(&app, id).unwrap();
            if kind == Control::Text {
                assert_eq!(
                    target.events[0].fields[0].current_argument(),
                    OscArgument::String(text.into())
                );
                no_send(&receiver);
            } else {
                assert_eq!(target.parameters[0].fields[0].value, Some(0.25));
                one_send(&receiver);
            }
        }
    }
}

#[test]
fn toggle_and_enum_operate_on_the_first_interaction_while_selecting() {
    for kind in [Control::Toggle, Control::Enum] {
        for expanded in [true, false] {
            let (mut app, id, field) = fixture(kind, true, expanded);
            let receiver = receiver(&mut app);
            let control = control(id, true);
            let bounds = Interaction::bounds(&app, id, control);
            let point = bounds.center();
            let mut interaction = Interaction::new();
            interaction.press(&mut app, id, point);
            interaction.release(&mut app, id, point);
            selection(&app, id, &field, expanded);
            if kind == Control::Enum {
                no_send(&receiver);
                // The menu opens below the dropdown; choose its second option.
                let option = Point::new(point.x, bounds.y + bounds.height + 30.0);
                interaction.step(
                    &mut app,
                    id,
                    Event::Mouse(mouse::Event::CursorMoved { position: option }),
                    option,
                );
                interaction.press(&mut app, id, option);
                interaction.release(&mut app, id, option);
            }
            one_send(&receiver);
            assert_eq!(
                target(&app, id).unwrap().1.parameters[0].fields[0].value,
                Some(1.0)
            );
            assert!(app.control_focus.has(control));
        }
    }
}

#[test]
fn target_entry_arrows_preserve_the_mixers_on_release_hold() {
    use crate::mixer_band::MixerMessage;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    act(&mut app, Action::Add(id, false));
    let track = app.project().device(id).unwrap().0.id;
    let reference = ParameterRef::volume(track);
    let mut output = app
        .project()
        .parameter(&reference)
        .unwrap()
        .output
        .into_owned();
    output.enabled = true;
    output.send_mode = SendMode::OnRelease;
    run(
        &mut app,
        ProjectEdit::SetParameterOutput {
            target: reference.clone(),
            output,
        },
    );
    let receiver = receiver(&mut app);
    let _ = crate::update::update(
        &mut app,
        Message::Mixer(MixerMessage::DragVolume(track, 0.3)),
    );
    for arrow in [
        keyboard::key::Named::ArrowUp,
        keyboard::key::Named::ArrowDown,
    ] {
        let before = selected(&app, id);
        let _ = crate::update::update(
            &mut app,
            Message::KeyPressed {
                key: keyboard::Key::Named(arrow),
                modifiers: keyboard::Modifiers::NONE,
                repeat: false,
                ignored: true,
            },
        );
        assert_ne!(selected(&app, id), before);
        assert_eq!(
            app.parameter_gesture
                .held(&crate::mixer_device::volume_field(track)),
            Some(0.3)
        );
        let track = app.project().track(track).unwrap();
        assert_eq!(crate::mixer_device::shown_volume(&app, track), 0.3);
        assert_eq!(track.mixer_device.volume.value, 1.0);
        no_send(&receiver);
    }
    let _ = crate::update::update(
        &mut app,
        Message::Mixer(MixerMessage::ReleaseVolume(track, 0.3)),
    );
    assert!(app.parameter_gesture.is_empty());
    assert_eq!(
        app.project().parameter(&reference).unwrap().fields[0].value,
        0.3
    );
    one_send(&receiver);
}

#[test]
fn event_controls_only_edit_current_values_and_the_first_bang_selects_and_fires_once() {
    for kind in [
        Control::Slider,
        Control::Dial,
        Control::NumberBox,
        Control::Text,
        Control::Toggle,
        Control::Enum,
    ] {
        for expanded in [true, false] {
            let (mut app, id, field) = fixture(kind, false, expanded);
            let receiver = receiver(&mut app);
            let default = target(&app, id).unwrap().1.events[0].fields[0]
                .default
                .clone();
            let control = control(id, false);
            let bounds = Interaction::bounds(&app, id, control);
            let point = bounds.center();
            let mut interaction = Interaction::new();
            // Repeat with the argument already selected as well, after the double-click window
            // so the second round is a fresh click and not a reset.
            for round in 0..2 {
                if round == 1 {
                    interaction.clock.advance(Duration::from_millis(310));
                }
                interaction.press(&mut app, id, point);
                selection(&app, id, &field, expanded);
                match kind {
                    Control::Dial => {
                        let moved = Point::new(point.x, point.y - 20.0);
                        interaction.step(
                            &mut app,
                            id,
                            Event::Mouse(mouse::Event::CursorMoved { position: moved }),
                            moved,
                        );
                    }
                    Control::NumberBox | Control::Text => {
                        interaction.release(&mut app, id, point);
                        interaction.step(
                            &mut app,
                            id,
                            key(
                                keyboard::Key::Character("a".into()),
                                None,
                                keyboard::Modifiers::COMMAND,
                            ),
                            point,
                        );
                        let text = if kind == Control::Text { "cue" } else { "0.25" };
                        for ch in text.chars() {
                            let ch = ch.to_string();
                            interaction.step(
                                &mut app,
                                id,
                                key(
                                    keyboard::Key::Character(ch.clone().into()),
                                    Some(&ch),
                                    keyboard::Modifiers::NONE,
                                ),
                                point,
                            );
                        }
                        interaction.step(
                            &mut app,
                            id,
                            key(
                                keyboard::Key::Named(keyboard::key::Named::Enter),
                                None,
                                keyboard::Modifiers::NONE,
                            ),
                            point,
                        );
                    }
                    Control::Enum => {
                        let option = Point::new(point.x, bounds.y + bounds.height + 30.0);
                        interaction.step(
                            &mut app,
                            id,
                            Event::Mouse(mouse::Event::CursorMoved { position: option }),
                            option,
                        );
                        interaction.press(&mut app, id, option);
                        interaction.release(&mut app, id, option);
                    }
                    _ => {}
                }
                interaction.release(&mut app, id, point);
                assert!(app.control_focus.has(control));
                assert_eq!(
                    target(&app, id).unwrap().1.events[0].fields[0].default,
                    default
                );
                no_send(&receiver);
            }
            let stored = &target(&app, id).unwrap().1.events[0].fields[0];
            let expected = match kind {
                Control::Text => OscArgument::String("cue".into()),
                Control::Enum => OscArgument::String("Two".into()),
                Control::Toggle => OscArgument::Bool(false),
                Control::Dial => OscArgument::Float(0.4),
                Control::Slider => OscArgument::Float(0.0),
                Control::NumberBox => OscArgument::Float(0.25),
            };
            assert_eq!(
                stored.current_argument(),
                expected,
                "{kind:?}, expanded={expanded}"
            );
            if expanded {
                let other = target(&app, id).unwrap().1.parameters[0].key.clone();
                act(&mut app, Action::Select(id, other));
            }
            let bang = ControlId::field_action(
                View::Track,
                DraftKey::Target(id, TargetField::Face(false, 0, 0)),
                "fire",
            );
            let point = Interaction::bounds(&app, id, bang).center();
            let revision = app.project_revision();
            for _ in 0..2 {
                interaction.press(&mut app, id, point);
                if expanded {
                    assert_eq!(selected(&app, id), Some(field.entry.clone()));
                }
                let packets = receiver
                    .wait_for_messages(1, Duration::from_secs(1))
                    .unwrap();
                assert_eq!(packets.len(), 1);
                interaction.release(&mut app, id, point);
                no_send(&receiver);
                assert_eq!(
                    app.project_revision(),
                    revision,
                    "a bang is a command, not an edit"
                );
            }
        }
    }
}

#[test]
fn expanded_sliders_and_dials_keep_exact_typing_ready_after_escape_until_editor_selection() {
    for kind in [Control::Slider, Control::Dial] {
        let (mut app, id, field) = fixture(kind, true, true);
        let control = control(id, true);
        let point = Interaction::bounds(&app, id, control).center();
        let mut interaction = Interaction::new();
        interaction.press(&mut app, id, point);
        interaction.release(&mut app, id, point);
        let task = crate::update::update(
            &mut app,
            Message::KeyPressed {
                key: keyboard::Key::Character("0".into()),
                modifiers: keyboard::Modifiers::NONE,
                repeat: false,
                ignored: true,
            },
        );
        interaction.task(&mut app, id, task);
        assert_eq!(
            app.device_chain.face.exact.as_ref().unwrap(),
            &(field.clone(), "0".into())
        );
        assert!(app.control_focus.has(control));
        let stored = target(&app, id).unwrap().1.parameters[0].fields[0].current_argument();
        interaction.step(
            &mut app,
            id,
            key(
                keyboard::Key::Named(keyboard::key::Named::Escape),
                None,
                keyboard::Modifiers::NONE,
            ),
            point,
        );
        let _ = crate::update::update(&mut app, Message::CancelField);
        assert!(app.device_chain.face.exact.is_none());
        assert!(app.control_focus.has(control));
        assert_eq!(
            target(&app, id).unwrap().1.parameters[0].fields[0].current_argument(),
            stored
        );
        let task = crate::update::update(
            &mut app,
            Message::KeyPressed {
                key: keyboard::Key::Character("7".into()),
                modifiers: keyboard::Modifiers::NONE,
                repeat: false,
                ignored: true,
            },
        );
        interaction.task(&mut app, id, task);
        interaction.step(
            &mut app,
            id,
            key(
                keyboard::Key::Character("5".into()),
                Some("5"),
                keyboard::Modifiers::NONE,
            ),
            point,
        );
        assert_eq!(
            app.device_chain.face.exact.as_ref().unwrap(),
            &(field.clone(), "75".into())
        );
        let _ = crate::update::update(&mut app, Message::CancelField);
        act(&mut app, Action::Select(id, field.entry));
        assert!(app.control_focus.current().is_none());
    }
}

#[test]
fn composed_face_controls_have_one_focus_and_contextual_help_wrapper() {
    for kind in [
        Control::Slider,
        Control::Dial,
        Control::NumberBox,
        Control::Text,
        Control::Toggle,
        Control::Enum,
    ] {
        for expanded in [true, false] {
            let (mut app, id, _) = fixture(kind, false, expanded);
            let wanted = control(id, false);
            let mut widget = super::super::super::frame::device(
                &app,
                app.project().device(id).unwrap().1,
                View::Track,
            );
            let controls = crate::control_focus::test_registry::rectangles_in(&mut widget, SIZE);
            assert_eq!(
                controls.iter().filter(|(id, _)| *id == wanted).count(),
                1,
                "{kind:?}, expanded={expanded}"
            );
            let bang = ControlId::field_action(
                View::Track,
                DraftKey::Target(id, TargetField::Face(false, 0, 0)),
                "fire",
            );
            assert_eq!(controls.iter().filter(|(id, _)| *id == bang).count(), 1);
            let bang_point = controls
                .iter()
                .find(|(id, _)| *id == bang)
                .unwrap()
                .1
                .center();
            let point = controls
                .iter()
                .find(|(id, _)| *id == wanted)
                .unwrap()
                .1
                .center();
            drop(widget);
            let mut interaction = Interaction::new();
            let messages = interaction.step(
                &mut app,
                id,
                Event::Mouse(mouse::Event::CursorMoved { position: point }),
                point,
            );
            let help = match kind {
                Control::Text => crate::info::Id::TargetText,
                Control::NumberBox => crate::info::Id::TargetNumber,
                Control::Toggle => crate::info::Id::TargetToggle,
                Control::Enum => crate::info::Id::TargetEnum,
                _ => crate::info::Id::TargetValue,
            };
            let subjects: Vec<_> = messages
                .iter()
                .filter_map(|message| match message {
                    Message::Info(crate::info::Hover::Enter(_, entry, subject))
                        if *entry == help =>
                    {
                        Some(subject.clone())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                subjects,
                [Some(crate::info::Subject::TargetField(
                    first_event(&app, id),
                    first_event_field(&app, id),
                ))],
                "{kind:?}, expanded={expanded}"
            );
            let messages = interaction.step(
                &mut app,
                id,
                Event::Mouse(mouse::Event::CursorMoved {
                    position: bang_point,
                }),
                bang_point,
            );
            let subjects: Vec<_> = messages
                .iter()
                .filter_map(|message| match message {
                    Message::Info(crate::info::Hover::Enter(
                        _,
                        crate::info::Id::TargetFire,
                        subject,
                    )) => Some(subject.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                subjects,
                [Some(crate::info::Subject::TargetEntry(first_event(
                    &app, id
                )))]
            );
        }
    }
}

#[test]
fn composed_face_focus_draws_brackets_for_slider_and_dial_and_one_box_for_number() {
    use iced::advanced::renderer::Headless;
    for kind in [Control::Slider, Control::Dial, Control::NumberBox] {
        for expanded in [true, false] {
            let (mut app, id, _) = fixture(kind, false, expanded);
            let wanted = control(id, false);
            let bounds = Interaction::bounds(&app, id, wanted);
            let _ = crate::update::update(&mut app, Message::ControlFocused(wanted));
            let mut widget = super::super::super::frame::device(
                &app,
                app.project().device(id).unwrap().1,
                View::Track,
            );
            let mut tree = Tree::new(&widget);
            let mut renderer = crate::grid::test_renderer();
            let node = widget.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, SIZE),
            );
            widget.as_widget().draw(
                &tree,
                &mut renderer,
                &iced::Theme::Dark,
                &iced::advanced::renderer::Style::default(),
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &Rectangle::with_size(SIZE),
            );
            let pixels =
                renderer.screenshot(Size::new(1000, 900), 1.0, app.theme.detail_view_background);
            let pixel = |x: f32, y: f32| {
                let offset = (y.ceil() as usize * 1000 + x.ceil() as usize) * 4;
                &pixels[offset..offset + 4]
            };
            let stroke = app.theme.focus_stroke.into_rgba8();
            // Away from the rounded corner, the top edge belongs to both outline styles.
            assert_eq!(pixel(bounds.x + 3.0, bounds.y), stroke, "{kind:?}");
            let middle = pixel(bounds.x + bounds.width / 2.0, bounds.y);
            if kind == Control::NumberBox {
                assert_eq!(middle, stroke, "a boxed control keeps its stroke");
            } else {
                assert_ne!(
                    middle, stroke,
                    "{kind:?} must leave the edge between brackets clear"
                );
            }
        }
    }
}

#[test]
fn expanded_slider_first_drag_inside_scrolled_chain_keeps_its_value() {
    for mode in [SendMode::Continuous, SendMode::OnRelease] {
        let (mut app, id, field) = fixture(Control::Slider, true, true);
        run(
            &mut app,
            edit(
                id,
                TargetEdit::SetParameterSendMode {
                    key: field.entry.clone(),
                    mode,
                },
            ),
        );
        act(&mut app, Action::Value(field.clone(), 0.4, true));
        act(&mut app, Action::Add(id, false));
        let receiver = receiver(&mut app);
        let bounds = Interaction::bounds(&app, id, control(id, true));
        let start = bounds.center();
        let mut interaction = Interaction::new();
        interaction.scroll_x = 600.0;
        interaction.press(&mut app, id, start);
        selection(&app, id, &field, true);
        assert_eq!(
            target(&app, id).unwrap().1.parameters[0].fields[0].value,
            Some(0.4)
        );
        no_send(&receiver);
        let moved = Point::new(start.x + bounds.width * 0.1, start.y);
        interaction.step(
            &mut app,
            id,
            Event::Mouse(mouse::Event::CursorMoved { position: moved }),
            moved,
        );
        if mode == SendMode::OnRelease {
            assert_eq!(
                target(&app, id).unwrap().1.parameters[0].fields[0].value,
                Some(0.4)
            );
            assert_eq!(super::super::collapsed::held(&app, &field), Some(0.5));
            no_send(&receiver);
        } else {
            assert_eq!(
                target(&app, id).unwrap().1.parameters[0].fields[0].value,
                Some(0.5)
            );
            one_send(&receiver);
        }
        interaction.release(&mut app, id, moved);
        assert_eq!(
            target(&app, id).unwrap().1.parameters[0].fields[0].value,
            Some(0.5)
        );
        if mode == SendMode::OnRelease {
            one_send(&receiver);
        } else {
            no_send(&receiver);
        }
    }
}

#[test]
fn slider_click_then_type_commits_with_enter_click_outside_or_window_blur() {
    for expanded in [true, false] {
        for parameter in [true, false] {
            for finish in ["enter", "outside", "blur", "selection"] {
                let (mut app, id, _) = fixture(Control::Slider, parameter, expanded);
                if parameter && finish == "outside" {
                    act(&mut app, Action::Add(id, true));
                    if !expanded {
                        app.device_chain.entry.clear();
                        app.device_chain.field.clear();
                    }
                }
                let receiver = receiver(&mut app);
                let control = control(id, parameter);
                let point = Interaction::bounds(&app, id, control).center();
                let mut interaction = Interaction::new();
                let revision = app.project_revision();
                interaction.press(&mut app, id, point);
                interaction.release(&mut app, id, point);
                assert!(app.control_focus.has(control));
                assert_eq!(app.project_revision(), revision);
                no_send(&receiver);
                // The window subscription forwards the uncaptured first digit, starting
                // the existing Number input. Run its complete returned task on the tree.
                let task = crate::update::update(
                    &mut app,
                    Message::KeyPressed {
                        key: keyboard::Key::Character("0".into()),
                        modifiers: keyboard::Modifiers::NONE,
                        repeat: false,
                        ignored: true,
                    },
                );
                interaction.task(&mut app, id, task);
                for ch in [".", "2", "5"] {
                    interaction.step(
                        &mut app,
                        id,
                        key(
                            keyboard::Key::Character(ch.into()),
                            Some(ch),
                            keyboard::Modifiers::NONE,
                        ),
                        point,
                    );
                }
                assert_eq!(app.device_chain.face.exact.as_ref().unwrap().1, "0.25");
                interaction.check_exact(&mut app, id);
                assert!(
                    app.device_chain.face.exact.is_some(),
                    "focused typing must not blur-commit"
                );
                no_send(&receiver);
                if finish == "outside" {
                    let next = ControlId::field(
                        View::Track,
                        DraftKey::Target(id, TargetField::Face(true, 1, 0)),
                    );
                    let outside = if parameter {
                        Interaction::bounds(&app, id, next).center()
                    } else {
                        Point::new(SIZE.width - 1.0, SIZE.height - 1.0)
                    };
                    interaction.press(&mut app, id, outside);
                    interaction.release(&mut app, id, outside);
                    interaction.check_exact(&mut app, id);
                    if parameter {
                        assert!(app.control_focus.has(next));
                        if expanded {
                            assert_eq!(
                                selected(&app, id),
                                Some(target(&app, id).unwrap().1.parameters[1].key.clone())
                            );
                        }
                    }
                } else if finish == "selection" {
                    let main = app.project().main.id;
                    let _ = crate::update::update(
                        &mut app,
                        Message::Grid(crate::grid::Action::SelectTrack(main, false)),
                    );
                    assert_eq!(app.selection.tracks, vec![main]);
                } else if finish == "blur" {
                    interaction.step(
                        &mut app,
                        id,
                        Event::Window(iced::window::Event::Unfocused),
                        point,
                    );
                    let _ = crate::update::update(&mut app, Message::CommitField);
                } else {
                    interaction.step(
                        &mut app,
                        id,
                        key(
                            keyboard::Key::Named(keyboard::key::Named::Enter),
                            None,
                            keyboard::Modifiers::NONE,
                        ),
                        point,
                    );
                }
                assert!(app.device_chain.face.exact.is_none());
                let (_, target) = target(&app, id).unwrap();
                let field = if parameter {
                    &target.parameters[0].fields[0]
                } else {
                    &target.events[0].fields[0]
                };
                assert_eq!(field.current_argument(), OscArgument::Float(0.25));
                if parameter {
                    one_send(&receiver);
                } else {
                    no_send(&receiver);
                }
            }
        }
    }
}

#[test]
fn slider_double_click_sends_only_the_default_once() {
    for expanded in [true, false] {
        let (mut app, id, field) = fixture(Control::Slider, true, expanded);
        // The argument default is zero; start elsewhere before attaching the receiver.
        act(&mut app, Action::Value(field, 0.8, true));
        act(&mut app, Action::Add(id, false));
        let receiver = receiver(&mut app);
        let point = Interaction::bounds(&app, id, control(id, true)).center();
        let mut interaction = Interaction::new();
        interaction.press(&mut app, id, point);
        interaction.release(&mut app, id, point);
        no_send(&receiver);
        interaction.press(&mut app, id, point);
        interaction.release(&mut app, id, point);
        assert_eq!(
            target(&app, id).unwrap().1.parameters[0].fields[0].value,
            Some(0.0)
        );
        one_send(&receiver);
    }
}

#[test]
fn exact_blur_reply_ignores_stale_drafts_projects_and_focus() {
    let (mut app, id, field) = fixture(Control::Slider, true, true);
    act(&mut app, Action::Exact(field.clone(), "0.25".into()));
    let generation = app.project_generation;
    for (text, generation, focused) in [
        ("0.5", generation, false),
        ("0.25", generation + 1, false),
        ("0.25", generation, true),
    ] {
        act(
            &mut app,
            Action::ExactFocusChecked(field.clone(), text.into(), generation, focused),
        );
        assert!(app.device_chain.face.exact.is_some());
        assert_eq!(
            target(&app, id).unwrap().1.parameters[0].fields[0].value,
            Some(0.0)
        );
    }
    act(&mut app, Action::Exact(field.clone(), "2".into()));
    act(
        &mut app,
        Action::ExactFocusChecked(field, "2".into(), generation, false),
    );
    assert!(app.device_chain.face.error);
    assert_eq!(app.device_chain.face.exact.as_ref().unwrap().1, "2");
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[0].value,
        Some(0.0)
    );
}

#[test]
fn an_exact_value_committed_by_losing_focus_is_a_step_apart_from_the_click_that_follows() {
    // The same rule for the Target face's typed value.
    let (mut app, _id, field) = fixture(Control::Slider, true, true);
    let track = app.project().tracks[0].id;
    act(&mut app, Action::Exact(field.clone(), "0.25".into()));
    let generation = app.project_generation;
    act(
        &mut app,
        Action::ExactFocusChecked(field.clone(), "0.25".into(), generation, false),
    );
    assert!(app.device_chain.face.exact.is_none(), "the draft committed");
    let committed = app.project().clone();
    let _ = crate::update::update(
        &mut app,
        crate::messages::Message::Mixer(crate::mixer_band::MixerMessage::Activator(track)),
    );
    assert_ne!(app.project(), &committed, "the click changed the set");
    app.engine
        .execute(qtable_engine::engine::Command::Undo)
        .unwrap();
    assert_eq!(
        app.project(),
        &committed,
        "one undo takes back the click alone"
    );
}

#[test]
fn numeric_face_arrows_capture_navigation_and_send_according_to_the_gesture_mode() {
    for kind in [Control::Slider, Control::Dial, Control::NumberBox] {
        for mode in [SendMode::Continuous, SendMode::OnRelease] {
            let (mut app, id, field) = fixture(kind, true, true);
            definition(
                &mut app,
                id,
                TargetEdit::SetParameterSendMode {
                    key: field.entry.clone(),
                    mode,
                },
            );
            let receiver = receiver(&mut app);
            let control = control(id, true);
            let point = Interaction::bounds(&app, id, control).center();
            let mut interaction = Interaction::new();
            interaction.press(&mut app, id, point);
            interaction.release(&mut app, id, point);
            let before = selected(&app, id);
            let reference = qtable_engine::model::FieldRef {
                parameter: ParameterRef {
                    track: app.project().device(id).unwrap().0.id,
                    device: qtable_engine::model::DeviceRef::Chain(id),
                    key: qtable_engine::model::ParameterKey(field.entry.0.clone()),
                },
                field: field.field.clone(),
            };
            let revision = app.project_revision();
            for fine in [false, true] {
                let modifiers = if fine {
                    keyboard::Modifiers::SHIFT
                } else {
                    keyboard::Modifiers::NONE
                };
                interaction.step(
                    &mut app,
                    id,
                    key(
                        keyboard::Key::Named(keyboard::key::Named::ArrowUp),
                        None,
                        modifiers,
                    ),
                    point,
                );
                assert!(interaction.captured);
                let _ = crate::update::update(
                    &mut app,
                    Message::KeyPressed {
                        key: keyboard::Key::Named(keyboard::key::Named::ArrowUp),
                        modifiers,
                        repeat: fine,
                        ignored: false,
                    },
                );
                assert_eq!(selected(&app, id), before);
            }
            let held = kind != Control::NumberBox && mode == SendMode::OnRelease;
            assert_eq!(app.project_revision() - revision, if held { 0 } else { 2 });
            if held {
                no_send(&receiver);
            }
            interaction.step(
                &mut app,
                id,
                Event::Keyboard(keyboard::Event::KeyReleased {
                    key: keyboard::Key::Named(keyboard::key::Named::ArrowUp),
                    modified_key: keyboard::Key::Named(keyboard::key::Named::ArrowUp),
                    physical_key: keyboard::key::Physical::Unidentified(
                        keyboard::key::NativeCode::Unidentified,
                    ),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::NONE,
                }),
                point,
            );
            assert_eq!(app.project().field(&reference).unwrap().value, 0.0125);
            assert!(app.parameter_gesture.held(&reference).is_none());
            assert_eq!(app.project_revision() - revision, if held { 1 } else { 2 });
            if held {
                one_send(&receiver);
            }
        }
    }
}

#[test]
fn integer_number_keeps_nudging_between_zero_and_one_without_reselection() {
    for expanded in [true, false] {
        let (mut app, id, field) = fixture(Control::NumberBox, true, expanded);
        definition(
            &mut app,
            id,
            TargetEdit::SetFieldType {
                entry: field.entry.clone(),
                field: field.field.clone(),
                value_type: FieldType::Int,
            },
        );
        let point = Interaction::bounds(&app, id, control(id, true)).center();
        let mut interaction = Interaction::new();
        interaction.press(&mut app, id, point);
        interaction.release(&mut app, id, point);
        for (arrow, expected) in [
            (keyboard::key::Named::ArrowUp, 1.0),
            (keyboard::key::Named::ArrowDown, 0.0),
            (keyboard::key::Named::ArrowUp, 1.0),
            (keyboard::key::Named::ArrowDown, 0.0),
        ] {
            interaction.step(
                &mut app,
                id,
                key(keyboard::Key::Named(arrow), None, keyboard::Modifiers::NONE),
                point,
            );
            assert!(interaction.captured);
            assert_eq!(
                target(&app, id).unwrap().1.parameters[0].fields[0].value,
                Some(expected)
            );
            assert!(app.device_chain.face.exact.is_none());
        }
    }
}

#[test]
fn integer_dial_click_and_shift_drag_use_consistent_pointer_coordinates() {
    for fine in [false, true] {
        let (mut app, id, field) = fixture(Control::Dial, true, true);
        definition(
            &mut app,
            id,
            TargetEdit::SetFieldType {
                entry: field.entry.clone(),
                field: field.field.clone(),
                value_type: FieldType::Int,
            },
        );
        definition(
            &mut app,
            id,
            TargetEdit::SetFieldRange {
                entry: field.entry.clone(),
                field: field.field.clone(),
                range: Some([0.0, 127.0]),
            },
        );
        app.selection.shift_down = fine;
        let point = Interaction::bounds(&app, id, control(id, true)).center();
        let mut interaction = Interaction::new();
        interaction.press(&mut app, id, point);
        // Scrollables translate the cursor into content coordinates but retain raw
        // window coordinates in the event. This is a stationary pointer after press.
        let moved = |p: Point| {
            Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(p.x, p.y - 250.0),
            })
        };
        interaction.step(&mut app, id, moved(point), point);
        interaction.release(&mut app, id, point);
        assert_eq!(
            target(&app, id).unwrap().1.parameters[0].fields[0].value,
            Some(0.0)
        );
        // Begin the next drag directly, without making it a double-click reset.
        interaction.step(
            &mut app,
            id,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Point::new(point.x + 10.0, point.y),
        );
        let next = Point::new(point.x + 10.0, point.y - 10.0);
        interaction.step(&mut app, id, moved(next), next);
        interaction.release(&mut app, id, next);
        assert_eq!(
            target(&app, id).unwrap().1.parameters[0].fields[0].value,
            Some(if fine { 3.0 } else { 13.0 })
        );
    }
}

#[test]
fn selected_toggle_enter_uses_the_same_action_as_click() {
    let (mut app, id, _) = fixture(Control::Toggle, true, true);
    let point = Interaction::bounds(&app, id, control(id, true)).center();
    let mut interaction = Interaction::new();
    interaction.press(&mut app, id, point);
    interaction.release(&mut app, id, point);
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[0].value,
        Some(1.0)
    );
    for expected in [0.0, 1.0] {
        interaction.step(
            &mut app,
            id,
            key(
                keyboard::Key::Named(keyboard::key::Named::Enter),
                None,
                keyboard::Modifiers::NONE,
            ),
            point,
        );
        assert!(interaction.captured);
        assert_eq!(
            target(&app, id).unwrap().1.parameters[0].fields[0].value,
            Some(expected)
        );
    }
}

/// The first event of the Target device `id`, by its stable keys.
fn first_event(app: &App, id: DeviceId) -> crate::info::EntryRef {
    let (track, device) = app.project().device(id).unwrap();
    let target = device.target().unwrap();
    crate::info::EntryRef::target(false, track.id, id, &target.events[0].key)
}

/// The key of that event's first argument.
fn first_event_field(app: &App, id: DeviceId) -> qtable_engine::model::FieldKey {
    let target = app.project().device(id).unwrap().1.target().unwrap();
    target.events[0].fields[0].key.clone()
}
