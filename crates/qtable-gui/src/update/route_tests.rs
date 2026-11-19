//! Routes-tab drafts and route toggling through the public update router.
use super::update;
use crate::app::App;
use crate::drafts::DraftKey;
use crate::drafts::OutputField;
use crate::drafts::TrackField;
use crate::grid;
use crate::messages::{EditMessage, Message, OutputMessage};
use crate::output_band::RouteChoice;
use qtable_engine::engine::Engine;
use qtable_engine::model::Project;

fn send(app: &mut App, message: OutputMessage) {
    let _ = update(app, Message::Outputs(message));
}

fn send_edit(app: &mut App, message: EditMessage) {
    let _ = update(app, Message::Edit(message));
}

fn app_with_routing() -> App {
    let mut app = App::new(
        Engine::new(Project::default()).unwrap(),
        String::new(),
        None,
    );
    let _ = update(&mut app, Message::Edit(EditMessage::AddTrack));
    send(&mut app, OutputMessage::AddOutput);
    app
}

#[test]
fn multi_track_choice_is_one_revision_and_restores_with_one_inverse_edit() {
    use qtable_engine::{edit::ProjectEdit, engine::Command, model::Route};
    let mut app = app_with_routing();
    for _ in 0..2 {
        send_edit(&mut app, EditMessage::AddTrack);
    }
    let tracks: Vec<_> = app.project().tracks.iter().map(|t| t.id).collect();
    let main = app.project().main.id;
    let output = app.project().outputs[0].id;
    let originals = vec![
        (tracks[0], Route::empty()),
        (tracks[1], Route::single(main)),
        (tracks[2], Route::single(output)),
    ];
    app.engine
        .execute(Command::Edit(ProjectEdit::SetTrackRoutes {
            routes: originals.clone(),
        }))
        .unwrap();
    for (index, track) in tracks.iter().enumerate() {
        let _ = update(
            &mut app,
            Message::Grid(grid::Action::SelectTrack(*track, index > 0)),
        );
    }
    let before = app.project().clone();
    let revision = app.project_revision();
    send(
        &mut app,
        OutputMessage::ChooseRoute(tracks[0], RouteChoice::Destination(output)),
    );
    assert_eq!(
        app.project_revision(),
        revision + 1,
        "one revision for the whole choice"
    );
    for track in tracks {
        assert!(app.project().resolved_route(track).contains(&output));
    }
    // There is no undo/history API yet. One inverse batch restores this edit's full scope.
    app.engine
        .execute(Command::Edit(ProjectEdit::SetTrackRoutes {
            routes: originals,
        }))
        .unwrap();
    assert_eq!(app.project_revision(), revision + 2);
    assert_eq!(app.project(), &before);
}

#[test]
fn group_choice_sets_every_selected_member_in_one_revision() {
    use qtable_engine::{edit::ProjectEdit, engine::Command, model::Route};
    let mut app = app_with_routing();
    send_edit(&mut app, EditMessage::AddTrack);
    let members: Vec<_> = app.project().tracks.iter().map(|t| t.id).collect();
    app.engine
        .execute(Command::Edit(ProjectEdit::CreateGroup {
            tracks: members.clone(),
        }))
        .unwrap();
    let output = app.project().outputs[0].id;
    app.engine
        .execute(Command::Edit(ProjectEdit::SetTrackRoutes {
            routes: vec![
                (members[0], Route::Group),
                (members[1], Route::single(output)),
            ],
        }))
        .unwrap();
    app.selection.tracks = members.clone();
    let revision = app.project_revision();
    send(
        &mut app,
        OutputMessage::ChooseRoute(members[0], RouteChoice::Group),
    );
    assert_eq!(app.project_revision(), revision + 1);
    for member in members {
        assert_eq!(app.project()[member].route, Route::Group);
    }
}

#[test]
fn output_fields_commit_previous_drafts_and_clear_the_last_on_enter() {
    let mut app = app_with_routing();
    let track = app.project().tracks[0].id;
    let output = app.project().outputs[0].id;
    for message in [
        OutputMessage::OutputHostChanged(output, "127.0.0.2".into()),
        OutputMessage::OutputPortChanged(output, "9001".into()),
        OutputMessage::TrackDelayChanged(track, "-2.5".into()),
    ] {
        send(&mut app, message);
    }
    assert_eq!(app.drafts.values.len(), 1);
    assert_eq!(
        app.project().outputs[0].destination().unwrap().host,
        "127.0.0.2"
    );
    assert_eq!(app.project().outputs[0].destination().unwrap().port, 9001);
    for message in [
        OutputMessage::OutputHostSubmitted(output, "127.0.0.2".into()),
        OutputMessage::OutputPortSubmitted(output, "9001".into()),
        OutputMessage::TrackDelaySubmitted(track, "-2.5".into()),
    ] {
        send(&mut app, message);
        assert!(app.status.message().is_none());
    }
    assert_eq!(
        app.project().outputs[0].destination().unwrap().host,
        "127.0.0.2"
    );
    assert_eq!(app.project().outputs[0].destination().unwrap().port, 9001);
    assert_eq!(app.project().tracks[0].delay_ms, -2.5);
    assert!(app.drafts.values.is_empty());
    // A rejected value keeps its draft for correction.
    send(
        &mut app,
        OutputMessage::OutputPortChanged(output, "invalid".into()),
    );
    send(
        &mut app,
        OutputMessage::OutputPortSubmitted(output, "invalid".into()),
    );
    assert_eq!(app.project().outputs[0].destination().unwrap().port, 9001);
    assert_eq!(
        app.drafts
            .values
            .get(&DraftKey::Output(output, OutputField::Port))
            .unwrap(),
        "invalid"
    );
    assert!(app.status.message().is_some());
}

