//! The Detail View's Output tab: every setting of the selected output. The output column's
//! band keeps the receiver labels and mixer switches.
use crate::app::App;
use crate::control::rule::{Axis, Level};
use crate::control::{ControlKind, Number, SizeRole, State, Text};
use crate::control_focus::View;
use crate::detail_layout::{self, Header, Layout};
use crate::drafts::{Control, ControlId, DraftKey, OutputField};
use crate::info::Id;
use crate::inspector_panel::Tab;
use crate::messages::{Message, OutputMessage};
use crate::style::detail::{self, CONTROL_GAP};
use iced::widget::{column, row};
use iced::{Element, Length};
use qtable_engine::model::{Track, TrackId};

pub(crate) fn view(app: &App) -> Layout<'_, Message> {
    let Some(output) = app.selection.anchor.and_then(|id| app.project().output(id)) else {
        return Layout::empty("No output selected.");
    };
    let section = |id, title, help, body| {
        app.detail_layout.section(
            (Tab::Output, id),
            title,
            help,
            body,
            Message::ToggleDetailSection,
        )
    };

    Layout {
        header: Header {
            colour: crate::outputs::to_colour(output.colour),
            content: detail::heading(output.name.clone()).into(),
        },
        leading: None,
        sections: vec![
            section(
                detail_layout::OUTPUT,
                "Output",
                Id::DetailOutputSection,
                destination_section(app, output, View::Output),
            ),
            section(
                detail_layout::THROUGHPUT,
                "Throughput",
                Id::DetailThroughputSection,
                crate::throughput::view(app, output.id),
            ),
        ],
        content: Some(crate::device_chain::view(app, output, View::Output)),
    }
}

/// Destination controls shared by every destination-capable track, including unset Main.
pub(crate) fn destination_section<'a>(
    app: &'a App,
    track: &'a Track,
    view: View,
) -> Element<'a, Message> {
    if !track.capabilities().destination {
        return iced::widget::Space::new().into();
    }
    column![
        endpoint(app, track, view),
        crate::control::rule::rule(&app.theme, Axis::Horizontal, Level::Group),
        crate::stage_settings::view(app, track.id, view)
    ]
    .spacing(CONTROL_GAP)
    .into()
}

fn draft(app: &App, key: DraftKey, model: String) -> String {
    app.drafts.values.get(&key).cloned().unwrap_or(model)
}

/// An Output field's descriptor, marked invalid while its draft is rejected.
fn control<K: ControlKind>(app: &App, view: View, key: DraftKey, help: Id, kind: K) -> Control<K> {
    Control::new(ControlId::field(view, key), help, kind)
        .invalid(app.drafts.errors.contains_key(&key))
}

fn endpoint<'a>(app: &'a App, output: &'a Track, view: View) -> Element<'a, Message> {
    let id = output.id;
    let destination = output
        .destination()
        .or(app.dormant_destinations.get(&id).map(|o| &o.destination));
    let controls = app.controls();
    let host = draft(
        app,
        DraftKey::Output(id, OutputField::Host),
        destination.map_or_else(String::new, |d| d.host.clone()),
    );
    let port = draft(
        app,
        DraftKey::Output(id, OutputField::Port),
        destination.map_or(9000, |d| d.port).to_string(),
    );
    let mut body = column![
        row![
            controls
                .labelled(
                    "Host",
                    State::default(),
                    controls.text(
                        control(
                            app,
                            view,
                            DraftKey::Output(id, OutputField::Host),
                            Id::OutputHost,
                            Text {
                                placeholder: "host".into()
                            }
                        ),
                        &host,
                        move |value| Message::Outputs(OutputMessage::OutputHostChanged(id, value)),
                        Message::Outputs(OutputMessage::OutputHostSubmitted(id, host.clone())),
                    ),
                )
                .width(Length::FillPortion(3)),
            controls
                .labelled(
                    "Port",
                    State::default(),
                    controls.number(
                        control(
                            app,
                            view,
                            DraftKey::Output(id, OutputField::Port),
                            Id::OutputPort,
                            Number::new("port")
                        )
                        .nudge(crate::control::Nudge::integer(1.0, f64::from(u16::MAX)))
                        .role(SizeRole::Port),
                        &port,
                        move |value| Message::Outputs(OutputMessage::OutputPortChanged(id, value)),
                        Message::Outputs(OutputMessage::OutputPortSubmitted(id, port.clone())),
                        None,
                    ),
                )
                .width(Length::FillPortion(2)),
        ]
        .spacing(CONTROL_GAP),
        crate::outputs::mode_toggles(app, output, view),
    ]
    .spacing(CONTROL_GAP)
    .width(Length::Fixed(detail::OUTPUT_COLUMN_WIDTH));
    if let Some(note) = crate::outputs::unresolved_note(app, output) {
        body = body.push(note);
    }
    body.into()
}

/// Commits a draft of one of the output's settings through its Enter handler.
pub(crate) fn commit(app: &mut App, output: TrackId, field: OutputField, value: String) {
    let message = match field {
        OutputField::Host => OutputMessage::OutputHostSubmitted(output, value),
        OutputField::Port => OutputMessage::OutputPortSubmitted(output, value),
        OutputField::Latency => OutputMessage::OutputLatencySubmitted(output, value),
        OutputField::FrameRate | OutputField::Budget | OutputField::Refresh => {
            OutputMessage::StageSubmit(DraftKey::Output(output, field))
        }
    };
    let _ = crate::update::outputs::update(app, message);
}
