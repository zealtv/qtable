//! The selected hit's argument values, beneath the timeline.
//! There is no position field: a hit's position is where it sits on the grid. Argument types
//! and Absolute belong to the event's definition on its device (target-device design).
use super::{ArgKind, Message, field_key};
use crate::control::argument_text;
use crate::control::{Dropdown, Nudge, Number, SizeRole, State, Text, TimeField, Toggle};
use crate::control_focus::View;
use crate::drafts::{ClipField, Control, ControlId, Controls, DraftKey};
use crate::info::{Help, Id};
use crate::style::{self, detail};
use iced::widget::{row, text};
use iced::{Element, Length};
use qtable_engine::model::{
    ArgumentRef, Breakpoint, Clip, Event, FieldDef, OscArgument, Project, TrackId,
};
use std::collections::HashMap;

/// Draft text and errors for the clip's fields, keyed as the inspector keys them.
pub(super) struct Fields<'a> {
    pub(super) track: TrackId,
    pub(super) slot: usize,
    pub(super) drafts: &'a HashMap<DraftKey, String>,
    pub(super) errors: &'a HashMap<DraftKey, String>,
    pub(super) controls: Controls<'a>,
}
impl Fields<'_> {
    fn value(&self, field: ClipField, model: String) -> String {
        self.drafts
            .get(&field_key(self.track, self.slot, field))
            .cloned()
            .unwrap_or(model)
    }
    fn input(
        &self,
        field: ClipField,
        value: &str,
        (role, help): (SizeRole, Id),
        on_submit: Message,
    ) -> Element<'static, Message> {
        let key = field_key(self.track, self.slot, field);
        let error = crate::drafts::field_error(self.drafts, self.errors, key, Ok(()));
        self.controls.text(
            Control::new(
                ControlId::field(View::Clip, key),
                help,
                Text {
                    placeholder: "".into(),
                },
            )
            .role(role)
            .invalid(error.is_some()),
            value,
            move |s| Message::Draft(key, s),
            on_submit,
        )
    }
}

fn hint(message: impl iced::widget::text::IntoFragment<'static>) -> iced::widget::Text<'static> {
    text(message).size(detail::TEXT_SIZE).color(style::DISABLED)
}

pub(super) fn view(
    project: &Project,
    clip: &Clip,
    fields: &Fields,
    (lane, selected, envelope, point): (
        Option<usize>,
        Option<usize>,
        Option<ArgumentRef>,
        Option<usize>,
    ),
) -> Element<'static, Message> {
    if let Some(target) = envelope {
        return detail::panel(envelope_row(project, clip, fields, (&target, point))).into();
    }
    let lanes = clip.lane_refs();
    let hit = selected
        .and_then(|i| clip.events.get(i).map(|e| (i, e)))
        .filter(|(_, e)| lane.and_then(|l| lanes.get(l)) == Some(&e.definition));
    hit.map_or_else(
        || iced::widget::Space::new().into(),
        |(index, event)| hit_panel(project, fields, index, event),
    )
}

pub(super) fn hit_panel(
    project: &Project,
    fields: &Fields,
    index: usize,
    event: &Event,
) -> Element<'static, Message> {
    let definitions = project
        .track(fields.track)
        .and_then(|t| t.event_def(&event.definition))
        .map(|(_, d)| d.fields.as_slice())
        .unwrap_or_default();
    detail::panel(hit_row(fields, index, event, definitions)).into()
}

/// The selected point's Position, Value and Curve. The three fields stay in place and
/// disable when no point is selected; lane identity is already shown by the view.
fn envelope_row(
    project: &Project,
    clip: &Clip,
    fields: &Fields,
    (target, point): (&ArgumentRef, Option<usize>),
) -> Element<'static, Message> {
    let envelope = clip
        .lanes
        .iter()
        .find_map(|l| l.envelope().filter(|e| e.target == *target));
    let points = envelope.map_or(&[][..], |e| e.points.as_slice());
    let selected = point.and_then(|i| points.get(i).map(|p| (i, p)));
    point_fields(
        project,
        fields,
        target,
        selected.map(|(i, p)| (i, p, points.len())),
    )
}

