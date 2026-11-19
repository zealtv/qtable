use super::view::{EDITOR_WIDTH, ENTRY_WIDTH};
use super::*;
use crate::messages::Message;
use qtable_engine::engine::Engine;
use qtable_engine::model::NewDevice;

fn app() -> (App, DeviceId) {
    let mut app = App::new(
        Engine::new(crate::files::new_project()).unwrap(),
        "120".into(),
        None,
    );
    let track = app.project().tracks[0].id;
    let _ = crate::update::update(
        &mut app,
        Message::Grid(crate::grid::Action::SelectTrack(track, false)),
    );
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::AddDevice {
            track,
            kind: NewDevice::Target,
            before: None,
        }),
    );
    super::super::reconcile(&mut app);
    let id = app.project().track(track).unwrap().devices[0].id;
    (app, id)
}

fn act(app: &mut App, action: Action) {
    let _ = crate::update::update(
        app,
        Message::Device(crate::device_chain::Action::Target(action)),
    );
}

fn field(app: &mut App, key: DraftKey, text: &str) {
    act(app, Action::Draft(key, text.into()));
    act(app, Action::Submit(key));
}

#[test]
fn plus_adds_entries_with_free_paths_and_selects_them() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    act(&mut app, Action::Add(id, false));
    act(&mut app, Action::Add(id, true));
    let (_, t) = target(&app, id).unwrap();
    assert_eq!(t.events.len(), 2);
    assert_eq!(t.events[0].path, OscAddress::Relative("/event".into()));
    assert_eq!(t.events[1].path, OscAddress::Relative("/event2".into()));
    assert_eq!(t.parameters[0].path, OscAddress::Relative("/param".into()));
    assert_eq!(selected(&app, id), Some(t.parameters[0].key.clone()));
    assert_eq!(app.selection.devices, [id]);
}

#[test]
fn delete_removes_the_entry_not_the_device_and_arrows_move_the_selection() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    act(&mut app, Action::Add(id, false));
    act(&mut app, Action::Add(id, true));
    let keys = target_keys(&app, id);
    assert!(move_selection(&mut app, -1));
    assert_eq!(selected(&app, id), Some(keys[1].clone()));
    assert!(move_selection(&mut app, -1));
    assert!(move_selection(&mut app, -1), "clamped at the first entry");
    assert_eq!(selected(&app, id), Some(keys[0].clone()));
    let _ = crate::device_chain::shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    assert_eq!(target_keys(&app, id), keys[1..]);
    assert!(app.project().device(id).is_some());
    // Clicking the title clears the entry selection, so Delete then removes the device.
    select_entry(&mut app, id, keys[1].clone());
    let _ = crate::update::update(
        &mut app,
        Message::Device(crate::device_chain::Action::Begin(id, iced::Point::ORIGIN)),
    );
    assert!(selected(&app, id).is_none());
    let _ = crate::device_chain::shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    assert!(app.project().device(id).is_none());
}

#[test]
fn info_lists_addresses_and_the_use_count() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    let track = app.project().device(id).unwrap().0.id;
    let key = {
        let target = app.project().device(id).unwrap().1.target().unwrap();
        target.events[0].key.clone()
    };
    let subject =
        crate::info::Subject::TargetEntry(crate::info::EntryRef::target(false, track, id, &key));
    let lines = crate::info::addresses(app.project(), &subject).unwrap();
    assert!(
        lines
            .iter()
            .all(|l| l.ends_with("/event") || l.contains("No destination"))
    );
    assert_eq!(
        crate::info::uses(app.project(), &subject).as_deref(),
        Some("Used by 0 lanes in 0 clips")
    );
}

fn definition(app: &mut App, id: DeviceId, edit: TargetEdit) {
    act(app, Action::Definition(id, edit));
}

fn property(app: &mut App, id: DeviceId, index: usize, property: DefinitionField, text: &str) {
    field(
        app,
        DraftKey::Target(id, TargetField::Definition(index, property)),
        text,
    );
}

fn fields(app: &App, id: DeviceId) -> &[FieldDef] {
    let (_, t) = target(app, id).unwrap();
    match entry(t, &selected(app, id).unwrap()).unwrap() {
        Entry::Event(e) => &e.fields,
        Entry::Parameter(p) => &p.fields,
    }
}

#[test]
fn field_blocks_edit_all_fields_and_keep_frozen_keys_after_moves() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    definition(
        &mut app,
        id,
        TargetEdit::AddField {
            entry: entry.clone(),
            value_type: FieldType::Int,
        },
    );
    let second = fields(&app, id)[1].key.clone();
    property(&mut app, id, 1, DefinitionField::Name, "radius");
    property(&mut app, id, 1, DefinitionField::Max, "100");
    property(&mut app, id, 1, DefinitionField::Default, "10");
    property(&mut app, id, 1, DefinitionField::Threshold, "0.5");
    // A structural edit clears indexed drafts before their rows move.
    property(&mut app, id, 0, DefinitionField::Min, "-");
    definition(
        &mut app,
        id,
        TargetEdit::MoveField {
            entry: entry.clone(),
            field: second.clone(),
            to: 0,
        },
    );
    assert!(app.drafts.values.is_empty());
    property(&mut app, id, 0, DefinitionField::Name, "size");
    let field = &fields(&app, id)[0];
    assert_eq!(field.key, second);
    assert_eq!(field.name, "size");
    assert_eq!(field.range, Some([0.0, 100.0]));
    assert_eq!(field.default, OscArgument::Int(10));
    assert_eq!(field.threshold, 0.5);
    assert_eq!(
        field.value,
        Some(0.0),
        "definitions do not edit held values"
    );
    definition(
        &mut app,
        id,
        TargetEdit::RemoveField {
            entry: entry.clone(),
            field: second,
        },
    );
    let last = fields(&app, id)[0].key.clone();
    definition(&mut app, id, TargetEdit::RemoveField { entry, field: last });
    assert_eq!(fields(&app, id).len(), 1);
    assert!(app.status.message().is_some());
}

#[test]
fn rejected_defaults_and_ranges_keep_their_draft_and_status_error() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    for (property_kind, text) in [
        (DefinitionField::Min, "2"),
        (DefinitionField::Default, "NaN"),
        (DefinitionField::Max, "1e100"),
        (DefinitionField::Threshold, "-1"),
    ] {
        property(&mut app, id, 0, property_kind, text);
        let key = DraftKey::Target(id, TargetField::Definition(0, property_kind));
        assert_eq!(app.drafts.values[&key], text);
        assert!(app.drafts.errors.contains_key(&key));
        assert!(app.status.message().is_some());
    }
    property(&mut app, id, 0, DefinitionField::Default, "0.125");
    assert_eq!(fields(&app, id)[0].default, OscArgument::Float(0.125));
    assert_eq!(fields(&app, id)[0].value, Some(0.0));
}

#[test]
fn events_share_blocks_with_typed_defaults_and_optional_ranges() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    let entry = selected(&app, id).unwrap();
    for kind in [
        FieldType::Float,
        FieldType::Int,
        FieldType::Bool,
        FieldType::String,
    ] {
        definition(
            &mut app,
            id,
            TargetEdit::AddField {
                entry: entry.clone(),
                value_type: kind,
            },
        );
    }
    for (index, text) in ["0.25", "3", "true", "hello"].into_iter().enumerate() {
        property(&mut app, id, index, DefinitionField::Default, text);
    }
    let f = fields(&app, id);
    assert_eq!(
        f.iter().map(|f| f.default.clone()).collect::<Vec<_>>(),
        [
            OscArgument::Float(0.25),
            OscArgument::Int(3),
            OscArgument::Bool(true),
            OscArgument::String("hello".into())
        ]
    );
    // Editing a default leaves the current value as it was.
    assert_eq!(
        f.iter().map(FieldDef::current_argument).collect::<Vec<_>>(),
        [
            OscArgument::Float(0.0),
            OscArgument::Int(0),
            OscArgument::Bool(false),
            OscArgument::String(String::new())
        ]
    );
    let key = f[0].key.clone();
    definition(
        &mut app,
        id,
        TargetEdit::SetFieldRange {
            entry: entry.clone(),
            field: key.clone(),
            range: Some([0.0, 1.0]),
        },
    );
    definition(
        &mut app,
        id,
        TargetEdit::SetFieldControl {
            entry: entry.clone(),
            field: key.clone(),
            control: qtable_engine::model::Control::Dial,
        },
    );
    assert_eq!(
        fields(&app, id)[0].control,
        qtable_engine::model::Control::Dial
    );
    definition(
        &mut app,
        id,
        TargetEdit::SetFieldType {
            entry,
            field: key,
            value_type: FieldType::String,
        },
    );
    assert_eq!(
        fields(&app, id)[0].control,
        qtable_engine::model::Control::Text
    );
}

#[test]
fn enum_editor_keeps_typed_values_and_string_labels_together() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    let key = fields(&app, id)[0].key.clone();
    definition(
        &mut app,
        id,
        TargetEdit::SetFieldControl {
            entry: entry.clone(),
            field: key.clone(),
            control: qtable_engine::model::Control::Enum,
        },
    );
    property(&mut app, id, 0, DefinitionField::OptionLabel(1), "High");
    property(&mut app, id, 0, DefinitionField::OptionValue(1), "99.5");
    assert_eq!(fields(&app, id)[0].options[1].label, "High");
    assert_eq!(
        fields(&app, id)[0].options[1].value,
        OscArgument::Float(99.5)
    );
    definition(
        &mut app,
        id,
        TargetEdit::SetFieldType {
            entry,
            field: key,
            value_type: FieldType::String,
        },
    );
    property(&mut app, id, 0, DefinitionField::OptionLabel(1), "wide");
    assert_eq!(fields(&app, id)[0].options[1].label, "wide");
    assert_eq!(
        fields(&app, id)[0].options[1].value,
        OscArgument::String("wide".into())
    );
}

