//! Output and route messages (`OutputMessage`), routed as a complete typed domain.
use crate::app::App;
use crate::drafts::{DraftKey, OutputField, TrackField};
use crate::edit_actions::{execute_edit, run_edit};
use crate::fields::parse_port;
use crate::messages::{Message, OutputMessage};
use crate::output_band::choose_route;
use iced::Task;
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::{Destination, Output, OutputStage, TrackId, UdpMode};
use qtable_engine::model::{OUTPUT_LATENCY, TRACK_DELAY};

pub(crate) fn update(app: &mut App, message: OutputMessage) -> Task<Message> {
    // Any output action dismisses the open popover, except opening one. Toggling a route
    // inside its open list keeps the list, since it picks several.
    let keeps_overlay = match message {
        OutputMessage::OpenRouteDropdown(_) => true,
        OutputMessage::ChooseRoute(..) => app.route_dropdown().is_some(),
        _ => false,
    };
    if !keeps_overlay {
        app.close_overlay();
    }
    match message {
        message @ (OutputMessage::StageDraft(..)
        | OutputMessage::StageSubmit(..)
        | OutputMessage::ToggleBudget(_)
        | OutputMessage::ToggleRefresh(_)
        | OutputMessage::ToggleBundle(_)) => crate::stage_settings::update(app, message),
        OutputMessage::ChooseRoute(owner, choice) => choose_route(app, owner, choice),
        OutputMessage::OpenRouteDropdown(owner) => {
            app.open_route_dropdown(owner);
        }
        OutputMessage::AddOutput => add_output(app),
        OutputMessage::TrackDelayChanged(track, value) => {
            app.drafts
                .values
                .insert(DraftKey::Track(track, TrackField::Delay), value);
        }
        OutputMessage::TrackDelaySubmitted(track, input) => {
            let key = DraftKey::Track(track, TrackField::Delay);
            let current = app.project().track(track).map(|t| t.delay_ms);
            submit_delay(app, key, &input, current, |delay_ms| {
                Command::Edit(ProjectEdit::SetTrackDelay { track, delay_ms })
            });
        }
        OutputMessage::Ungroup(group) => {
            let selected = app.selection.tracks.contains(&group);
            let groups: Vec<_> = app
                .project()
                .group_tracks()
                .filter(|g| {
                    if selected {
                        app.selection.tracks.contains(&g.id)
                    } else {
                        g.id == group
                    }
                })
                .map(|g| g.id)
                .collect();
            // Each explicit Group is ungrouped once, including nested selected headers.
            // Engine edits retain descendant identities and own mapping cleanup.
            let promoted: Vec<_> = groups
                .iter()
                .flat_map(|id| app.project().group_member_ids(*id).unwrap_or_default())
                .collect();
            let changed = run_edit(
                app,
                Command::Edit(ProjectEdit::UngroupTracks {
                    groups: groups.clone(),
                }),
            );
            if changed {
                for group in groups {
                    app.drafts.forget(DraftKey::Track(group, TrackField::Name));
                }
            }
            if changed
                && let Some(first) = app
                    .project()
                    .tracks
                    .iter()
                    .find(|t| promoted.contains(&t.id))
                    .map(|t| t.id)
            {
                let action = if app.project()[first].is_group() {
                    crate::grid::Action::SelectGroup(first, false)
                } else {
                    crate::grid::Action::SelectTrack(first, false)
                };
                super::grid_actions::update(app, action);
            }
        }
        OutputMessage::CreateGroupAroundSelected => {
            if let Some(plan) = crate::header_actions::grouping_plan(app) {
                let first = plan.roots[0];
                execute_edit(
                    app,
                    Command::Edit(ProjectEdit::CreateGroup { tracks: plan.roots }),
                );
                if let Some(group) = app.project().track(first).and_then(|t| t.group) {
                    super::grid_actions::update(
                        app,
                        crate::grid::Action::SelectGroup(group, false),
                    );
                }
            }
        }
        OutputMessage::OutputHostChanged(id, value) => {
            app.drafts
                .values
                .insert(DraftKey::Output(id, OutputField::Host), value);
        }
        OutputMessage::OutputHostSubmitted(id, host) => {
            if let Some(mut output) = find_output(app, id) {
                let optional = app.project()[id].capabilities().destination_optional;
                if optional && host.trim().is_empty() {
                    // Clearing the host unsets the destination; its other settings stay dormant.
                    output.destination.host.clear();
                    if set_destination(app, id, None) {
                        app.dormant_destinations.insert(id, output);
                        app.drafts.forget(DraftKey::Output(id, OutputField::Host));
                    }
                    return Task::none();
                }
                output.destination.host = host;
                if store_destination(app, id, output) {
                    app.drafts.forget(DraftKey::Output(id, OutputField::Host));
                }
            }
        }
        OutputMessage::OutputPortChanged(id, value) => {
            app.drafts
                .values
                .insert(DraftKey::Output(id, OutputField::Port), value);
        }
        OutputMessage::OutputPortSubmitted(id, input) => {
            if let Some(mut output) = find_output(app, id) {
                match parse_port(&input) {
                    Ok(port) => {
                        output.destination.port = port;
                        if store_destination(app, id, output) {
                            app.drafts.forget(DraftKey::Output(id, OutputField::Port));
                        }
                    }
                    Err(error) => app.status.show_error(error),
                }
            }
        }
        OutputMessage::ToggleOutputMode(id) => {
            if let Some(mut output) = find_output(app, id) {
                output.destination.mode = if output.destination.mode == UdpMode::Unicast {
                    UdpMode::Broadcast
                } else {
                    UdpMode::Unicast
                };
                store_destination(app, id, output);
            }
        }
        OutputMessage::OutputLatencyChanged(id, value) => {
            app.drafts
                .values
                .insert(DraftKey::Output(id, OutputField::Latency), value);
        }
        OutputMessage::OutputLatencySubmitted(id, input) => {
            if let Some(current) = app
                .project()
                .track(id)
                .filter(|t| t.capabilities().destination)
                .map(|t| t.delay_ms)
            {
                match OUTPUT_LATENCY.parse(&input) {
                    Ok(_)
                        if !app
                            .drafts
                            .values
                            .contains_key(&DraftKey::Output(id, OutputField::Latency))
                            && OUTPUT_LATENCY.is_untouched_display(&input, current) => {}
                    Ok(latency_ms) => {
                        if run_edit(
                            app,
                            Command::Edit(ProjectEdit::SetTrackDelay {
                                track: id,
                                delay_ms: latency_ms,
                            }),
                        ) {
                            app.drafts
                                .forget(DraftKey::Output(id, OutputField::Latency));
                        }
                    }
                    Err(error) => app.status.show_error(error),
                }
            }
        }
        OutputMessage::ToggleOutputEnabled(id) => {
            if let Some(active) = app
                .project()
                .track(id)
                .filter(|t| t.capabilities().destination)
                .map(|t| !t.mixer.active)
            {
                run_edit(
                    app,
                    Command::Edit(ProjectEdit::SetActive {
                        strips: vec![id],
                        active,
                    }),
                );
            }
        }
    }
    Task::none()
}

