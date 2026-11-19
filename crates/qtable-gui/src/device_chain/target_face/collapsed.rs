//! Generated controls in whole-entry columns; the title strip selects an entry while expanded.
use super::{Action, edit, run, target};
use crate::app::App;
use crate::control::device_layout;
use crate::control::{self, Dropdown, Number, SizeRole, Text, Toggle, ValueFormat};
use crate::control_focus::View;
use crate::drafts::{Control, ControlId, DraftKey, TargetField};
use crate::info::{EntryRef, Help, Id, Subject};
use crate::messages::Message;
use crate::parameter_gesture;
use crate::style::detail;
use iced::widget::{column, container, mouse_area, row, scrollable, text};
use iced::{Element, Length, Task};
use qtable_engine::edit::{ProjectEdit, TargetEdit};
use qtable_engine::model::{
    Control as Kind, ControlRef, Device, DeviceId, DeviceRef, EntryKey, EventRef, FieldDef,
    FieldRef, FieldType, FieldView, ParameterKey, ParameterRef, TargetDevice,
};

/// Room for cells below the headings.
const HEIGHT: f32 =
    detail::DEVICE_BODY_HEIGHT - 2.0 * detail::DEVICE_BODY_INSET - detail::DEVICE_HEADING;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FaceField {
    pub device: DeviceId,
    pub entry: EntryKey,
    pub field: qtable_engine::model::FieldKey,
    pub parameter: bool,
}

#[derive(Default)]
pub(crate) struct State {
    pub exact: Option<(FaceField, String)>,
    pub error: bool,
}

pub(super) fn fields<'a>(app: &'a App, key: &FaceField) -> Option<&'a FieldDef> {
    let (_, t) = target(app, key.device)?;
    let fields = if key.parameter {
        &t.parameters.iter().find(|p| p.key == key.entry)?.fields
    } else {
        &t.events.iter().find(|e| e.key == key.entry)?.fields
    };
    fields.iter().find(|f| f.key == key.field)
}

/// The face control's spec, named as its label reads, so a range refusal names the argument.
fn named_spec(app: &App, key: &FaceField) -> Option<qtable_engine::model::ParameterSpec> {
    let (_, t) = target(app, key.device)?;
    if key.parameter {
        let p = t.parameters.iter().find(|p| p.key == key.entry)?;
        p.field_spec(p.fields.iter().position(|f| f.key == key.field)?)
    } else {
        let e = t.events.iter().find(|e| e.key == key.entry)?;
        e.field_spec(e.fields.iter().position(|f| f.key == key.field)?)
    }
}

fn reference(app: &App, key: &FaceField) -> Option<FieldRef> {
    Some(FieldRef {
        parameter: ParameterRef {
            track: app.project().device(key.device)?.0.id,
            device: DeviceRef::Chain(key.device),
            key: ParameterKey(key.entry.0.clone()),
        },
        field: key.field.clone(),
    })
}

/// The face control's On Release hold, if one is in progress.
#[cfg(test)]
pub(crate) fn held(app: &App, key: &FaceField) -> Option<f64> {
    app.parameter_gesture.held(&reference(app, key)?)
}

/// The face control a held Target argument belongs to.
fn face_key(app: &App, field: &FieldRef) -> Option<FaceField> {
    let DeviceRef::Chain(device) = field.parameter.device else {
        return None;
    };
    (app.project().device(device)?.0.id == field.parameter.track).then(|| FaceField {
        device,
        entry: EntryKey(field.parameter.key.0.clone()),
        field: field.field.clone(),
        parameter: true,
    })
}

/// Clears the face's own state and the Target holds it showed, as a fold, a definition edit,
/// Escape or a vanished field does. The Mixer's hold is not the face's to drop.
pub(crate) fn reset_face(app: &mut App) {
    app.device_chain.face = Default::default();
    app.parameter_gesture.end_chain();
}

/// Choosing from the face: the device, and while expanded the argument's entry and tab too.
fn choose_argument(app: &mut App, key: &FaceField) {
    choose(app, key.device);
    super::select_argument(app, key.device, &key.entry, &key.field);
}

fn choose(app: &mut App, device: DeviceId) {
    app.selection.devices = vec![device];
    app.selection.device_anchor = Some(device);
    app.focus = crate::focus::Focus::Detail;
}

/// Return the argument to its default as a release: one edit, and no hold or typed draft left.
pub(super) fn reset(app: &mut App, key: FaceField) {
    let Some(default) = fields(app, &key).map(FieldDef::default_value) else {
        return;
    };
    if app
        .device_chain
        .face
        .exact
        .as_ref()
        .is_some_and(|(k, _)| k == &key)
    {
        app.device_chain.face.exact = None;
        app.device_chain.face.error = false;
    }
    value(app, key, default, true);
}