/// Find a real widget by its registered help and click through its focus/help wrappers.
fn click_help(app: &App, id: DeviceId, help: crate::info::Id) -> Vec<Message> {
    use crate::control_focus::View;
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    use iced::{Event, Point, Rectangle, Size, mouse};
    let (_, target) = target(app, id).unwrap();
    let key = selected(app, id).unwrap();
    let mode = match entry(target, &key).unwrap() {
        Entry::Event(_) => None,
        Entry::Parameter(p) => Some(p.send_mode),
    };
    let mut widget = super::definitions::view(app, id, &key, fields(app, id), mode, View::Track);
    let renderer = crate::grid::test_renderer();
    let mut tree = Tree::new(&widget);
    let size = Size::new(super::view::INSPECTOR_WIDTH - 16.0, 2000.0);
    let node =
        widget
            .as_widget_mut()
            .layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
    fn centers(layout: Layout<'_>, points: &mut Vec<Point>) {
        points.push(layout.bounds().center());
        for child in layout.children() {
            centers(child, points);
        }
    }
    let mut points = vec![];
    centers(Layout::new(&node), &mut points);
    for point in points {
        let mut messages = vec![];
        widget.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(mouse::Event::CursorMoved { position: point }),
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
        if !messages.iter().any(|m| {
            matches!(m,
            Message::Info(crate::info::Hover::Enter(_, found, _)) if *found == help)
        }) {
            continue;
        }
        messages.clear();
        for event in [
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            widget.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(event),
                Layout::new(&node),
                mouse::Cursor::Available(point),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(size),
            );
        }
        messages.retain(|m| matches!(m, Message::Device(..)));
        return messages;
    }
    panic!("No widget with help {help:?}");
}

#[test]
fn rendered_field_buttons_add_remove_and_keep_the_last_parameter_field() {
    use crate::info::Id;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    assert!(click_help(&app, id, Id::TargetFieldRemove).is_empty());
    let messages = click_help(&app, id, Id::TargetFieldAdd);
    assert!(matches!(
        messages.as_slice(),
        [Message::Device(crate::device_chain::Action::Target(
            Action::Definition(_, TargetEdit::AddField { .. })
        ))]
    ));
    for message in messages {
        let _ = crate::update::update(&mut app, message);
    }
    assert_eq!(fields(&app, id).len(), 2);
    let messages = click_help(&app, id, Id::TargetFieldRemove);
    for message in messages {
        let _ = crate::update::update(&mut app, message);
    }
    assert_eq!(fields(&app, id).len(), 1);
}

#[test]
fn rendered_bool_default_is_a_dropdown() {
    use crate::info::Id;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    let entry = selected(&app, id).unwrap();
    definition(
        &mut app,
        id,
        TargetEdit::AddField {
            entry,
            value_type: FieldType::Bool,
        },
    );
    // A click opens the False/True dropdown; it never flips the default like a toggle did.
    for message in click_help(&app, id, Id::TargetDefault) {
        let _ = crate::update::update(&mut app, message);
    }
    assert_eq!(fields(&app, id)[0].default, OscArgument::Bool(false));
}

#[test]
fn entry_name_path_and_absolute_keep_their_existing_edits() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    field(&mut app, DraftKey::Target(id, TargetField::Name), "Kick");
    field(
        &mut app,
        DraftKey::Target(id, TargetField::EntryPath),
        "kick",
    );
    field(&mut app, DraftKey::Target(id, TargetField::Path), "bop");
    act(&mut app, Action::ToggleAbsolute(id));
    let (_, target) = target(&app, id).unwrap();
    assert_eq!(target.events[0].name, "Kick");
    assert_eq!(target.events[0].path, OscAddress::Absolute("/kick".into()));
    assert_eq!(target.path.as_str(), "/bop");
}

fn commit_exact(app: &mut App) {
    let field = app.device_chain.face.exact.as_ref().unwrap().0.clone();
    act(app, Action::CommitExact(field));
}

fn face_field(
    app: &App,
    id: DeviceId,
    parameter: bool,
    index: usize,
    position: usize,
) -> FaceField {
    let (_, t) = target(app, id).unwrap();
    let (entry, fields) = if parameter {
        let p = &t.parameters[index];
        (&p.key, &p.fields)
    } else {
        let e = &t.events[index];
        (&e.key, &e.fields)
    };
    FaceField {
        device: id,
        entry: entry.clone(),
        field: fields[position].key.clone(),
        parameter,
    }
}

#[test]
fn folded_controls_keep_entry_selection_and_delete_the_device() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    act(&mut app, Action::Fold(id));
    let field = face_field(&app, id, true, 0, 0);
    act(&mut app, Action::Value(field, 0.75, true));
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[0].value,
        Some(0.75)
    );
    assert_eq!(selected(&app, id), Some(entry));
    assert!(selected_of_device(&app).is_none());
    let _ = crate::device_chain::shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    assert!(app.project().device(id).is_none());
}

#[test]
fn reset_returns_a_field_to_its_default_as_one_edit_and_clears_the_hold_and_draft() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    definition(
        &mut app,
        id,
        TargetEdit::AddField {
            entry: entry.clone(),
            value_type: FieldType::Float,
        },
    );
    definition(
        &mut app,
        id,
        TargetEdit::SetParameterSendMode {
            key: entry,
            mode: qtable_engine::model::SendMode::OnRelease,
        },
    );
    act(&mut app, Action::Fold(id));
    let field = face_field(&app, id, true, 0, 1);
    let default = target(&app, id).unwrap().1.parameters[0].fields[1].default_value();
    act(&mut app, Action::Value(field.clone(), 0.625, true));
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[1].value,
        Some(0.625)
    );
    // An On Release hold and a typed draft are both pending when the double click lands.
    act(&mut app, Action::Value(field.clone(), 0.75, false));
    act(&mut app, Action::Exact(field.clone(), "0.5".into()));
    assert!(super::collapsed::held(&app, &field).is_some());
    act(&mut app, Action::Reset(field));
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[1].value,
        Some(default)
    );
    assert!(app.parameter_gesture.is_empty());
    assert!(app.device_chain.face.exact.is_none());
    assert!(!app.device_chain.face.error);
}

#[test]
fn face_drag_defers_on_release_and_exact_entry_rejects_then_commits() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    definition(
        &mut app,
        id,
        TargetEdit::AddField {
            entry: entry.clone(),
            value_type: FieldType::Float,
        },
    );
    definition(
        &mut app,
        id,
        TargetEdit::SetParameterSendMode {
            key: entry,
            mode: qtable_engine::model::SendMode::OnRelease,
        },
    );
    act(&mut app, Action::Fold(id));
    let field = face_field(&app, id, true, 0, 1);
    act(&mut app, Action::Value(field.clone(), 0.625, false));
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[1].value,
        Some(0.0)
    );
    act(&mut app, Action::Value(field.clone(), 0.625, true));
    let p = &target(&app, id).unwrap().1.parameters[0];
    assert_eq!(p.fields[0].value, Some(0.0));
    assert_eq!(p.fields[1].value, Some(0.625));
    let width = super::super::width(app.project().device(id).unwrap().1);
    act(&mut app, Action::Exact(field.clone(), "2.0".into()));
    commit_exact(&mut app);
    assert!(app.device_chain.face.error);
    assert!(app.device_chain.face.exact.is_some());
    // The refusal names the argument, never a bare "must be between".
    let message = app.status.message().unwrap().to_owned();
    assert!(
        message.ends_with(" must be between 0.00 and 1.00") && !message.starts_with(' '),
        "{message}"
    );
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[1].value,
        Some(0.625)
    );
    act(&mut app, Action::Exact(field.clone(), "0.125".into()));
    let other = face_field(&app, id, true, 0, 0);
    act(&mut app, Action::CommitExact(other));
    assert!(
        app.device_chain.face.exact.is_some(),
        "another input cannot submit this draft"
    );
    commit_exact(&mut app);
    assert!(app.device_chain.face.exact.is_none());
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[1].value,
        Some(0.125)
    );
    assert_eq!(
        super::super::width(app.project().device(id).unwrap().1),
        width
    );
    act(&mut app, Action::Exact(field, "0.7".into()));
    let _ = crate::update::update(&mut app, Message::CancelField);
    assert!(app.device_chain.face.exact.is_none());
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[1].value,
        Some(0.125)
    );
}

#[test]
fn event_face_edits_typed_current_values_and_inactive_parameter_values() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    let entry = selected(&app, id).unwrap();
    for kind in [FieldType::Int, FieldType::Bool, FieldType::String] {
        definition(
            &mut app,
            id,
            TargetEdit::AddField {
                entry: entry.clone(),
                value_type: kind,
            },
        );
    }
    act(&mut app, Action::Add(id, true));
    act(&mut app, Action::Fold(id));
    let int = face_field(&app, id, false, 0, 0);
    let boolean = face_field(&app, id, false, 0, 1);
    let string = face_field(&app, id, false, 0, 2);
    act(&mut app, Action::Value(boolean, 1.0, true));
    act(&mut app, Action::Exact(int, "42".into()));
    commit_exact(&mut app);
    act(&mut app, Action::Exact(string, "cue".into()));
    commit_exact(&mut app);
    let event = &target(&app, id).unwrap().1.events[0];
    // The face edits current values; the defaults stay as the definition set them.
    assert_eq!(
        event
            .fields
            .iter()
            .map(|f| f.default.clone())
            .collect::<Vec<_>>(),
        vec![
            OscArgument::Int(0),
            OscArgument::Bool(false),
            OscArgument::String(String::new())
        ]
    );
    assert_eq!(
        event
            .fields
            .iter()
            .map(FieldDef::current_argument)
            .collect::<Vec<_>>(),
        vec![
            OscArgument::Int(42),
            OscArgument::Bool(true),
            OscArgument::String("cue".into())
        ]
    );
    let _ = crate::device_chain::update(&mut app, crate::device_chain::Action::Toggle(id));
    let parameter = face_field(&app, id, true, 0, 0);
    act(&mut app, Action::Value(parameter, 0.25, true));
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[0].value,
        Some(0.25)
    );
}

