//! Target definitions and all dependent hits change in one project edit.
use super::{EditReport, lanes};
use crate::model::*;

#[derive(Clone, Debug, PartialEq)]
pub enum TargetEdit {
    SetPath(String),
    AddEvent {
        path: OscAddress,
        name: String,
    },
    AddParameter {
        path: OscAddress,
        name: String,
    },
    RemoveEntries {
        keys: Vec<EntryKey>,
    },
    SetName {
        key: EntryKey,
        name: String,
    },
    SetEntryPath {
        key: EntryKey,
        path: OscAddress,
    },
    AddField {
        entry: EntryKey,
        value_type: FieldType,
    },
    RemoveField {
        entry: EntryKey,
        field: FieldKey,
    },
    MoveField {
        entry: EntryKey,
        field: FieldKey,
        to: usize,
    },
    SetFieldName {
        entry: EntryKey,
        field: FieldKey,
        name: String,
    },
    SetFieldType {
        entry: EntryKey,
        field: FieldKey,
        value_type: FieldType,
    },
    SetFieldControl {
        entry: EntryKey,
        field: FieldKey,
        control: Control,
    },
    SetFieldRange {
        entry: EntryKey,
        field: FieldKey,
        range: Option<[f64; 2]>,
    },
    SetFieldOptions {
        entry: EntryKey,
        field: FieldKey,
        options: Vec<EnumOption>,
    },
    SetFieldDefault {
        entry: EntryKey,
        field: FieldKey,
        default: OscArgument,
    },
    SetFieldThreshold {
        entry: EntryKey,
        field: FieldKey,
        threshold: f64,
    },
    /// Set an event field's current value, which a bang sends. The default stays as it is.
    SetFieldValue {
        entry: EntryKey,
        field: FieldKey,
        value: OscArgument,
    },
    SetParameterSendMode {
        key: EntryKey,
        mode: SendMode,
    },
}

fn err(message: &str) -> ModelError {
    ModelError(message.into())
}

fn parameter<'a>(
    target: &'a mut TargetDevice,
    key: &EntryKey,
) -> Result<&'a mut ParameterDef, ModelError> {
    target
        .parameters
        .iter_mut()
        .find(|p| &p.key == key)
        .ok_or_else(|| err("parameter not found"))
}