/// A level in the Value dropdown: an Enum option by index.
#[derive(Clone, PartialEq)]
struct Level(usize, String);
impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.1)
    }
}

fn point_fields(
    project: &Project,
    fields: &Fields,
    target: &ArgumentRef,
    selected: Option<(usize, &Breakpoint, usize)>,
) -> Element<'static, Message> {
    let (track, slot) = (fields.track, fields.slot);
    let owner = project.track(track);
    let spec = owner.and_then(|t| t.argument_spec(target));
    let axis = owner.and_then(|t| t.argument_axis(target));
    let definition = owner.and_then(|t| super::lane_model::argument_field(t, target));
    let enabled = selected.is_some();
    let signature = project.time_signature;
    let number =
        |field: ClipField, text: String, help: Id, nudge: Nudge, default: Option<f64>, on: bool| {
            let key = field_key(track, slot, field);
            if field == ClipField::PointPosition {
                return fields.controls.time_field(
                    Control::new(
                        ControlId::field(View::Clip, key),
                        help,
                        TimeField::position(signature),
                    )
                    .role(SizeRole::Fill)
                    .enabled(on)
                    .invalid(fields.errors.contains_key(&key)),
                    &text,
                    move |s| Message::Draft(key, s),
                    Message::PointText(track, slot, field, text.clone()),
                );
            }
            let kind = Number::new("");
            fields.controls.number(
                {
                    let control = Control::new(ControlId::field(View::Clip, key), help, kind)
                        .nudge(nudge)
                        .role(SizeRole::Fill)
                        .enabled(on)
                        .invalid(fields.errors.contains_key(&key));
                    match default {
                        Some(default) => control.default_to(default),
                        None => control,
                    }
                },
                &text,
                move |s| Message::Draft(key, s),
                Message::PointText(track, slot, field, text.clone()),
                None,
            )
        };
    let position = number(
        ClipField::PointPosition,
        selected.map_or_else(String::new, |(_, p, _)| {
            fields.value(
                ClipField::PointPosition,
                super::musical_time::format_position(p.time, project.time_signature),
            )
        }),
        Id::PointPosition,
        Nudge::FLOAT,
        None,
        enabled,
    );
    let value_key = field_key(track, slot, ClipField::PointValue);
    let value_id = ControlId::field(View::Clip, value_key);
    let value: Element<'static, Message> = match (definition.map(|d| d.control), &spec) {
        (Some(qtable_engine::model::Control::Toggle), _) => {
            let on = selected.is_some_and(|(_, p, _)| p.value >= 0.5);
            fields.controls.toggle(
                Control::new(
                    value_id,
                    Id::PointValue,
                    Toggle {
                        label: if on { "On" } else { "Off" }.into(),
                    },
                )
                .role(SizeRole::Fill)
                .lit(on)
                .enabled(enabled),
                Message::PointValue(track, slot, if on { 0.0 } else { 1.0 }),
            )
        }
        (Some(qtable_engine::model::Control::Enum), _) => {
            let levels: Vec<Level> = definition
                .map(|d| {
                    d.options
                        .iter()
                        .enumerate()
                        .map(|(i, o)| Level(i, o.label.clone()))
                        .collect()
                })
                .unwrap_or_default();
            let chosen = selected
                .and_then(|(_, p, _)| levels.get(p.value.round().max(0.0) as usize).cloned());
            fields.controls.dropdown(
                Control::new(value_id, Id::PointValue, Dropdown)
                    .role(SizeRole::Fill)
                    .enabled(enabled),
                levels,
                chosen,
                move |level: Level| Message::PointValue(track, slot, level.0 as f64),
            )
        }
        _ => number(
            ClipField::PointValue,
            selected.map_or_else(String::new, |(_, p, _)| {
                let shown = spec
                    .as_ref()
                    .map_or_else(|| p.value.to_string(), |s| s.format(p.value));
                fields.value(ClipField::PointValue, shown)
            }),
            Id::PointValue,
            spec.as_ref().map_or(Nudge::FLOAT, Nudge::from_spec),
            spec.as_ref().map(|s| s.default),
            enabled,
        ),
    };
    // A last point has no leaving segment, and Discrete arguments have no curvature.
    let bends = axis != Some(qtable_engine::model::Axis::Discrete)
        && selected.is_some_and(|(i, _, len)| i + 1 < len);
    let curve = number(
        ClipField::PointCurve,
        selected.map_or_else(String::new, |(_, p, _)| {
            fields.value(
                ClipField::PointCurve,
                qtable_engine::model::format_float(f64::from(p.curve), Some(2)),
            )
        }),
        Id::PointCurve,
        Nudge::continuous(-1.0, 1.0, 0.05),
        Some(0.0),
        bends,
    );
    row![
        fields
            .controls
            .labelled("Position", State::default(), position),
        fields.controls.labelled("Value", State::default(), value),
        fields.controls.labelled(
            "Curve",
            State {
                enabled: bends,
                ..State::default()
            },
            curve
        ),
    ]
    .spacing(detail::CONTROL_GAP)
    .width(Length::Fill)
    .into()
}

