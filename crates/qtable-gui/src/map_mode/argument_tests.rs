//! Which Target arguments offer a mapping, by the controls the whole window declares in Map
//! Mode, and whether a learned mapping drives them. Every parameter argument maps in both modes, and so does every
//! non-Text argument of an event: it sets the value the event's Bang sends and sends nothing.
use super::tests::{act, send};
use super::*;
use iced::Size;
use qtable_engine::model::*;

fn unranged(key: &str, value_type: FieldType, parameter: bool) -> FieldDef {
    let mut field = FieldDef::new(FieldKey(key.into()), value_type, parameter);
    field.range = None;
    field.control = Control::NumberBox;
    field.seed_current();
    field
}

fn ranged(key: &str, value_type: FieldType, parameter: bool, control: Control) -> FieldDef {
    let mut field = FieldDef::new(FieldKey(key.into()), value_type, parameter);
    if matches!(value_type, FieldType::Float | FieldType::Int) {
        field.range = Some([0.0, 8.0]);
    }
    field.control = control;
    field.seed_current();
    field
}

fn entries(parameter: bool) -> Vec<(&'static str, FieldDef)> {
    use FieldType::*;
    let mut text = FieldDef::new(FieldKey("value".into()), String, false);
    text.control = Control::Text;
    text.seed_current();
    let mut enumerated = FieldDef::new(FieldKey("value".into()), String, parameter);
    enumerated.control = Control::Enum;
    if enumerated.options.is_empty() {
        enumerated.options = ["One", "Two"]
            .map(|label| EnumOption {
                label: label.into(),
                value: OscArgument::String(label.into()),
            })
            .to_vec();
        enumerated.default = enumerated.options[0].value.clone();
    }
    enumerated.seed_current();
    let mut all = vec![
        (
            "float_slider",
            ranged("value", Float, parameter, Control::Slider),
        ),
        (
            "float_dial",
            ranged("value", Float, parameter, Control::Dial),
        ),
        (
            "float_box",
            ranged("value", Float, parameter, Control::NumberBox),
        ),
        (
            "int_slider",
            ranged("value", Int, parameter, Control::Slider),
        ),
        (
            "int_box",
            ranged("value", Int, parameter, Control::NumberBox),
        ),
        (
            "bool_toggle",
            ranged("value", Bool, parameter, Control::Toggle),
        ),
        ("string_enum", enumerated),
    ];
    if !parameter {
        all.push(("float_unranged", unranged("value", Float, false)));
        all.push(("int_unranged", unranged("value", Int, false)));
        all.push(("string_text", text));
    }
    all
}

pub(crate) fn project(expanded: bool) -> Project {
    let mut project = crate::files::new_project();
    let track = &mut project.tracks[0];
    track.devices.push(NewDevice::Target.build(DeviceId(1)));
    track.devices[0].expanded = expanded;
    let device = track.devices[0].target_mut().unwrap();
    for (key, field) in entries(true) {
        device.parameters.push(ParameterDef {
            key: EntryKey(format!("p_{key}")),
            name: String::new(),
            path: OscAddress::Relative(format!("/p_{key}")),
            fields: vec![field],
            send_mode: SendMode::Continuous,
        });
    }
    for (key, field) in entries(false) {
        device.events.push(EventDef {
            key: EntryKey(format!("e_{key}")),
            name: String::new(),
            path: OscAddress::Relative(format!("/e_{key}")),
            fields: vec![field],
        });
    }
    project
}

fn declared(app: &App) -> Vec<ControlRef> {
    let mut view = crate::view::view(app);
    let renderer = crate::grid::test_renderer();
    let mut tree = iced::advanced::widget::Tree::new(&view);
    let node = view.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &iced::advanced::layout::Limits::new(Size::ZERO, Size::new(1800.0, 1200.0)),
    );
    crate::control::map_layer::collect(|operation| {
        view.as_widget_mut().operate(
            &mut tree,
            iced::advanced::Layout::new(&node),
            &renderer,
            operation,
        );
    })
    .into_iter()
    .map(|(_, control)| control)
    .collect()
}

