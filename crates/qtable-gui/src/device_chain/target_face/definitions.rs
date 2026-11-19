//! Shared field blocks for event and parameter definitions.
use super::{Action, view::msg};
use crate::app::App;
use crate::control::rule::{Axis, Level, rule};
use crate::control::{
    Button, Dropdown, Face, Number, SizeRole, State, Text, Toggle, ValueFormat, argument_text,
};
use crate::control_focus::View;
use crate::drafts::{Control, ControlId, DefinitionField, DraftKey, TargetField};
use crate::info::Id;
use crate::messages::Message;
use crate::style::detail;
use iced::widget::{column, container, row, scrollable};
use iced::{Element, Length};
use qtable_engine::edit::TargetEdit;
use qtable_engine::model::{
    Control as FieldControl, DeviceId, EntryKey, EnumOption, FieldDef, FieldType, OscArgument,
    SendMode,
};
use std::borrow::Cow;
use std::fmt;

/// The inline label column of every inspector row, wide enough for "Threshold".
const INSPECTOR_LABEL_WIDTH: f32 = 56.0;
/// The label column of Min and Max in the three-column Range row: room for "Min" and "Max" only.
const RANGE_ROW_LABEL_WIDTH: f32 = 24.0;

#[derive(Clone, PartialEq)]
struct Pick<T>(T, String);
impl<T> fmt::Display for Pick<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.1)
    }
}

fn control_choice(control: FieldControl) -> Pick<FieldControl> {
    Pick(
        control,
        match control {
            FieldControl::NumberBox => "Number".into(),
            _ => format!("{control:?}"),
        },
    )
}

/// The dim hint in an unnamed field's Name input; the name itself stays empty.
fn name_placeholder(field: &FieldDef, position: usize) -> Cow<'static, str> {
    if field.name.is_empty() {
        FieldDef::position_label(position).into()
    } else {
        "".into()
    }
}

fn show(value: f64) -> String {
    qtable_engine::model::format_float(value, None)
}

/// Which input edits a field's Default: chosen by the Type, with Enum's options the one
/// Control that changes it (a String Enum picks among its options).
#[derive(Debug, PartialEq)]
enum DefaultInput {
    Options,
    Bool,
    Number,
    Text,
}

fn default_input(field: &FieldDef) -> DefaultInput {
    match (field.control, field.value_type) {
        (FieldControl::Enum, _) => DefaultInput::Options,
        (_, FieldType::Bool) => DefaultInput::Bool,
        (_, FieldType::Float | FieldType::Int) => DefaultInput::Number,
        (_, FieldType::String) => DefaultInput::Text,
    }
}

/// The Control dropdown's items: what the engine allows for the argument's type and range.
fn control_choices(field: &FieldDef, parameter: bool) -> Vec<Pick<FieldControl>> {
    field
        .value_type
        .controls(parameter, field.range.is_some())
        .iter()
        .copied()
        .map(control_choice)
        .collect()
}

/// Min and Max text: Int ranges show whole numbers, Float ranges their shortest decimals.
fn range_text(field: &FieldDef, value: f64) -> String {
    ValueFormat::field_bound(field).display(value)
}