/// Commit a Track Delay field: Track, group track and Main share one parse and one
/// "untouched display" rule, so a two-decimal display never overwrites the precise value.
fn submit_delay(
    app: &mut App,
    key: DraftKey,
    input: &str,
    current: Option<f64>,
    command: impl FnOnce(f64) -> Command,
) {
    match TRACK_DELAY.parse(input) {
        Ok(_)
            if !app.drafts.values.contains_key(&key)
                && current.is_some_and(|c| TRACK_DELAY.is_untouched_display(input, c)) => {}
        Ok(delay_ms) => {
            if run_edit(app, command(delay_ms)) {
                app.drafts.forget(key);
            }
        }
        Err(error) => app.status.show_error(error),
    }
}

/// Set only the destination component; the engine rejects an unknown output.
fn set_destination(app: &mut App, output: TrackId, destination: Option<Output>) -> bool {
    run_edit(
        app,
        Command::Edit(ProjectEdit::SetDestination {
            output,
            destination,
        }),
    )
}

/// Insert an output after the last one, taking the next palette colour so columns differ.
fn add_output(app: &mut App) {
    let count = app.project().outputs.len();
    execute_edit(
        app,
        Command::Edit(ProjectEdit::AddOutput {
            name: format!("Output {}", count + 1),
            host: "127.0.0.1".into(),
            port: 9000,
        }),
    );
}