pub(super) fn value(app: &mut App, key: FaceField, value: f64, release: bool) {
    let Some(field) = fields(app, &key) else {
        return;
    };
    let spec = named_spec(app, &key).unwrap_or_else(|| field.spec());
    let value = match spec.check(value) {
        Ok(value) => value,
        Err(error) => {
            app.status.show_error(error);
            return;
        }
    };
    if key.parameter {
        let reference = reference(app, &key).unwrap();
        let send = if release {
            parameter_gesture::release(app, &reference, value)
        } else {
            parameter_gesture::drag(app, &reference, value)
        };
        if send.is_some() {
            run(
                app,
                ProjectEdit::SetParameter {
                    target: reference.parameter,
                    values: vec![(reference.field, value)],
                },
            );
        }
    } else {
        let argument = argument(field, value);
        if argument != field.current_argument() {
            run(
                app,
                edit(
                    key.device,
                    TargetEdit::SetFieldValue {
                        entry: key.entry.clone(),
                        field: key.field.clone(),
                        value: argument,
                    },
                ),
            );
        }
    }
    choose_argument(app, &key);
}

fn argument(field: &FieldDef, value: f64) -> qtable_engine::model::OscArgument {
    FieldView {
        key: std::borrow::Cow::Borrowed(&field.key),
        spec: std::borrow::Cow::Owned(field.spec()),
        value,
        value_type: field.value_type,
        control: field.control,
        options: &field.options,
        threshold: 0.0,
    }
    .argument()
}

/// Query after a mouse release has reached the text input. Match the captured draft and
/// project generation on reply, just as other pending fields and device renames do.
pub(crate) fn check_exact(app: &App) -> Task<Message> {
    let Some((field, text)) = app.device_chain.face.exact.clone() else {
        return Task::none();
    };
    let generation = app.project_generation;
    crate::field_focus::check_widget_focus(iced::widget::Id::new("target-face-exact")).map(
        move |focused| {
            super::view::msg(Action::ExactFocusChecked(
                field.clone(),
                text.clone(),
                generation,
                focused,
            ))
        },
    )
}

pub(super) fn commit(app: &mut App, submitted: &FaceField) {
    let Some((key, text)) = app.device_chain.face.exact.clone() else {
        return;
    };
    if &key != submitted {
        return;
    }
    let Some(field) = fields(app, &key) else {
        return;
    };
    let built = if field.control == Kind::Text {
        Ok(edit(
            key.device,
            TargetEdit::SetFieldValue {
                entry: key.entry.clone(),
                field: key.field.clone(),
                value: qtable_engine::model::OscArgument::String(text.clone()),
            },
        ))
    } else {
        named_spec(app, &key)
            .unwrap_or_else(|| field.spec())
            .parse(&text)
            .map(|value| {
                if key.parameter {
                    let reference = reference(app, &key).unwrap();
                    ProjectEdit::SetParameter {
                        target: reference.parameter,
                        values: vec![(reference.field, value)],
                    }
                } else {
                    edit(
                        key.device,
                        TargetEdit::SetFieldValue {
                            entry: key.entry.clone(),
                            field: key.field.clone(),
                            value: argument(field, value),
                        },
                    )
                }
            })
    };
    let ok = match built {
        Ok(edit) => run(app, edit).is_some(),
        Err(error) => {
            app.status.show_error(error);
            false
        }
    };
    app.device_chain.face.error = !ok;
    if ok {
        app.device_chain.face.exact = None;
        if let Some(reference) = key.parameter.then(|| reference(app, &key)).flatten() {
            app.parameter_gesture.end(&reference);
        }
    }
    // Exact entry selected its argument when typing began. A blur commit must keep
    // the selection established by the outside click.
}

pub(super) fn fire(app: &mut App, device: DeviceId, key: EntryKey) {
    let Some((track, _)) = app.project().device(device) else {
        return;
    };
    let track = track.id;
    choose(app, device);
    if app
        .project()
        .device(device)
        .is_some_and(|(_, d)| d.expanded)
    {
        super::set_entry(app, device, key.clone());
    }
    crate::edit_actions::execute(
        app,
        qtable_engine::engine::Command::FireEvent {
            track,
            event: qtable_engine::model::EventRef { device, key },
        },
    );
}