pub(super) fn apply(
    project: &mut Project,
    device: DeviceId,
    edit: TargetEdit,
) -> Result<EditReport, ModelError> {
    let track_id = project
        .device(device)
        .ok_or_else(|| err("device not found"))?
        .0
        .id;
    let track = project.track_mut(track_id).unwrap();
    let index = track.device_index(device).unwrap();
    let target = track.devices[index]
        .target_mut()
        .ok_or_else(|| err("device is not a Target"))?;
    let mut conform = None;
    let mut report = EditReport::default();
    match edit {
        TargetEdit::SetPath(text) => target.path = Path::new(lead_with_slash(text))?,
        TargetEdit::AddEvent { path: piece, name } => {
            let path = piece.normalised()?;
            target.events.push(EventDef {
                key: target.fresh_key(&path),
                path,
                name,
                fields: vec![],
            });
        }
        TargetEdit::AddParameter { path: piece, name } => {
            let path = piece.normalised()?;
            target.parameters.push(ParameterDef {
                key: target.fresh_key(&path),
                path,
                name,
                fields: vec![FieldDef::new(FieldKey("f1".into()), FieldType::Float, true)],
                send_mode: SendMode::Continuous,
            });
        }
        TargetEdit::RemoveEntries { keys } => {
            if keys.is_empty() {
                return Err(err("no entries selected"));
            }
            for key in &keys {
                if !target.events.iter().any(|e| &e.key == key)
                    && !target.parameters.iter().any(|p| &p.key == key)
                {
                    return Err(err("entry not found"));
                }
            }
            target.events.retain(|e| !keys.contains(&e.key));
            target.parameters.retain(|p| !keys.contains(&p.key));
            report = lanes::remove_matching(track, |r| r.device == device && keys.contains(&r.key));
        }
        TargetEdit::SetName { key, name } => {
            if let Some(e) = target.events.iter_mut().find(|e| e.key == key) {
                e.name = name;
            } else {
                parameter(target, &key)?.name = name;
            }
        }
        TargetEdit::SetEntryPath { key, path: piece } => {
            let path = piece.normalised()?;
            if let Some(e) = target.events.iter_mut().find(|e| e.key == key) {
                e.path = path;
            } else {
                parameter(target, &key)?.path = path;
            }
        }
        TargetEdit::SetParameterSendMode { key, mode } => parameter(target, &key)?.send_mode = mode,
        edit => {
            let entry = match &edit {
                TargetEdit::AddField { entry, .. }
                | TargetEdit::RemoveField { entry, .. }
                | TargetEdit::MoveField { entry, .. }
                | TargetEdit::SetFieldName { entry, .. }
                | TargetEdit::SetFieldType { entry, .. }
                | TargetEdit::SetFieldControl { entry, .. }
                | TargetEdit::SetFieldRange { entry, .. }
                | TargetEdit::SetFieldOptions { entry, .. }
                | TargetEdit::SetFieldDefault { entry, .. }
                | TargetEdit::SetFieldValue { entry, .. }
                | TargetEdit::SetFieldThreshold { entry, .. } => entry.clone(),
                _ => unreachable!(),
            };
            let (fields, is_parameter) =
                if let Some(e) = target.events.iter_mut().find(|e| e.key == entry) {
                    (&mut e.fields, false)
                } else {
                    (&mut parameter(target, &entry)?.fields, true)
                };
            let before: Vec<_> = fields.iter().map(|f| f.key.clone()).collect();
            match edit {
                TargetEdit::AddField { value_type, .. } => {
                    if fields.len() >= FieldDef::MAX_ARGUMENTS {
                        return Err(err(&format!(
                            "cannot add arguments beyond {}",
                            FieldDef::MAX_ARGUMENTS
                        )));
                    }
                    fields.push(FieldDef::new(
                        FieldKey::fresh(fields, ""),
                        value_type,
                        is_parameter,
                    ));
                }
                TargetEdit::RemoveField { field, .. } => {
                    if is_parameter && fields.len() == 1 {
                        return Err(err("cannot remove the last parameter argument"));
                    }
                    let index = fields
                        .iter()
                        .position(|f| f.key == field)
                        .ok_or_else(|| err("argument not found"))?;
                    fields.remove(index);
                }
                TargetEdit::MoveField { field, to, .. } => {
                    if to >= fields.len() {
                        return Err(err("argument position out of range"));
                    }
                    let index = fields
                        .iter()
                        .position(|f| f.key == field)
                        .ok_or_else(|| err("argument not found"))?;
                    let field = fields.remove(index);
                    fields.insert(to, field);
                }
                edit => {
                    let key = match &edit {
                        TargetEdit::SetFieldName { field, .. }
                        | TargetEdit::SetFieldType { field, .. }
                        | TargetEdit::SetFieldControl { field, .. }
                        | TargetEdit::SetFieldRange { field, .. }
                        | TargetEdit::SetFieldOptions { field, .. }
                        | TargetEdit::SetFieldDefault { field, .. }
                        | TargetEdit::SetFieldValue { field, .. }
                        | TargetEdit::SetFieldThreshold { field, .. } => field,
                        _ => unreachable!(),
                    };
                    let f = fields
                        .iter_mut()
                        .find(|f| &f.key == key)
                        .ok_or_else(|| err("argument not found"))?;
                    match edit {
                        TargetEdit::SetFieldName { name, .. } => f.name = name,
                        TargetEdit::SetFieldType { value_type, .. } => {
                            retype(f, value_type, is_parameter)?
                        }
                        TargetEdit::SetFieldControl { control, .. } => {
                            let current = if f.control == Control::Enum {
                                current_argument(f).as_ref().map(argument_number)
                            } else {
                                f.value
                            };
                            if control == Control::Enum && f.value_type == FieldType::Bool {
                                return Err(err(
                                    "control is not allowed for this field type and range",
                                ));
                            }
                            f.control = control;
                            if matches!(control, Control::Slider | Control::Dial)
                                && f.range.is_none()
                            {
                                // A Slider or Dial needs a range: switch one on.
                                f.range = f.value_type.default_range();
                            }
                            if control == Control::Enum && f.options.is_empty() {
                                f.options = (1..=2)
                                    .filter_map(|n| EnumOption::numbered(f.value_type, n))
                                    .collect();
                                f.default = f.options[0].value.clone();
                                f.value = f.value.map(|_| 0.0);
                            }
                            if control != Control::Enum {
                                f.options.clear();
                                if control == Control::Toggle {
                                    f.default = toggle_default(
                                        f.value_type,
                                        argument_number(&f.default) >= 0.5,
                                    );
                                    f.value = current.map(|v| f64::from(v >= 0.5));
                                } else if f.value_type != FieldType::String {
                                    let spec = f.spec();
                                    f.default = numeric_default(
                                        f.value_type,
                                        argument_number(&f.default).clamp(spec.min, spec.max),
                                    );
                                    f.value = current
                                        .map(|v| spec.check(v.clamp(spec.min, spec.max)))
                                        .transpose()
                                        .map_err(ModelError)?;
                                }
                            }
                        }
                        TargetEdit::SetFieldRange { range, .. } => {
                            f.range = range;
                            if range.is_none()
                                && matches!(f.control, Control::Slider | Control::Dial)
                            {
                                // Without a range there is nothing for the bar or knob to span.
                                f.control = Control::NumberBox;
                            }
                            if let Some([min, max]) = range {
                                if !min.is_finite() || !max.is_finite() || min >= max {
                                    return Err(err(
                                        "argument range must be finite with min less than max",
                                    ));
                                }
                                if !matches!(f.control, Control::Enum | Control::Toggle) {
                                    let clamp = |v: f64| v.clamp(min, max);
                                    f.default =
                                        numeric_default(f.value_type, clamp(f.default_value()));
                                    f.value = f.value.map(clamp);
                                }
                            }
                        }
                        TargetEdit::SetFieldOptions { options, .. } => {
                            let current = current_argument(f);
                            let previous = f.default.clone();
                            f.options = options;
                            if !f.options.iter().any(|o| o.value == previous)
                                && let Some(o) = f.options.first()
                            {
                                f.default = o.value.clone();
                            }
                            f.value = current.map(|value| {
                                f.options
                                    .iter()
                                    .position(|o| o.value == value)
                                    .map(|i| i as f64)
                                    .unwrap_or_else(|| f.default_value())
                            });
                        }
                        TargetEdit::SetFieldDefault { default, .. } => f.default = default,
                        TargetEdit::SetFieldValue { value, .. } => {
                            if is_parameter {
                                return Err(err("parameter values are set with SetParameter"));
                            }
                            require_type(f, &value)?;
                            if f.control == Control::Text {
                                let OscArgument::String(text) = value else {
                                    unreachable!()
                                };
                                f.text = Some(text);
                            } else {
                                let number = match f.control {
                                    Control::Enum => f
                                        .options
                                        .iter()
                                        .position(|o| o.value == value)
                                        .ok_or_else(|| err("value is not one of the options"))?
                                        as f64,
                                    _ => argument_number(&value),
                                };
                                f.value = Some(f.spec().check(number).map_err(ModelError)?);
                            }
                        }
                        TargetEdit::SetFieldThreshold { threshold, .. } => f.threshold = threshold,
                        _ => unreachable!(),
                    }
                }
            }
            for f in fields.iter_mut() {
                f.seed_current();
            }
            FieldDef::validate_fields(fields, is_parameter)?;
            if !is_parameter {
                conform = Some((entry, before, fields.clone()));
            }
        }
    }
    if let Some((key, before, fields)) = conform {
        let signature: Vec<_> = fields.iter().map(|f| f.value_type).collect();
        let mut converted = 0;
        for clip in track.slots.iter_mut().filter_map(|s| s.clip.as_mut()) {
            for hit in clip
                .events
                .iter_mut()
                .filter(|e| e.definition.device == device && e.definition.key == key)
            {
                let old = std::mem::take(&mut hit.arguments);
                hit.arguments = fields
                    .iter()
                    .map(|f| {
                        before
                            .iter()
                            .position(|k| *k == f.key)
                            .and_then(|i| old.get(i))
                            .cloned()
                            .map(|v| v.convert(f.value_type))
                            .unwrap_or_else(|| f.default.clone())
                    })
                    .collect();
                converted += usize::from(
                    before.iter().any(|k| !fields.iter().any(|f| f.key == *k))
                        || fields.iter().any(|f| {
                            before
                                .iter()
                                .position(|k| *k == f.key)
                                .and_then(|i| old.get(i))
                                .is_some_and(|v| FieldType::of(v) != f.value_type)
                        }),
                );
            }
        }
        report.converted(converted, &signature);
    }
    Ok(report)
}

