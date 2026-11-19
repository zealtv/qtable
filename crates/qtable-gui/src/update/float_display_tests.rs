//! Floats display with at least two decimals and retain all model precision.
use super::update;
use crate::app::App;
use crate::drafts::DraftKey;
use crate::drafts::TrackField;
use crate::files;
use crate::messages::{Message, OutputMessage};
use qtable_engine::engine::Engine;
use qtable_engine::model::TRACK_DELAY;

fn send(app: &mut App, message: Message) {
    let _ = update(app, message);
}

fn app() -> App {
    App::new(
        Engine::new(files::new_project()).unwrap(),
        "120".into(),
        None,
    )
}

#[test]
fn floats_display_two_decimals() {
    assert_eq!(TRACK_DELAY.format(0.0), "0.00");
    assert_eq!(TRACK_DELAY.format(-12.5), "-12.50");
    assert_eq!(TRACK_DELAY.format(0.1234), "0.1234");
}

#[test]
fn untouched_submit_keeps_the_precise_track_delay() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    send(
        &mut app,
        Message::Outputs(OutputMessage::TrackDelayChanged(track, "0.12345678".into())),
    );
    send(
        &mut app,
        Message::Outputs(OutputMessage::TrackDelaySubmitted(
            track,
            "0.12345678".into(),
        )),
    );
    assert_eq!(app.project().tracks[0].delay_ms, 0.12345678);
    let revision = app.project_revision();
    // Submitting the rounded read-out without typing is a no-op.
    send(
        &mut app,
        Message::Outputs(OutputMessage::TrackDelaySubmitted(track, "0.1235".into())),
    );
    assert_eq!(app.project().tracks[0].delay_ms, 0.12345678);
    assert_eq!(app.project_revision(), revision);
    // Typing a shorter value is an explicit edit and applies.
    send(
        &mut app,
        Message::Outputs(OutputMessage::TrackDelayChanged(track, "0.12".into())),
    );
    send(
        &mut app,
        Message::Outputs(OutputMessage::TrackDelaySubmitted(track, "0.12".into())),
    );
    assert_eq!(app.project().tracks[0].delay_ms, 0.12);
}

#[test]
fn untouched_submit_keeps_the_precise_output_latency() {
    let mut app = app();
    send(&mut app, Message::Outputs(OutputMessage::AddOutput));
    let output = app.project().outputs[0].id;
    send(
        &mut app,
        Message::Outputs(OutputMessage::OutputLatencyChanged(
            output,
            "0.12345678".into(),
        )),
    );
    send(
        &mut app,
        Message::Outputs(OutputMessage::OutputLatencySubmitted(
            output,
            "0.12345678".into(),
        )),
    );
    assert_eq!(app.project().outputs[0].delay_ms, 0.12345678);
    send(
        &mut app,
        Message::Outputs(OutputMessage::OutputLatencySubmitted(
            output,
            "0.1235".into(),
        )),
    );
    assert_eq!(app.project().outputs[0].delay_ms, 0.12345678);
}

#[test]
fn invalid_drafts_stay_editable() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    send(
        &mut app,
        Message::Outputs(OutputMessage::TrackDelayChanged(track, "abc".into())),
    );
    send(
        &mut app,
        Message::Outputs(OutputMessage::TrackDelaySubmitted(track, "abc".into())),
    );
    assert_eq!(
        app.drafts
            .values
            .get(&DraftKey::Track(track, TrackField::Delay))
            .map(String::as_str),
        Some("abc")
    );
    assert_eq!(app.project().tracks[0].delay_ms, 0.0);
}

#[test]
fn tempo_shows_two_decimals_and_an_untouched_submit_keeps_its_precision() {
    let mut app = app();
    send(&mut app, Message::TempoChanged("120.126789".into()));
    send(&mut app, Message::TempoSubmitted);
    assert_eq!(app.project().tempo.bpm(), 120.126789);
    // Tempo uses the four-decimal display limit without changing the model.
    assert_eq!(app.tempo_input, "120.1268");
    let revision = app.project_revision();
    send(&mut app, Message::TempoSubmitted);
    assert_eq!(app.project().tempo.bpm(), 120.126789);
    assert_eq!(app.project_revision(), revision);
    // Typing a shorter value is an explicit edit and applies.
    send(&mut app, Message::TempoChanged("120.13".into()));
    send(&mut app, Message::TempoSubmitted);
    assert_eq!(app.project().tempo.bpm(), 120.13);
    // A rejected draft stays in the field with its error.
    send(&mut app, Message::TempoChanged("fast".into()));
    send(&mut app, Message::TempoSubmitted);
    assert_eq!(app.tempo_input, "fast");
    assert_eq!(app.project().tempo.bpm(), 120.13);
}