/// Click a registered control in the actual fixed-height frame.
fn click_face(
    app: &App,
    id: DeviceId,
    control: crate::drafts::ControlId,
    press_only: bool,
) -> Vec<Message> {
    face_gesture(app, id, control, press_only, None, false)
}

fn face_gesture(
    app: &App,
    id: DeviceId,
    control: crate::drafts::ControlId,
    press_only: bool,
    motion: Option<f32>,
    blur: bool,
) -> Vec<Message> {
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    use iced::{Event, Rectangle, Size, mouse};
    let mut widget = super::super::frame::device(
        app,
        app.project().device(id).unwrap().1,
        crate::control_focus::View::Track,
    );
    let bounds = crate::control_focus::test_registry::rectangles(&mut widget)
        .into_iter()
        .find(|(id, _)| *id == control)
        .unwrap()
        .1;
    let renderer = crate::grid::test_renderer();
    let mut tree = Tree::new(&widget);
    let size = Size::new(600.0, 900.0);
    let node =
        widget
            .as_widget_mut()
            .layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
    let point = bounds.center();
    let mut messages = vec![];
    let mut events = vec![Event::Mouse(mouse::Event::ButtonPressed(
        mouse::Button::Left,
    ))];
    if let Some(dy) = motion {
        events.push(Event::Mouse(mouse::Event::CursorMoved {
            position: iced::Point::new(point.x, point.y + dy),
        }));
    }
    if blur {
        events.push(Event::Window(iced::window::Event::Unfocused));
    } else if !press_only {
        events.push(Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        )));
    }
    let mut cursor = point;
    for event in events {
        if let Event::Mouse(mouse::Event::CursorMoved { position }) = &event {
            cursor = *position;
        }
        widget.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(cursor),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
    }
    messages
}

#[test]
fn disclosure_draws_a_centred_triangle_on_the_title_background() {
    use crate::control_focus::{ControlOwner, View};
    use crate::drafts::ControlId;
    use iced::advanced::{Layout, layout, renderer::Headless, widget::Tree};
    use iced::{Color, Rectangle, Size, mouse};

    let (mut app, id) = app();
    for selected in [false, true] {
        app.selection.devices = if selected { vec![id] } else { vec![] };
        for expanded in [false, true] {
            act(&mut app, Action::Fold(id));
            assert_eq!(app.project().device(id).unwrap().1.expanded, expanded);
            for hovered in [false, true] {
                let mut widget = super::super::frame::device(
                    &app,
                    app.project().device(id).unwrap().1,
                    View::Track,
                );
                let bounds = crate::control_focus::test_registry::rectangles(&mut widget)
                    .into_iter()
                    .find(|(control, _)| {
                        *control
                            == ControlId::action(
                                View::Track,
                                "device-fold",
                                ControlOwner::Device(id),
                            )
                    })
                    .unwrap()
                    .1;
                assert_eq!(bounds.size(), Size::new(20.0, 20.0));
                let mut renderer = crate::grid::test_renderer();
                let mut tree = Tree::new(&widget);
                let size = Size::new(600.0, 900.0);
                let node = widget.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, size),
                );
                widget.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &iced::Theme::Dark,
                    &iced::advanced::renderer::Style::default(),
                    Layout::new(&node),
                    if hovered {
                        mouse::Cursor::Available(bounds.center())
                    } else {
                        mouse::Cursor::Unavailable
                    },
                    &Rectangle::with_size(size),
                );
                let pixels = renderer.screenshot(Size::new(600, 900), 1.0, Color::BLACK);
                let pixel = |x: u32, y: u32| {
                    let at = ((bounds.y as u32 + y) * 600 + bounds.x as u32 + x) as usize * 4;
                    &pixels[at..at + 4]
                };
                let background = if selected {
                    app.theme.selection_background
                } else {
                    app.theme.device_title_background
                }
                .into_rgba8();
                for (x, y) in [(0, 0), (19, 0), (0, 19), (19, 19), (10, 2), (10, 17)] {
                    assert_eq!(
                        pixel(x, y),
                        background,
                        "disclosure must keep the title background"
                    );
                }
                let painted: Vec<_> = (0..20)
                    .flat_map(|y| (0..20).map(move |x| (x, y)))
                    .filter(|&(x, y)| pixel(x, y) != background)
                    .collect();
                let left = painted.iter().map(|p| p.0).min().unwrap();
                let right = painted.iter().map(|p| p.0).max().unwrap();
                let top = painted.iter().map(|p| p.1).min().unwrap();
                let bottom = painted.iter().map(|p| p.1).max().unwrap();
                assert!((left + right).abs_diff(19) <= 1);
                assert!((top + bottom).abs_diff(19) <= 1);
                assert!(right - left <= 8 && bottom - top <= 9);
                let left_half = painted.iter().filter(|p| p.0 < 10).count();
                let right_half = painted.iter().filter(|p| p.0 >= 10).count();
                assert_eq!(
                    left_half < right_half,
                    expanded,
                    "triangle must point toward the other face"
                );
                let ink = if selected {
                    crate::control::theme::ink(app.theme.selection_background)
                } else {
                    app.theme.control_foreground
                };
                assert_eq!(pixel(10, 10), ink.into_rgba8());
            }
        }
    }
}

#[test]
fn rendered_disclosure_toggle_slider_and_trigger_use_their_own_controls() {
    use crate::control_focus::{ControlOwner, View};
    use crate::drafts::ControlId;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    definition(
        &mut app,
        id,
        TargetEdit::SetFieldType {
            entry: entry.clone(),
            field: qtable_engine::model::FieldKey("f1".into()),
            value_type: FieldType::Bool,
        },
    );
    act(&mut app, Action::Add(id, true));
    let selected_entry = selected(&app, id);
    for message in click_face(
        &app,
        id,
        ControlId::action(View::Track, "device-fold", ControlOwner::Device(id)),
        false,
    ) {
        let _ = crate::update::update(&mut app, message);
    }
    assert!(!app.project().device(id).unwrap().1.expanded);
    let toggle = ControlId::field(
        View::Track,
        DraftKey::Target(id, TargetField::Face(true, 0, 0)),
    );
    for message in click_face(&app, id, toggle, false) {
        let _ = crate::update::update(&mut app, message);
    }
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[0].value,
        Some(1.0)
    );
    assert_eq!(selected(&app, id), selected_entry);
    let slider = ControlId::field(
        View::Track,
        DraftKey::Target(id, TargetField::Face(true, 1, 0)),
    );
    for message in click_face(&app, id, slider, false) {
        let _ = crate::update::update(&mut app, message);
    }
    assert_eq!(
        target(&app, id).unwrap().1.parameters[1].fields[0].value,
        Some(0.0)
    );
    assert!(app.control_focus.has(slider));
    let task = keyboard(
        &mut app,
        &iced::keyboard::Key::Character("0".into()),
        iced::keyboard::Modifiers::NONE,
        true,
    );
    assert!(task.is_some());
    assert!(app.device_chain.face.exact.is_some());
    let _ = crate::update::update(&mut app, Message::CancelField);
    let fire = ControlId::field_action(
        View::Track,
        DraftKey::Target(id, TargetField::Face(false, 0, 0)),
        "fire",
    );
    let messages = click_face(&app, id, fire, true);
    assert_eq!(
        messages
            .iter()
            .filter(|m| matches!(
                m,
                Message::Device(crate::device_chain::Action::Target(Action::Fire(..)))
            ))
            .count(),
        1
    );
}

#[test]
fn rendered_trigger_sends_current_values_once_while_stopped_without_an_edit() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    use qtable_engine::model::{BeatTime, DeviceId, Project, Route, Track, TrackId};
    use qtable_test_support::TestReceiver;
    use std::time::Duration;
    let receiver = TestReceiver::bind().unwrap();
    let mut track = Track::new(TrackId(1), "Track", 0);
    track.route = Route::single(TrackId(2));
    let hit = track.hit(
        BeatTime::ZERO,
        OscAddress::Relative("/cue".into()),
        vec![OscArgument::Float(0.0)],
        || DeviceId(1),
    );
    let id = hit.definition.device;
    track.devices[0].expanded = false;
    let project = Project {
        tracks: vec![track],
        outputs: vec![Track::new_output(
            TrackId(2),
            "Out",
            "127.0.0.1",
            receiver.addr().port(),
        )],
        ..Project::default()
    };
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    let _ = crate::update::update(
        &mut app,
        Message::Grid(crate::grid::Action::SelectTrack(TrackId(1), false)),
    );
    let field = face_field(&app, id, false, 0, 0);
    act(&mut app, Action::Exact(field, "0.75".into()));
    commit_exact(&mut app);
    let default = app.project().tracks[0].devices[0].kind.clone();
    let target = default.target().unwrap();
    assert_eq!(target.events[0].fields[0].default, OscArgument::Float(0.0));
    let revision = app.project_revision();
    let project = app.project().clone();
    let fire = ControlId::field_action(
        View::Track,
        DraftKey::Target(id, TargetField::Face(false, 0, 0)),
        "fire",
    );
    for message in click_face(&app, id, fire, true) {
        let _ = crate::update::update(&mut app, message);
    }
    assert_eq!(app.project_revision(), revision);
    assert_eq!(app.project(), &project);
    assert!(!app.snapshot.playing);
    let packets = receiver
        .wait_for_messages(1, Duration::from_secs(1))
        .unwrap();
    assert_eq!(packets[0].1.addr, "/cue");
    assert_eq!(packets[0].1.args[0].clone().float(), Some(0.75));
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(80))
            .is_err()
    );
}