pub(super) fn reconcile(app: &mut App) {
    let valid = |key: &FaceField| {
        app.project()
            .device(key.device)
            .is_some_and(|(track, _)| super::super::shown_track(app) == Some(track.id))
            && fields(app, key).is_some()
            && super::super::shown(app)
            && app.chrome.detail_open
    };
    if app
        .device_chain
        .face
        .exact
        .as_ref()
        .is_some_and(|(k, _)| !valid(k))
        || app
            .parameter_gesture
            .chain_fields()
            .any(|field| face_key(app, field).is_none_or(|k| !valid(&k)))
    {
        reset_face(app);
        app.control_focus.clear();
    }
}

/// Adjacent dials share a row; other controls retain their full column width.
fn field_rows(fields: &[FieldDef]) -> Vec<std::ops::Range<usize>> {
    let mut rows = Vec::new();
    let mut start = 0;
    while start < fields.len() {
        let paired = fields[start].control == Kind::Dial
            && fields
                .get(start + 1)
                .is_some_and(|f| f.control == Kind::Dial);
        let end = start + if paired { 2 } else { 1 };
        rows.push(start..end);
        start = end;
    }
    rows
}

/// A one-field parameter's title strip already names it, so its control carries no label.
fn labelled(fields: &[FieldDef], parameter: bool) -> bool {
    !(parameter && fields.len() == 1)
}

/// Number box, Text and Enum carry their label in a row above; a Toggle, Slider and
/// action name themselves, and a Dial draws its label inside its cell.
pub(super) fn label_row(control: Kind) -> bool {
    matches!(control, Kind::NumberBox | Kind::Text | Kind::Enum)
}

fn field_height(fields: &[FieldDef], parameter: bool) -> f32 {
    let rows = field_rows(fields);
    let labelled = labelled(fields, parameter);
    rows.iter()
        .map(|r| match fields[r.start].control {
            Kind::Dial => crate::control::dial::HEIGHT,
            control if labelled && label_row(control) => detail::DEVICE_LABELLED_ROW,
            _ => detail::CONTROL_HEIGHT,
        })
        .sum::<f32>()
        + rows.len().saturating_sub(1) as f32 * detail::DEVICE_ROW_GAP
}

/// A cell: its edge, an 18 px title strip, then its fields' controls.
fn cell_height(fields: &[FieldDef], parameter: bool) -> f32 {
    2.0 * detail::DEVICE_CELL_EDGE
        + detail::DEVICE_CELL_STRIP
        + if fields.is_empty() {
            0.0
        } else {
            detail::DEVICE_ROW_GAP + field_height(fields, parameter) + detail::DEVICE_CELL_PADDING
        }
}

fn heights(target: &TargetDevice) -> Vec<f32> {
    target
        .events
        .iter()
        .map(|e| cell_height(&e.fields, false))
        .chain(
            target
                .parameters
                .iter()
                .map(|p| cell_height(&p.fields, true)),
        )
        .collect()
}

/// Ranges of whole blocks, packed into the fixed body. Tall sections stand alone.
fn columns(heights: &[f32]) -> Vec<std::ops::Range<usize>> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut used = 0.0;
    for (index, height) in heights.iter().enumerate() {
        if index > start && used + detail::CONTROL_GAP + height > HEIGHT {
            result.push(start..index);
            start = index;
            used = 0.0;
        }
        used += if index > start {
            detail::CONTROL_GAP
        } else {
            0.0
        } + height;
    }
    result.push(start..heights.len());
    result
}

/// A column holding one block taller than the body scrolls on its own.
fn is_tall(sizes: &[f32], range: &std::ops::Range<usize>) -> bool {
    range.len() == 1 && sizes[range.start] > HEIGHT
}

/// The Events and Parameters sections, each with its columns as ranges over all cells.
/// Events flow in columns extending right; Parameters always start a new column.
fn sections(
    target: &TargetDevice,
    sizes: &[f32],
) -> Vec<(&'static str, Vec<std::ops::Range<usize>>)> {
    let split = target.events.len();
    let shifted = |range: std::ops::Range<usize>, by: usize| range.start + by..range.end + by;
    let mut result = Vec::new();
    if split > 0 {
        result.push(("EVENTS", columns(&sizes[..split])));
    }
    if split < sizes.len() {
        let parameters = columns(&sizes[split..])
            .into_iter()
            .map(|r| shifted(r, split))
            .collect();
        result.push(("PARAMETERS", parameters));
    }
    result
}

/// A section's width: its columns, any scroll gutters and the gaps between them.
fn span(sizes: &[f32], columns: &[std::ops::Range<usize>]) -> f32 {
    let count = columns.len() as f32;
    let scrolling = columns.iter().filter(|r| is_tall(sizes, r)).count() as f32;
    count * detail::DEVICE_CELL_WIDTH
        + scrolling * detail::DEVICE_SCROLL_GUTTER
        + (count - 1.0) * detail::CONTROL_GAP
}

