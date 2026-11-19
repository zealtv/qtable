//! Live-style stacked Clip and Launch sections; GUI state never enters project files.
use super::clip_panel_update::percent_b;
use super::musical_time::{format_duration, format_multiplier};
use super::{Message, choices::LaunchQuantChoice, field_key};
use crate::control::rule::{Axis, Level};
use crate::control::{
    Dropdown, Number, Readout, SizeRole, State as ControlState, TimeField, Toggle,
};
use crate::control_focus::{ControlOwner, View};
use crate::detail_layout::{self, Header, Layout, Section};
use crate::drafts::{ClipField, Control, ControlId, Controls, DraftKey};
use crate::info::{Help, Id};
use crate::inspector_panel::Tab;
use crate::style::{self, detail};
use iced::widget::{button, column, container, row, text};
use iced::{Border, Color, Element, Length, alignment};
use qtable_engine::model::{Clip, Project, TimeSignature, TrackId};
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct State {
    pub(crate) editor: super::event_editor::State,
    pub(crate) palette_open: bool,
}

struct Fields<'a> {
    track: TrackId,
    slot: usize,
    drafts: &'a HashMap<DraftKey, String>,
    errors: &'a HashMap<DraftKey, String>,
    controls: Controls<'a>,
    signature: TimeSignature,
}

impl Fields<'_> {
    fn value(&self, field: ClipField, model: String) -> String {
        self.drafts
            .get(&field_key(self.track, self.slot, field))
            .cloned()
            .unwrap_or(model)
    }

    /// A clip button or picker's focus identity.
    fn id(&self, name: &'static str) -> ControlId {
        ControlId::action(
            View::Clip,
            name,
            ControlOwner::ClipSlot(self.track, self.slot),
        )
    }

    fn number(
        &self,
        field: ClipField,
        value: &str,
        (role, help): (SizeRole, Id),
        on_submit: Message,
    ) -> Element<'static, Message> {
        let key = field_key(self.track, self.slot, field);
        let signature = self.signature;
        let time_kind = match field {
            ClipField::Length | ClipField::FollowTime => Some(TimeField::duration(signature)),
            ClipField::Start | ClipField::End | ClipField::Position => {
                Some(TimeField::position(signature))
            }
            _ => None,
        };
        if let Some(kind) = time_kind {
            return self.controls.time_field(
                Control::new(ControlId::field(View::Clip, key), help, kind)
                    .role(role)
                    .invalid(self.errors.contains_key(&key)),
                value,
                move |value| Message::Draft(key, value),
                on_submit,
            );
        }
        let number = match field {
            ClipField::PercentA | ClipField::PercentB => {
                Number::new("").adjust_text(crate::control::adjustment::percent)
            }
            ClipField::Multiplier => {
                Number::new("").adjust_text(super::musical_time::adjust_multiplier)
            }
            _ => Number::new(""),
        };
        self.controls.number(
            Control::new(ControlId::field(View::Clip, key), help, number)
                .role(role)
                .invalid(self.errors.contains_key(&key)),
            value,
            move |value| Message::Draft(key, value),
            on_submit,
            None,
        )
    }

    /// A value shown in a box that takes no focus; `enabled` false dims it.
    fn readout(
        &self,
        (name, help): (&'static str, Id),
        value: impl Into<String>,
        enabled: bool,
    ) -> Element<'static, Message> {
        self.controls.readout_box(
            Control::new(self.id(name), help, Readout::right()).enabled(enabled),
            value,
        )
    }

    fn toggle(
        &self,
        (name, help): (&'static str, Id),
        label: &str,
        on: bool,
        on_press: Message,
    ) -> Element<'static, Message> {
        self.controls.toggle(
            Control::new(
                self.id(name),
                help,
                Toggle {
                    label: label.into(),
                },
            )
            .role(SizeRole::Fill)
            .lit(on),
            on_press,
        )
    }
}