#[test]
fn rendered_dial_drag_previews_on_release_and_blur_commits_then_exact_entry_works() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    definition(
        &mut app,
        id,
        TargetEdit::SetFieldControl {
            entry: entry.clone(),
            field: qtable_engine::model::FieldKey("f1".into()),
            control: qtable_engine::model::Control::Dial,
        },
    );
    definition(
        &mut app,
        id,
        TargetEdit::SetParameterSendMode {
            key: entry,
            mode: qtable_engine::model::SendMode::OnRelease,
        },
    );
    act(&mut app, Action::Fold(id));
    let control = ControlId::field(
        View::Track,
        DraftKey::Target(id, TargetField::Face(true, 0, 0)),
    );
    let key = face_field(&app, id, true, 0, 0);
    let mut widget =
        super::super::frame::device(&app, app.project().device(id).unwrap().1, View::Track);
    let bounds = crate::control_focus::test_registry::rectangles(&mut widget)
        .into_iter()
        .find(|(id, _)| *id == control)
        .unwrap()
        .1;
    assert_eq!(bounds.width, crate::control::dial::WIDTH);
    assert_eq!(bounds.height, crate::control::dial::HEIGHT);
    drop(widget);
    let width = super::super::width(app.project().device(id).unwrap().1);
    let messages = face_gesture(&app, id, control, true, Some(-25.0), true);
    let mut values = 0;
    for message in messages {
        let release = matches!(
            &message,
            Message::Device(crate::device_chain::Action::Target(Action::Value(
                _,
                _,
                true
            )))
        );
        if matches!(
            &message,
            Message::Device(crate::device_chain::Action::Target(Action::Value(..)))
        ) {
            values += 1;
            assert_eq!(
                target(&app, id).unwrap().1.parameters[0].fields[0].value,
                Some(0.0)
            );
        }
        let _ = crate::update::update(&mut app, message);
        if release {
            assert_eq!(
                target(&app, id).unwrap().1.parameters[0].fields[0].value,
                Some(0.25)
            );
        }
    }
    assert_eq!(values, 2, "one drag step and one blur release");
    assert!(app.control_focus.has(control));
    assert!(
        keyboard(
            &mut app,
            &iced::keyboard::Key::Character("0".into()),
            iced::keyboard::Modifiers::NONE,
            true
        )
        .is_some()
    );
    act(&mut app, Action::Exact(key, "0.625".into()));
    commit_exact(&mut app);
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[0].value,
        Some(0.625)
    );
    assert_eq!(
        super::super::width(app.project().device(id).unwrap().1),
        width
    );
}

#[test]
fn dial_int_fields_snap_for_events_and_inactive_parameters() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    for parameter in [false, true] {
        let (mut app, id) = app();
        act(&mut app, Action::Add(id, parameter));
        let entry = selected(&app, id).unwrap();
        if !parameter {
            definition(
                &mut app,
                id,
                TargetEdit::AddField {
                    entry: entry.clone(),
                    value_type: FieldType::Int,
                },
            );
        }
        let field = qtable_engine::model::FieldKey("f1".into());
        for edit in [
            TargetEdit::SetFieldType {
                entry: entry.clone(),
                field: field.clone(),
                value_type: FieldType::Int,
            },
            TargetEdit::SetFieldRange {
                entry: entry.clone(),
                field: field.clone(),
                range: Some([0.0, 10.0]),
            },
            TargetEdit::SetFieldControl {
                entry,
                field,
                control: qtable_engine::model::Control::Dial,
            },
        ] {
            definition(&mut app, id, edit);
        }
        run(
            &mut app,
            ProjectEdit::SetDevicesActive {
                devices: vec![id],
                active: false,
            },
        );
        act(&mut app, Action::Fold(id));
        let control = ControlId::field(
            View::Track,
            DraftKey::Target(id, TargetField::Face(parameter, 0, 0)),
        );
        for message in face_gesture(&app, id, control, false, Some(-26.0), false) {
            let _ = crate::update::update(&mut app, message);
        }
        let (_, t) = target(&app, id).unwrap();
        if parameter {
            assert_eq!(t.parameters[0].fields[0].value, Some(3.0));
        } else {
            assert_eq!(
                t.events[0].fields[0].current_argument(),
                OscArgument::Int(3)
            );
            assert_eq!(t.events[0].fields[0].default, OscArgument::Int(0));
        }
    }
}

#[test]
fn adding_options_after_removal_uses_a_free_wire_value() {
    use crate::info::Id;
    for kind in [FieldType::Float, FieldType::Int, FieldType::String] {
        let (mut app, id) = app();
        act(&mut app, Action::Add(id, true));
        let entry = selected(&app, id).unwrap();
        let key = fields(&app, id)[0].key.clone();
        definition(
            &mut app,
            id,
            TargetEdit::SetFieldType {
                entry: entry.clone(),
                field: key.clone(),
                value_type: kind,
            },
        );
        definition(
            &mut app,
            id,
            TargetEdit::SetFieldControl {
                entry: entry.clone(),
                field: key.clone(),
                control: qtable_engine::model::Control::Enum,
            },
        );
        assert_eq!(
            fields(&app, id)[0]
                .options
                .iter()
                .map(|o| o.label.as_str())
                .collect::<Vec<_>>(),
            ["One", "Two"]
        );
        for message in click_help(&app, id, Id::TargetOptionAdd) {
            let _ = crate::update::update(&mut app, message);
        }
        assert_eq!(fields(&app, id)[0].options.len(), 3);
        assert_eq!(fields(&app, id)[0].options[2].label, "Three");
        for message in click_help(&app, id, Id::TargetOptionRemove) {
            let _ = crate::update::update(&mut app, message);
        }
        assert_eq!(fields(&app, id)[0].options.len(), 2);
        for message in click_help(&app, id, Id::TargetOptionAdd) {
            let _ = crate::update::update(&mut app, message);
        }
        let options = &fields(&app, id)[0].options;
        assert_eq!(options.len(), 3);
        let expected = match kind {
            FieldType::Float => OscArgument::Float(1.0),
            FieldType::Int => OscArgument::Int(1),
            FieldType::String => OscArgument::String("One".into()),
            FieldType::Bool => unreachable!(),
        };
        assert_eq!(options[2].value, expected);
        assert_eq!(options[2].label, "One");
        let before = options.clone();
        property(
            &mut app,
            id,
            0,
            if kind == FieldType::String {
                DefinitionField::OptionLabel(1)
            } else {
                DefinitionField::OptionValue(1)
            },
            if kind == FieldType::String {
                "Two"
            } else {
                "2"
            },
        );
        assert_eq!(fields(&app, id)[0].options, before);
        assert!(
            app.drafts
                .errors
                .values()
                .any(|e| e.contains("Enum option values must be unique"))
        );
    }
}

#[test]
fn option_add_uses_free_small_numbers_even_with_large_existing_values() {
    use crate::info::Id;
    for (kind, values, expected) in [
        (
            FieldType::Float,
            vec![OscArgument::Float(0.0), OscArgument::Float(16_777_216.0)],
            OscArgument::Float(1.0),
        ),
        (
            FieldType::Float,
            vec![OscArgument::Float(0.0), OscArgument::Float(f32::MAX)],
            OscArgument::Float(1.0),
        ),
        (
            FieldType::Int,
            vec![OscArgument::Int(0), OscArgument::Int(i32::MAX)],
            OscArgument::Int(1),
        ),
        (
            FieldType::String,
            vec![
                OscArgument::String("wide".into()),
                OscArgument::String("narrow".into()),
            ],
            OscArgument::String("One".into()),
        ),
    ] {
        let (mut app, id) = app();
        act(&mut app, Action::Add(id, true));
        let entry = selected(&app, id).unwrap();
        let key = fields(&app, id)[0].key.clone();
        definition(
            &mut app,
            id,
            TargetEdit::SetFieldType {
                entry: entry.clone(),
                field: key.clone(),
                value_type: kind,
            },
        );
        definition(
            &mut app,
            id,
            TargetEdit::SetFieldControl {
                entry: entry.clone(),
                field: key.clone(),
                control: qtable_engine::model::Control::Enum,
            },
        );
        let options = values
            .into_iter()
            .enumerate()
            .map(|(i, value)| {
                let label = match &value {
                    OscArgument::String(v) => v.clone(),
                    _ => i.to_string(),
                };
                qtable_engine::model::EnumOption { label, value }
            })
            .collect();
        definition(
            &mut app,
            id,
            TargetEdit::SetFieldOptions {
                entry,
                field: key,
                options,
            },
        );
        let messages = click_help(&app, id, Id::TargetOptionAdd);
        assert!(!messages.is_empty());
        for message in messages {
            let _ = crate::update::update(&mut app, message);
        }
        let option = fields(&app, id)[0].options.last().unwrap();
        assert_eq!(option.value, expected);
        assert_eq!(option.label, "One");
    }
}

