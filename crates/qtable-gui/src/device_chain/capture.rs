//! Unattended chain captures through ordinary app actions, without desktop input.
use super::*;
use qtable_engine::model::{NewDevice, TrackId};

pub(crate) fn capture(app: &mut App) {
    let Ok(mode) = std::env::var("QTABLE_CAPTURE_CHAIN") else {
        return;
    };
    let Some(track) = shown_track(app) else {
        return;
    };
    if mode.starts_with("face") {
        return capture_face(app, track, &mode);
    }
    if let Some(kind) = mode.strip_prefix("args-") {
        return capture_args(app, track, kind);
    }
    if let Some(device) = mode
        .strip_prefix("select-entry-")
        .and_then(|n| n.parse::<usize>().ok())
        .and_then(|n| Some((n, app.project().track(track)?.devices.first()?.id)))
    {
        let keys = target_face::target_keys(app, device.1);
        if let Some(key) = keys.get(device.0) {
            let _ = target_face::update(app, target_face::Action::Select(device.1, key.clone()));
        }
        return;
    }
    while app.project().track(track).unwrap().devices.len() < 2 {
        crate::edit_actions::run_edit(
            app,
            Command::Edit(ProjectEdit::AddDevice {
                track,
                kind: NewDevice::Target,
                before: None,
            }),
        );
    }
    reconcile(app);
    let devices: Vec<_> = app
        .project()
        .track(track)
        .unwrap()
        .devices
        .iter()
        .map(|d| d.id)
        .collect();
    let _ = update(app, Action::Select(devices[0]));
    let _ = update(app, Action::Toggle(devices[1]));
    if mode == "focus-activator" {
        // Focus the first device's Activator, as a click on it would.
        let _ = crate::update::update(
            app,
            Message::ControlFocused(crate::drafts::ControlId::action(
                View::Track,
                "device-on",
                crate::control_focus::ControlOwner::Device(devices[0]),
            )),
        );
    } else if mode == "menu" {
        let _ = update(app, Action::Menu(devices[0], Point::new(70.0, 450.0)));
    } else if mode == "drag" {
        let _ = update(app, Action::Begin(devices[1], Point::ORIGIN));
        drag_cursor(app, Point::new(10.0, 0.0));
        let _ = update(app, Action::Over(Some(Some(devices[0]))));
    }
}

/// Hold an illustrative insertion marker after native window activation events.
pub(crate) fn hold_drag(app: &mut App) {
    if std::env::var("QTABLE_CAPTURE_CHAIN").as_deref() != Ok("drag") {
        return;
    }
    let Some(track) = shown_track(app).and_then(|id| app.project().track(id)) else {
        return;
    };
    if track.devices.len() >= 2 {
        app.device_chain.drag = Some(Drag {
            device: track.devices[1].id,
            origin: Point::ORIGIN,
            moved: true,
            over: Some(Some(track.devices[0].id)),
        });
    }
}

/// `face-event` or `face-parameter`: two parameters (one value moved) beside migrated events.
/// `face-bool-off` / `face-bool-on` add both Bool states and select the named one.
fn capture_face(app: &mut App, track: TrackId, mode: &str) {
    use super::target_face::{self, Action as Face};
    use qtable_engine::edit::TargetEdit;
    use qtable_engine::model::{FieldType, OscAddress};
    let Some(device) = app
        .project()
        .track(track)
        .and_then(|t| t.devices.first())
        .map(|d| d.id)
    else {
        return;
    };
    for (path, name, kind) in [
        ("/opacity", "Opacity", FieldType::Float),
        ("/speed", "", FieldType::Int),
    ] {
        let _ = target_face::update(app, Face::Add(device, true));
        let key = target_face::target_keys(app, device).pop().unwrap();
        for edit in [
            TargetEdit::SetEntryPath {
                key: key.clone(),
                path: OscAddress::Relative(path.into()),
            },
            TargetEdit::SetName {
                key: key.clone(),
                name: name.into(),
            },
            TargetEdit::SetFieldType {
                field: qtable_engine::model::FieldKey("f1".into()),
                entry: key.clone(),
                value_type: kind,
            },
        ] {
            crate::edit_actions::run_edit(
                app,
                Command::Edit(ProjectEdit::EditTarget { device, edit }),
            );
        }
        if kind == FieldType::Float {
            crate::edit_actions::run_edit(
                app,
                Command::Edit(ProjectEdit::set_value(
                    qtable_engine::model::ParameterRef {
                        track,
                        device: qtable_engine::model::DeviceRef::Chain(device),
                        key: qtable_engine::model::ParameterKey(key.0),
                    },
                    0.62,
                )),
            );
        }
    }
    if mode.starts_with("face-bool-") {
        for (name, on) in [("Bool Off", false), ("Bool On", true)] {
            let _ = target_face::update(app, Face::Add(device, true));
            let key = target_face::target_keys(app, device).pop().unwrap();
            let _ = target_face::update(
                app,
                Face::Definition(
                    device,
                    TargetEdit::SetFieldType {
                        entry: key.clone(),
                        field: qtable_engine::model::FieldKey("f1".into()),
                        value_type: FieldType::Bool,
                    },
                ),
            );
            crate::edit_actions::run_edit(
                app,
                Command::Edit(ProjectEdit::EditTarget {
                    device,
                    edit: TargetEdit::SetName {
                        key: key.clone(),
                        name: name.into(),
                    },
                }),
            );
            if on {
                crate::edit_actions::run_edit(
                    app,
                    Command::Edit(ProjectEdit::set_value(
                        qtable_engine::model::ParameterRef {
                            track,
                            device: qtable_engine::model::DeviceRef::Chain(device),
                            key: qtable_engine::model::ParameterKey(key.0),
                        },
                        1.0,
                    )),
                );
            }
        }
    }
    if matches!(mode, "face-expanded" | "face-collapsed" | "face-dial") {
        let key = target_face::target_keys(app, device).pop().unwrap();
        for edit in [
            TargetEdit::SetFieldControl {
                entry: key.clone(),
                field: qtable_engine::model::FieldKey("f1".into()),
                control: qtable_engine::model::Control::NumberBox,
            },
            TargetEdit::AddField {
                entry: key.clone(),
                value_type: FieldType::Bool,
            },
            TargetEdit::AddField {
                entry: key.clone(),
                value_type: FieldType::String,
            },
        ] {
            let _ = target_face::update(app, Face::Definition(device, edit));
        }
        if mode == "face-dial" {
            let _ = target_face::update(app, Face::Add(device, true));
            let key = target_face::target_keys(app, device).pop().unwrap();
            for edit in [
                TargetEdit::SetName {
                    key: key.clone(),
                    name: "Point".into(),
                },
                TargetEdit::AddField {
                    entry: key.clone(),
                    value_type: FieldType::Int,
                },
            ] {
                let _ = target_face::update(app, Face::Definition(device, edit));
            }
            for (field, name, value) in [("f1", "X", 0.25), ("f2", "Y", 1.0)] {
                for edit in [
                    TargetEdit::SetFieldControl {
                        entry: key.clone(),
                        field: qtable_engine::model::FieldKey(field.into()),
                        control: qtable_engine::model::Control::Dial,
                    },
                    TargetEdit::SetFieldName {
                        entry: key.clone(),
                        field: qtable_engine::model::FieldKey(field.into()),
                        name: name.into(),
                    },
                ] {
                    let _ = target_face::update(app, Face::Definition(device, edit));
                }
                let _ = target_face::update(
                    app,
                    Face::Value(
                        target_face::FaceField {
                            device,
                            entry: key.clone(),
                            field: qtable_engine::model::FieldKey(field.into()),
                            parameter: true,
                        },
                        value,
                        true,
                    ),
                );
            }
        }
        crate::edit_actions::run_edit(
            app,
            Command::Edit(ProjectEdit::SetDeviceExpanded {
                device,
                expanded: mode == "face-expanded",
            }),
        );
    }
    let keys = target_face::target_keys(app, device);
    let pick = match mode {
        "face-parameter" | "face-bool-off" => keys.len() - 2,
        "face-bool-on" => keys.len() - 1,
        _ => 0,
    };
    let _ = target_face::update(app, Face::Select(device, keys[pick].clone()));
}