#[test]
fn submitting_the_shown_value_of_an_untouched_field_changes_nothing() {
    let mut app = app_with_routing();
    let output = app.project().outputs[0].id;
    send_edit(
        &mut app,
        EditMessage::PathSubmitted(output, "/synth".into()),
    );
    assert_eq!(app.project().outputs[0].path.as_str(), "/synth");
    // The view sends what the field shows; Enter on it re-submits that value.
    send_edit(
        &mut app,
        EditMessage::PathSubmitted(output, "/synth".into()),
    );
    assert_eq!(app.project().outputs[0].path.as_str(), "/synth");
    send(
        &mut app,
        OutputMessage::OutputPortSubmitted(output, "9000".into()),
    );
    assert!(app.status.message().is_none());
    assert_eq!(app.project().outputs[0].destination().unwrap().port, 9000);
}

#[test]
fn removed_entities_leave_no_draft_for_a_reused_id() {
    let mut app = app_with_routing();
    send(&mut app, OutputMessage::AddOutput);
    let second = app.project().outputs[1].id;
    send(
        &mut app,
        OutputMessage::OutputHostChanged(second, "10.0.0.99".into()),
    );
    send_edit(&mut app, EditMessage::DeleteTrack(second));
    send(&mut app, OutputMessage::AddOutput);
    let again = app.project().outputs[1].id;
    assert_eq!(again, second);
    assert!(
        !app.drafts
            .values
            .contains_key(&DraftKey::Output(again, OutputField::Host))
    );
    let output = app.project().outputs[0].id;
    send_edit(&mut app, EditMessage::PathChanged(output, "/x".into()));
    send_edit(&mut app, EditMessage::DeleteTrack(output));
    assert!(app.drafts.values.is_empty());
}

#[test]
fn group_toggles_preserve_fan_out_and_member_choices_replace_inheritance() {
    let mut app = App::new(
        Engine::new(Project::default()).unwrap(),
        String::new(),
        None,
    );
    for _ in 0..2 {
        let _ = update(&mut app, Message::Edit(EditMessage::AddTrack));
    }
    send(&mut app, OutputMessage::AddOutput);
    send(&mut app, OutputMessage::AddOutput);
    let first = app.project().tracks[0].id;
    let second = app.project().tracks[1].id;
    let outputs: Vec<_> = app.project().outputs.iter().map(|t| t.id).collect();
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(first, false)),
    );
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(second, true)),
    );
    send(&mut app, OutputMessage::CreateGroupAroundSelected);
    let group = app.project().group_tracks().next().unwrap().id;
    let routed: Vec<_> = std::iter::once(app.project().main.id)
        .chain(outputs.iter().copied())
        .collect();
    for output in &outputs {
        send(
            &mut app,
            OutputMessage::ChooseRoute(group, RouteChoice::Destination(*output)),
        );
    }
    assert_eq!(
        app.project().group_tracks().next().unwrap().route,
        qtable_engine::model::Route::Outputs(routed.clone())
    );
    for track in [first, second] {
        crate::edit_actions::execute_edit(
            &mut app,
            qtable_engine::engine::Command::Edit(
                qtable_engine::edit::ProjectEdit::SetTrackRoutes {
                    routes: vec![(track, qtable_engine::model::Route::Group)],
                },
            ),
        );
    }
    // Selecting just one member makes it the only target.
    let _ = update(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(first, false)),
    );
    let owner = first;
    send(
        &mut app,
        OutputMessage::ChooseRoute(owner, RouteChoice::Destination(outputs[0])),
    );
    assert_eq!(app.project().resolved_route(first), [outputs[0]]);
    assert_eq!(app.project().resolved_route(second), routed);
    send(
        &mut app,
        OutputMessage::ChooseRoute(owner, RouteChoice::Destination(outputs[0])),
    );
    assert!(app.project().resolved_route(first).is_empty());
    app.drafts
        .values
        .insert(DraftKey::Track(group, TrackField::Name), "Rhythm".into());
    crate::edit_actions::submit_name(
        &mut app,
        DraftKey::Track(group, TrackField::Name),
        qtable_engine::edit::ProjectEdit::RenameTrack {
            track: group,
            name: "Rhythm".into(),
        },
    );
    assert_eq!(app.project().group_tracks().next().unwrap().name, "Rhythm");
    assert!(
        !app.drafts
            .values
            .contains_key(&DraftKey::Track(group, TrackField::Name))
    );
    app.drafts
        .values
        .insert(DraftKey::Track(group, TrackField::Name), "Half".into());
    send(&mut app, OutputMessage::Ungroup(group));
    assert!(
        !app.drafts
            .values
            .contains_key(&DraftKey::Track(group, TrackField::Name))
    );
}