fn param(device: DeviceId, key: &str) -> FieldRef {
    FieldRef {
        parameter: ParameterRef {
            track: TrackId(1),
            device: DeviceRef::Chain(device),
            key: ParameterKey(format!("p_{key}")),
        },
        field: FieldKey("value".into()),
    }
}

/// The fixture the capture opens is this project; regenerate it with
/// `QTABLE_REGENERATE_FIXTURES=1 cargo test -p qtable-gui map_arguments_fixture`.
#[test]
fn map_arguments_fixture_matches_the_builder() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/map-arguments.ron");
    let project = project(false);
    if std::env::var_os("QTABLE_REGENERATE_FIXTURES").is_some() {
        qtable_engine::file::save(&path, &project).unwrap();
    }
    assert_eq!(qtable_engine::file::load(&path).unwrap(), project);
}

fn learned_app(expanded: bool, mode: Mode) -> (App, DeviceId) {
    let engine = qtable_engine::engine::Engine::new(project(expanded)).unwrap();
    let mut app = App::new(engine, "120.00".into(), None);
    let _ = crate::update::update(
        &mut app,
        crate::messages::Message::Grid(crate::grid::Action::SelectTrack(TrackId(1), false)),
    );
    act(&mut app, Action::Toggle);
    if mode == Mode::Key {
        act(&mut app, Action::ToggleKey);
    }
    let device = app.project().tracks[0].devices[0].id;
    (app, device)
}

/// The control that drives `entry`'s argument, on a parameter or an event.
fn control_for(device: DeviceId, key: &str, parameter: bool) -> ControlRef {
    if parameter {
        ControlRef::Argument(param(device, key))
    } else {
        ControlRef::EventArgument {
            track: TrackId(1),
            event: EventRef {
                device,
                key: EntryKey(format!("e_{key}")),
            },
            field: FieldKey("value".into()),
        }
    }
}

/// The stored value the argument's face holds.
fn held(app: &App, device: DeviceId, key: &str, parameter: bool) -> Option<f64> {
    match control_for(device, key, parameter) {
        ControlRef::Argument(field) => app.project().field(&field).map(|view| view.value),
        ControlRef::EventArgument { event, .. } => {
            let (_, definition) = app.project().tracks[0].event_def(&event)?;
            definition.fields[0].value
        }
        _ => unreachable!(),
    }
}

/// Every kind of argument is lit, learns a source of either kind, and then the source moves it,
/// on the collapsed face and the expanded one alike. A Text argument is not lit.
fn every_argument_maps_and_drives(parameter: bool) {
    for expanded in [false, true] {
        for mode in [Mode::Midi, Mode::Key] {
            let (mut app, device) = learned_app(expanded, mode);
            let switch = if mode == Mode::Key {
                Action::ToggleKey
            } else {
                Action::Toggle
            };
            let found = declared(&app);
            for (n, (key, _)) in entries(parameter).into_iter().enumerate() {
                let what = format!("{key} (expanded {expanded}, {mode:?}, parameter {parameter})");
                let control = control_for(device, key, parameter);
                if key == "string_text" {
                    assert!(!found.contains(&control), "{what} is not lit");
                    continue;
                }
                assert!(found.contains(&control), "{what} is lit");
                let before = app.project().mappings.len();
                act(&mut app, Action::Pick(control));
                let letter = ((b'a' + n as u8) as char).to_string();
                let k = iced::keyboard::Key::Character(letter.as_str().into());
                let none = iced::keyboard::Modifiers::empty();
                let press = |app: &mut App| match mode {
                    Mode::Midi => send(app, &[0xb0, 20 + n as u8, 127]),
                    Mode::Key => {
                        super::keys::input(app, &k, &k, none, true, false);
                        super::keys::release_all(app);
                    }
                };
                match mode {
                    Mode::Midi => send(&mut app, &[0xb0, 20 + n as u8, 1]),
                    Mode::Key => press(&mut app),
                }
                assert_eq!(app.project().mappings.len(), before + 1, "{what} learns");
                assert_eq!(app.status.message(), None, "{what} learns without an error");
                let at = held(&app, device, key, parameter);
                act(&mut app, switch.clone());
                press(&mut app);
                let moved = held(&app, device, key, parameter);
                assert_ne!(moved, at, "{what} moves when its source arrives");
                act(&mut app, switch.clone());
            }
        }
    }
}

