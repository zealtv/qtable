//! The mapping inspector, pinned under the mapping list at a fixed height: the selected
//! mapping's full identity, then Mode, Takeover · Sensitivity and Invert · Min · Max.
//! Which settings show, and which dim, is the model's one rule, `RemoteMapping::facets`: a
//! setting it hides is absent here, one it dims stays in place, disabled and keeping its value,
//! so nothing moves when the Mode changes. Built from the control layer.
use super::labels::{self, Limits};
use super::{Action, Cell};
use crate::app::App;
use crate::control::rule::{Axis, Level, rule};
use crate::control::{Dropdown, Nudge, Number, SizeRole, State as ControlState, Toggle};
use crate::control_focus::View;
use crate::drafts::{Control, ControlId, DraftKey};
use crate::info::Id;
use crate::messages::Message;
use crate::style::detail::{
    CONTROL_GAP, DEVICE_ROW_GAP, MAPPING_INSPECTOR_HEIGHT, MAPPING_INSPECTOR_INSET,
    MAPPING_INSPECTOR_MIN_WIDTH, MAPPING_STRIP_HEIGHT, TEXT_SIZE, heading, pair, scrollbar,
};
use iced::widget::{Space, column, container, responsive, row, scrollable, text};
use iced::{Alignment, Element, Length};
use qtable_engine::model::{
    ControlDescriptor, ControlValue, Facet, Facets, RemoteMapping, SourceMode, Takeover,
};
use std::fmt;

/// A dropdown entry that carries its value; `Display` is the label shown.
#[derive(Clone, Debug, PartialEq)]
struct Choice<T> {
    value: T,
    label: String,
}

impl<T> fmt::Display for Choice<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

fn state(facet: Facet) -> ControlState {
    ControlState {
        enabled: facet.is_enabled(),
        ..Default::default()
    }
}

/// The inspector in `height` px: its fixed content, scrolling inside itself when the window is
/// too short for it or the Browser narrower than the default.
pub(super) fn view(app: &App, height: f32) -> Element<'_, Message> {
    container(responsive(move |size| {
        let width = size.width.max(MAPPING_INSPECTOR_MIN_WIDTH);
        let content = container(body(app))
            .width(width)
            .height(MAPPING_INSPECTOR_HEIGHT);
        if size.width >= MAPPING_INSPECTOR_MIN_WIDTH && size.height >= MAPPING_INSPECTOR_HEIGHT {
            return content.into();
        }
        scrollable(content)
            .direction(scrollable::Direction::Both {
                vertical: scrollbar(),
                horizontal: scrollbar(),
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }))
    .width(Length::Fill)
    .height(height)
    .clip(true)
    .into()
}

/// The selected row of the active mode, with its control's descriptor.
fn selected(app: &App) -> Option<(&RemoteMapping, ControlDescriptor)> {
    let id = app.map_mode.selected_row?;
    super::shown(app).find(|(row, _)| row.id == id)
}

/// What the inspector shows for the selected mapping, or one dim line with none.
fn body(app: &App) -> Element<'_, Message> {
    let Some((mapping, descriptor)) = selected(app) else {
        return container(
            app.controls()
                .readout(
                    "Select a mapping to edit it.",
                    ControlState {
                        enabled: false,
                        ..Default::default()
                    },
                )
                .wrapping(text::Wrapping::WordOrGlyph),
        )
        .padding(MAPPING_INSPECTOR_INSET)
        .width(Length::Fill)
        .into();
    };
    let facets = mapping.facets(&mapping.control, &ControlValue::from(&descriptor));
    let theme = app.theme;
    let ink = theme.control_foreground;
    // Identity, read only and unclipped: the Name as the title strip, the Path (which wraps)
    // and the source written out.
    let title = container(
        heading(descriptor.label.clone())
            .color(ink)
            .wrapping(text::Wrapping::None),
    )
    .padding([2.0, MAPPING_INSPECTOR_INSET])
    .height(MAPPING_STRIP_HEIGHT)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .clip(true)
    .style(move |_| iced::widget::container::Style {
        background: Some(theme.device_title_background.into()),
        ..Default::default()
    });
    let mut details = column![
        text(labels::path(&descriptor))
            .size(TEXT_SIZE)
            .color(ink)
            .wrapping(text::Wrapping::WordOrGlyph),
        text(labels::source_line(&mapping.source))
            .size(TEXT_SIZE)
            .color(ink)
            .wrapping(text::Wrapping::WordOrGlyph),
    ]
    .spacing(DEVICE_ROW_GAP);
    for group in [
        mode_group(app, mapping, &facets),
        range_group(app, mapping, &descriptor, &facets),
    ]
    .into_iter()
    .flatten()
    {
        details = details
            .push(container(rule(&theme, Axis::Horizontal, Level::Group)).padding([2, 0]))
            .push(group);
    }
    column![
        title,
        container(details)
            .padding(MAPPING_INSPECTOR_INSET)
            .width(Length::Fill)
    ]
    .into()
}

