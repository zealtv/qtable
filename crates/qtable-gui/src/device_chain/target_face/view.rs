//! The expanded Target: entry editor, generated face and argument inspector.
use super::{Action, Entry, entry, selected, target};
use crate::app::App;
use crate::control::path_field::Prefix;
use crate::control::rule::{Axis, Level, rule};
use crate::control::{Button, Dropdown, Face, PathField, SizeRole, State, Text as TextKind};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::{Control, ControlId, DraftKey, TargetField};
use crate::info::{EntryRef, Id, Subject};
use crate::messages::{EditMessage, Message};
use crate::style::detail;
use iced::widget::{Column, Space, column, container, row, text};
use iced::{Element, Length};
use qtable_engine::model::{
    Device, DeviceId, EntryKey, OscAddress, PathLevel, SendMode, TargetDevice, Track,
};

/// The combined entry editor and argument inspector width, including one divider.
/// The face sits between them, with a second divider accounted for by the device frame.
pub(in crate::device_chain) const EDITOR_WIDTH: f32 = ENTRY_WIDTH + 1.0 + INSPECTOR_WIDTH;
/// The entry editor column, padding included.
pub(super) const ENTRY_WIDTH: f32 = 270.0;
/// The argument inspector: its pairs of 56 px labels and 72 px choices need a 260 px row.
pub(super) const INSPECTOR_WIDTH: f32 = 284.0;
/// Label column of the entry editor; wide enough for "Target Path" and "Param Path".
const ENTRY_LABEL_WIDTH: f32 = 64.0;

pub(super) fn msg(action: Action) -> Message {
    Message::Device(crate::device_chain::Action::Target(action))
}

/// A 1 px vertical rule between two areas of the expanded Target.
fn divider<'a>(app: &App) -> Element<'a, Message> {
    rule(&app.theme, Axis::Vertical, Level::Deep)
}

pub(in crate::device_chain) fn view<'a>(
    app: &'a App,
    device: &'a Device,
    location: View,
) -> Element<'a, Message> {
    let face = super::collapsed::view(app, device, location);
    if !device.expanded {
        return face;
    }
    let Some((track, target)) = target(app, device.id) else {
        return face;
    };
    let chosen = selected(app, device.id);
    let found = chosen
        .as_ref()
        .and_then(|k| entry(target, k).map(|e| (k, e)));
    let entries = entries(app, device, track, target, found.as_ref(), location);
    let inspector = match found {
        Some((key, found)) => inspector(app, device.id, key, found, location),
        None => Space::new().into(),
    };
    row![
        container(entries)
            .width(ENTRY_WIDTH)
            .height(Length::Fill)
            .padding(detail::PANEL_INSET),
        divider(app),
        container(face).width(super::collapsed::width(target)),
        divider(app),
        container(inspector)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(detail::PANEL_INSET),
    ]
    .into()
}

/// The entry editor column: the Target Path, the adds, then the selected entry's Name, Path
/// and Send Mode, with Delete at the foot. Inline labels keep six rows in the 153 px
/// between the insets; the width leaves room for the parent path beside Absolute.
fn entries<'a>(
    app: &'a App,
    device: &'a Device,
    track: &'a Track,
    target: &'a TargetDevice,
    found: Option<&(&EntryKey, Entry<'a>)>,
    location: View,
) -> Element<'a, Message> {
    let id = device.id;
    let controls = app.controls();
    let state = State::default();
    let path_key = DraftKey::Target(id, TargetField::Path);
    let path_text = app
        .drafts
        .values
        .get(&path_key)
        .cloned()
        .unwrap_or_else(|| target.path.as_str().to_owned());
    let path = controls.inline(
        "Target Path",
        ENTRY_LABEL_WIDTH,
        state,
        controls.path(
            Control::new(
                ControlId::field(location, path_key),
                Id::TargetPath,
                PathField {
                    placeholder: "",
                    prefix: Prefix::of(app.project(), track.id, PathLevel::Under, false),
                    absolute: None,
                    subject: Subject::TargetPath(track.id, id),
                },
            )
            .invalid(app.drafts.errors.contains_key(&path_key)),
            &path_text,
            move |v| msg(Action::Draft(path_key, v)),
            msg(Action::Submit(path_key)),
            Message::Edit(EditMessage::FocusField(path_key)),
            None,
        ),
    );
    let button = |label: &'static str, name: &'static str, help: Id, action: Action, on: bool| {
        controls.button(
            Control::new(
                ControlId::action(location, name, ControlOwner::Device(id)),
                help,
                Button {
                    face: Face::Text(label.into()),
                    latched: false,
                },
            )
            .role(SizeRole::Fill)
            .enabled(on),
            msg(action),
        )
    };
    let adds = row![
        button(
            "+ Event",
            "target-add-event",
            Id::TargetAddEvent,
            Action::Add(id, false),
            true
        ),
        button(
            "+ Param",
            "target-add-parameter",
            Id::TargetAddParameter,
            Action::Add(id, true),
            true
        ),
    ]
    .spacing(detail::CONTROL_GAP);
    let mut body = column![adds].spacing(detail::CONTROL_GAP);
    if let Some((key, found)) = found {
        body = properties(app, track, id, key, *found, location, body);
    } else {
        body = body.push(
            text("Select an event or parameter on the face, or add one")
                .size(detail::TEXT_SIZE)
                .color(app.theme.text_disabled),
        );
    }
    let attached = found.is_none_or(|(_, entry)| match entry {
        Entry::Event(e) => matches!(e.path, OscAddress::Relative(_)),
        Entry::Parameter(p) => matches!(p.path, OscAddress::Relative(_)),
    });
    let dependants = body.push(Space::new().height(Length::Fill)).push(button(
        "Delete",
        "target-delete-entry",
        Id::TargetDeleteEntry,
        Action::Remove(id),
        found.is_some(),
    ));
    column![
        path,
        crate::control::device_layout::dependants(app.theme, attached, dependants)
    ]
    .spacing(detail::CONTROL_GAP)
    .into()
}

