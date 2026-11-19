//! Shared destination stage controls; Main without a host shows held settings disabled.
use crate::app::App;
use crate::control::{Number, SizeRole, State, Toggle};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::OutputField;
use crate::drafts::{Control, ControlId, DraftKey};
use crate::info::Id;
use crate::messages::{Message, OutputMessage};
use crate::style::detail;
use iced::widget::{column, row};
use iced::{Element, Length};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::{BUDGET, FRAME_RATE, OutputStage, ParameterSpec, REFRESH, TrackId};

fn field(key: DraftKey) -> Option<(TrackId, &'static ParameterSpec, Id)> {
    match key {
        DraftKey::Output(id, OutputField::FrameRate) => {
            Some((id, &FRAME_RATE, Id::OutputFrameRate))
        }
        DraftKey::Output(id, OutputField::Budget) => Some((id, &BUDGET, Id::OutputBudget)),
        DraftKey::Output(id, OutputField::Refresh) => Some((id, &REFRESH, Id::OutputRefresh)),
        _ => None,
    }
}

fn held_stage(app: &App, id: TrackId) -> OutputStage {
    app.project()
        .track(id)
        .and_then(|t| t.output_stage())
        .or_else(|| app.dormant_destinations.get(&id).map(|o| &o.stage))
        .cloned()
        .unwrap_or_default()
}

fn value(stage: &OutputStage, key: DraftKey) -> f64 {
    match key {
        DraftKey::Output(_, OutputField::FrameRate) => stage.frame_rate,
        DraftKey::Output(_, OutputField::Budget) => f64::from(stage.budget),
        DraftKey::Output(_, OutputField::Refresh) => stage.refresh,
        _ => unreachable!(),
    }
}

fn enabled(app: &App, id: TrackId, stage: &OutputStage, key: DraftKey) -> bool {
    app.project()
        .track(id)
        .and_then(|t| t.output_stage())
        .is_some()
        && match key {
            DraftKey::Output(_, OutputField::Budget) => stage.budget_on,
            DraftKey::Output(_, OutputField::Refresh) => stage.refresh_on,
            _ => true,
        }
}

pub(crate) fn number(app: &App, key: DraftKey, view: View, role: SizeRole) -> Element<'_, Message> {
    let (id, spec, help) = field(key).unwrap();
    let stage = held_stage(app, id);
    let text = app
        .drafts
        .values
        .get(&key)
        .cloned()
        .unwrap_or_else(|| spec.format(value(&stage, key)));
    app.controls().number(
        Control::new(ControlId::field(view, key), help, Number::new("0"))
            .role(role)
            .nudge(crate::control::Nudge::from_spec(spec))
            .default_to(spec.default)
            .enabled(enabled(app, id, &stage, key))
            .invalid(app.drafts.errors.contains_key(&key)),
        &text,
        move |text| Message::Outputs(OutputMessage::StageDraft(key, text)),
        Message::Outputs(OutputMessage::StageSubmit(key)),
        None,
    )
}

pub(crate) fn view(app: &App, id: TrackId, view: View) -> Element<'_, Message> {
    let stage = held_stage(app, id);
    let available = app
        .project()
        .track(id)
        .and_then(|t| t.output_stage())
        .is_some();
    let controls = app.controls();
    let toggle = |name, help, label: &str, on, msg| {
        controls.toggle(
            Control::new(
                ControlId::action(view, name, ControlOwner::Track(id)),
                help,
                Toggle {
                    label: label.into(),
                },
            )
            .role(SizeRole::Tag)
            .enabled(available)
            .lit(on),
            Message::Outputs(msg),
        )
    };
    let switched = |label, key, on, help, name, msg| {
        controls.labelled(
            label,
            State {
                enabled: available && on,
                ..State::default()
            },
            row![
                number(app, key, view, SizeRole::Fill),
                toggle(name, help, if on { "On" } else { "Off" }, on, msg)
            ]
            .spacing(detail::CONTROL_GAP),
        )
    };
    column![
        detail::pair(
            controls.labelled(
                "Frame Rate (Hz)",
                State {
                    enabled: available,
                    ..State::default()
                },
                number(
                    app,
                    DraftKey::Output(id, OutputField::FrameRate),
                    view,
                    SizeRole::Fill
                ),
            ),
            controls.labelled(
                if app.project().output(id).is_some() {
                    "Latency (ms)"
                } else {
                    "Track Delay (ms)"
                },
                State::default(),
                if let Some(output) = app.project().output(id) {
                    crate::delay_fields::latency_input(app, output, view, SizeRole::Fill)
                } else {
                    crate::delay_fields::delay_input(app, id, view, SizeRole::Fill)
                },
            ),
        ),
        detail::pair(
            switched(
                "Budget (msg/s)",
                DraftKey::Output(id, OutputField::Budget),
                stage.budget_on,
                Id::OutputBudget,
                "budget",
                OutputMessage::ToggleBudget(id)
            ),
            switched(
                "Refresh (s)",
                DraftKey::Output(id, OutputField::Refresh),
                stage.refresh_on,
                Id::OutputRefresh,
                "refresh",
                OutputMessage::ToggleRefresh(id)
            ),
        ),
        detail::pair(
            controls.labelled(
                "Bundle",
                State {
                    enabled: available,
                    ..State::default()
                },
                toggle(
                    "bundle",
                    Id::OutputBundle,
                    if stage.bundle { "On" } else { "Off" },
                    stage.bundle,
                    OutputMessage::ToggleBundle(id)
                )
            ),
            iced::widget::Space::new().width(Length::Fill),
        ),
    ]
    .spacing(detail::CONTROL_GAP)
    .width(Length::Fixed(detail::OUTPUT_COLUMN_WIDTH))
    .into()
}