#[test]
fn cells_are_118_wide_with_the_bang_at_the_top_right_and_110_px_controls() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    act(&mut app, Action::Add(id, true));
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    definition(
        &mut app,
        id,
        TargetEdit::AddField {
            entry,
            value_type: FieldType::Float,
        },
    );
    for message in click_face(
        &app,
        id,
        ControlId::action(
            View::Track,
            "device-fold",
            crate::control_focus::ControlOwner::Device(id),
        ),
        false,
    ) {
        let _ = crate::update::update(&mut app, message);
    }
    let mut widget =
        super::super::frame::device(&app, app.project().device(id).unwrap().1, View::Track);
    let rectangles = crate::control_focus::test_registry::rectangles(&mut widget);
    let bounds = |control: ControlId| rectangles.iter().find(|(id, _)| *id == control).unwrap().1;
    let fire = bounds(ControlId::field_action(
        View::Track,
        DraftKey::Target(id, TargetField::Face(false, 0, 0)),
        "fire",
    ));
    let one = bounds(ControlId::field(
        View::Track,
        DraftKey::Target(id, TargetField::Face(true, 0, 0)),
    ));
    assert_eq!((fire.width, fire.height), (18.0, 18.0));
    assert_eq!(one.width, 110.0);
    // The cell's right edge sits 4 px beyond the controls; the bang abuts the 1 px edge.
    // Parameters always start a new column, 4 px beyond the events' cell.
    assert_eq!(one.x, fire.x + fire.width + 1.0 + 4.0 + 1.0 + 3.0);
    // Both sections sit under a 16 px heading (24 px is the frame's header and 4 px inset), then
    // each cell's 18 px title strip.
    assert_eq!(fire.y, 24.0 + 16.0 + 1.0);
    assert!(one.y >= fire.y + 18.0);
}

#[test]
fn headings_sit_over_each_section_and_enum_labels_over_their_dropdowns() {
    use crate::control_focus::{ControlOwner, View};
    use crate::drafts::ControlId;
    let (mut app, id) = app();
    for parameter in [false, true] {
        act(&mut app, Action::Add(id, parameter));
        let entry = selected(&app, id).unwrap();
        // A new parameter already holds one field.
        if !parameter {
            definition(
                &mut app,
                id,
                TargetEdit::AddField {
                    entry: entry.clone(),
                    value_type: FieldType::String,
                },
            );
        }
        let field = fields(&app, id)[0].key.clone();
        definition(
            &mut app,
            id,
            TargetEdit::SetFieldControl {
                entry,
                field,
                control: qtable_engine::model::Control::Enum,
            },
        );
    }
    let fold = ControlId::action(View::Track, "device-fold", ControlOwner::Device(id));
    for message in click_face(&app, id, fold, false) {
        let _ = crate::update::update(&mut app, message);
    }
    let mut widget =
        super::super::frame::device(&app, app.project().device(id).unwrap().1, View::Track);
    let rectangles = crate::control_focus::test_registry::rectangles(&mut widget);
    let bounds = |control: ControlId| rectangles.iter().find(|(id, _)| *id == control).unwrap().1;
    let fire = bounds(ControlId::field_action(
        View::Track,
        DraftKey::Target(id, TargetField::Face(false, 0, 0)),
        "fire",
    ));
    let field = |parameter| {
        bounds(ControlId::field(
            View::Track,
            DraftKey::Target(id, TargetField::Face(parameter, 0, 0)),
        ))
    };
    // Cells start below the 16 px heading; an event's Enum drops 14 + 2 px for its label, a
    // one-field parameter's (named by its title strip) does not.
    assert_eq!(fire.y, 24.0 + 16.0 + 1.0);
    assert_eq!(field(false).y, fire.y + 18.0 + 2.0 + 14.0 + 2.0);
    assert_eq!(field(true).y, fire.y + 18.0 + 2.0);
    // Focus now belongs to the dropdown itself, whose natural height fits within the
    // unchanged 20 px face row (the removed outer wrapper used to report the row bounds).
    assert!(field(false).height > 0.0 && field(false).height <= 20.0);
    assert_eq!(field(false).height, field(true).height);
    assert_eq!(field(false).width, 110.0);
}

#[test]
fn number_text_and_enum_share_one_labelled_row_with_the_label_above() {
    use crate::control_focus::{ControlOwner, View};
    use crate::drafts::ControlId;
    use qtable_engine::model::Control as Kind;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    let entry = selected(&app, id).unwrap();
    for _ in 0..3 {
        definition(
            &mut app,
            id,
            TargetEdit::AddField {
                entry: entry.clone(),
                value_type: FieldType::String,
            },
        );
    }
    let keys: Vec<_> = fields(&app, id).iter().map(|f| f.key.clone()).collect();
    for (key, control) in keys.iter().zip([Kind::NumberBox, Kind::Text, Kind::Enum]) {
        definition(
            &mut app,
            id,
            TargetEdit::SetFieldControl {
                entry: entry.clone(),
                field: key.clone(),
                control,
            },
        );
    }
    let fold = ControlId::action(View::Track, "device-fold", ControlOwner::Device(id));
    for message in click_face(&app, id, fold, false) {
        let _ = crate::update::update(&mut app, message);
    }
    let mut widget =
        super::super::frame::device(&app, app.project().device(id).unwrap().1, View::Track);
    let rectangles = crate::control_focus::test_registry::rectangles(&mut widget);
    let field = |position| {
        rectangles
            .iter()
            .find(|(control, _)| {
                *control
                    == ControlId::field(
                        View::Track,
                        DraftKey::Target(id, TargetField::Face(false, 0, position)),
                    )
            })
            .unwrap()
            .1
    };
    let (number, text, enumeration) = (field(0), field(1), field(2));
    // The input fills the 110 px row; its label sits in the 14 px row above, so the first
    // field starts 14 + 2 px below the strip's gap, and each labelled row is 36 px plus 2 px.
    // Focus follows each input's natural bounds inside the unchanged 20 px row.
    for bounds in [number, text, enumeration] {
        assert_eq!(bounds.width, 110.0);
        assert!(bounds.height > 0.0 && bounds.height <= 20.0);
        assert_eq!(bounds.x, number.x);
    }
    assert_eq!(text.y, number.y + 36.0 + 2.0);
    assert_eq!(enumeration.y, text.y + 36.0 + 2.0);
    assert!(
        collapsed::label_row(Kind::NumberBox)
            && collapsed::label_row(Kind::Text)
            && collapsed::label_row(Kind::Enum)
    );
    assert!(
        !collapsed::label_row(Kind::Toggle)
            && !collapsed::label_row(Kind::Slider)
            && !collapsed::label_row(Kind::Dial)
    );
}

/// Click a point of the device frame, as a pointer would.
fn click_point(app: &App, id: DeviceId, point: iced::Point) -> Vec<Message> {
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    use iced::{Event, Rectangle, Size, mouse};
    let mut widget = super::super::frame::device(
        app,
        app.project().device(id).unwrap().1,
        crate::control_focus::View::Track,
    );
    let renderer = crate::grid::test_renderer();
    let mut tree = Tree::new(&widget);
    let size = Size::new(1200.0, 900.0);
    let node =
        widget
            .as_widget_mut()
            .layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
    let mut messages = vec![];
    for button in [
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ] {
        widget.as_widget_mut().update(
            &mut tree,
            &button,
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
    }
    messages
}

/// The first event's bang, and a point on the title strip to its left.
fn first_title(app: &App, id: DeviceId) -> (iced::Rectangle, iced::Point) {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    let mut widget =
        super::super::frame::device(app, app.project().device(id).unwrap().1, View::Track);
    let fire = crate::control_focus::test_registry::rectangles(&mut widget)
        .into_iter()
        .find(|(control, _)| {
            *control
                == ControlId::field_action(
                    View::Track,
                    DraftKey::Target(id, TargetField::Face(false, 0, 0)),
                    "fire",
                )
        })
        .unwrap()
        .1;
    (fire, iced::Point::new(fire.x - 20.0, fire.center_y()))
}

#[test]
fn clicking_a_title_strip_selects_the_entry_only_while_expanded() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    let first = selected(&app, id).unwrap();
    act(&mut app, Action::Add(id, false));
    assert_ne!(selected(&app, id), Some(first.clone()));
    let (_, title) = first_title(&app, id);
    for message in click_point(&app, id, title) {
        let _ = crate::update::update(&mut app, message);
    }
    assert_eq!(selected(&app, id), Some(first.clone()));
    // Collapsed, the title selects nothing.
    act(&mut app, Action::Fold(id));
    let second = target_keys(&app, id)[1].clone();
    act(&mut app, Action::Select(id, second));
    let (_, title) = first_title(&app, id);
    assert!(click_point(&app, id, title).is_empty());
}

#[test]
fn expanding_puts_the_entry_editor_left_of_the_face_and_folding_removes_both_editors() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    act(&mut app, Action::Add(id, true));
    let key = selected(&app, id).unwrap();
    let face = target(&app, id).map(|(_, t)| collapsed::width(t)).unwrap();
    let device = |app: &App| app.project().device(id).unwrap().1.clone();
    assert!(device(&app).expanded);
    assert_eq!(
        super::super::face::width(&device(&app)),
        face + 1.0 + EDITOR_WIDTH
    );
    let expanded_bang = first_title(&app, id).0;
    act(&mut app, Action::Fold(id));
    assert!(!device(&app).expanded);
    assert_eq!(super::super::face::width(&device(&app)), face);
    // Folding shifts the same face left by the entry editor and its divider.
    let collapsed_bang = first_title(&app, id).0;
    assert_eq!(collapsed_bang.x, expanded_bang.x - ENTRY_WIDTH - 1.0);
    assert_eq!(collapsed_bang.y, expanded_bang.y);
    assert_eq!(collapsed_bang.size(), expanded_bang.size());
    let mut widget =
        super::super::frame::device(&app, app.project().device(id).unwrap().1, View::Track);
    let controls = crate::control_focus::test_registry::rectangles(&mut widget);
    drop(widget);
    for field in [
        TargetField::Path,
        TargetField::Definition(0, DefinitionField::Name),
    ] {
        assert!(!controls.iter().any(|(control, _)| {
            *control == ControlId::field(View::Track, DraftKey::Target(id, field))
        }));
    }
    assert_eq!(selected(&app, id), Some(key.clone()));
    act(&mut app, Action::Fold(id));
    assert!(device(&app).expanded);
    assert_eq!(first_title(&app, id).0, expanded_bang);
    assert_eq!(selected(&app, id), Some(key));
}