/// `args-<event|param>-<type>[-<control>][-ranged]`: a new entry on the track's first Target
/// with one argument of that type and control, selected so the argument inspector shows it.
/// Ranges: `-ranged` switches an event's Range on; Slider and Dial arrive ranged anyway.
fn capture_args(app: &mut App, track: TrackId, kind: &str) {
    use super::target_face::{self, Action as Face};
    use qtable_engine::edit::TargetEdit;
    use qtable_engine::model::{Control, FieldKey, FieldType};
    let Some(device) = app
        .project()
        .track(track)
        .and_then(|t| t.devices.first())
        .map(|d| d.id)
    else {
        return;
    };
    let parts: Vec<&str> = kind.split('-').collect();
    let parameter = parts.first() == Some(&"param");
    let keys = target_face::target_keys(app, device);
    // A parameter reuses the fixture's one-argument Opacity (entry 1), so the face keeps its width and
    // the inspector stays in the window; an event is added.
    let key = if parameter {
        keys[1].clone()
    } else {
        let _ = target_face::update(app, Face::Add(device, false));
        // Events list before parameters, so the new entry isn't last: take the one that wasn't there.
        target_face::target_keys(app, device)
            .into_iter()
            .find(|k| !keys.contains(k))
            .unwrap()
    };
    // An event's new argument is f1; Opacity's one argument has its own key.
    let field = if parameter {
        target_face::target(app, device)
            .and_then(|(_, t)| target_face::entry(t, &key))
            .and_then(|e| target_face::entry_fields(&e).first().map(|f| f.key.clone()))
            .unwrap_or_else(|| FieldKey("f1".into()))
    } else {
        FieldKey("f1".into())
    };
    let value_type = match parts.get(1).copied() {
        Some("int") => FieldType::Int,
        Some("bool") => FieldType::Bool,
        Some("string") => FieldType::String,
        _ => FieldType::Float,
    };
    let mut edits = vec![];
    if !parameter {
        edits.push(TargetEdit::AddField {
            entry: key.clone(),
            value_type,
        });
    } else {
        edits.push(TargetEdit::SetFieldType {
            entry: key.clone(),
            field: field.clone(),
            value_type,
        });
    }
    let control = match parts.get(2).copied() {
        Some("slider") => Some(Control::Slider),
        Some("dial") => Some(Control::Dial),
        Some("enum") => Some(Control::Enum),
        Some("toggle") => Some(Control::Toggle),
        Some("number") => Some(Control::NumberBox),
        _ => None,
    };
    if parts.last() == Some(&"ranged") {
        edits.push(TargetEdit::SetFieldRange {
            entry: key.clone(),
            field: field.clone(),
            range: value_type.default_range(),
        });
    }
    if let Some(control) = control {
        edits.push(TargetEdit::SetFieldControl {
            entry: key.clone(),
            field: field.clone(),
            control,
        });
    }
    for edit in edits {
        let _ = target_face::update(app, Face::Definition(device, edit));
    }
    let _ = target_face::update(app, Face::Select(device, key));
}
