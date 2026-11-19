//! The Target face, and beside it, once expanded, the editor for the entry selected on it.
//! Every change is one `EditTarget`, so nothing here owns
//! model state; the entry selection is GUI state on the chain (`State::entry`).
use crate::app::App;
use crate::drafts::{DefinitionField, DraftKey, TargetField};
use crate::focus::Focus;
use iced::Task;
use qtable_engine::edit::{EditReport, ProjectEdit, TargetEdit};
use qtable_engine::engine::Command;
use qtable_engine::model::{
    DeviceId, EntryKey, FieldDef, FieldType, OscAddress, OscArgument, ParameterDef, TargetDevice,
    Track,
};

pub(super) mod collapsed;
mod definitions;
pub(super) use collapsed::width;
pub(crate) use collapsed::{FaceField, check_exact, keyboard, reset_face};
mod view;
pub(super) use view::{EDITOR_WIDTH, view};

#[derive(Debug, Clone)]
pub(crate) enum Action {
    Fold(DeviceId),
    Value(FaceField, f64, bool),
    Exact(FaceField, String),
    CommitExact(FaceField),
    ExactFocusChecked(FaceField, String, u64, bool),
    /// A double click on a Number box: the argument's default, as one edit.
    Reset(FaceField),
    Fire(DeviceId, EntryKey),
    Select(DeviceId, EntryKey),
    /// Choose the field tab the editor shows for the selected entry.
    SelectField(DeviceId, qtable_engine::model::FieldKey),
    /// Delete the device's selected entry.
    Remove(DeviceId),
    /// Add an event, or a parameter when `true`.
    Add(DeviceId, bool),
    Draft(DraftKey, String),
    Submit(DraftKey),
    ToggleAbsolute(DeviceId),
    Definition(DeviceId, TargetEdit),
}

/// An entry of a Target, found by key.
#[derive(Clone, Copy)]
pub(super) enum Entry<'a> {
    Event(&'a qtable_engine::model::EventDef),
    Parameter(&'a ParameterDef),
}

pub(super) fn target(app: &App, id: DeviceId) -> Option<(&Track, &TargetDevice)> {
    let (track, device) = app.project().device(id)?;
    Some((track, device.target()?))
}

pub(super) fn entry<'a>(target: &'a TargetDevice, key: &EntryKey) -> Option<Entry<'a>> {
    target
        .events
        .iter()
        .find(|e| &e.key == key)
        .map(Entry::Event)
        .or_else(|| {
            target
                .parameters
                .iter()
                .find(|p| &p.key == key)
                .map(Entry::Parameter)
        })
}

/// The selected entry of a device, if the model still has it.
pub(super) fn selected(app: &App, id: DeviceId) -> Option<EntryKey> {
    let key = app.device_chain.entry.get(&id)?;
    entry(target(app, id)?.1, key).map(|_| key.clone())
}

/// The fields of an entry.
pub(super) fn entry_fields<'a>(found: &Entry<'a>) -> &'a [FieldDef] {
    match found {
        Entry::Event(e) => &e.fields,
        Entry::Parameter(p) => &p.fields,
    }
}

/// The selected entry and the field its editor tab shows: the chosen tab, else the first.
pub(super) fn edited_field(
    app: &App,
    id: DeviceId,
) -> Option<(EntryKey, qtable_engine::model::FieldKey)> {
    let key = selected(app, id)?;
    let found = entry(target(app, id)?.1, &key)?;
    let fields = entry_fields(&found);
    let chosen = app
        .device_chain
        .field
        .get(&id)
        .and_then(|k| fields.iter().find(|f| &f.key == k))
        .or_else(|| fields.first())?;
    Some((key, chosen.key.clone()))
}

fn run(app: &mut App, edit: ProjectEdit) -> Option<EditReport> {
    crate::edit_actions::run_edit_report(app, Command::Edit(edit), None)
}

fn edit(device: DeviceId, edit: TargetEdit) -> ProjectEdit {
    ProjectEdit::EditTarget { device, edit }
}