#[test]
fn the_editor_deletes_the_selected_entry_and_an_empty_target_offers_both_adds() {
    let (mut app, id) = app();
    assert!(target_keys(&app, id).is_empty());
    act(&mut app, Action::Add(id, false));
    act(&mut app, Action::Add(id, true));
    assert_eq!(target_keys(&app, id).len(), 2);
    act(&mut app, Action::Remove(id));
    assert_eq!(target_keys(&app, id).len(), 1);
    assert!(target(&app, id).unwrap().1.parameters.is_empty());
}

/// A parameter with two more Float fields, the editor's tab on the last.
fn three_fields() -> (App, DeviceId, EntryKey) {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    for _ in 0..2 {
        definition(
            &mut app,
            id,
            TargetEdit::AddField {
                entry: entry.clone(),
                value_type: FieldType::Float,
            },
        );
    }
    (app, id, entry)
}

fn tab(app: &App, id: DeviceId) -> qtable_engine::model::FieldKey {
    edited_field(app, id).unwrap().1
}

#[test]
fn tabs_pick_the_edited_field_and_follow_adds_and_entry_changes() {
    let (mut app, id, entry) = three_fields();
    let keys: Vec<_> = fields(&app, id).iter().map(|f| f.key.clone()).collect();
    assert_eq!(tab(&app, id), keys[2], "a new field opens on its own tab");
    act(&mut app, Action::SelectField(id, keys[0].clone()));
    assert_eq!(tab(&app, id), keys[0]);
    // The tab leaves with a different entry, and the new entry opens on its first field.
    act(&mut app, Action::Add(id, true));
    let other = fields(&app, id)[0].key.clone();
    assert_eq!(tab(&app, id), other);
    act(&mut app, Action::Select(id, entry));
    assert_eq!(tab(&app, id), keys[0]);
    // A draft typed under one tab does not carry to the next.
    property(&mut app, id, 0, DefinitionField::Min, "-");
    act(&mut app, Action::SelectField(id, keys[1].clone()));
    assert!(app.drafts.values.is_empty());
}

#[test]
fn the_selected_tab_reorders_and_deletes_its_field() {
    let (mut app, id, entry) = three_fields();
    let keys: Vec<_> = fields(&app, id).iter().map(|f| f.key.clone()).collect();
    act(&mut app, Action::SelectField(id, keys[1].clone()));
    definition(
        &mut app,
        id,
        TargetEdit::MoveField {
            entry: entry.clone(),
            field: keys[1].clone(),
            to: 0,
        },
    );
    let order: Vec<_> = fields(&app, id).iter().map(|f| f.key.clone()).collect();
    assert_eq!(order, [keys[1].clone(), keys[0].clone(), keys[2].clone()]);
    assert_eq!(tab(&app, id), keys[1], "the tab moves with its field");
    definition(
        &mut app,
        id,
        TargetEdit::RemoveField {
            entry: entry.clone(),
            field: keys[1].clone(),
        },
    );
    assert_eq!(tab(&app, id), keys[0], "the neighbour takes over");
    act(&mut app, Action::SelectField(id, keys[2].clone()));
    definition(
        &mut app,
        id,
        TargetEdit::RemoveField {
            entry,
            field: keys[2].clone(),
        },
    );
    assert_eq!(tab(&app, id), keys[0], "removing the last field steps left");
}

#[test]
fn the_editor_renders_one_field_block_and_one_tab_per_field() {
    use crate::info::Id;
    let (mut app, id, entry) = three_fields();
    property(&mut app, id, 0, DefinitionField::Name, "x");
    property(&mut app, id, 1, DefinitionField::Name, "y");
    let keys: Vec<_> = fields(&app, id).iter().map(|f| f.key.clone()).collect();
    // The first tab found is field 1's (named x): clicking it selects that field.
    let messages = click_help(&app, id, Id::TargetFieldTab);
    assert!(matches!(
        messages.as_slice(),
        [Message::Device(crate::device_chain::Action::Target(
            Action::SelectField(_, key)
        ))] if *key == keys[0]
    ));
    // The block under the strip belongs to the tab: field 3's Name input only.
    let (_, target) = target(&app, id).unwrap();
    let mode = match entry_of(target, &entry) {
        Entry::Parameter(p) => Some(p.send_mode),
        Entry::Event(_) => None,
    };
    let mut widget = super::definitions::view(
        &app,
        id,
        &entry,
        fields(&app, id),
        mode,
        crate::control_focus::View::Track,
    );
    let ids: Vec<_> = crate::control_focus::test_registry::rectangles(&mut widget)
        .into_iter()
        .map(|(control, _)| control)
        .collect();
    let name = |index| {
        crate::drafts::ControlId::field(
            crate::control_focus::View::Track,
            DraftKey::Target(id, TargetField::Definition(index, DefinitionField::Name)),
        )
    };
    assert!(ids.contains(&name(2)));
    assert!(!ids.contains(&name(0)) && !ids.contains(&name(1)));
}

fn entry_of<'a>(target: &'a TargetDevice, key: &EntryKey) -> Entry<'a> {
    entry(target, key).unwrap()
}

#[test]
fn the_face_outlines_the_control_of_the_edited_field_only() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    use iced::advanced::{Layout, layout, renderer::Headless, widget::Tree};
    use iced::{Color, Rectangle, Size, mouse};
    let (mut app, id, _) = three_fields();
    if !app.project().device(id).unwrap().1.expanded {
        act(&mut app, Action::Fold(id));
    }
    let keys: Vec<_> = fields(&app, id).iter().map(|f| f.key.clone()).collect();
    let outline = app.theme.selection_background.into_rgba8();
    for (shown, other) in [(0, 1), (2, 0)] {
        act(&mut app, Action::SelectField(id, keys[shown].clone()));
        let mut widget =
            super::super::frame::device(&app, app.project().device(id).unwrap().1, View::Track);
        let rectangles = crate::control_focus::test_registry::rectangles(&mut widget);
        let bounds = |position: usize| {
            let wanted = ControlId::field(
                View::Track,
                DraftKey::Target(id, TargetField::Face(true, 0, position)),
            );
            rectangles.iter().find(|(c, _)| *c == wanted).unwrap().1
        };
        let mut renderer = crate::grid::test_renderer();
        let mut tree = Tree::new(&widget);
        let size = Size::new(800.0, 400.0);
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        widget.as_widget().draw(
            &tree,
            &mut renderer,
            &iced::Theme::Dark,
            &iced::advanced::renderer::Style::default(),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &Rectangle::with_size(size),
        );
        let pixels = renderer.screenshot(Size::new(800, 400), 1.0, Color::BLACK);
        // The left edge, halfway down the control.
        let edge = |position: usize| {
            let b = bounds(position);
            let at = ((b.y as u32 + 10) * 800 + b.x as u32) as usize * 4;
            pixels[at..at + 4].to_vec()
        };
        assert_eq!(edge(shown), outline, "field {shown} is outlined");
        assert_ne!(edge(other), outline, "field {other} is not");
    }
}

#[test]
fn number_boxes_and_editor_numbers_use_compact_device_widths() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    use crate::style::detail;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    let key = fields(&app, id)[0].key.clone();
    definition(
        &mut app,
        id,
        TargetEdit::SetFieldControl {
            entry,
            field: key,
            control: qtable_engine::model::Control::NumberBox,
        },
    );
    if !app.project().device(id).unwrap().1.expanded {
        act(&mut app, Action::Fold(id));
    }
    let mut widget =
        super::super::frame::device(&app, app.project().device(id).unwrap().1, View::Track);
    let rectangles = crate::control_focus::test_registry::rectangles_in(
        &mut widget,
        iced::Size::new(1000.0, 900.0),
    );
    let bounds = |control| rectangles.iter().find(|(id, _)| *id == control).unwrap().1;
    let field_id = |field| ControlId::field(View::Track, DraftKey::Target(id, field));
    assert_eq!(
        bounds(field_id(TargetField::Face(true, 0, 0))).width,
        detail::DEVICE_NUMBER_WIDTH
    );
    for property in [
        DefinitionField::Min,
        DefinitionField::Max,
        DefinitionField::Default,
        DefinitionField::Threshold,
    ] {
        assert_eq!(
            bounds(field_id(TargetField::Definition(0, property))).width,
            detail::DEVICE_NUMBER_WIDTH,
            "{property:?}"
        );
    }
    for name in ["type", "control"] {
        let control = ControlId::field_action(
            View::Track,
            DraftKey::Target(id, TargetField::Definition(0, DefinitionField::Name)),
            name,
        );
        assert_eq!(bounds(control).width, detail::DEVICE_CHOICE_WIDTH);
    }
    let min = bounds(field_id(TargetField::Definition(0, DefinitionField::Min)));
    let max = bounds(field_id(TargetField::Definition(0, DefinitionField::Max)));
    assert_eq!(min.y, max.y);
    assert!(min.x + min.width <= max.x);
    assert!(
        bounds(field_id(TargetField::Definition(0, DefinitionField::Name))).width
            > detail::DEVICE_NUMBER_WIDTH
    );
}