/// Commit a destination edit. An optional destination with no host yet cannot go to the engine
/// (a destination needs a host), so the edit waits in `dormant_destinations` until one is typed.
fn store_destination(app: &mut App, id: TrackId, output: Output) -> bool {
    let unset = app
        .project()
        .track(id)
        .is_some_and(|t| t.destination().is_none());
    if unset && output.destination.host.trim().is_empty() {
        app.dormant_destinations.insert(id, output);
        return true;
    }
    let committed = set_destination(app, id, Some(output));
    if committed {
        app.dormant_destinations.remove(&id);
    }
    committed
}

/// The destination component to edit: its own, else the dormant one kept when its host was
/// cleared, else a blank one.
fn find_output(app: &App, id: TrackId) -> Option<Output> {
    let track = app
        .project()
        .track(id)
        .filter(|t| t.capabilities().destination)?;
    Some(
        track
            .output
            .clone()
            .or_else(|| app.dormant_destinations.get(&id).cloned())
            .unwrap_or(Output {
                destination: Destination {
                    host: String::new(),
                    port: 9000,
                    mode: UdpMode::Unicast,
                },
                stage: OutputStage::default(),
            }),
    )
}

/// The field an output message drafts into, for the pending-field path.
pub(crate) fn changed(message: &OutputMessage) -> Option<DraftKey> {
    match message {
        OutputMessage::StageDraft(key, _) => Some(*key),
        OutputMessage::TrackDelayChanged(id, _) => Some(DraftKey::Track(*id, TrackField::Delay)),
        OutputMessage::OutputHostChanged(id, _) => Some(DraftKey::Output(*id, OutputField::Host)),
        OutputMessage::OutputPortChanged(id, _) => Some(DraftKey::Output(*id, OutputField::Port)),
        OutputMessage::OutputLatencyChanged(id, _) => {
            Some(DraftKey::Output(*id, OutputField::Latency))
        }
        _ => None,
    }
}