#[test]
fn every_parameter_argument_maps_and_drives_in_both_modes() {
    every_argument_maps_and_drives(true);
}

#[test]
fn every_event_argument_maps_and_drives_in_both_modes() {
    every_argument_maps_and_drives(false);
}

/// An event offers its Bang and one control per non-Text argument. A learned argument sets the
/// face's value and sends no OSC; the Bang is a separate mapping.
#[test]
fn an_event_offers_its_bang_and_its_arguments() {
    for expanded in [false, true] {
        let (app, device) = learned_app(expanded, Mode::Midi);
        let found = declared(&app);
        for (key, _) in entries(false) {
            let bang = ControlRef::Event {
                track: TrackId(1),
                event: EventRef {
                    device,
                    key: EntryKey(format!("e_{key}")),
                },
            };
            assert!(found.contains(&bang), "{key} has a lit Bang");
        }
        let event_arguments = found
            .iter()
            .filter(|control| matches!(control, ControlRef::EventArgument { .. }))
            .count();
        assert_eq!(
            event_arguments,
            entries(false)
                .iter()
                .filter(|(key, _)| *key != "string_text")
                .count(),
            "one per non-Text argument (expanded {expanded})"
        );
        let chain_arguments = found
            .iter()
            .filter(|control| {
                matches!(
                    control,
                    ControlRef::Argument(FieldRef { parameter, .. })
                        if matches!(parameter.device, DeviceRef::Chain(_))
                )
            })
            .count();
        assert_eq!(
            chain_arguments,
            entries(true).len(),
            "the parameters' arguments stay lit, one per parameter"
        );
    }
}

/// A multi-argument parameter's row reads like an event argument's: the parameter's name, then
/// the argument's, so two parameters each with an x and a y no longer share their labels. A
/// one-argument parameter keeps its own name.
#[test]
fn a_multi_argument_parameter_row_is_labelled_by_parameter_and_argument() {
    let mut project = project(false);
    let device = project.tracks[0].devices[0].target_mut().unwrap();
    let xy = ["x", "y"].map(|k| {
        let mut field = FieldDef::new(FieldKey(k.into()), FieldType::Float, true);
        field.name = if k == "y" { "y".into() } else { String::new() };
        field
    });
    device.parameters.push(ParameterDef {
        key: EntryKey("position".into()),
        name: "Position".into(),
        path: OscAddress::Relative("/position".into()),
        fields: xy.to_vec(),
        send_mode: SendMode::Continuous,
    });
    let control = ControlRef::Argument(FieldRef {
        parameter: ParameterRef {
            track: TrackId(1),
            device: DeviceRef::Chain(DeviceId(1)),
            key: ParameterKey("position".into()),
        },
        field: FieldKey("y".into()),
    });
    let described = describe_control(&project, &control).unwrap();
    assert_eq!(described.label, "Position y");
    assert_eq!(
        described.path, "Track / Target",
        "the parameter is not in the path"
    );
    let first = ControlRef::Argument(FieldRef {
        field: FieldKey("x".into()),
        ..match control {
            ControlRef::Argument(field) => field,
            _ => unreachable!(),
        }
    });
    assert_eq!(
        describe_control(&project, &first).unwrap().label,
        "Position Arg 1"
    );
    // A parameter with one argument keeps its own name.
    let single = ControlRef::Argument(param(DeviceId(1), "float_slider"));
    let label = describe_control(&project, &single).unwrap().label;
    assert_eq!(label, "/p_float_slider");
}