#[test]
fn the_expanded_target_lays_out_entry_editor_face_and_argument_inspector_without_scrolling() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    use crate::style::detail;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    if !app.project().device(id).unwrap().1.expanded {
        act(&mut app, Action::Fold(id));
    }
    let face = target(&app, id).map(|(_, t)| collapsed::width(t)).unwrap();
    let mut widget =
        super::super::frame::device(&app, app.project().device(id).unwrap().1, View::Track);
    let rectangles = crate::control_focus::test_registry::rectangles_in(
        &mut widget,
        iced::Size::new(1000.0, 900.0),
    );
    let bounds = |control| rectangles.iter().find(|(id, _)| *id == control).unwrap().1;
    let field = |field| ControlId::field(View::Track, DraftKey::Target(id, field));
    let target_path = bounds(field(TargetField::Path));
    let entry_path = bounds(field(TargetField::EntryPath));
    let argument_name = bounds(field(TargetField::Definition(0, DefinitionField::Name)));
    let face_control = bounds(field(TargetField::Face(true, 0, 0)));
    let entry_area = 0.0..ENTRY_WIDTH;
    let face_area = entry_area.end + 1.0..entry_area.end + 1.0 + face;
    // Target Path, the entry's fields and Delete sit in the entry column; the argument's
    // fields in the inspector to its right.
    for control in [target_path, entry_path] {
        assert!(entry_area.contains(&control.x));
        assert!(control.x + control.width <= entry_area.end);
    }
    assert!(face_area.contains(&face_control.x));
    assert!(face_control.x + face_control.width <= face_area.end);
    assert!(argument_name.x >= face_area.end + 1.0);
    // A typical argument needs no vertical scroll: every control ends above the body inset.
    let limit =
        detail::DEVICE_TITLE_HEIGHT + detail::DEVICE_BODY_HEIGHT - f32::from(detail::PANEL_INSET);
    for (control, rectangle) in &rectangles {
        let editor = rectangle.x < entry_area.end || rectangle.x > face_area.end;
        assert!(
            !editor || rectangle.y + rectangle.height <= limit,
            "{control:?} ends at {}",
            rectangle.y + rectangle.height
        );
    }
}

/// Tour feedback, 2026-10-16: "type and control should be grouped. So perhaps control should be below
/// type. And range should be grouped with minimum and maximum somehow."
#[test]
fn the_inspector_groups_control_under_type_range_over_min_and_max_and_threshold_apart() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    use crate::style::detail;
    for parameter in [true, false] {
        let (mut app, id) = app();
        act(&mut app, Action::Add(id, parameter));
        if !parameter {
            // A new event has no arguments yet.
            let entry = selected(&app, id).unwrap();
            definition(
                &mut app,
                id,
                TargetEdit::AddField {
                    entry,
                    value_type: qtable_engine::model::FieldType::Float,
                },
            );
        }
        if !app.project().device(id).unwrap().1.expanded {
            act(&mut app, Action::Fold(id));
        }
        let mut widget =
            super::super::frame::device(&app, app.project().device(id).unwrap().1, View::Track);
        let rectangles = crate::control_focus::test_registry::rectangles_in(
            &mut widget,
            iced::Size::new(1000.0, 900.0),
        );
        let bounds = |control: ControlId| {
            rectangles
                .iter()
                .find(|(id, _)| *id == control)
                .unwrap_or_else(|| panic!("no rectangle for {control:?}"))
                .1
        };
        let key = |field| DraftKey::Target(id, TargetField::Definition(0, field));
        let action = |field, name| ControlId::field_action(View::Track, key(field), name);
        let name = bounds(ControlId::field(View::Track, key(DefinitionField::Name)));
        let kind = bounds(action(DefinitionField::Name, "type"));
        let control = bounds(action(DefinitionField::Name, "control"));
        let min = bounds(ControlId::field(View::Track, key(DefinitionField::Min)));
        let max = bounds(ControlId::field(View::Track, key(DefinitionField::Max)));
        // Only a continuous parameter argument has a Send group; an event's is hidden.
        let threshold = parameter.then(|| {
            bounds(ControlId::field(
                View::Track,
                key(DefinitionField::Threshold),
            ))
        });
        // Control sits directly below Type: the same column, the next row.
        assert_eq!(control.x, kind.x, "parameter {parameter}");
        assert_eq!(control.width, kind.width);
        assert!(control.y > kind.y && control.y < kind.y + 2.0 * kind.height);
        assert_eq!(kind.y, name.y);
        // Min and Max share a row below Control, with a rule's room between the groups.
        assert_eq!(min.y, max.y);
        assert!(min.y > control.y + control.height + 2.0 * detail::CONTROL_GAP);
        if !parameter {
            // The Range switch heads its row: Range | Min | Max, the switch first and as wide as
            // a column, not shrunk to its label.
            let range = bounds(action(DefinitionField::Min, "range"));
            assert_eq!(range.y + range.height / 2.0, min.y + min.height / 2.0);
            assert!(range.x + range.width <= min.x && min.x < max.x);
            assert!(range.width > 60.0, "range {range:?}");
            assert!(range.y > control.y + control.height);
        }
        let limit = detail::DEVICE_TITLE_HEIGHT + detail::DEVICE_BODY_HEIGHT
            - f32::from(detail::PANEL_INSET);
        let Some(threshold) = threshold else {
            assert!(
                rectangles
                    .iter()
                    .all(|(id, _)| *id
                        != ControlId::field(View::Track, key(DefinitionField::Threshold))),
                "an event shows no Threshold"
            );
            continue;
        };
        // Threshold governs sending, so it forms its own group below Min and Max, a rule's room
        // away, in the left column, and the body still ends below it (no vertical scroll).
        assert!(threshold.y > min.y + min.height + 2.0 * detail::CONTROL_GAP);
        assert_eq!(threshold.x, min.x);
        assert_eq!(threshold.height, min.height, "Threshold is squeezed");
        let slack = limit - (threshold.y + threshold.height);
        assert!(slack >= 0.0, "slack {slack}");
    }
}

/// Tour feedback, 2026-11-02: "you have argument one, argument two, then the plus."
#[test]
fn the_add_argument_button_follows_the_last_tab() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    definition(
        &mut app,
        id,
        TargetEdit::AddField {
            entry,
            value_type: qtable_engine::model::FieldType::Float,
        },
    );
    let mut widget =
        super::super::frame::device(&app, app.project().device(id).unwrap().1, View::Track);
    let rectangles = crate::control_focus::test_registry::rectangles_in(
        &mut widget,
        iced::Size::new(1000.0, 900.0),
    );
    // Each button keys on the index of the editor that builds it: tabs their own, "+" the first.
    let action = |index, name| {
        let key = DraftKey::Target(id, TargetField::Definition(index, DefinitionField::Name));
        let wanted = ControlId::field_action(View::Track, key, name);
        rectangles.iter().find(|(c, _)| *c == wanted).unwrap().1
    };
    let (tab, add, back) = (action(1, "tab"), action(0, "add-field"), action(1, "up"));
    assert_eq!(tab.y + tab.height / 2.0, add.y + add.height / 2.0);
    assert!(add.x >= tab.x + tab.width, "tab {tab:?} add {add:?}");
    assert!(add.x < tab.x + tab.width + 12.0, "tab {tab:?} add {add:?}");
    assert!(add.x + add.width < back.x, "add {add:?} back {back:?}");
}

#[test]
fn an_enum_argument_hides_range_and_min_max_and_other_numbers_keep_them() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    let key = fields(&app, id)[0].key.clone();
    let has = |app: &App, field: DefinitionField, action: Option<&'static str>| {
        let mut widget =
            super::super::frame::device(app, app.project().device(id).unwrap().1, View::Track);
        let key = DraftKey::Target(id, TargetField::Definition(0, field));
        let wanted = match action {
            Some(name) => ControlId::field_action(View::Track, key, name),
            None => ControlId::field(View::Track, key),
        };
        crate::control_focus::test_registry::rectangles_in(
            &mut widget,
            iced::Size::new(1000.0, 900.0),
        )
        .iter()
        .any(|(control, _)| *control == wanted)
    };
    assert!(has(&app, DefinitionField::Min, None));
    definition(
        &mut app,
        id,
        TargetEdit::SetFieldControl {
            entry,
            field: key,
            control: qtable_engine::model::Control::Enum,
        },
    );
    for (field, action) in [
        (DefinitionField::Min, None),
        (DefinitionField::Max, None),
        (DefinitionField::Min, Some("range")),
    ] {
        assert!(!has(&app, field, action), "{field:?} {action:?}");
    }
    assert!(has(&app, DefinitionField::Name, Some("add-option")));
    // Threshold applies only to a continuous number, so an Enum's Send group is hidden too and
    // returns with the Slider (2026-11-02).
    assert!(!has(&app, DefinitionField::Threshold, None));
}

#[test]
fn the_send_group_shows_for_a_continuous_parameter_argument_only() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    use qtable_engine::model::Control;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    let entry = selected(&app, id).unwrap();
    let key = fields(&app, id)[0].key.clone();
    let shows = |app: &App| {
        let mut widget =
            super::super::frame::device(app, app.project().device(id).unwrap().1, View::Track);
        let wanted = ControlId::field(
            View::Track,
            DraftKey::Target(id, TargetField::Definition(0, DefinitionField::Threshold)),
        );
        crate::control_focus::test_registry::rectangles_in(
            &mut widget,
            iced::Size::new(1000.0, 900.0),
        )
        .iter()
        .any(|(control, _)| *control == wanted)
    };
    for (control, shown) in [
        (Control::Slider, true),
        (Control::Dial, true),
        (Control::NumberBox, true),
        (Control::Toggle, false),
        (Control::Enum, false),
        (Control::Slider, true),
    ] {
        definition(
            &mut app,
            id,
            TargetEdit::SetFieldControl {
                entry: entry.clone(),
                field: key.clone(),
                control,
            },
        );
        assert_eq!(shows(&app), shown, "{control:?}");
    }
    for value_type in [FieldType::Bool, FieldType::String] {
        definition(
            &mut app,
            id,
            TargetEdit::SetFieldType {
                entry: entry.clone(),
                field: key.clone(),
                value_type,
            },
        );
        assert!(!shows(&app), "{value_type:?}");
    }
}