pub(in crate::device_chain) fn width(target: &TargetDevice) -> f32 {
    let sizes = heights(target);
    let sections = sections(target, &sizes);
    // The empty face's hint text takes one cell's width.
    let cells = if sections.is_empty() {
        detail::DEVICE_CELL_WIDTH
    } else {
        0.0
    };
    cells
        + sections.iter().map(|(_, c)| span(&sizes, c)).sum::<f32>()
        + sections.len().saturating_sub(1) as f32 * detail::CONTROL_GAP
        + 2.0 * detail::DEVICE_BODY_INSET
}

pub(super) fn view<'a>(app: &'a App, device: &'a Device, location: View) -> Element<'a, Message> {
    let (track, target) = target(app, device.id).unwrap();
    if target.events.is_empty() && target.parameters.is_empty() {
        return container(
            text(if device.expanded {
                "Add events and parameters on the left"
            } else {
                "Expand to add events and parameters"
            })
            .size(detail::TEXT_SIZE)
            .color(app.theme.text_disabled),
        )
        .padding(detail::DEVICE_BODY_INSET)
        .into();
    }
    let sizes = heights(target);
    let mut flow = row![].spacing(detail::CONTROL_GAP);
    for (heading, ranges) in sections(target, &sizes) {
        let span = span(&sizes, &ranges);
        let mut columns = row![].spacing(detail::CONTROL_GAP);
        for range in ranges {
            let tall = is_tall(&sizes, &range);
            let mut blocks = column![]
                .spacing(detail::CONTROL_GAP)
                .width(detail::DEVICE_CELL_WIDTH);
            for at in range {
                let parameter = at >= target.events.len();
                let index = if parameter {
                    at - target.events.len()
                } else {
                    at
                };
                let (key, name, fields) = if parameter {
                    let p = &target.parameters[index];
                    (&p.key, p.display_name().to_owned(), &p.fields)
                } else {
                    let e = &target.events[index];
                    (
                        &e.key,
                        track
                            .event_label(&qtable_engine::model::EventRef {
                                device: device.id,
                                key: e.key.clone(),
                            })
                            .into_owned(),
                        &e.fields,
                    )
                };
                let entry = EntryRef::target(parameter, track.id, device.id, key);
                let subject = Subject::TargetEntry(entry.clone());
                let title = container(
                    device_layout::title_text(app.theme, name.clone(), device.active)
                        .wrapping(text::Wrapping::None),
                )
                .width(Length::Fill)
                .height(detail::DEVICE_CELL_STRIP)
                .align_y(iced::alignment::Vertical::Center)
                .clip(true);
                // Expanded, the face doubles as the item list: the title selects the entry.
                let title: Element<'a, Message> = if device.expanded {
                    mouse_area(title)
                        .on_press(super::view::msg(Action::Select(device.id, key.clone())))
                        .into()
                } else {
                    title.into()
                };
                let mut strip = row![title.info_about(
                    if parameter {
                        Id::TargetParameter
                    } else {
                        Id::TargetEvent
                    },
                    subject.clone(),
                    Message::Info
                )]
                .align_y(iced::Alignment::Center)
                .height(detail::DEVICE_CELL_STRIP);
                if !parameter {
                    let bang =
                        crate::control::widgets::Controls::new(app.theme, &app.control_focus)
                            .about(&subject)
                            .canvas(
                                Control::new(
                                    ControlId::field_action(
                                        location,
                                        DraftKey::Target(
                                            device.id,
                                            TargetField::Face(false, index, 0),
                                        ),
                                        "fire",
                                    ),
                                    Id::TargetFire,
                                    control::Bang {
                                        active: device.active,
                                    },
                                )
                                .drives(ControlRef::Event {
                                    track: track.id,
                                    event: EventRef {
                                        device: device.id,
                                        key: key.clone(),
                                    },
                                })
                                .lit(app.event_feedback.lit(
                                    track.id,
                                    &EventRef {
                                        device: device.id,
                                        key: key.clone(),
                                    },
                                    std::time::Instant::now(),
                                ))
                                .role(SizeRole::Fixed {
                                    width: detail::DEVICE_CELL_STRIP,
                                    height: detail::DEVICE_CELL_STRIP,
                                }),
                                Some(super::view::msg(Action::Fire(device.id, key.clone()))),
                            );
                    strip = strip.push(bang);
                }
                let mut body = column![].spacing(detail::DEVICE_ROW_GAP);
                for range in field_rows(fields) {
                    let paired = range.len() > 1;
                    let mut controls =
                        row![].spacing(if paired { 0.0 } else { detail::CONTROL_GAP });
                    for position in range {
                        let field = &fields[position];
                        let label = if labelled(fields, parameter) {
                            field.display_name(position).into_owned()
                        } else {
                            String::new()
                        };
                        controls = controls.push(control(
                            app,
                            device,
                            FaceField {
                                device: device.id,
                                entry: key.clone(),
                                field: field.key.clone(),
                                parameter,
                            },
                            field,
                            label,
                            location,
                            (index, position),
                            Subject::TargetField(entry.clone(), field.key.clone()),
                        ));
                    }
                    body = body.push(controls);
                }
                let chosen =
                    device.expanded && super::selected(app, device.id).as_ref() == Some(key);
                let block = device_layout::cell_with(
                    app.theme,
                    device_layout::Title::Element(strip.into()),
                    detail::DEVICE_CELL_WIDTH,
                    Length::Shrink,
                    (!fields.is_empty()).then(|| body.into()),
                    chosen,
                );
                blocks = blocks.push(block);
            }
            let section: Element<'a, Message> = if tall {
                scrollable(blocks)
                    .direction(scrollable::Direction::Vertical(detail::compact_scrollbar()))
                    .width(detail::DEVICE_CELL_WIDTH + detail::DEVICE_SCROLL_GUTTER)
                    .height(Length::Fill)
                    .into()
            } else {
                container(blocks)
                    .width(detail::DEVICE_CELL_WIDTH)
                    .height(Length::Fill)
                    .into()
            };
            columns = columns.push(section);
        }
        flow = flow.push(
            container(device_layout::section_heading(app.theme, heading, columns)).width(span),
        );
    }
    container(flow)
        .padding(detail::DEVICE_BODY_INSET)
        .height(Length::Fill)
        .into()
}