fn forget_fields(app: &mut App, id: DeviceId) {
    app.drafts.values.retain(
        |key, _| !matches!(key, DraftKey::Target(d, f) if *d == id && *f != TargetField::Path),
    );
    app.drafts.errors.retain(
        |key, _| !matches!(key, DraftKey::Target(d, f) if *d == id && *f != TargetField::Path),
    );
}

/// Selects the entry, and its device as the only selected one.
pub(super) fn select_entry(app: &mut App, id: DeviceId, key: EntryKey) {
    set_entry(app, id, key);
    app.control_focus.clear();
}

/// Shared selection state, without disturbing the face control handling this interaction.
fn set_entry(app: &mut App, id: DeviceId, key: EntryKey) {
    let Some((track, _)) = target(app, id) else {
        return;
    };
    let track = track.id;
    if app.device_chain.entry.get(&id) != Some(&key) {
        forget_fields(app, id);
        app.device_chain.field.remove(&id);
    }
    app.device_chain.entry.insert(id, key);
    app.device_chain.track = Some(track);
    app.selection.devices = vec![id];
    app.selection.device_anchor = Some(id);
    app.focus = Focus::Detail;
}

/// Expanded, selects an argument's entry and its inspector tab, as the face's controls do on a
/// press. Collapsed, nothing is selected.
pub(super) fn select_argument(
    app: &mut App,
    id: DeviceId,
    key: &EntryKey,
    field: &qtable_engine::model::FieldKey,
) {
    let expanded = app
        .project()
        .device(id)
        .is_some_and(|(_, device)| device.expanded);
    if !expanded {
        return;
    }
    if selected(app, id).as_ref() == Some(key)
        && edited_field(app, id).is_some_and(|(_, shown)| &shown == field)
    {
        return;
    }
    set_entry(app, id, key.clone());
    if app.device_chain.field.get(&id) != Some(field) {
        forget_fields(app, id);
    }
    app.device_chain.field.insert(id, field.clone());
}

/// The entry and argument behind a face control, found by the indices its control id carries.
pub(crate) fn select_face_control(
    app: &mut App,
    id: DeviceId,
    parameter: bool,
    index: usize,
    position: usize,
) {
    let Some((_, target)) = target(app, id) else {
        return;
    };
    let found = if parameter {
        target
            .parameters
            .get(index)
            .map(|p| (p.key.clone(), p.fields.get(position)))
    } else {
        target
            .events
            .get(index)
            .map(|e| (e.key.clone(), e.fields.get(position)))
    };
    if let Some((key, Some(field))) = found {
        let field = field.key.clone();
        select_argument(app, id, &key, &field);
    }
}

/// Clicking the title bar leaves the entry, so Delete acts on the device again.
pub(super) fn clear_entry(app: &mut App, id: DeviceId) {
    if app.device_chain.entry.remove(&id).is_some() {
        app.device_chain.field.remove(&id);
        forget_fields(app, id);
    }
}

/// Drops selections the model no longer has, after every update.
pub(super) fn reconcile(app: &mut App) {
    collapsed::reconcile(app);
    let ids: Vec<_> = app.device_chain.entry.keys().copied().collect();
    for id in ids {
        if selected(app, id).is_none() {
            app.device_chain.entry.remove(&id);
            app.device_chain.field.remove(&id);
            forget_fields(app, id);
        }
    }
}

fn free_path(target: &TargetDevice, base: &str) -> String {
    let used = |text: &str| {
        target
            .events
            .iter()
            .map(|e| &e.path)
            .chain(target.parameters.iter().map(|p| &p.path))
            .any(|path| path.text() == text)
    };
    let mut path = base.to_owned();
    let mut n = 2;
    while used(&path) {
        path = format!("{base}{n}");
        n += 1;
    }
    path
}