fn argument_label(definitions: &[FieldDef], position: usize) -> String {
    definitions
        .get(position)
        .map(|field| field.display_name(position).into_owned())
        .unwrap_or_else(|| FieldDef::position_label(position))
}

fn hit_row(
    fields: &Fields,
    index: usize,
    event: &Event,
    definitions: &[FieldDef],
) -> Element<'static, Message> {
    let (track, slot) = (fields.track, fields.slot);
    let mut args = row![
        text("Hit arguments")
            .size(detail::TEXT_SIZE)
            .height(detail::CONTROL_HEIGHT)
            .align_y(iced::Alignment::Center)
    ]
    .spacing(detail::CONTROL_GAP)
    .width(Length::Fill)
    .align_y(iced::alignment::Vertical::Bottom);
    if event.arguments.is_empty() {
        args = args.push(hint("No arguments. Add them on the device."));
    }
    for (argument, arg) in event.arguments.iter().enumerate() {
        let label = argument_label(definitions, argument);
        let width = argument_role(arg).width();
        args = args.push(
            fields
                .controls
                .labelled(
                    label,
                    State::default(),
                    argument_control(fields, index, argument, arg),
                )
                .width(width),
        );
    }
    // Remove hit follows the last argument onto its line.
    args.push(
        style::flat("Remove hit")
            .on_press(Message::RemoveEvent(track, slot, index))
            .info(Id::RemoveEvent, Message::Info),
    )
    .wrap()
    .vertical_spacing(detail::CONTROL_GAP)
    .into()
}

/// Bool values commit immediately; the other types retain their text draft/submit flow.
fn argument_role(value: &OscArgument) -> SizeRole {
    match value {
        OscArgument::Bool(_) => SizeRole::HitBool,
        OscArgument::String(_) => SizeRole::HitString,
        _ => SizeRole::HitNumber,
    }
}