pub(crate) fn update(app: &mut App, message: OutputMessage) {
    let id = match &message {
        OutputMessage::StageDraft(key, _) | OutputMessage::StageSubmit(key) => {
            let Some((id, _, _)) = field(*key) else {
                return;
            };
            id
        }
        OutputMessage::ToggleBudget(id)
        | OutputMessage::ToggleRefresh(id)
        | OutputMessage::ToggleBundle(id) => *id,
        _ => return,
    };
    let Some(mut stage) = app
        .project()
        .track(id)
        .and_then(|t| t.output_stage())
        .cloned()
    else {
        return;
    };
    let key = match message {
        OutputMessage::StageDraft(key, text) => {
            if enabled(app, id, &stage, key) {
                app.drafts.values.insert(key, text);
                app.drafts.errors.remove(&key);
            }
            return;
        }
        OutputMessage::StageSubmit(key) => {
            if !enabled(app, id, &stage, key) {
                return;
            }
            let Some(text) = app.drafts.values.get(&key) else {
                return;
            };
            let (_, spec, _) = field(key).unwrap();
            match spec.parse(text) {
                Ok(value) => match key {
                    DraftKey::Output(_, OutputField::FrameRate) => stage.frame_rate = value,
                    DraftKey::Output(_, OutputField::Budget) => stage.budget = value as u32,
                    DraftKey::Output(_, OutputField::Refresh) => stage.refresh = value,
                    _ => unreachable!(),
                },
                Err(error) => {
                    app.drafts.errors.insert(key, error.clone());
                    app.status.show_error(error);
                    return;
                }
            }
            Some(key)
        }
        OutputMessage::ToggleBudget(_) => {
            stage.budget_on = !stage.budget_on;
            None
        }
        OutputMessage::ToggleRefresh(_) => {
            stage.refresh_on = !stage.refresh_on;
            None
        }
        OutputMessage::ToggleBundle(_) => {
            stage.bundle = !stage.bundle;
            None
        }
        _ => return,
    };
    if crate::edit_actions::run_edit(
        app,
        Command::Edit(ProjectEdit::SetOutputStage { output: id, stage }),
    ) {
        if let Some(key) = key {
            app.drafts.forget(key);
        }
    } else if let Some(key) = key
        && let Some(error) = app.status.message()
    {
        app.drafts.errors.insert(key, error.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drafts::TrackField;
    use qtable_engine::engine::Engine;
    use qtable_engine::model::{Project, Track};

    fn app() -> App {
        App::new(
            Engine::new(Project {
                tracks: vec![Track::new(TrackId(1), "Track", 0)],
                outputs: vec![Track::new_output(TrackId(2), "Out", "127.0.0.1", 9000)],
                ..Project::default()
            })
            .unwrap(),
            String::new(),
            None,
        )
    }
    fn send(app: &mut App, msg: OutputMessage) {
        let _ = crate::update::update(app, Message::Outputs(msg));
    }
    fn submit(app: &mut App, key: DraftKey, text: &str) {
        send(app, OutputMessage::StageDraft(key, text.into()));
        send(app, OutputMessage::StageSubmit(key));
    }

    #[test]
    fn stage_edits_validate_commit_on_blur_and_preserve_disabled_values() {
        let mut app = app();
        let id = TrackId(2);
        let frame = DraftKey::Output(id, OutputField::FrameRate);
        submit(&mut app, frame, "61");
        assert_eq!(held_stage(&app, id).frame_rate, 30.0);
        assert_eq!(app.drafts.values[&frame], "61");
        assert!(app.drafts.errors.contains_key(&frame));
        assert!(app.status.message().unwrap().contains("Frame Rate"));
        send(
            &mut app,
            OutputMessage::StageDraft(frame, "44.12345678".into()),
        );
        let _ = crate::update::update(&mut app, Message::CommitField);
        assert_eq!(held_stage(&app, id).frame_rate, 44.12345678);
        // Enter on the displayed value without a draft must retain all stored precision.
        send(&mut app, OutputMessage::StageSubmit(frame));
        assert_eq!(held_stage(&app, id).frame_rate, 44.12345678);
        for (key, toggle, typed) in [
            (
                DraftKey::Output(id, OutputField::Budget),
                OutputMessage::ToggleBudget(id),
                "600",
            ),
            (
                DraftKey::Output(id, OutputField::Refresh),
                OutputMessage::ToggleRefresh(id),
                "0.8",
            ),
        ] {
            let original = value(&held_stage(&app, id), key);
            submit(&mut app, key, typed);
            assert_eq!(value(&held_stage(&app, id), key), original);
            send(&mut app, toggle.clone());
            submit(&mut app, key, typed);
            let stored = value(&held_stage(&app, id), key);
            send(&mut app, toggle.clone());
            submit(&mut app, key, "999");
            assert_eq!(value(&held_stage(&app, id), key), stored);
            send(&mut app, toggle);
            assert_eq!(value(&held_stage(&app, id), key), stored);
        }
        send(&mut app, OutputMessage::ToggleBundle(id));
        assert!(held_stage(&app, id).bundle);
    }

    #[test]
    fn main_keeps_stage_when_host_is_cleared_and_restored() {
        let mut app = app();
        let id = app.project().main.id;
        submit(&mut app, DraftKey::Output(id, OutputField::FrameRate), "44");
        send(&mut app, OutputMessage::ToggleBudget(id));
        submit(&mut app, DraftKey::Output(id, OutputField::Budget), "600");
        send(&mut app, OutputMessage::ToggleRefresh(id));
        submit(&mut app, DraftKey::Output(id, OutputField::Refresh), "0.8");
        send(&mut app, OutputMessage::ToggleBundle(id));
        let before = held_stage(&app, id);
        send(
            &mut app,
            OutputMessage::OutputHostSubmitted(id, String::new()),
        );
        assert!(app.project().main.output.is_none());
        assert_eq!(held_stage(&app, id), before);
        submit(&mut app, DraftKey::Output(id, OutputField::FrameRate), "60");
        send(&mut app, OutputMessage::ToggleBudget(id));
        assert_eq!(held_stage(&app, id), before);
        send(
            &mut app,
            OutputMessage::OutputPortSubmitted(id, "9100".into()),
        );
        send(
            &mut app,
            OutputMessage::OutputHostSubmitted(id, "127.0.0.1".into()),
        );
        assert_eq!(held_stage(&app, id), before);
        assert_eq!(app.project().main.destination().unwrap().port, 9100);
        assert!(app.dormant_destinations.is_empty());
    }

    #[test]
    fn stage_pairs_stay_level_and_timing_fields_have_only_their_detail_home() {
        let mut app = app();
        let id = TrackId(2);
        let bounds = |app: &App| {
            let mut panel = view(app, id, View::Output);
            crate::control_focus::test_registry::rectangles(&mut panel)
        };
        let before = bounds(&app);
        send(&mut app, OutputMessage::ToggleBudget(id));
        send(&mut app, OutputMessage::ToggleRefresh(id));
        assert_eq!(bounds(&app), before);
        let rect = |key| {
            before
                .iter()
                .find(|(c, _)| *c == ControlId::field(View::Output, key))
                .unwrap()
                .1
        };
        let budget = rect(DraftKey::Output(id, OutputField::Budget));
        let refresh = rect(DraftKey::Output(id, OutputField::Refresh));
        assert_eq!(budget.y, refresh.y);
        assert_eq!(budget.width, refresh.width);
        assert_eq!(
            refresh.x - budget.x,
            (detail::OUTPUT_COLUMN_WIDTH + detail::CONTROL_GAP) / 2.0
        );
        let id = TrackId(1);
        let mut strip = crate::throughput::strip(&app, id);
        assert!(
            crate::control_focus::test_registry::controls::<_, DraftKey>(&mut strip).is_empty()
        );
        let mut chain = crate::device_chain::view(&app, &app.project()[id], View::Track);
        let controls = crate::control_focus::test_registry::controls(&mut chain);
        assert!(controls.contains(&ControlId::field(
            View::Track,
            DraftKey::Track(id, TrackField::Delay)
        )));
        for (id, view) in [
            (TrackId(2), View::Output),
            (app.project().main.id, View::Main),
        ] {
            let mut strip = crate::throughput::strip(&app, id);
            assert!(
                crate::control_focus::test_registry::controls::<_, DraftKey>(&mut strip).is_empty()
            );
            let mut panel =
                crate::output_detail::destination_section(&app, &app.project()[id], view);
            let controls = crate::control_focus::test_registry::controls(&mut panel);
            assert!(controls.contains(&ControlId::field(
                view,
                DraftKey::Output(id, OutputField::FrameRate)
            )));
            let delay = if view == View::Output {
                DraftKey::Output(id, OutputField::Latency)
            } else {
                DraftKey::Track(id, TrackField::Delay)
            };
            assert!(controls.contains(&ControlId::field(view, delay)));
        }
    }
}