/// Mode, then Takeover · Sensitivity: the settings that say how the source is read.
fn mode_group<'a>(
    app: &'a App,
    mapping: &'a RemoteMapping,
    facets: &Facets,
) -> Option<Element<'a, Message>> {
    let mut group = column![].spacing(DEVICE_ROW_GAP);
    let mut any = false;
    if !facets.mode.is_hidden() {
        any = true;
        let chooser =
            app.controls()
                .labelled("Mode", state(facets.mode), mode(app, mapping, facets.mode));
        // A CC's nine modes need the full width ("Relative (Signed Bit 2) Linear"); a note's or
        // key's two take the left of two equal columns rather than stretching.
        group = group.push(if mapping.source.supports_relative() {
            Element::from(chooser)
        } else {
            pair(chooser, Space::new()).into()
        });
    }
    if !facets.takeover.is_hidden() || !facets.sensitivity.is_hidden() {
        any = true;
        // Takeover belongs to Absolute and Sensitivity to Relative, so one of them dims in
        // either mode; both keep their columns.
        let takeover: Element<'a, Message> = if facets.takeover.is_hidden() {
            Space::new().into()
        } else {
            app.controls()
                .labelled(
                    "Takeover",
                    state(facets.takeover),
                    takeover(app, mapping, facets.takeover),
                )
                .into()
        };
        let sensitivity: Element<'a, Message> = if facets.sensitivity.is_hidden() {
            Space::new().into()
        } else {
            app.controls()
                .labelled(
                    "Sensitivity",
                    state(facets.sensitivity),
                    sensitivity(app, mapping, facets.sensitivity),
                )
                .into()
        };
        group = group.push(pair(takeover, sensitivity));
    }
    any.then(|| group.into())
}

/// Invert · Min · Max as three equal columns, after the argument inspector's Range | Min | Max.
fn range_group<'a>(
    app: &'a App,
    mapping: &'a RemoteMapping,
    descriptor: &ControlDescriptor,
    facets: &Facets,
) -> Option<Element<'a, Message>> {
    if facets.invert.is_hidden() && facets.min.is_hidden() && facets.max.is_hidden() {
        return None;
    }
    let limits = labels::named(facets.limits, descriptor);
    let labelled = !facets.min.is_hidden() || !facets.max.is_hidden();
    let end = |cell: Cell, facet: Facet| -> Element<'a, Message> {
        if facet.is_hidden() {
            return Space::new().width(Length::Fill).into();
        }
        let label = if cell == Cell::Min { "Min" } else { "Max" };
        app.controls()
            .labelled(
                label,
                state(facet),
                limit(app, mapping, &limits, cell, facet),
            )
            .into()
    };
    let invert: Element<'a, Message> = if facets.invert.is_hidden() {
        Space::new().width(Length::Fill).into()
    } else {
        invert(app, mapping, facets.invert)
    };
    let columns = row![
        container(invert).width(Length::Fill),
        container(end(Cell::Min, facets.min)).width(Length::Fill),
        container(end(Cell::Max, facets.max)).width(Length::Fill),
    ]
    .spacing(CONTROL_GAP)
    // Invert has no label: it lines up with the fields, under Min's and Max's labels.
    .align_y(if labelled {
        Alignment::End
    } else {
        Alignment::Start
    });
    Some(columns.into())
}

fn mode<'a>(app: &'a App, mapping: &'a RemoteMapping, facet: Facet) -> Element<'a, Message> {
    let id = mapping.id;
    let source = &mapping.source;
    let options: Vec<Choice<SourceMode>> = labels::modes(source)
        .iter()
        .map(|&value| Choice {
            value,
            label: labels::mode_label(source, value).to_owned(),
        })
        .collect();
    let selected = Some(Choice {
        value: mapping.mode,
        label: labels::mode_label(source, mapping.mode).to_owned(),
    });
    app.controls().dropdown(
        Control::new(
            ControlId::field_action(View::Mappings, DraftKey::Mapping(id, Cell::Mode), "mode"),
            Id::MappingMode,
            Dropdown,
        )
        .role(SizeRole::Fill)
        .enabled(facet.is_enabled()),
        options,
        selected,
        move |choice| Message::Map(Action::SetMode(id, choice.value)),
    )
}

fn takeover<'a>(app: &'a App, mapping: &'a RemoteMapping, facet: Facet) -> Element<'a, Message> {
    let id = mapping.id;
    let options: Vec<Choice<Takeover>> = labels::TAKEOVERS
        .iter()
        .map(|&value| Choice {
            value,
            label: labels::takeover_label(value).to_owned(),
        })
        .collect();
    let selected = options
        .iter()
        .find(|choice| choice.value == mapping.takeover)
        .cloned();
    app.controls().dropdown(
        Control::new(
            ControlId::field_action(
                View::Mappings,
                DraftKey::Mapping(id, Cell::Takeover),
                "takeover",
            ),
            Id::MappingTakeover,
            Dropdown,
        )
        .role(SizeRole::Fill)
        .enabled(facet.is_enabled()),
        options,
        selected,
        move |choice| Message::Map(Action::SetTakeover(id, choice.value)),
    )
}