fn argument_control(
    fields: &Fields,
    event: usize,
    argument: usize,
    value: &OscArgument,
) -> Element<'static, Message> {
    let (track, slot) = (fields.track, fields.slot);
    let field = ClipField::EventArgument { event, argument };
    if let OscArgument::Bool(on) = value {
        fields.controls.toggle(
            Control::new(
                ControlId::field(View::Clip, field_key(track, slot, field)),
                Id::ArgumentValue,
                Toggle {
                    label: on.to_string(),
                },
            )
            .role(SizeRole::HitBool)
            .lit(*on),
            Message::Argument(
                track,
                slot,
                event,
                argument,
                "bool".into(),
                (!on).to_string(),
            ),
        )
    } else {
        let raw = fields.value(field, argument_text(value));
        let key = field_key(track, slot, field);
        let submit = Message::Argument(
            track,
            slot,
            event,
            argument,
            ArgKind::of(value).name().into(),
            raw.clone(),
        );
        if matches!(value, OscArgument::Int(_) | OscArgument::Float(_)) {
            let error = crate::drafts::field_error(fields.drafts, fields.errors, key, Ok(()));
            fields.controls.number(
                Control::new(
                    ControlId::field(View::Clip, key),
                    Id::ArgumentValue,
                    Number::new(""),
                )
                .nudge(if matches!(value, OscArgument::Int(_)) {
                    crate::control::Nudge::integer(f64::from(i32::MIN), f64::from(i32::MAX))
                } else {
                    crate::control::Nudge::FLOAT
                })
                .role(SizeRole::HitNumber)
                .invalid(error.is_some()),
                &raw,
                move |s| Message::Draft(key, s),
                submit,
                None,
            )
        } else {
            fields.input(
                field,
                &raw,
                (SizeRole::HitString, Id::ArgumentValue),
                submit,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::{layout, renderer::Headless, widget::Tree};
    use iced::{Renderer, Size};
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    #[test]
    fn hit_labels_match_face_positions_instead_of_wire_type_letters() {
        let mut definitions: Vec<_> = [
            qtable_engine::model::FieldType::Int,
            qtable_engine::model::FieldType::Float,
            qtable_engine::model::FieldType::Bool,
            qtable_engine::model::FieldType::String,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, kind)| {
            FieldDef::new(
                qtable_engine::model::FieldKey(format!("f{}", i + 1)),
                kind,
                false,
            )
        })
        .collect();
        definitions[1].name = "Level".into();
        assert_eq!(argument_label(&definitions, 0), "Arg 1");
        assert_eq!(argument_label(&definitions, 1), "Level");
        assert_eq!(argument_label(&definitions, 2), "Arg 3");
        assert_eq!(argument_label(&definitions, 3), "Arg 4");
        definitions.swap(0, 3);
        assert_eq!(argument_label(&definitions, 0), "Arg 1");
        assert_eq!(argument_label(&definitions, 3), "Arg 4");
        assert_eq!(argument_label(&[], 0), "Arg 1");
    }

    #[test]
    fn argument_values_use_typed_controls_and_compact_widths() {
        let (drafts, errors) = (HashMap::new(), HashMap::new());
        let focus = crate::drafts::ControlFocus::default();
        let fields = Fields {
            track: TrackId(1),
            slot: 0,
            drafts: &drafts,
            errors: &errors,
            controls: Controls::new(crate::control::LIVE_DARK, &focus),
        };
        let input: Element<'_, Message> = iced::widget::text_input("", "").into();
        let button: Element<'_, Message> = iced::widget::button("true").into();
        let input_tag = Tree::new(&input).tag;
        let button_tag = Tree::new(&button).tag;
        fn contains(tree: &Tree, tag: iced::advanced::widget::tree::Tag) -> bool {
            tree.tag == tag || tree.children.iter().any(|child| contains(child, tag))
        }
        for value in [
            OscArgument::Bool(false),
            OscArgument::Bool(true),
            OscArgument::Int(3),
            OscArgument::Float(0.125),
            OscArgument::String("payload".into()),
        ] {
            let control = argument_control(&fields, 0, 0, &value);
            let tree = Tree::new(&control);
            let expected_width = match value {
                OscArgument::Bool(_) => 36.0,
                OscArgument::String(_) => 140.0,
                _ => 64.0,
            };
            assert_eq!(
                control.as_widget().size().width,
                Length::Fixed(expected_width)
            );
            let is_bool = matches!(value, OscArgument::Bool(_));
            assert_eq!(contains(&tree, input_tag), !is_bool, "{value:?}");
            assert_eq!(contains(&tree, button_tag), is_bool, "{value:?}");
        }
    }
    #[test]
    fn strip_has_finite_compact_layout_and_leaves_space_for_the_lanes() {
        let project =
            qtable_engine::file::load(std::path::Path::new("../../examples/generic-receiver.ron"))
                .unwrap();
        let track = &project.tracks[0];
        let clip = track.slots[0].clip.as_ref().unwrap();
        let mut future = std::pin::pin!(Renderer::new(
            iced::Font::default(),
            iced::Pixels(crate::style::detail::TEXT_SIZE as f32),
            Some("tiny-skia")
        ));
        let renderer = match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(Some(renderer)) => renderer,
            _ => panic!("software renderer must be available"),
        };
        let (drafts, errors) = (HashMap::new(), HashMap::new());
        let focus = crate::drafts::ControlFocus::default();
        let fields = Fields {
            track: track.id,
            slot: 0,
            drafts: &drafts,
            errors: &errors,
            controls: Controls::new(crate::control::LIVE_DARK, &focus),
        };
        let mut strip = view(&project, clip, &fields, (Some(0), Some(0), None, None));
        let mut tree = Tree::new(&strip);
        let node = strip.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(520.0, 220.0)),
        );
        fn finite(node: &layout::Node) {
            assert!(node.bounds().width.is_finite() && node.bounds().height.is_finite());
            for child in node.children() {
                finite(child);
            }
        }
        finite(&node);
        // Labels add one line above hit values at this width.
        assert!(node.size().height < 120.0, "strip uses {:?}", node.size());
        assert_eq!(node.size().width, 520.0);
    }
    fn software_renderer() -> Renderer {
        let mut future = std::pin::pin!(Renderer::new(
            iced::Font::default(),
            iced::Pixels(crate::style::detail::TEXT_SIZE as f32),
            Some("tiny-skia")
        ));
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(Some(renderer)) => renderer,
            _ => panic!("software renderer must be available"),
        }
    }
    fn seven_argument_clip() -> (Project, usize) {
        let mut project =
            qtable_engine::file::load(std::path::Path::new("../../examples/generic-receiver.ron"))
                .unwrap();
        let arguments = vec![
            OscArgument::Int(0),
            OscArgument::Float(0.5),
            OscArgument::String("hello".into()),
            OscArgument::Bool(true),
            OscArgument::Int(7),
            OscArgument::Float(-1.25),
            OscArgument::String("wrapped".into()),
        ];
        // A fresh event with the seven-argument signature, played by the first hit.
        let track = project.tracks[0].id;
        let path = qtable_engine::model::OscAddress::Relative("/seven".into());
        let at = qtable_engine::model::BeatTime::ZERO;
        let hit = project.hit(track, at, path, arguments).unwrap();
        let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
        clip.lanes.clear();
        clip.events[0] = hit;
        (project, 0)
    }
    #[test]
    fn hit_arguments_wrap_onto_further_lines_and_keep_the_tail_last() {
        let renderer = software_renderer();
        let (project, _) = seven_argument_clip();
        let track = &project.tracks[0];
        let event = &track.slots[0].clip.as_ref().unwrap().events[0];
        let (drafts, errors) = (HashMap::new(), HashMap::new());
        let focus = crate::drafts::ControlFocus::default();
        let fields = Fields {
            track: track.id,
            slot: 0,
            drafts: &drafts,
            errors: &errors,
            controls: Controls::new(crate::control::LIVE_DARK, &focus),
        };
        let lines = |width: f32| {
            let mut row = hit_row(&fields, 0, event, &[]);
            let mut tree = Tree::new(&row);
            let node = row.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(width, 600.0)),
            );
            let items = node.children();
            // Label, seven argument groups, then Remove hit.
            assert_eq!(items.len(), 9);
            for item in items {
                assert!(
                    item.bounds().x + item.bounds().width <= width + 0.5,
                    "{:?} overflows {width}",
                    item.bounds()
                );
            }
            // Items on one line share a bottom edge, so judge lines by overlap:
            // nothing starts below the tail's bottom, and a line break moves past the last one.
            let tail = items.last().unwrap().bounds();
            if width == 2000.0 {
                let baseline = items[0].bounds().y + items[0].bounds().height;
                for item in items {
                    let b = item.bounds();
                    assert!(
                        (b.y + b.height - baseline).abs() < 0.01,
                        "label, fields and Remove hit share the value-row bottom: {b:?}"
                    );
                }
            }
            let mut count = 1;
            let mut line_bottom = items[0].bounds().y + items[0].bounds().height;
            for item in items {
                let b = item.bounds();
                assert!(b.y <= tail.y + tail.height, "{b:?} below the tail {tail:?}");
                if b.y >= line_bottom {
                    count += 1;
                    line_bottom = b.y + b.height;
                } else {
                    line_bottom = line_bottom.max(b.y + b.height);
                }
            }
            count
        };
        assert_eq!(lines(2000.0), 1);
        assert!(lines(600.0) >= 2);
        assert!(lines(320.0) > lines(600.0));
    }
}