/// The field an output message submits.
pub(crate) fn submitted(message: &OutputMessage) -> Option<DraftKey> {
    match message {
        OutputMessage::StageSubmit(key) => Some(*key),
        OutputMessage::TrackDelaySubmitted(id, _) => Some(DraftKey::Track(*id, TrackField::Delay)),
        OutputMessage::OutputHostSubmitted(id, _) => Some(DraftKey::Output(*id, OutputField::Host)),
        OutputMessage::OutputPortSubmitted(id, _) => Some(DraftKey::Output(*id, OutputField::Port)),
        OutputMessage::OutputLatencySubmitted(id, _) => {
            Some(DraftKey::Output(*id, OutputField::Latency))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtable_engine::{engine::Engine, model::Project};

    fn app() -> App {
        App::new(
            Engine::new(Project::default()).unwrap(),
            String::new(),
            None,
        )
    }

    #[test]
    fn each_output_setting_is_one_edit_and_preserves_other_track_state() {
        let mut app = app();
        add_output(&mut app);
        let id = app.project().outputs[0].id;
        let mut expected = app.project().clone();
        let cases = [
            OutputMessage::OutputHostSubmitted(id, "localhost".into()),
            OutputMessage::OutputPortSubmitted(id, "9100".into()),
            OutputMessage::ToggleOutputMode(id),
            OutputMessage::OutputLatencySubmitted(id, "12.3456".into()),
            OutputMessage::ToggleOutputEnabled(id),
        ];
        for message in cases {
            match &message {
                OutputMessage::OutputHostSubmitted(_, host) => {
                    expected[id].destination_mut().unwrap().host = host.clone()
                }
                OutputMessage::OutputPortSubmitted(..) => {
                    expected[id].destination_mut().unwrap().port = 9100
                }
                OutputMessage::ToggleOutputMode(_) => {
                    expected[id].destination_mut().unwrap().mode = UdpMode::Broadcast
                }
                OutputMessage::OutputLatencySubmitted(..) => expected[id].delay_ms = 12.3456,
                OutputMessage::ToggleOutputEnabled(_) => expected[id].mixer.active = false,
                _ => unreachable!(),
            }
            let revision = app.project_revision();
            let _ = update(&mut app, message);
            assert_eq!(app.project_revision(), revision + 1);
            assert_eq!(app.project(), &expected);
        }
    }

    #[test]
    fn invalid_output_edits_keep_the_project_and_draft_and_untouched_latency_keeps_precision() {
        let mut app = app();
        add_output(&mut app);
        let id = app.project().outputs[0].id;
        let _ = update(
            &mut app,
            OutputMessage::OutputLatencySubmitted(id, "12.345678".into()),
        );
        let before = app.project().clone();
        let revision = app.project_revision();
        let _ = update(
            &mut app,
            OutputMessage::OutputLatencySubmitted(id, "12.3457".into()),
        );
        assert_eq!(app.project_revision(), revision);
        assert_eq!(app.project(), &before);
        for (key, changed, submitted) in [
            (
                DraftKey::Output(id, OutputField::Host),
                OutputMessage::OutputHostChanged(id, String::new()),
                OutputMessage::OutputHostSubmitted(id, String::new()),
            ),
            (
                DraftKey::Output(id, OutputField::Port),
                OutputMessage::OutputPortChanged(id, "0".into()),
                OutputMessage::OutputPortSubmitted(id, "0".into()),
            ),
            (
                DraftKey::Output(id, OutputField::Latency),
                OutputMessage::OutputLatencyChanged(id, "1001".into()),
                OutputMessage::OutputLatencySubmitted(id, "1001".into()),
            ),
        ] {
            let _ = update(&mut app, changed);
            let draft = app.drafts.values[&key].clone();
            let _ = update(&mut app, submitted);
            assert_eq!(app.project_revision(), revision);
            assert_eq!(app.project(), &before);
            assert_eq!(app.drafts.values[&key], draft);
            assert!(app.status.message().is_some());
        }
    }

    #[test]
    fn main_host_restoration_keeps_dormant_stage_but_preserves_new_delay_and_volume() {
        let mut app = app();
        let id = app.project().main.id;
        let _ = update(&mut app, OutputMessage::ToggleBundle(id));
        let stage = app.project()[id].output.as_ref().unwrap().stage.clone();
        let revision = app.project_revision();
        let _ = update(
            &mut app,
            OutputMessage::OutputHostSubmitted(id, String::new()),
        );
        assert_eq!(app.project_revision(), revision + 1);
        assert!(app.project()[id].output.is_none());
        let _ = update(
            &mut app,
            OutputMessage::TrackDelaySubmitted(id, "-5".into()),
        );
        app.engine
            .execute(Command::Edit(ProjectEdit::set_value(
                qtable_engine::model::ParameterRef::volume(id),
                0.4,
            )))
            .unwrap();
        let revision = app.project_revision();
        let _ = update(
            &mut app,
            OutputMessage::OutputPortSubmitted(id, "9101".into()),
        );
        let _ = update(&mut app, OutputMessage::ToggleOutputMode(id));
        assert_eq!(app.project_revision(), revision);
        let _ = update(
            &mut app,
            OutputMessage::OutputHostSubmitted(id, "127.0.0.1".into()),
        );
        assert_eq!(app.project_revision(), revision + 1);
        let main = &app.project()[id];
        assert_eq!(main.delay_ms, -5.0);
        assert_eq!(main.mixer_device.volume.value, 0.4);
        assert_eq!(main.output.as_ref().unwrap().stage, stage);
        assert_eq!(main.destination().unwrap().port, 9101);
        assert_eq!(main.destination().unwrap().mode, UdpMode::Broadcast);
        assert!(app.dormant_destinations.is_empty());
    }
}