struct Editor<'a> {
    app: &'a App,
    device: DeviceId,
    entry: EntryKey,
    location: View,
    index: usize,
}
impl<'a> Editor<'a> {
    fn key(&self, property: DefinitionField) -> DraftKey {
        DraftKey::Target(self.device, TargetField::Definition(self.index, property))
    }
    fn action(&self, edit: TargetEdit) -> Message {
        msg(Action::Definition(self.device, edit))
    }
    fn button(
        &self,
        label: &'static str,
        name: &'static str,
        help: Id,
        enabled: bool,
        edit: TargetEdit,
    ) -> Element<'a, Message> {
        self.app.controls().button(
            Control::new(
                ControlId::field_action(self.location, self.key(DefinitionField::Name), name),
                help,
                Button {
                    face: Face::Text(label.into()),
                    latched: false,
                },
            )
            .enabled(enabled),
            self.action(edit),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn input(
        &self,
        label: &'static str,
        property: DefinitionField,
        stored: String,
        help: Id,
        numeric: bool,
        enabled: bool,
        placeholder: Cow<'static, str>,
    ) -> Element<'a, Message> {
        self.input_at(
            INSPECTOR_LABEL_WIDTH,
            label,
            property,
            stored,
            help,
            numeric,
            enabled,
            placeholder,
        )
    }
    /// `input` with its label column set: Min and Max use a short one in a three-column row.
    #[allow(clippy::too_many_arguments)]
    fn input_at(
        &self,
        label_width: f32,
        label: &'static str,
        property: DefinitionField,
        stored: String,
        help: Id,
        numeric: bool,
        enabled: bool,
        placeholder: Cow<'static, str>,
    ) -> Element<'a, Message> {
        let controls = self.app.controls();
        let key = self.key(property);
        let value = self.app.drafts.values.get(&key).cloned().unwrap_or(stored);
        let bad = self.app.drafts.errors.contains_key(&key);
        let id = ControlId::field(self.location, key);
        let changed = move |v| msg(Action::Draft(key, v));
        let submit = msg(Action::Submit(key));
        let input = if numeric {
            controls.number(
                Control::new(id, help, Number::new("0"))
                    .nudge(
                        if super::target(self.app, self.device)
                            .and_then(|(_, t)| super::entry(t, &self.entry))
                            .and_then(|e| super::entry_fields(&e).get(self.index))
                            .is_some_and(|f| f.value_type == FieldType::Int)
                            && property != DefinitionField::Threshold
                        {
                            crate::control::Nudge::integer(f64::from(i32::MIN), f64::from(i32::MAX))
                        } else {
                            crate::control::Nudge::FLOAT
                        },
                    )
                    .role(SizeRole::DeviceNumber)
                    .enabled(enabled)
                    .invalid(bad),
                &value,
                changed,
                submit,
                None,
            )
        } else {
            controls.text(
                Control::new(id, help, Text { placeholder })
                    .role(SizeRole::Fill)
                    .enabled(enabled)
                    .invalid(bad),
                &value,
                changed,
                submit,
            )
        };
        controls
            .inline(
                label,
                label_width,
                State {
                    enabled,
                    ..State::default()
                },
                input,
            )
            .into()
    }
    /// One field's tab: its label as the face shows it, lit while the editor shows it.
    fn tab(&self, field: &FieldDef, shown: bool) -> Element<'a, Message> {
        let device = self.device;
        self.app.controls().toggle(
            Control::new(
                ControlId::field_action(self.location, self.key(DefinitionField::Name), "tab"),
                Id::TargetFieldTab,
                Toggle {
                    label: field.display_name(self.index).into_owned(),
                },
            )
            .role(SizeRole::Content)
            .lit(shown),
            msg(Action::SelectField(device, field.key.clone())),
        )
    }
    fn picker<T: Clone + PartialEq + 'a>(
        &self,
        label: &'static str,
        name: &'static str,
        help: Id,
        choices: Vec<Pick<T>>,
        current: T,
        edit: impl Fn(T) -> TargetEdit + 'a,
    ) -> Element<'a, Message> {
        let controls = self.app.controls();
        let device = self.device;
        let selected = choices.iter().find(|p| p.0 == current).cloned();
        controls
            .inline(
                label,
                INSPECTOR_LABEL_WIDTH,
                State::default(),
                controls.dropdown(
                    Control::new(
                        ControlId::field_action(
                            self.location,
                            self.key(DefinitionField::Name),
                            name,
                        ),
                        help,
                        Dropdown,
                    )
                    .role(if matches!(name, "type" | "control") {
                        SizeRole::DeviceChoice
                    } else {
                        SizeRole::Fill
                    }),
                    choices,
                    selected,
                    move |p: Pick<T>| msg(Action::Definition(device, edit(p.0))),
                ),
            )
            .into()
    }
    /// Move left, move right and remove, at the end of the tab strip.
    fn arrange(&self, field: &FieldDef, count: usize, parameter: bool) -> Element<'a, Message> {
        let entry = self.entry.clone();
        let key = field.key.clone();
        row![
            self.button(
                "◀",
                "up",
                Id::TargetFieldMove,
                self.index > 0,
                TargetEdit::MoveField {
                    entry: entry.clone(),
                    field: key.clone(),
                    to: self.index.saturating_sub(1)
                }
            ),
            self.button(
                "▶",
                "down",
                Id::TargetFieldMove,
                self.index + 1 < count,
                TargetEdit::MoveField {
                    entry: entry.clone(),
                    field: key.clone(),
                    to: self.index + 1
                }
            ),
            self.button(
                "×",
                "remove",
                Id::TargetFieldRemove,
                !parameter || count > 1,
                TargetEdit::RemoveField {
                    entry: entry.clone(),
                    field: key.clone()
                }
            ),
        ]
        .spacing(detail::CONTROL_GAP)
        .into()
    }
    fn block(&self, field: &'a FieldDef, mode: Option<SendMode>) -> Element<'a, Message> {
        let parameter = mode.is_some();
        let entry = self.entry.clone();
        let key = field.key.clone();
        let types = [
            FieldType::Float,
            FieldType::Int,
            FieldType::Bool,
            FieldType::String,
        ]
        .into_iter()
        .map(|t| Pick(t, format!("{t:?}")))
        .collect();
        let type_picker = self.picker(
            "Type",
            "type",
            Id::TargetFieldType,
            types,
            field.value_type,
            {
                let entry = entry.clone();
                let field = key.clone();
                move |value_type| TargetEdit::SetFieldType {
                    entry: entry.clone(),
                    field: field.clone(),
                    value_type,
                }
            },
        );
        let choices = control_choices(field, parameter);
        let control = self.picker(
            "Control",
            "control",
            Id::TargetFieldControl,
            choices,
            field.control,
            {
                let entry = entry.clone();
                let field = key.clone();
                move |control| TargetEdit::SetFieldControl {
                    entry: entry.clone(),
                    field: field.clone(),
                    control,
                }
            },
        );
        let numeric = matches!(field.value_type, FieldType::Float | FieldType::Int);
        // Min and Max dim while the Range switch is off; the switch itself is for an event.
        let ranged = field.range.is_some();
        let range = field.range.unwrap_or([0.0, 1.0]);
        // An Enum's options define its values, so it shows them where a range would go.
        let enum_control = field.control == FieldControl::Enum;
        let default = if default_input(field) == DefaultInput::Options {
            let options = field
                .options
                .iter()
                .enumerate()
                .map(|(i, o)| Pick(i, o.label.clone()))
                .collect();
            self.picker(
                "Default",
                "default",
                Id::TargetDefault,
                options,
                field.default_value() as usize,
                {
                    let entry = entry.clone();
                    let field_key = key.clone();
                    let options = field.options.clone();
                    move |i| TargetEdit::SetFieldDefault {
                        entry: entry.clone(),
                        field: field_key.clone(),
                        default: options[i].value.clone(),
                    }
                },
            )
        } else if default_input(field) == DefaultInput::Bool {
            let current = matches!(field.default, OscArgument::Bool(true));
            self.picker(
                "Default",
                "default",
                Id::TargetDefault,
                vec![Pick(false, "False".into()), Pick(true, "True".into())],
                current,
                {
                    let entry = entry.clone();
                    let field_key = key.clone();
                    move |on| TargetEdit::SetFieldDefault {
                        entry: entry.clone(),
                        field: field_key.clone(),
                        default: OscArgument::Bool(on),
                    }
                },
            )
        } else {
            self.input(
                "Default",
                DefinitionField::Default,
                argument_text(&field.default),
                Id::TargetDefault,
                numeric,
                true,
                "".into(),
            )
        };
        let mut body = column![
            detail::pair(
                self.input(
                    "Name",
                    DefinitionField::Name,
                    field.name.clone(),
                    Id::TargetFieldName,
                    false,
                    true,
                    name_placeholder(field, self.index),
                ),
                type_picker
            ),
            // Control sits directly below Type, in the same column.
            detail::pair(default, control),
        ]
        .spacing(detail::CONTROL_GAP)
        .height(Length::Fill);
        // The argument's identity sits above the rule; what follows depends on the kind chosen
        // there, and shows only what applies to it (2026-11-02): an Enum its options, a
        // continuous numeric argument its range, and nothing for a Toggle, Text, Bool or String.
        // A kind with nothing below leaves no empty rule either.
        if enum_control || field.takes_range() {
            body = body.push(rule(&self.app.theme, Axis::Horizontal, Level::Group));
        }
        if enum_control {
            body = body
                .push(self.add_option(field))
                .push(self.options(field, parameter));
        } else if field.takes_range() {
            let bound = |label_width, label, property, help, value| {
                self.input_at(
                    label_width,
                    label,
                    property,
                    value,
                    help,
                    true,
                    ranged,
                    "".into(),
                )
            };
            let (min, max) = (range_text(field, range[0]), range_text(field, range[1]));
            body = body.push(if parameter {
                // No Range switch on a parameter: Min and Max take the two columns of the rows above.
                detail::pair(
                    bound(
                        INSPECTOR_LABEL_WIDTH,
                        "Min",
                        DefinitionField::Min,
                        Id::TargetMin,
                        min,
                    ),
                    bound(
                        INSPECTOR_LABEL_WIDTH,
                        "Max",
                        DefinitionField::Max,
                        Id::TargetMax,
                        max,
                    ),
                )
            } else {
                // The Range switch fills the first of three equal columns, heading the row it
                // governs: Range | Min | Max. A full-width heading row above them costs the
                // 12 pt this inspector doesn't have.
                let value = field.default_value();
                let proposed = field
                    .range
                    .is_none()
                    .then_some([value.min(0.0), value.max(1.0)]);
                let switch = self.app.controls().toggle(
                    Control::new(
                        ControlId::field_action(
                            self.location,
                            self.key(DefinitionField::Min),
                            "range",
                        ),
                        Id::TargetFieldRange,
                        Toggle {
                            label: "Range".into(),
                        },
                    )
                    .role(SizeRole::Fill)
                    .lit(field.range.is_some()),
                    self.action(TargetEdit::SetFieldRange {
                        entry: entry.clone(),
                        field: key.clone(),
                        range: proposed,
                    }),
                );
                row![
                    container(switch).width(Length::Fill),
                    container(bound(
                        RANGE_ROW_LABEL_WIDTH,
                        "Min",
                        DefinitionField::Min,
                        Id::TargetMin,
                        min
                    ))
                    .width(Length::Fill),
                    container(bound(
                        RANGE_ROW_LABEL_WIDTH,
                        "Max",
                        DefinitionField::Max,
                        Id::TargetMax,
                        max
                    ))
                    .width(Length::Fill),
                ]
                .spacing(detail::CONTROL_GAP)
                .align_y(iced::Alignment::Center)
            });
        }
        // Threshold governs what is sent (a change smaller than it waits a frame), not the value's
        // range, so it forms its own group below the range. It applies only to a continuous
        // numeric parameter argument; for any other kind, and for every event, the group is
        // hidden (2026-11-02), which also leaves an Enum's options list the room. Every
        // Frame is a send mode, not a kind, so it only dims the field.
        if parameter && field.takes_threshold() {
            let threshold = self.input(
                "Threshold",
                DefinitionField::Threshold,
                show(field.threshold),
                Id::TargetThreshold,
                true,
                mode != Some(SendMode::EveryFrame),
                "".into(),
            );
            body = body
                .push(rule(&self.app.theme, Axis::Horizontal, Level::Group))
                .push(detail::pair(threshold, iced::widget::Space::new()));
        }
        body.into()
    }
    fn add_option(&self, field: &'a FieldDef) -> Element<'a, Message> {
        let mut options = field.options.clone();
        let next = EnumOption::next_unused(field.value_type, &field.options);
        let can_add = options.len() < 64 && next.is_some();
        if let Some(option) = next {
            options.push(option);
        }
        self.button(
            "+ Option",
            "add-option",
            Id::TargetOptionAdd,
            can_add,
            TargetEdit::SetFieldOptions {
                entry: self.entry.clone(),
                field: field.key.clone(),
                options,
            },
        )
    }
    fn options(&self, field: &'a FieldDef, parameter: bool) -> Element<'a, Message> {
        let mut body = column![].spacing(detail::LABEL_GAP);
        for (index, option) in field.options.iter().enumerate() {
            let label = self.input(
                "Label",
                DefinitionField::OptionLabel(index),
                option.label.clone(),
                Id::TargetOptionLabel,
                false,
                true,
                "".into(),
            );
            // A String option sends its Label, so it has no Value of its own to show; the
            // Label keeps the left column and the right stays empty, as under Threshold.
            let value: Element<'a, Message> = if field.value_type == FieldType::String {
                iced::widget::Space::new().into()
            } else {
                self.input(
                    "Value",
                    DefinitionField::OptionValue(index),
                    argument_text(&option.value),
                    Id::TargetOptionValue,
                    true,
                    true,
                    "".into(),
                )
            };
            let mut options = field.options.clone();
            options.remove(index);
            // Give every option button its own focus identity.
            let remove = self.app.controls().button(
                Control::new(
                    ControlId::field_action(
                        self.location,
                        self.key(DefinitionField::OptionLabel(index)),
                        "remove",
                    ),
                    Id::TargetOptionRemove,
                    Button {
                        face: Face::Text("×".into()),
                        latched: false,
                    },
                )
                .enabled(field.options.len() > if parameter { 2 } else { 1 }),
                self.action(TargetEdit::SetFieldOptions {
                    entry: self.entry.clone(),
                    field: field.key.clone(),
                    options,
                }),
            );
            body = body.push(
                row![detail::pair(label, value), remove]
                    .spacing(detail::CONTROL_GAP)
                    .align_y(iced::Alignment::End),
            );
        }
        // The tall part of an argument: it scrolls inside its own list, and takes the room
        // the Range switch and Min/Max leave.
        scrollable(body)
            .direction(scrollable::Direction::Vertical(detail::scrollbar()))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

pub(super) fn view<'a>(
    app: &'a App,
    device: DeviceId,
    entry: &EntryKey,
    fields: &'a [FieldDef],
    mode: Option<SendMode>,
    location: View,
) -> Element<'a, Message> {
    let editor = Editor {
        app,
        device,
        entry: entry.clone(),
        location,
        index: 0,
    };
    let at = super::edited_field(app, device)
        .and_then(|(_, key)| fields.iter().position(|f| f.key == key))
        .unwrap_or(0);
    // The tabs, then "+" straight after the last one; they scroll together when they overflow.
    let tabs = fields
        .iter()
        .enumerate()
        .fold(row![], |tabs, (index, field)| {
            let editor = Editor {
                index,
                entry: entry.clone(),
                ..editor
            };
            tabs.push(editor.tab(field, index == at))
        })
        .push(editor.button(
            "+",
            "add-field",
            Id::TargetFieldAdd,
            fields.len() < FieldDef::MAX_ARGUMENTS,
            TargetEdit::AddField {
                entry: entry.clone(),
                value_type: FieldType::Float,
            },
        ));
    let mut tabs = row![
        scrollable(
            tabs.spacing(detail::CONTROL_GAP)
                .align_y(iced::Alignment::Center)
        )
        .direction(scrollable::Direction::Horizontal(detail::scrollbar()))
        .width(Length::Fill),
    ]
    .spacing(detail::CONTROL_GAP)
    .align_y(iced::Alignment::Center);
    if let Some(field) = fields.get(at) {
        let editor = Editor {
            index: at,
            entry: entry.clone(),
            ..editor
        };
        tabs = tabs.push(editor.arrange(field, fields.len(), mode.is_some()));
    }
    let mut body = column![tabs]
        .spacing(detail::CONTROL_GAP)
        .height(Length::Fill);
    if let Some(field) = fields.get(at) {
        let editor = Editor {
            index: at,
            entry: entry.clone(),
            ..editor
        };
        body = body
            .push(rule(&editor.app.theme, Axis::Horizontal, Level::Group))
            .push(editor.block(field, mode));
    }
    body.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_box_choice_uses_the_complete_short_label() {
        let choice = control_choice(FieldControl::NumberBox);
        assert_eq!(choice.0, FieldControl::NumberBox);
        assert_eq!(choice.to_string(), "Number");
    }

    #[test]
    fn unnamed_fields_hint_their_position_and_named_ones_do_not() {
        let mut field = FieldDef::new(
            qtable_engine::model::FieldKey("f2".into()),
            FieldType::Float,
            true,
        );
        assert_eq!(name_placeholder(&field, 1), "Arg 2");
        assert!(field.name.is_empty());
        field.name = "Level".into();
        assert_eq!(name_placeholder(&field, 1), "");
    }

    fn field(value_type: FieldType, control: FieldControl) -> FieldDef {
        let mut field = FieldDef::new(
            qtable_engine::model::FieldKey("f1".into()),
            value_type,
            false,
        );
        field.control = control;
        field
    }

    #[test]
    fn default_input_follows_the_type_and_only_enum_overrides_it() {
        use DefaultInput::*;
        for (value_type, control, input) in [
            (FieldType::Float, FieldControl::Slider, Number),
            (FieldType::Float, FieldControl::NumberBox, Number),
            (FieldType::Float, FieldControl::Toggle, Number),
            (FieldType::Int, FieldControl::NumberBox, Number),
            (FieldType::Int, FieldControl::Toggle, Number),
            (FieldType::Bool, FieldControl::Toggle, Bool),
            (FieldType::String, FieldControl::Text, Text),
            (FieldType::String, FieldControl::Enum, Options),
            (FieldType::Int, FieldControl::Enum, Options),
        ] {
            assert_eq!(
                default_input(&field(value_type, control)),
                input,
                "{value_type:?} {control:?}"
            );
        }
    }

    #[test]
    fn the_control_dropdown_offers_slider_and_dial_on_an_unranged_number() {
        for value_type in [FieldType::Float, FieldType::Int] {
            let unranged = field(value_type, FieldControl::NumberBox);
            assert_eq!(unranged.range, None);
            let labels: Vec<_> = control_choices(&unranged, false)
                .into_iter()
                .map(|Pick(_, label)| label)
                .collect();
            assert_eq!(labels, ["Slider", "Dial", "Number"]);
        }
    }

    #[test]
    fn int_ranges_show_whole_numbers_and_floats_keep_decimals() {
        let int = field(FieldType::Int, FieldControl::NumberBox);
        assert_eq!(range_text(&int, 0.0), "0");
        assert_eq!(range_text(&int, 1.0), "1");
        assert_eq!(range_text(&int, -12.0), "-12");
        let float = field(FieldType::Float, FieldControl::NumberBox);
        assert_eq!(range_text(&float, 0.0), "0.00");
        assert_eq!(range_text(&float, 1.0), "1.00");
    }
}