/// What the inspector of the first argument of a new parameter or event shows of its kind-
/// dependent controls: (Range switch, Min, Max, option Value).
fn kind_dependent_controls(app: &App, id: DeviceId) -> (bool, bool, bool, bool) {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    let mut widget =
        super::super::frame::device(app, app.project().device(id).unwrap().1, View::Track);
    let rectangles = crate::control_focus::test_registry::rectangles_in(
        &mut widget,
        iced::Size::new(1000.0, 900.0),
    );
    let has = |wanted: ControlId| rectangles.iter().any(|(control, _)| *control == wanted);
    let key = |field| DraftKey::Target(id, TargetField::Definition(0, field));
    (
        has(ControlId::field_action(
            View::Track,
            key(DefinitionField::Min),
            "range",
        )),
        has(ControlId::field(View::Track, key(DefinitionField::Min))),
        has(ControlId::field(View::Track, key(DefinitionField::Max))),
        has(ControlId::field(
            View::Track,
            key(DefinitionField::OptionValue(0)),
        )),
    )
}

#[test]
fn range_controls_show_only_for_a_kind_that_has_a_range() {
    use qtable_engine::model::Control;
    for parameter in [true, false] {
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
        let key = fields(&app, id)[0].key.clone();
        let set_control = |app: &mut App, control| {
            definition(
                app,
                id,
                TargetEdit::SetFieldControl {
                    entry: entry.clone(),
                    field: key.clone(),
                    control,
                },
            );
        };
        // An event's Range switch comes with its Min and Max; a parameter has no switch.
        let continuous = (!parameter, true, true, false);
        let none = (false, false, false, false);
        if !parameter {
            // Turn the range on, as a Toggle or Enum needs one.
            definition(
                &mut app,
                id,
                TargetEdit::SetFieldRange {
                    entry: entry.clone(),
                    field: key.clone(),
                    range: Some([0.0, 1.0]),
                },
            );
        }
        for (control, expected) in [
            (Control::Slider, continuous),
            (Control::Dial, continuous),
            (Control::NumberBox, continuous),
            (Control::Toggle, none),
            // An Enum shows its options (with a Value each, for a Float) where a range would be.
            (Control::Enum, (false, false, false, true)),
            (Control::NumberBox, continuous),
        ] {
            set_control(&mut app, control);
            assert_eq!(
                kind_dependent_controls(&app, id),
                expected,
                "parameter {parameter}, {control:?}"
            );
        }
        for value_type in [FieldType::Int, FieldType::Bool, FieldType::String] {
            definition(
                &mut app,
                id,
                TargetEdit::SetFieldType {
                    entry: entry.clone(),
                    field: key.clone(),
                    value_type,
                },
            );
            let expected = match value_type {
                FieldType::Int => continuous,
                _ => none,
            };
            assert_eq!(
                kind_dependent_controls(&app, id),
                expected,
                "parameter {parameter}, {value_type:?}"
            );
        }
    }
}

#[test]
fn a_range_switched_off_keeps_its_min_and_max_in_place_dimmed() {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    let entry = selected(&app, id).unwrap();
    definition(
        &mut app,
        id,
        TargetEdit::AddField {
            entry,
            value_type: FieldType::Float,
        },
    );
    // A new event's Float argument has no range yet: the controls apply but are off.
    assert_eq!(kind_dependent_controls(&app, id), (true, true, true, false));
}

#[test]
fn a_string_enum_option_shows_no_value_field_but_a_numeric_one_does() {
    use qtable_engine::model::Control;
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, false));
    let entry = selected(&app, id).unwrap();
    definition(
        &mut app,
        id,
        TargetEdit::AddField {
            entry: entry.clone(),
            value_type: FieldType::Float,
        },
    );
    let key = fields(&app, id)[0].key.clone();
    definition(
        &mut app,
        id,
        TargetEdit::SetFieldRange {
            entry: entry.clone(),
            field: key.clone(),
            range: Some([0.0, 1.0]),
        },
    );
    definition(
        &mut app,
        id,
        TargetEdit::SetFieldControl {
            entry: entry.clone(),
            field: key.clone(),
            control: Control::Enum,
        },
    );
    assert!(kind_dependent_controls(&app, id).3, "Float options");
    definition(
        &mut app,
        id,
        TargetEdit::SetFieldType {
            entry,
            field: key,
            value_type: FieldType::String,
        },
    );
    assert_eq!(
        kind_dependent_controls(&app, id),
        (false, false, false, false)
    );
}

/// An event, then a two-argument parameter, with the event selected.
fn event_and_two_argument_parameter() -> (App, DeviceId) {
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    let parameter = selected(&app, id).unwrap();
    definition(
        &mut app,
        id,
        TargetEdit::AddField {
            entry: parameter,
            value_type: FieldType::Float,
        },
    );
    act(&mut app, Action::Add(id, false));
    (app, id)
}

#[test]
fn a_face_edit_while_expanded_selects_its_entry_and_argument_and_still_applies() {
    let (mut app, id) = event_and_two_argument_parameter();
    let event = selected(&app, id).unwrap();
    let field = face_field(&app, id, true, 0, 1);
    act(&mut app, Action::Value(field.clone(), 0.5, true));
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[1].value,
        Some(0.5)
    );
    assert_eq!(selected(&app, id), Some(field.entry.clone()));
    assert_ne!(selected(&app, id), Some(event));
    assert_eq!(edited_field(&app, id), Some((field.entry, field.field)));
    // Another argument of the same entry moves only the tab.
    let first = face_field(&app, id, true, 0, 0);
    act(&mut app, Action::Value(first.clone(), 0.25, false));
    assert_eq!(edited_field(&app, id), Some((first.entry, first.field)));
}

#[test]
fn typing_in_a_face_number_box_and_firing_a_bang_select_while_expanded() {
    let (mut app, id) = event_and_two_argument_parameter();
    let event = selected(&app, id).unwrap();
    let field = face_field(&app, id, true, 0, 1);
    act(&mut app, Action::Exact(field.clone(), "0.3".into()));
    assert_eq!(edited_field(&app, id), Some((field.entry, field.field)));
    act(&mut app, Action::Fire(id, event.clone()));
    assert_eq!(selected(&app, id), Some(event));
}

#[test]
fn pressing_a_face_control_while_expanded_selects_its_argument() {
    use crate::control_focus::View;
    use crate::drafts::ControlId;
    let (mut app, id) = event_and_two_argument_parameter();
    let control = ControlId::field(
        View::Track,
        DraftKey::Target(id, TargetField::Face(true, 0, 1)),
    );
    for message in click_face(&app, id, control, true) {
        let _ = crate::update::update(&mut app, message);
    }
    let field = face_field(&app, id, true, 0, 1);
    assert_eq!(edited_field(&app, id), Some((field.entry, field.field)));
}

#[test]
fn a_face_edit_while_collapsed_selects_no_entry() {
    let (mut app, id) = event_and_two_argument_parameter();
    act(&mut app, Action::Fold(id));
    app.device_chain.entry.clear();
    app.device_chain.field.clear();
    let field = face_field(&app, id, true, 0, 1);
    act(&mut app, Action::Value(field.clone(), 0.5, true));
    assert_eq!(
        target(&app, id).unwrap().1.parameters[0].fields[1].value,
        Some(0.5)
    );
    assert_eq!(selected(&app, id), None);
    assert!(app.device_chain.field.is_empty());
    let control = crate::drafts::ControlId::field(
        crate::control_focus::View::Track,
        DraftKey::Target(id, TargetField::Face(true, 0, 0)),
    );
    for message in click_face(&app, id, control, true) {
        let _ = crate::update::update(&mut app, message);
    }
    assert_eq!(selected(&app, id), None);
}

#[path = "live_selection_tests.rs"]
mod live_selection;

#[test]
fn the_face_declares_its_parameter_arguments_and_event_bangs() {
    use qtable_engine::model::{ControlRef, EventRef, describe_control};
    let (mut app, id) = app();
    act(&mut app, Action::Add(id, true));
    act(&mut app, Action::Add(id, false));
    let (track, device) = app.project().device(id).unwrap();
    let target = device.target().unwrap();
    let parameter = target.parameters[0].key.clone();
    let event = target.events[0].key.clone();
    let field = target.parameters[0].fields[0].key.clone();
    let track = track.id;
    let mut element = super::super::face::view(
        &app,
        app.project().device(id).unwrap().1,
        crate::control_focus::View::Track,
    );
    let declared =
        crate::control_focus::test_registry::declared(&mut element, iced::Size::new(1600.0, 600.0));
    let argument = ControlRef::Argument(qtable_engine::model::FieldRef {
        parameter: qtable_engine::model::ParameterRef {
            track,
            device: qtable_engine::model::DeviceRef::Chain(id),
            key: qtable_engine::model::ParameterKey(parameter.0),
        },
        field,
    });
    let bang = ControlRef::Event {
        track,
        event: EventRef {
            device: id,
            key: event,
        },
    };
    // The event's argument defaults drive nothing; only the parameter and the bang do.
    assert_eq!(declared.len(), 2, "{declared:?}");
    for control in [argument, bang] {
        assert!(declared.contains(&control), "{control:?}");
        assert!(
            describe_control(app.project(), &control).is_some(),
            "{control:?}"
        );
    }
}