#[derive(Clone, PartialEq)]
struct OptionLabel(usize, String);
impl std::fmt::Display for OptionLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.1)
    }
}

/// The dropdown lists bare option labels; the field's name sits above it.
fn option_labels(field: &FieldDef) -> Vec<OptionLabel> {
    field
        .options
        .iter()
        .enumerate()
        .map(|(i, o)| OptionLabel(i, o.label.clone()))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn control<'a>(
    app: &'a App,
    device: &Device,
    key: FaceField,
    field: &'a FieldDef,
    label: String,
    location: View,
    (index, position): (usize, usize),
    subject: Subject,
) -> Element<'a, Message> {
    let dial = field.control == Kind::Dial;
    let (width, height) = if dial {
        (crate::control::dial::WIDTH, crate::control::dial::HEIGHT)
    } else if field.control == Kind::NumberBox {
        (detail::DEVICE_NUMBER_WIDTH, detail::CONTROL_HEIGHT)
    } else {
        (detail::DEVICE_CONTROL_WIDTH, detail::CONTROL_HEIGHT)
    };
    let id = ControlId::field(
        location,
        DraftKey::Target(
            key.device,
            TargetField::Face(key.parameter, index, position),
        ),
    );
    let stored = field.value.unwrap_or_else(|| field.default_value());
    let value = if key.parameter {
        reference(app, &key)
            .and_then(|reference| parameter_gesture::shown_value(app, &reference))
            .unwrap_or(stored)
    } else {
        stored
    };
    // A deactivated device stays editable; its controls draw in their inactive colours.
    let inactive = !device.active;
    let controls = app.controls().about(&subject);
    // A numeric argument is what a controller can drive, on a parameter and on an event alike
    // (an event's sets the value its Bang sends and sends nothing itself); a text argument is
    // not.
    let drives = (field.control != Kind::Text)
        .then(|| {
            if key.parameter {
                reference(app, &key).map(ControlRef::Argument)
            } else {
                Some(ControlRef::EventArgument {
                    track: app.project().device(key.device)?.0.id,
                    event: EventRef {
                        device: key.device,
                        key: key.entry.clone(),
                    },
                    field: key.field.clone(),
                })
            }
        })
        .flatten();
    let exact = app
        .device_chain
        .face
        .exact
        .as_ref()
        .filter(|(k, _)| k == &key);
    let help = if field.control == Kind::Text {
        Id::TargetText
    } else if exact.is_some() {
        Id::TargetNumber
    } else {
        match field.control {
            Kind::Toggle => Id::TargetToggle,
            Kind::Enum => Id::TargetEnum,
            Kind::NumberBox => Id::TargetNumber,
            Kind::Text => Id::TargetText,
            Kind::Slider | Kind::Dial => Id::TargetValue,
        }
    };
    // A label keeps its ink in an inactive device, as in Live: only a control that is itself
    // disabled would dim it, and no face control is.
    let label_state = control::State::default();
    let caption = (label_row(field.control) && !label.is_empty()).then(|| label.clone());
    let widget = if exact.is_some() || matches!(field.control, Kind::NumberBox | Kind::Text) {
        let stored = if field.control == Kind::Text {
            field.text.clone().unwrap_or_default()
        } else {
            display(field, value)
        };
        let draft_text = exact.map(|(_, s)| s.as_str()).unwrap_or(&stored);
        let draft_key = key.clone();
        let on_input = move |s| super::view::msg(Action::Exact(draft_key.clone(), s));
        let on_submit = super::view::msg(Action::CommitExact(key.clone()));
        let invalid = exact.is_some() && app.device_chain.face.error;
        // Active exact drafts share an id for blur queries and the canvas-to-input handoff.
        let exact_id = exact
            .is_some()
            .then(|| iced::widget::Id::new("target-face-exact"));
        let input = if field.control == Kind::Text {
            let text = Control::new(
                id,
                help,
                Text {
                    placeholder: "".into(),
                },
            )
            .role(SizeRole::Fill)
            .inactive(inactive)
            .invalid(invalid)
            .input_id(exact_id);
            controls.text(text, draft_text, on_input, on_submit)
        } else {
            let number = Control::new(id, help, Number::new(""))
                .drives_when(drives.clone())
                .nudge(
                    if field.range.is_some() || field.value_type == FieldType::Int {
                        crate::control::Nudge::from_spec(&field.spec())
                    } else {
                        crate::control::Nudge::FLOAT
                    },
                )
                .role(SizeRole::Fill)
                .inactive(inactive)
                .invalid(invalid)
                .input_id(exact_id);
            let reset = super::view::msg(Action::Reset(key.clone()));
            controls.number(number, draft_text, on_input, on_submit, Some(reset))
        };
        if dial {
            column![
                container(
                    controls
                        .readout(label, label_state)
                        .wrapping(text::Wrapping::None)
                )
                .width(Length::Fill)
                .clip(true),
                iced::widget::Space::new().height(Length::Fill),
                input,
            ]
            .align_x(iced::Alignment::Center)
            .into()
        } else {
            input
        }
    } else {
        match field.control {
            Kind::Toggle => controls.toggle(
                Control::new(
                    id,
                    help,
                    Toggle {
                        label: toggle_label(field, label, value),
                    },
                )
                .drives_when(drives.clone())
                .role(SizeRole::Fill)
                .inactive(inactive)
                .lit(value != 0.0),
                super::view::msg(Action::Value(
                    key.clone(),
                    if value == 0.0 { 1.0 } else { 0.0 },
                    true,
                )),
            ),
            Kind::Enum => {
                let options = option_labels(field);
                let selected = options.get(value.round() as usize).cloned();
                let key = key.clone();
                controls.dropdown(
                    Control::new(id, help, Dropdown)
                        .drives_when(drives.clone())
                        .role(SizeRole::Fill),
                    options,
                    selected,
                    move |option| {
                        super::view::msg(Action::Value(key.clone(), option.0 as f64, true))
                    },
                )
            }
            Kind::Dial => {
                let spec = field.spec();
                let (min, max) = (spec.min, spec.max);
                let drag = key.clone();
                let release = key.clone();
                controls.dial(
                    Control::new(
                        id,
                        help,
                        crate::control::Dial {
                            fraction: (value - min) / (max - min),
                            label,
                            value: display(field, value),
                        },
                    )
                    .drives_when(drives.clone())
                    .inactive(inactive)
                    .default_to((field.default_value() - min) / (max - min))
                    .nudge(crate::control::Nudge::from_spec(&spec).fraction()),
                    move |fraction| {
                        super::view::msg(Action::Value(
                            drag.clone(),
                            min + fraction * (max - min),
                            false,
                        ))
                    },
                    move |fraction| {
                        super::view::msg(Action::Value(
                            release.clone(),
                            min + fraction * (max - min),
                            true,
                        ))
                    },
                )
            }
            Kind::Slider => {
                let spec = field.spec();
                let (min, max) = (spec.min, spec.max);
                let drag = key.clone();
                let release = key.clone();
                controls.slider(
                    Control::new(
                        id,
                        help,
                        crate::control::Slider {
                            fraction: (value - min) / (max - min),
                            label: if label.is_empty() {
                                display(field, value)
                            } else {
                                format!("{label} {}", display(field, value))
                            },
                        },
                    )
                    .drives_when(drives.clone())
                    .inactive(inactive)
                    .default_to((field.default_value() - min) / (max - min))
                    .nudge(crate::control::Nudge::from_spec(&spec).fraction()),
                    move |fraction| {
                        super::view::msg(Action::Value(
                            drag.clone(),
                            min + fraction * (max - min),
                            false,
                        ))
                    },
                    move |fraction| {
                        super::view::msg(Action::Value(
                            release.clone(),
                            min + fraction * (max - min),
                            true,
                        ))
                    },
                )
            }
            Kind::NumberBox | Kind::Text => unreachable!(),
        }
    };
    // Keep the same tree on both sides of selection: swapping Container for Stack here
    // discards the canvas drag or text input state after the very first press.
    let edited = device.expanded
        && super::edited_field(app, device.id)
            .is_some_and(|(entry, field)| entry == key.entry && field == key.field);
    // Control focus already identifies this argument. Leave the editor-selection edge to
    // arguments selected from the editor, so it cannot paint a box over focused brackets.
    let selection_edge = edited && !app.control_focus.has(id);
    let colour = app.theme.selection_background;
    let control: Element<'a, Message> = iced::widget::stack![
        container(widget).width(width).height(height),
        container(iced::widget::Space::new())
            .width(width)
            .height(height)
            .style(move |_| container::Style {
                border: iced::Border {
                    color: colour,
                    width: if selection_edge {
                        detail::DEVICE_CELL_EDGE
                    } else {
                        0.0
                    },
                    ..Default::default()
                },
                ..Default::default()
            }),
    ]
    .into();
    let control = if key.parameter {
        parameter_gesture::menu(app, control, &subject)
    } else {
        control
    };
    match caption {
        // One placement for Number box, Text and Enum: above, left-aligned, in a 14 px row.
        Some(caption) => container(device_layout::labelled(
            controls,
            caption,
            label_state,
            control,
        ))
        .width(width)
        .into(),
        None => control,
    }
}