/// Pushes the selected entry's Name, Path and (for a parameter) Send Mode onto `body`.
#[allow(clippy::too_many_arguments)]
fn properties<'a>(
    app: &'a App,
    track: &'a Track,
    id: DeviceId,
    key: &EntryKey,
    found: Entry<'a>,
    location: View,
    mut body: Column<'a, Message>,
) -> Column<'a, Message> {
    let controls = app.controls();
    let state = State::default();
    let draft = |field: TargetField, stored: String| {
        let key = DraftKey::Target(id, field);
        (
            key,
            app.drafts.values.get(&key).cloned().unwrap_or(stored),
            app.drafts.errors.contains_key(&key),
        )
    };
    let (name, path, parameter) = match found {
        Entry::Event(e) => (&e.name, &e.path, None),
        Entry::Parameter(p) => (&p.name, &p.path, Some(p)),
    };
    let subject = Subject::TargetEntry(EntryRef::target(parameter.is_some(), track.id, id, key));
    let (name_key, name_text, name_bad) = draft(TargetField::Name, name.clone());
    body = body.push(
        controls.inline(
            "Name",
            ENTRY_LABEL_WIDTH,
            state,
            controls.text(
                Control::new(
                    ControlId::field(location, name_key),
                    Id::TargetName,
                    TextKind {
                        placeholder: "Same as path".into(),
                    },
                )
                .role(SizeRole::Fill)
                .invalid(name_bad),
                &name_text,
                move |v| msg(Action::Draft(name_key, v)),
                msg(Action::Submit(name_key)),
            ),
        ),
    );
    let (absolute, text_now) = match path {
        OscAddress::Absolute(t) => (true, t.clone()),
        OscAddress::Relative(t) => (false, t.clone()),
    };
    let (path_key, path_text, path_bad) = draft(TargetField::EntryPath, text_now);
    body = body.push(
        controls.inline(
            if parameter.is_some() {
                "Param Path"
            } else {
                "Event Path"
            },
            ENTRY_LABEL_WIDTH,
            state,
            controls.path(
                Control::new(
                    ControlId::field(location, path_key),
                    Id::TargetEntryPath,
                    PathField {
                        placeholder: "/path",
                        prefix: Prefix::of(
                            app.project(),
                            track.id,
                            PathLevel::Device(id),
                            absolute,
                        ),
                        absolute: Some(absolute),
                        subject,
                    },
                )
                .invalid(path_bad),
                &path_text,
                move |v| msg(Action::Draft(path_key, v)),
                msg(Action::Submit(path_key)),
                Message::Edit(EditMessage::FocusField(path_key)),
                Some(msg(Action::ToggleAbsolute(id))),
            ),
        ),
    );
    if let Some(parameter) = parameter {
        let key = key.clone();
        body = body.push(
            controls.inline(
                "Send",
                ENTRY_LABEL_WIDTH,
                state,
                controls.dropdown(
                    Control::new(
                        ControlId::action(location, "target-send", ControlOwner::Device(id)),
                        Id::TargetSend,
                        Dropdown,
                    )
                    .role(SizeRole::Fill),
                    &SendMode::ALL[..],
                    Some(parameter.send_mode),
                    move |mode| {
                        msg(Action::Definition(
                            id,
                            qtable_engine::edit::TargetEdit::SetParameterSendMode {
                                key: key.clone(),
                                mode,
                            },
                        ))
                    },
                ),
            ),
        );
    }
    body
}

/// The argument inspector: the selected entry's tabs and the chosen argument's properties.
fn inspector<'a>(
    app: &'a App,
    id: DeviceId,
    key: &EntryKey,
    found: Entry<'a>,
    location: View,
) -> Element<'a, Message> {
    let (definitions, mode) = match found {
        Entry::Event(e) => (&e.fields, None),
        Entry::Parameter(p) => (&p.fields, Some(p.send_mode)),
    };
    super::definitions::view(app, id, key, definitions, mode, location)
}