fn sensitivity<'a>(app: &'a App, mapping: &'a RemoteMapping, facet: Facet) -> Element<'a, Message> {
    let id = mapping.id;
    let key = DraftKey::Mapping(id, Cell::Sensitivity);
    let shown = app
        .drafts
        .values
        .get(&key)
        .cloned()
        .unwrap_or_else(|| labels::format_sensitivity(mapping.sensitivity));
    let [low, high] = qtable_engine::model::SENSITIVITY_RANGE.map(|factor| factor * 100.0);
    app.controls().number(
        Control::new(
            ControlId::field(View::Mappings, key),
            Id::MappingSensitivity,
            Number::new("").adjust_text(crate::control::adjustment::percent_in(low, high)),
        )
        .nudge(Nudge::integer(low, high))
        .role(SizeRole::Fill)
        .enabled(facet.is_enabled())
        .invalid(app.drafts.errors.contains_key(&key)),
        &shown,
        move |text| Message::Map(Action::FieldChanged(id, Cell::Sensitivity, text)),
        Message::Map(Action::FieldSubmitted(id, Cell::Sensitivity)),
        None,
    )
}

/// Invert: a latched button, lit while Min sits above Max; pressing it swaps the two.
fn invert<'a>(app: &'a App, mapping: &'a RemoteMapping, facet: Facet) -> Element<'a, Message> {
    let id = mapping.id;
    app.controls().toggle(
        Control::new(
            ControlId::field_action(
                View::Mappings,
                DraftKey::Mapping(id, Cell::Invert),
                "invert",
            ),
            Id::MappingInvert,
            Toggle {
                label: "Invert".into(),
            },
        )
        .role(SizeRole::Fill)
        .lit(mapping.min > mapping.max)
        .enabled(facet.is_enabled()),
        Message::Map(Action::Invert(id)),
    )
}

/// One of Min and Max, in the units its control uses: a Number for a range, the choices for
/// an Enum, Off and On for a switch, a percentage window for an Activator. Typing, dragging
/// and the status-bar errors are the v3.9 list's.
fn limit<'a>(
    app: &'a App,
    mapping: &'a RemoteMapping,
    limits: &Limits,
    cell: Cell,
    facet: Facet,
) -> Element<'a, Message> {
    let id = mapping.id;
    let (value, help) = if cell == Cell::Min {
        (mapping.min, Id::MappingMin)
    } else {
        (mapping.max, Id::MappingMax)
    };
    let key = DraftKey::Mapping(id, cell);
    let controls = app.controls();
    match limits {
        Limits::None => Space::new().into(),
        Limits::Number { .. } | Limits::Window => {
            let shown = app
                .drafts
                .values
                .get(&key)
                .cloned()
                .unwrap_or_else(|| labels::format_limit(limits, value));
            // Dragging steps by the control's own units, as the arrow keys do.
            let (kind, nudge) = match limits {
                Limits::Window => (
                    Number::new("").adjust_text(crate::control::adjustment::percent),
                    Nudge::integer(0.0, 100.0),
                ),
                Limits::Number {
                    integer: true,
                    range,
                } => {
                    let [low, high] = range.unwrap_or([f64::from(i32::MIN), f64::from(i32::MAX)]);
                    (Number::new(""), Nudge::integer(low, high))
                }
                Limits::Number {
                    range: Some([low, high]),
                    ..
                } => (
                    Number::new(""),
                    Nudge::continuous(*low, *high, (high - low) / 100.0),
                ),
                _ => (Number::new(""), Nudge::FLOAT),
            };
            controls.number(
                Control::new(ControlId::field(View::Mappings, key), help, kind)
                    .nudge(nudge)
                    .role(SizeRole::Fill)
                    .enabled(facet.is_enabled())
                    .invalid(app.drafts.errors.contains_key(&key)),
                &shown,
                move |text| Message::Map(Action::FieldChanged(id, cell, text)),
                Message::Map(Action::FieldSubmitted(id, cell)),
                None,
            )
        }
        Limits::Choice(_) | Limits::Switch => {
            let names: Vec<String> = match limits {
                Limits::Choice(choices) => choices.clone(),
                _ => vec!["Off".into(), "On".into()],
            };
            let options: Vec<Choice<usize>> = names
                .into_iter()
                .enumerate()
                .map(|(value, label)| Choice { value, label })
                .collect();
            let selected = options.get(value.round().max(0.0) as usize).cloned();
            controls.dropdown(
                Control::new(
                    ControlId::field_action(View::Mappings, key, "limit"),
                    help,
                    Dropdown,
                )
                .role(SizeRole::Fill)
                .enabled(facet.is_enabled()),
                options,
                selected,
                move |choice| Message::Map(Action::LimitChosen(id, cell, choice.value as f64)),
            )
        }
    }
}

#[cfg(test)]
mod tests;