fn toggle_label(field: &FieldDef, label: String, value: f64) -> String {
    if !field.name.is_empty() && !label.is_empty() {
        label
    } else if value == 0.0 {
        "Off".into()
    } else {
        "On".into()
    }
}

fn display(field: &FieldDef, value: f64) -> String {
    ValueFormat::field(field).display(value)
}

pub(crate) fn keyboard(
    app: &mut App,
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
    ignored: bool,
) -> Option<Task<Message>> {
    if !ignored
        || modifiers.command()
        || modifiers.control()
        || modifiers.alt()
        || app.device_chain.face.exact.is_some()
    {
        return None;
    }
    let iced::keyboard::Key::Character(text) = key else {
        return None;
    };
    if !text
        .chars()
        .all(|c| c.is_ascii_digit() || c == '-' || c == '.')
    {
        return None;
    }
    let DraftKey::Target(device, TargetField::Face(parameter, index, position)) =
        app.control_focus.current()?.field_key()?
    else {
        return None;
    };
    let (track, _) = app.project().device(device)?;
    if super::super::shown_track(app) != Some(track.id) {
        return None;
    }
    let (_, t) = target(app, device)?;
    let (entry, fields) = if parameter {
        let p = t.parameters.get(index)?;
        (&p.key, &p.fields)
    } else {
        let e = t.events.get(index)?;
        (&e.key, &e.fields)
    };
    let field = fields.get(position)?;
    if !matches!(field.control, Kind::Slider | Kind::Dial) {
        return None;
    }
    let action = Action::Exact(
        FaceField {
            device,
            entry: entry.clone(),
            field: field.key.clone(),
            parameter,
        },
        text.to_string(),
    );
    Some(super::update(app, action))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unnamed_toggles_show_state_and_named_toggles_keep_their_label() {
        for value_type in [FieldType::Bool, FieldType::Int, FieldType::Float] {
            let mut field = FieldDef::new(
                qtable_engine::model::FieldKey("f2".into()),
                value_type,
                true,
            );
            field.control = Kind::Toggle;
            assert_eq!(toggle_label(&field, "2".into(), 0.0), "Off");
            assert_eq!(toggle_label(&field, "2".into(), 1.0), "On");
            field.name = "Enabled".into();
            assert_eq!(toggle_label(&field, "Enabled".into(), 0.0), "Enabled");
            assert_eq!(toggle_label(&field, "Enabled".into(), 1.0), "Enabled");
            // A one-field parameter passes no label: its title strip names it.
            assert_eq!(toggle_label(&field, String::new(), 1.0), "On");
        }
    }

    #[test]
    fn enums_add_a_label_row_except_in_one_field_parameters() {
        let mut field = FieldDef::new(
            qtable_engine::model::FieldKey("f3".into()),
            FieldType::String,
            true,
        );
        field.control = Kind::Enum;
        field.name = "Shape".into();
        field.options = vec![qtable_engine::model::EnumOption {
            label: "Wide".into(),
            value: qtable_engine::model::OscArgument::String("Wide".into()),
        }];
        let texts: Vec<_> = option_labels(&field)
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(texts, ["Wide"]);
        let one = [field.clone()];
        assert!(!labelled(&one, true));
        assert!(labelled(&one, false));
        assert_eq!(field_height(&one, true), detail::CONTROL_HEIGHT);
        assert_eq!(field_height(&one, false), detail::DEVICE_LABELLED_ROW);
        let two = [field.clone(), field];
        assert!(labelled(&two, true));
        assert_eq!(
            field_height(&two, true),
            2.0 * detail::DEVICE_LABELLED_ROW + detail::DEVICE_ROW_GAP
        );
    }

    #[test]
    fn parameters_start_their_own_column_under_their_own_heading() {
        let entry = |key: &str| {
            (
                EntryKey(key.into()),
                qtable_engine::model::OscAddress::Relative(format!("/{key}")),
            )
        };
        let mut target = TargetDevice::default();
        assert!(sections(&target, &heights(&target)).is_empty());
        let (key, path) = entry("hit");
        target.events.push(qtable_engine::model::EventDef {
            key,
            name: String::new(),
            path,
            fields: Vec::new(),
        });
        let sizes = heights(&target);
        let only_events = sections(&target, &sizes);
        assert_eq!(only_events.len(), 1);
        assert_eq!(only_events[0].0, "EVENTS");
        assert_eq!(only_events[0].1, std::slice::from_ref(&(0..1)));
        let events_only_width = width(&target);
        let (key, path) = entry("level");
        target.parameters.push(qtable_engine::model::ParameterDef {
            key,
            name: String::new(),
            path,
            fields: Vec::new(),
            send_mode: qtable_engine::model::SendMode::default(),
        });
        let sizes = heights(&target);
        let both = sections(&target, &sizes);
        assert_eq!(both.len(), 2);
        assert_eq!(both[0].0, "EVENTS");
        assert_eq!(both[0].1, std::slice::from_ref(&(0..1)));
        assert_eq!(both[1].0, "PARAMETERS");
        assert_eq!(both[1].1, std::slice::from_ref(&(1..2)));
        // A short event and a short parameter still sit in separate columns.
        assert_eq!(
            width(&target),
            events_only_width + detail::CONTROL_GAP + detail::DEVICE_CELL_WIDTH
        );
    }

    #[test]
    fn event_blocks_have_no_trailing_heading_even_when_the_fields_scroll() {
        let mut target = TargetDevice::default();
        target.events.push(qtable_engine::model::EventDef {
            key: EntryKey("event".into()),
            name: "Event".into(),
            path: qtable_engine::model::OscAddress::Relative("/event".into()),
            fields: Vec::new(),
        });
        assert_eq!(heights(&target), [cell_height(&[], false)]);
        assert_eq!(cell_height(&[], false), 20.0);
        target.events[0].fields = (0..7)
            .map(|i| {
                FieldDef::new(
                    qtable_engine::model::FieldKey(format!("f{i}")),
                    FieldType::Int,
                    false,
                )
            })
            .collect();
        // Seven labelled rows: 7 x 36 px and six 2 px gaps, in a cell with its strip and edges.
        assert_eq!(heights(&target), [289.0]);
        assert_eq!(
            columns(&[heights(&target)[0], detail::CONTROL_HEIGHT]),
            [0..1, 1..2]
        );
    }

    #[test]
    fn paired_dials_and_mixed_rows_use_their_full_height() {
        let mut fields: Vec<_> = (0..5)
            .map(|i| {
                FieldDef::new(
                    qtable_engine::model::FieldKey(format!("f{i}")),
                    FieldType::Float,
                    true,
                )
            })
            .collect();
        for field in &mut fields {
            field.control = Kind::Dial;
        }
        assert_eq!(field_rows(&fields), [0..2, 2..4, 4..5]);
        assert_eq!(field_height(&fields, false), 208.0);
        fields[1].control = Kind::Slider;
        assert_eq!(field_rows(&fields), [0..1, 1..2, 2..4, 4..5]);
        assert_eq!(field_height(&fields, false), 230.0);
        assert_eq!(columns(&[field_height(&fields, false), 20.0]), [0..1, 1..2]);
        assert_eq!(
            2.0 * crate::control::dial::WIDTH,
            detail::DEVICE_CONTROL_WIDTH
        );
    }

    #[test]
    fn blocks_wrap_whole_and_tall_sections_stand_alone() {
        assert_eq!(columns(&[20.0, 20.0, 110.0]), [0..2, 2..3]);
        assert_eq!(columns(&[20.0, 400.0, 20.0]), [0..1, 1..2, 2..3]);
        let empty = columns(&[]);
        assert_eq!(empty.len(), 1);
        assert_eq!(empty[0], 0..0);
    }
}