pub(super) fn sections(
    project: &Project,
    clip: &Clip,
    selected: (TrackId, usize),
    drafts: &HashMap<DraftKey, String>,
    errors: &HashMap<DraftKey, String>,
    states: (&State, &detail_layout::State),
    controls: Controls<'_>,
) -> Layout<'static, Message> {
    let (state, layout_state) = states;
    let (track, slot) = selected;
    let fields = Fields {
        track,
        slot,
        drafts,
        errors,
        controls,
        signature: project.time_signature,
    };
    let colour = crate::outputs::to_colour(clip.colour);
    let name = fields.value(ClipField::Name, clip.name.clone());
    let name_key = crate::view::clip_name_key(track, slot);
    let name_invalid = errors.contains_key(&name_key);
    let title = Header {
        colour,
        content: row![
            button(
                container(iced::widget::Space::new().width(9).height(9))
                    .style(move |_| title_swatch_style(colour))
            )
            .padding(1)
            .on_press(Message::TogglePalette)
            .style(|_, _| button::Style {
                background: None,
                ..Default::default()
            })
            .info(Id::ColourChooser, Message::Info),
            // The name sits on the clip colour, so it keeps its own style and only takes focus.
            controls.wrap(
                ControlId::field(View::Clip, name_key),
                name_invalid,
                style::input("Clip name", &name)
                    .id(name_key.input_id())
                    .on_input(move |value| Message::Draft(name_key, value))
                    .on_submit(Message::ClipName(track, slot, name))
                    .on_enter(Message::FieldEntered)
                    .size(detail::HEADER_SIZE)
                    .font(iced::Font {
                        weight: iced::font::Weight::Semibold,
                        ..Default::default()
                    })
                    .style(move |_, status| title_name_style(colour, name_invalid, status))
                    .info(Id::ClipName, Message::Info),
            ),
        ]
        .spacing(4)
        .align_y(alignment::Vertical::Center)
        .into(),
    };
    let mut body = column![].spacing(0);
    if state.palette_open {
        let mut palette = row![].spacing(3);
        for entry in &style::PALETTE {
            let colour = entry.colour;
            let label = entry.inspector_label;
            palette = palette.push(crate::control::tooltip(
                button(iced::widget::Space::new().width(18).height(16))
                    .padding(0)
                    .on_press(Message::Colour(track, slot, colour))
                    .style(move |_, _| button::Style {
                        background: Some(crate::outputs::to_colour(colour).into()),
                        border: Border {
                            color: style::TEXT,
                            width: 1.0,
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .info(Id::ClipColour, Message::Info),
                label,
                iced::widget::tooltip::Position::Top,
            ));
        }
        body = body.push(palette);
    }
    let signature = project.time_signature;
    let playing = state.editor.playing;
    let marker = |field, label: &'static str, name, time, help| -> Element<'static, Message> {
        let value = fields.value(
            field,
            if field == ClipField::Length {
                format_duration(time, signature)
            } else {
                super::musical_time::format_position(time, signature)
            },
        );
        let number = fields.number(
            field,
            &value,
            (SizeRole::Fill, help),
            if field == ClipField::Length {
                Message::Length(track, slot, value.clone())
            } else {
                Message::Marker(track, slot, field, value.clone())
            },
        );
        column![
            row![
                text(label).size(detail::TEXT_SIZE).width(Length::Fill),
                fields.controls.button(
                    Control::new(
                        fields.id(name),
                        Id::ClipMarkerSet,
                        crate::control::Button {
                            face: crate::control::Face::Text("Set".into()),
                            latched: false
                        }
                    )
                    .enabled(playing),
                    Message::SetMarker(track, slot, field)
                ),
            ]
            .align_y(alignment::Vertical::Center),
            number
        ]
        .into()
    };
    let clip_body: Element<'static, Message> = column![
        detail::pair(
            marker(
                ClipField::Start,
                "Start",
                "set-start",
                clip.start,
                Id::ClipStart
            ),
            marker(ClipField::End, "End", "set-end", clip.end, Id::ClipEnd)
        ),
        fields.toggle(
            ("loop", Id::Loop),
            "Loop",
            clip.looping,
            Message::ToggleLoop(track, slot)
        ),
        detail::pair(
            marker(
                ClipField::Position,
                "Position",
                "set-position",
                clip.loop_start,
                Id::ClipPosition
            ),
            marker(
                ClipField::Length,
                "Length",
                "set-length",
                qtable_engine::model::BeatTime(clip.loop_end.ticks() - clip.loop_start.ticks()),
                Id::ClipLength
            )
        ),
        fields.controls.labelled(
            "Signature",
            ControlState::default(),
            fields.readout(
                ("signature", Id::Meter),
                format!("{} / {}", signature.numerator(), signature.denominator()),
                true
            )
        ),
    ]
    .spacing(detail::CONTROL_GAP)
    .into();
    Layout {
        header: title,
        leading: (state.palette_open || errors.contains_key(&name_key)).then(|| body.into()),
        sections: vec![
            Section {
                id: detail_layout::CLIP,
                title: "Clip",
                icon: Some(section_icon(false)),
                expanded: layout_state.expanded(Tab::Clip, detail_layout::CLIP),
                on_toggle: Message::ToggleSection(Tab::Clip, detail_layout::CLIP),
                help: Id::ClipSection,
                body: clip_body,
            },
            Section {
                id: detail_layout::LAUNCH,
                title: "Launch",
                icon: Some(section_icon(true)),
                expanded: layout_state.expanded(Tab::Clip, detail_layout::LAUNCH),
                on_toggle: Message::ToggleSection(Tab::Clip, detail_layout::LAUNCH),
                help: Id::LaunchSection,
                body: launch(project, clip, &fields),
            },
        ],
        content: None,
    }
}

fn title_name_style(
    colour: Color,
    invalid: bool,
    status: iced::widget::text_input::Status,
) -> iced::widget::text_input::Style {
    let ink = crate::control::theme::ink(colour);
    let mut field = (style::field_style(invalid))(&iced::Theme::Dark, status);
    field.background = colour.into();
    field.value = ink;
    field.placeholder = ink;
    if !invalid {
        field.border.width = 0.0;
    }
    field
}

fn title_swatch_style(colour: Color) -> container::Style {
    container::Style {
        background: Some(colour.into()),
        border: Border {
            color: crate::control::theme::ink(colour),
            width: 1.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn launch(project: &Project, clip: &Clip, fields: &Fields<'_>) -> Element<'static, Message> {
    let track = fields.track;
    let slot = fields.slot;
    let actions = &clip.follow_actions.actions;
    let enabled = actions.enabled;
    let b = percent_b(actions.chance_a, actions.chance_b);
    let percentages = |a: bool| {
        let field = if a {
            ClipField::PercentA
        } else {
            ClipField::PercentB
        };
        let value = fields.value(
            field,
            b.map(|b| if a { 100 - b } else { b })
                .unwrap_or(0)
                .to_string()
                + " %",
        );
        fields.number(
            field,
            &value,
            (SizeRole::Tag, Id::FollowChance),
            Message::FollowPercent(track, slot, a, value.clone()),
        )
    };
    let signature = project.time_signature;
    let mut controls = super::follow_controls::FollowControls {
        toggle: fields.toggle(
            ("follow-action", Id::FollowEnabled),
            "Follow Action",
            enabled,
            Message::FollowEnabled(track, slot, !enabled),
        ),
        actions: super::choices::action_pair(
            project,
            (
                fields.controls,
                View::Clip,
                ControlOwner::ClipSlot(track, slot),
            ),
            (actions.action_a, actions.action_b),
            move |choice| choice.to_message(track, slot, true),
            move |choice| choice.to_message(track, slot, false),
        ),
        chances: [percentages(true), percentages(false)],
        percentage_b: b,
        on_split: Box::new(move |b| Message::ChanceSplit(track, slot, b.round() as u8)),
        timing: follow_timing(clip, signature, fields),
    }
    .view(Message::Info);
    controls = controls
        .push(fields.controls.rule(Axis::Horizontal, Level::Group))
        .push(detail::pair(
            fields.controls.labelled(
                "Launch",
                ControlState {
                    enabled: false,
                    ..ControlState::default()
                },
                fields.readout(("launch-mode", Id::LaunchMode), "Trigger", false),
            ),
            fields.controls.labelled(
                "",
                ControlState::default(),
                fields.controls.wrap(
                    fields.id("legato"),
                    false,
                    detail::toggle("Legato", clip.legato)
                        .on_press(Message::ToggleLegato(track, slot))
                        .width(Length::Fill)
                        .info(Id::Legato, Message::Info),
                ),
            ),
        ))
        .push(
            fields.controls.labelled(
                "Quantize",
                ControlState::default(),
                fields.controls.dropdown(
                    Control::new(fields.id("quantisation"), Id::ClipQuantisation, Dropdown)
                        .role(SizeRole::Fill),
                    &LaunchQuantChoice::ALL[..],
                    Some(LaunchQuantChoice::from(clip.quantisation)),
                    move |q| Message::Quantisation(track, slot, q),
                ),
            ),
        );
    controls.into()
}

/// Live's Linked/Unlinked toggle with the field it governs: loops or a duration.
fn follow_timing(
    clip: &Clip,
    signature: TimeSignature,
    fields: &Fields<'_>,
) -> Element<'static, Message> {
    let (track, slot) = (fields.track, fields.slot);
    let linked = clip.follow_actions.linked;
    let toggle = fields.toggle(
        ("follow-linked", Id::FollowLink),
        if linked { "⊷ Linked" } else { "⊷ Unlinked" },
        linked,
        Message::FollowLinked(track, slot, !linked),
    );
    let value: Element<'static, Message> = if linked && !clip.looping {
        // Live shows an unclickable "Clip End" while Loop is off: no multiplier applies.
        fields.readout(("clip-end", Id::FollowMultiplier), "Clip End", false)
    } else if linked {
        let text = fields.value(
            ClipField::Multiplier,
            format_multiplier(clip.follow_actions.multiplier),
        );
        fields.number(
            ClipField::Multiplier,
            &text,
            (SizeRole::Fill, Id::FollowMultiplier),
            Message::Multiplier(track, slot, text.clone()),
        )
    } else {
        let time = clip.follow_actions.time;
        let text = fields.value(ClipField::FollowTime, format_duration(time, signature));
        fields.number(
            ClipField::FollowTime,
            &text,
            (SizeRole::Fill, Id::FollowTime),
            Message::FollowTime(track, slot, text.clone()),
        )
    };
    detail::pair(toggle, value).into()
}

fn section_icon(launch: bool) -> Element<'static, Message> {
    use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
    use iced::{Point, Rectangle, Size, mouse};
    struct Icon(bool);
    impl canvas::Program<Message> for Icon {
        type State = ();
        fn draw(
            &self,
            _: &(),
            renderer: &iced::Renderer,
            _: &iced::Theme,
            bounds: Rectangle,
            _: mouse::Cursor,
        ) -> Vec<Geometry> {
            let mut frame = Frame::new(renderer, bounds.size());
            let triangle = Path::new(|p| {
                p.move_to(Point::new(4.0, 3.0));
                p.line_to(Point::new(9.0, 6.0));
                p.line_to(Point::new(4.0, 9.0));
                p.close();
            });
            frame.fill(&triangle, style::TEXT);
            if self.0 {
                for x in [1.0, 11.0] {
                    frame.stroke(
                        &Path::line(Point::new(x, 2.0), Point::new(x, 10.0)),
                        Stroke::default().with_color(style::TEXT).with_width(1.0),
                    );
                }
            } else {
                frame.stroke(
                    &Path::rectangle(Point::new(1.0, 1.0), Size::new(10.0, 10.0)),
                    Stroke::default().with_color(style::TEXT).with_width(1.0),
                );
            }
            vec![frame.into_geometry()]
        }
    }
    canvas::Canvas::new(Icon(launch))
        .width(12)
        .height(12)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::text_input::Status;

    #[test]
    fn title_name_and_swatch_use_grid_contrast_and_preserve_validation() {
        for colour in qtable_engine::model::LIVE_CLIP_PALETTE
            .into_iter()
            .map(|c| Color::from_rgb8(c.red, c.green, c.blue))
            .chain([Color::BLACK, Color::WHITE])
        {
            let ink = crate::control::theme::ink(colour);
            for status in [
                Status::Active,
                Status::Hovered,
                Status::Focused { is_hovered: false },
            ] {
                let name = title_name_style(colour, false, status);
                assert_eq!(name.value, ink);
                assert_eq!(name.placeholder, ink);
                assert_eq!(name.background, colour.into());
                assert_eq!(name.border.width, 0.0);
                let invalid = title_name_style(colour, true, status);
                assert_eq!(invalid.value, ink);
                assert_eq!(invalid.border.color, Color::from_rgb8(231, 105, 66));
                assert_eq!(invalid.border.width, 1.0);
            }
            assert_eq!(title_swatch_style(colour).border.color, ink);
        }
        assert_eq!(
            title_name_style(Color::BLACK, false, Status::Active).value,
            Color::WHITE
        );
        assert_eq!(title_swatch_style(Color::BLACK).border.color, Color::WHITE);
        assert_eq!(
            title_name_style(Color::WHITE, false, Status::Active).value,
            Color::BLACK
        );
        assert_eq!(title_swatch_style(Color::WHITE).border.color, Color::BLACK);
    }
}