fn add(app: &mut App, id: DeviceId, parameter: bool) {
    let Some((_, target)) = target(app, id) else {
        return;
    };
    let before: Vec<_> = target
        .events
        .iter()
        .map(|e| e.key.clone())
        .chain(target.parameters.iter().map(|p| p.key.clone()))
        .collect();
    let path = OscAddress::Relative(free_path(
        target,
        if parameter { "/param" } else { "/event" },
    ));
    let kind = if parameter {
        TargetEdit::AddParameter {
            path,
            name: String::new(),
        }
    } else {
        TargetEdit::AddEvent {
            path,
            name: String::new(),
        }
    };
    if run(app, edit(id, kind)).is_none() {
        return;
    }
    let added = target_keys(app, id)
        .into_iter()
        .find(|k| !before.contains(k));
    if let Some(key) = added {
        select_entry(app, id, key);
    }
}

/// Every entry key in list order: events, then parameters.
pub(super) fn target_keys(app: &App, id: DeviceId) -> Vec<EntryKey> {
    target(app, id)
        .map(|(_, t)| {
            t.events
                .iter()
                .map(|e| e.key.clone())
                .chain(t.parameters.iter().map(|p| p.key.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Delete on a selected entry; `false` when the selection holds none, so Delete falls
/// through to the device.
pub(super) fn remove_selected(app: &mut App) -> bool {
    let Some((id, key)) = selected_of_device(app) else {
        return false;
    };
    remove_entry(app, id, key)
}

fn remove_entry(app: &mut App, id: DeviceId, key: EntryKey) -> bool {
    let Some((_, target)) = target(app, id) else {
        return false;
    };
    let name = match entry(target, &key) {
        Some(Entry::Event(e)) => e.display_name().to_owned(),
        Some(Entry::Parameter(p)) => p.display_name().to_owned(),
        None => return false,
    };
    if let Some(report) = run(app, edit(id, TargetEdit::RemoveEntries { keys: vec![key] })) {
        app.drafts.clear_inspector();
        crate::edit_actions::notify_removal(app, &report, &name);
        super::reconcile(app);
    }
    true
}

/// The one selected device and its selected entry.
pub(super) fn selected_of_device(app: &App) -> Option<(DeviceId, EntryKey)> {
    let [id] = app.selection.devices[..] else {
        return None;
    };
    if !app.project().device(id)?.1.expanded {
        return None;
    }
    Some((id, selected(app, id)?))
}

pub(super) fn move_selection(app: &mut App, delta: isize) -> bool {
    let Some((id, key)) = selected_of_device(app) else {
        return false;
    };
    let keys = target_keys(app, id);
    let Some(at) = keys.iter().position(|k| *k == key) else {
        return false;
    };
    let next = (at as isize + delta).clamp(0, keys.len() as isize - 1) as usize;
    select_entry(app, id, keys[next].clone());
    true
}

fn number(text: &str) -> Result<f64, String> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("\"{}\" is not a number", text.trim()))
}

/// Commit before a selection change can remove the exact input from the view.
pub(crate) fn commit_exact(app: &mut App) {
    if let Some((field, _)) = app.device_chain.face.exact.clone() {
        collapsed::commit(app, &field);
    }
}

pub(crate) fn update(app: &mut App, action: Action) -> Task<crate::messages::Message> {
    match action {
        Action::Fold(id) => {
            if let Some((_, device)) = app.project().device(id) {
                let expanded = !device.expanded;
                run(
                    app,
                    ProjectEdit::SetDeviceExpanded {
                        device: id,
                        expanded,
                    },
                );
                collapsed::reset_face(app);
                app.control_focus.clear();
            }
        }
        Action::Value(field, value, release) => collapsed::value(app, field, value, release),
        Action::Exact(field, text) => {
            // Only the opening character on a canvas control creates a new input.
            // Existing text inputs already have focus and a cursor; a delayed focus
            // task could otherwise blur them after an arrow nudge commits the draft.
            let opening_input = app
                .device_chain
                .face
                .exact
                .as_ref()
                .is_none_or(|(active, _)| active != &field)
                && collapsed::fields(app, &field).is_some_and(|definition| {
                    matches!(
                        definition.control,
                        qtable_engine::model::Control::Slider | qtable_engine::model::Control::Dial
                    )
                });
            app.selection.devices = vec![field.device];
            app.selection.device_anchor = Some(field.device);
            app.focus = Focus::Detail;
            select_argument(app, field.device, &field.entry, &field.field);
            app.device_chain.face.exact = Some((field, text));
            app.device_chain.face.error = false;
            // Exact entry already contains the character that opened it. Continue typing at
            // its end instead of replacing it with the Number input's focus selection.
            if opening_input {
                let id = iced::widget::Id::new("target-face-exact");
                return iced::widget::operation::focus(id.clone())
                    .chain(iced::widget::operation::move_cursor_to_end(id));
            }
        }
        Action::CommitExact(field) => collapsed::commit(app, &field),
        Action::ExactFocusChecked(field, text, generation, focused) => {
            if !focused
                && app.project_generation == generation
                && app.device_chain.face.exact.as_ref() == Some(&(field.clone(), text))
            {
                crate::gesture_steps::isolated(app, |app| collapsed::commit(app, &field));
            }
        }
        Action::Reset(field) => collapsed::reset(app, field),
        Action::Fire(id, key) => collapsed::fire(app, id, key),
        Action::Select(id, key) => select_entry(app, id, key),
        Action::SelectField(id, field) => {
            if app.device_chain.field.get(&id) != Some(&field) {
                forget_fields(app, id);
            }
            app.device_chain.field.insert(id, field);
            app.control_focus.clear();
        }
        Action::Remove(id) => {
            if let Some(key) = selected(app, id) {
                remove_entry(app, id, key);
            }
        }
        Action::Add(id, parameter) => add(app, id, parameter),
        Action::Draft(key, value) => {
            app.drafts.values.insert(key, value);
            app.drafts.errors.remove(&key);
        }
        Action::Submit(key) => submit(app, key),
        action => structure(app, action),
    }
    Task::none()
}

/// The dropdowns, toggles and field buttons: each is one definition edit.
fn structure(app: &mut App, action: Action) {
    let (id, edit) = match action {
        Action::ToggleAbsolute(id) => {
            let Some(key) = selected(app, id) else { return };
            let Some((_, t)) = target(app, id) else {
                return;
            };
            let path = match entry(t, &key) {
                Some(Entry::Event(e)) => e.path.clone(),
                Some(Entry::Parameter(p)) => p.path.clone(),
                None => return,
            };
            let path = match path {
                OscAddress::Relative(text) => OscAddress::Absolute(text),
                OscAddress::Absolute(text) => OscAddress::Relative(text),
            };
            (id, TargetEdit::SetEntryPath { key, path })
        }
        Action::Definition(id, edit) => (id, edit),
        _ => return,
    };
    // The tab follows the edit: a new field opens, a removed one hands over to its neighbour.
    let after = match &edit {
        TargetEdit::AddField { .. } => Some(None),
        TargetEdit::RemoveField { entry: key, field } => target(app, id)
            .and_then(|(_, t)| self::entry(t, key))
            .and_then(|found| entry_fields(&found).iter().position(|f| &f.key == field))
            .map(Some),
        _ => None,
    };
    if run(app, self::edit(id, edit)).is_some() {
        if let (Some(after), Some(key)) = (after, selected(app, id)) {
            let fields = target(app, id)
                .and_then(|(_, t)| self::entry(t, &key))
                .map(|found| entry_fields(&found));
            let next = fields.and_then(|f| match after {
                None => f.last(),
                Some(at) => f.get(at.min(f.len().saturating_sub(1))),
            });
            if let Some(next) = next {
                app.device_chain.field.insert(id, next.key.clone());
            }
        }
        collapsed::reset_face(app);
        app.control_focus.clear();
        // Definition rows may shift under their drafts.
        forget_fields(app, id);
    }
}

/// Commits a field's draft as one edit; a rejected draft stays for correction.
pub(crate) fn submit(app: &mut App, key: DraftKey) {
    let DraftKey::Target(id, field) = key else {
        return;
    };
    let Some(text) = app.drafts.values.get(&key).cloned() else {
        return;
    };
    let entry_key = selected(app, id);
    let Some((_, target)) = target(app, id) else {
        return;
    };
    let path_of = |key: &EntryKey| match entry(target, key) {
        Some(Entry::Event(e)) => Some(e.path.clone()),
        Some(Entry::Parameter(p)) => Some(p.path.clone()),
        None => None,
    };
    let built: Result<Option<ProjectEdit>, String> = match (field, &entry_key) {
        (TargetField::Path, _) => Ok(Some(edit(id, TargetEdit::SetPath(text)))),
        (TargetField::Name, Some(key)) => Ok(Some(edit(
            id,
            TargetEdit::SetName {
                key: key.clone(),
                name: text,
            },
        ))),
        (TargetField::EntryPath, Some(key)) => Ok(path_of(key).map(|old| {
            let path = match old {
                OscAddress::Relative(_) => OscAddress::Relative(text),
                OscAddress::Absolute(_) => OscAddress::Absolute(text),
            };
            edit(
                id,
                TargetEdit::SetEntryPath {
                    key: key.clone(),
                    path,
                },
            )
        })),
        (TargetField::Definition(index, property), Some(key)) => {
            let fields = match entry(target, key) {
                Some(Entry::Event(e)) => &e.fields,
                Some(Entry::Parameter(p)) => &p.fields,
                None => return,
            };
            let Some(field) = fields.get(index) else {
                return;
            };
            definition_edit(key, field, property, &text).map(|e| Some(edit(id, e)))
        }
        _ => Ok(None),
    };
    match built {
        Ok(Some(edit)) => {
            crate::edit_actions::run_edit_report(app, Command::Edit(edit), Some(key));
        }
        Ok(None) => app.drafts.forget(key),
        Err(error) => {
            app.drafts.errors.insert(key, error.clone());
            app.status.show_error(error);
        }
    }
}

#[cfg(test)]
mod tests;

fn argument(kind: FieldType, text: &str) -> Result<OscArgument, String> {
    match kind {
        FieldType::String => Ok(OscArgument::String(text.into())),
        FieldType::Bool => match text {
            "true" | "1" => Ok(OscArgument::Bool(true)),
            "false" | "0" => Ok(OscArgument::Bool(false)),
            _ => Err("Expected true or false".into()),
        },
        FieldType::Int => text
            .trim()
            .parse::<i32>()
            .map(OscArgument::Int)
            .map_err(|_| "Expected a whole number in the OSC Int range".into()),
        FieldType::Float => number(text).and_then(|v| {
            if (v as f32).is_finite() {
                Ok(OscArgument::Float(v as f32))
            } else {
                Err("Number is outside the OSC Float range".into())
            }
        }),
    }
}

fn definition_edit(
    entry: &EntryKey,
    field: &FieldDef,
    property: DefinitionField,
    text: &str,
) -> Result<TargetEdit, String> {
    let entry = entry.clone();
    let key = field.key.clone();
    Ok(match property {
        DefinitionField::Name => TargetEdit::SetFieldName {
            entry,
            field: key,
            name: text.into(),
        },
        DefinitionField::Min | DefinitionField::Max => {
            let mut range = field.range.unwrap_or([0.0, 1.0]);
            range[usize::from(property == DefinitionField::Max)] = number(text)?;
            TargetEdit::SetFieldRange {
                entry,
                field: key,
                range: Some(range),
            }
        }
        DefinitionField::Default => TargetEdit::SetFieldDefault {
            entry,
            field: key,
            default: argument(field.value_type, text)?,
        },
        DefinitionField::Threshold => TargetEdit::SetFieldThreshold {
            entry,
            field: key,
            threshold: number(text)?,
        },
        DefinitionField::OptionLabel(index) | DefinitionField::OptionValue(index) => {
            let mut options = field.options.clone();
            let option = options.get_mut(index).ok_or("Option no longer exists")?;
            if matches!(property, DefinitionField::OptionLabel(_)) {
                option.label = text.into();
                if field.value_type == FieldType::String {
                    option.value = OscArgument::String(text.into());
                }
            } else {
                option.value = argument(field.value_type, text)?;
            }
            TargetEdit::SetFieldOptions {
                entry,
                field: key,
                options,
            }
        }
    })
}
