//! Shared Track Delay and output Latency fields for Detail View.
use crate::app::App;
use crate::control::{Number, SizeRole, ValueFormat};
use crate::control_focus::View;
use crate::drafts::OutputField;
use crate::drafts::TrackField;
use crate::drafts::{Control, ControlId, DraftKey};
use crate::info::Id;
use crate::messages::{Message, OutputMessage};
use iced::Element;
use qtable_engine::model::{OUTPUT_LATENCY, TRACK_DELAY};
use qtable_engine::model::{Region, Track, TrackId};

const DELAY: ValueFormat = ValueFormat::Parameter(&TRACK_DELAY);
const LATENCY: ValueFormat = ValueFormat::Parameter(&OUTPUT_LATENCY);

/// A track's delay in milliseconds, sharing the Track tab's draft and commit path.
pub(crate) fn delay_input(
    app: &App,
    track: TrackId,
    view: View,
    role: SizeRole,
) -> Element<'_, Message> {
    let model = app.project().track(track).map(|t| t.delay_ms);
    delay_field(
        app,
        (DraftKey::Track(track, TrackField::Delay), view, role),
        match app.project().track_capabilities(track) {
            Some(c) if c.foldable => Id::GroupDelay,
            Some(c) if c.region == Region::Main => Id::MainDelay,
            _ => Id::TrackDelay,
        },
        model,
        move |value| OutputMessage::TrackDelayChanged(track, value),
        move |value| OutputMessage::TrackDelaySubmitted(track, value),
    )
}

pub(crate) fn latency_input<'a>(
    app: &'a App,
    output: &Track,
    view: View,
    role: SizeRole,
) -> Element<'a, Message> {
    let id = output.id;
    let key = DraftKey::Output(id, OutputField::Latency);
    let value = app
        .drafts
        .values
        .get(&key)
        .cloned()
        .unwrap_or_else(|| LATENCY.display(output.delay_ms));
    field(
        app,
        (key, view, role),
        Id::OutputLatency,
        value.clone(),
        move |value| OutputMessage::OutputLatencyChanged(id, value),
        OutputMessage::OutputLatencySubmitted(id, value),
    )
}

/// A delay field showing its draft, or the model value with two to four decimals.
fn delay_field<'a>(
    app: &'a App,
    (key, view, role): (DraftKey, View, SizeRole),
    help: Id,
    model: Option<f64>,
    changed: impl Fn(String) -> OutputMessage + 'a,
    submitted: impl FnOnce(String) -> OutputMessage,
) -> Element<'a, Message> {
    let value = app
        .drafts
        .values
        .get(&key)
        .cloned()
        .unwrap_or_else(|| model.map_or_else(String::new, |m| DELAY.display(m)));
    field(
        app,
        (key, view, role),
        help,
        value.clone(),
        changed,
        submitted(value),
    )
}

fn field<'a>(
    app: &'a App,
    (key, view, role): (DraftKey, View, SizeRole),
    help: Id,
    value: String,
    changed: impl Fn(String) -> OutputMessage + 'a,
    submitted: OutputMessage,
) -> Element<'a, Message> {
    app.controls().number(
        Control::new(ControlId::field(view, key), help, Number::new("0"))
            .role(role)
            .nudge(crate::control::Nudge::continuous(
                qtable_engine::model::TRACK_DELAY.min,
                qtable_engine::model::TRACK_DELAY.max,
                1.0,
            ))
            .default_to(qtable_engine::model::TRACK_DELAY.default)
            .invalid(app.drafts.errors.contains_key(&key)),
        &value,
        move |text| Message::Outputs(changed(text)),
        Message::Outputs(submitted),
        None,
    )
}

#[cfg(test)]
mod tests;