fn require_type(f: &FieldDef, value: &OscArgument) -> Result<(), ModelError> {
    if FieldType::of(value) == f.value_type {
        Ok(())
    } else {
        Err(err("value must match the argument's wire type"))
    }
}
fn numeric_default(kind: FieldType, value: f64) -> OscArgument {
    match kind {
        FieldType::Int => OscArgument::Int(value as i32),
        FieldType::Float => OscArgument::Float(value as f32),
        FieldType::Bool => OscArgument::Bool(value >= 0.5),
        FieldType::String => OscArgument::String(value.to_string()),
    }
}
fn toggle_default(kind: FieldType, on: bool) -> OscArgument {
    numeric_default(kind, f64::from(on))
}
fn argument_number(value: &OscArgument) -> f64 {
    match value {
        OscArgument::Float(v) => f64::from(*v),
        OscArgument::Int(v) => f64::from(*v),
        OscArgument::Bool(v) => f64::from(*v),
        OscArgument::String(_) => 0.0,
    }
}
fn current_argument(f: &FieldDef) -> Option<OscArgument> {
    f.value.and_then(|value| {
        if f.control == Control::Enum {
            f.options.get(value as usize).map(|o| o.value.clone())
        } else {
            Some(numeric_default(f.value_type, value))
        }
    })
}
fn retype(f: &mut FieldDef, kind: FieldType, parameter: bool) -> Result<(), ModelError> {
    let range = match kind {
        FieldType::Bool | FieldType::String => None,
        FieldType::Int => f
            .range
            .map(|[a, b]| {
                let min = a.floor().max(f64::from(i32::MIN));
                let max = b.ceil().min(f64::from(i32::MAX));
                if !a.is_finite() || !b.is_finite() || min >= max {
                    return Err(err("argument range cannot convert to an Int range"));
                }
                Ok([min, max])
            })
            .transpose()?,
        FieldType::Float => f.range,
    };
    let current = current_argument(f).map(|v| v.convert(kind));
    let was_enum = f.control == Control::Enum;
    f.default = f.default.clone().convert(kind);
    for option in &mut f.options {
        option.value = option.value.clone().convert(kind);
        if let OscArgument::String(v) = &option.value {
            option.label = v.clone();
        }
    }
    f.value_type = kind;
    f.range = range;
    if parameter && matches!(kind, FieldType::Float | FieldType::Int) && f.range.is_none() {
        f.range = Some([0.0, 1.0]);
    }
    if !kind
        .controls(parameter, f.range.is_some())
        .contains(&f.control)
    {
        f.control = kind.default_control(parameter, f.range.is_some());
    }
    if f.control == Control::Enum {
        if !was_enum {
            f.options = (1..=2)
                .filter_map(|n| EnumOption::numbered(kind, n))
                .collect();
            f.default = f.options[0].value.clone();
            f.value = Some(0.0);
        }
    } else {
        f.options.clear();
        // Non-enum numeric controls retain f64 precision for same-type edits.
        if kind == FieldType::Bool || was_enum {
            f.value = current.as_ref().map(argument_number);
        }
        if kind == FieldType::Int {
            f.value = f.value.map(f64::round);
        }
        if let Some([min, max]) = f.range
            && !matches!(f.control, Control::Toggle)
        {
            f.default = numeric_default(kind, f.default_value().clamp(min, max));
            f.value = f.value.map(|v| v.clamp(min, max));
        }
    }
    Ok(())
}
