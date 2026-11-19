//! Output column controls through the public update router.
use super::update;
use crate::app::App;
use crate::drafts::DraftKey;
use crate::drafts::OutputField;
use crate::drafts::TrackField;
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

/// Selects the output's column, as a click on its header does.
fn select(app: &mut App, id: qtable_engine::model::TrackId) {
    let _ = update(
        app,
        Message::Grid(crate::grid::Action::SelectColumn(
            crate::selection::ColumnOwner::Track(id),
            None,
        )),
    );
}

/// Renames through the column header's editor: start renaming, then commit.
fn rename(app: &mut App, id: qtable_engine::model::TrackId, name: &str) {
    select(app, id);
    app.header_actions.renaming = Some(crate::grid::RenameTarget::Header(id));
    crate::header_actions::commit_rename(app, name.into());
}

fn app_with_outputs(count: usize) -> App {
    let mut app = App::new(
        Engine::new(Project::default()).unwrap(),
        String::new(),
        None,
    );
    for _ in 0..count {
        send(&mut app, OutputMessage::AddOutput);
    }
    app
}

#[test]
fn inserted_outputs_take_successive_colours() {
    let app = app_with_outputs(2);
    let outputs = &app.project().outputs;
    assert_eq!(outputs.len(), 2);
    assert_ne!(outputs[0].colour, outputs[1].colour);
    assert_eq!(outputs[0].name, "Output 1");
    assert_eq!(outputs[1].name, "Output 2");
}

#[test]
fn enable_toggle_and_latency_edit_the_output() {
    let mut app = app_with_outputs(1);
    let id = app.project().outputs[0].id;
    send(&mut app, OutputMessage::ToggleOutputEnabled(id));
    assert!(!app.project().outputs[0].mixer.active);
    send(
        &mut app,
        OutputMessage::OutputLatencyChanged(id, "12.5".into()),
    );
    send(
        &mut app,
        OutputMessage::OutputLatencySubmitted(id, "12.5".into()),
    );
    assert_eq!(app.project().outputs[0].delay_ms, 12.5);
    assert!(
        !app.drafts
            .values
            .contains_key(&DraftKey::Output(id, OutputField::Latency))
    );
}

#[test]
fn rejected_latency_keeps_its_draft_and_the_model() {
    let mut app = app_with_outputs(1);
    let id = app.project().outputs[0].id;
    send(
        &mut app,
        OutputMessage::OutputLatencyChanged(id, "soon".into()),
    );
    send(
        &mut app,
        OutputMessage::OutputLatencySubmitted(id, "soon".into()),
    );
    assert_eq!(app.project().outputs[0].delay_ms, 0.0);
    assert!(
        app.drafts
            .values
            .contains_key(&DraftKey::Output(id, OutputField::Latency))
    );
}

#[test]
fn name_submit_renames_and_clicking_the_title_selects_without_recolouring() {
    let mut app = app_with_outputs(1);
    let id = app.project().outputs[0].id;
    let before = app.project().outputs[0].colour;
    rename(&mut app, id, "Lights");
    assert_eq!(app.project().outputs[0].name, "Lights");
    assert_eq!(app.project().outputs[0].colour, before);
    assert_eq!(app.selection.anchor, Some(id));
}

#[test]
fn context_menu_opens_for_a_column_and_closes_after_an_action() {
    let mut app = app_with_outputs(1);
    let id = app.project().outputs[0].id;
    let _ = update(&mut app, Message::Grid(crate::grid::Action::HeaderMenu(id)));
    assert_eq!(
        app.menu_target(),
        Some(crate::context_menu::MenuTarget::Header(id))
    );
    send_edit(&mut app, EditMessage::DeleteTrack(id));
    assert_eq!(app.menu_target(), None);
    assert!(app.project().outputs.is_empty());
    let _ = update(
        &mut app,
        Message::Grid(crate::grid::Action::RegionMenu(
            qtable_engine::model::Region::Outputs,
        )),
    );
    let _ = update(&mut app, Message::CloseOverlay);
    assert_eq!(app.menu_target(), None);
}

#[test]
fn route_toggles_fan_out_and_materialise_an_inherited_route() {
    use qtable_engine::model::Route;
    let mut app = app_with_outputs(2);
    let [a, b] = [0, 1].map(|i| app.project().outputs[i].id);
    let _ = update(
        &mut app,
        Message::Edit(crate::messages::EditMessage::AddTrack),
    );
    let track = app.project().tracks[0].id;
    let owner = track;
    send(
        &mut app,
        OutputMessage::ChooseRoute(owner, RouteChoice::Destination(a)),
    );
    send(
        &mut app,
        OutputMessage::ChooseRoute(owner, RouteChoice::Destination(b)),
    );
    assert_eq!(
        app.project().resolved_route(track),
        [app.project().main.id, a, b]
    );
    send(
        &mut app,
        OutputMessage::ChooseRoute(owner, RouteChoice::Destination(a)),
    );
    assert_eq!(
        app.project().resolved_route(track),
        [app.project().main.id, b]
    );
    assert_eq!(
        app.project().tracks[0].route,
        Route::Outputs(vec![app.project().main.id, b])
    );
}

#[test]
fn rename_preserves_output_identity_and_new_track_defaults() {
    let mut app = app_with_outputs(1);
    let id = app.project().outputs[0].id;
    let before = app.project().outputs[0].clone();
    rename(&mut app, id, "Front Lights");
    rename(&mut app, id, "Back Lights");
    let mut expected = before;
    expected.name = "Back Lights".into();
    assert_eq!(app.project().outputs[0], expected);
    let _ = update(
        &mut app,
        Message::Edit(crate::messages::EditMessage::AddTrack),
    );
    let existing = app.project().tracks[0].id;
    let before = app.project().resolved_route(existing).to_vec();
    let _ = update(
        &mut app,
        Message::Edit(crate::messages::EditMessage::AddTrack),
    );
    let added = app.project().tracks.last().unwrap().id;
    assert_eq!(app.project().resolved_route(added), [app.project().main.id]);
    assert_eq!(app.project().resolved_route(existing), before);
}

#[test]
fn main_destination_fields_configure_and_clear_an_unset_main() {
    let mut project = Project::default();
    project.main.output = None;
    let mut app = App::new(Engine::new(project).unwrap(), String::new(), None);
    let id = app.project().main.id;
    send(
        &mut app,
        OutputMessage::OutputHostSubmitted(id, "127.0.0.2".into()),
    );
    assert_eq!(app.project().main.destination().unwrap().host, "127.0.0.2");
    send(
        &mut app,
        OutputMessage::OutputPortSubmitted(id, "9010".into()),
    );
    send_edit(&mut app, EditMessage::PathSubmitted(id, "/show".into()));
    send(&mut app, OutputMessage::ToggleOutputMode(id));
    let destination = app.project().main.destination().unwrap();
    assert_eq!(destination.port, 9010);
    assert_eq!(app.project().main.path.as_str(), "/show");
    assert_eq!(destination.mode, qtable_engine::model::UdpMode::Broadcast);
    assert!(app.project().main.mixer.active && !app.project().main.mixer.solo);
    send(
        &mut app,
        OutputMessage::OutputHostSubmitted(id, String::new()),
    );
    assert!(app.project().main.output.is_none());
}

#[test]
fn port_then_host_on_an_unset_main_keeps_the_port() {
    let mut project = Project::default();
    project.main.output = None;
    let mut app = App::new(Engine::new(project).unwrap(), String::new(), None);
    let id = app.project().main.id;
    send(
        &mut app,
        OutputMessage::OutputPortChanged(id, "9100".into()),
    );
    send(
        &mut app,
        OutputMessage::OutputPortSubmitted(id, "9100".into()),
    );
    send_edit(&mut app, EditMessage::PathSubmitted(id, "/show".into()));
    send(&mut app, OutputMessage::ToggleOutputMode(id));
    assert!(app.project().main.output.is_none());
    assert!(app.drafts.errors.is_empty() && app.status.message().is_none());
    send(
        &mut app,
        OutputMessage::OutputHostSubmitted(id, "10.0.0.5".into()),
    );
    let destination = app.project().main.destination().unwrap();
    assert_eq!(destination.host, "10.0.0.5");
    assert_eq!(destination.port, 9100);
    assert_eq!(app.project().main.path.as_str(), "/show");
    assert_eq!(destination.mode, qtable_engine::model::UdpMode::Broadcast);
    assert!(app.dormant_destinations.is_empty());
}

#[test]
fn clearing_main_host_keeps_its_other_settings_dormant() {
    let mut app = App::new(
        Engine::new(Project::default()).unwrap(),
        String::new(),
        None,
    );
    let id = app.project().main.id;
    send(
        &mut app,
        OutputMessage::OutputPortSubmitted(id, "9200".into()),
    );
    send_edit(&mut app, EditMessage::PathSubmitted(id, "/x".into()));
    send(
        &mut app,
        OutputMessage::OutputHostSubmitted(id, String::new()),
    );
    assert!(app.project().main.output.is_none());
    send(
        &mut app,
        OutputMessage::OutputHostSubmitted(id, "host2".into()),
    );
    let destination = app.project().main.destination().unwrap();
    assert_eq!(destination.port, 9200);
    assert_eq!(app.project().main.path.as_str(), "/x");
    // An output keeps a required host: clearing it is refused, not unset.
    let mut app = app_with_outputs(1);
    let out = app.project().outputs[0].id;
    send(
        &mut app,
        OutputMessage::OutputHostSubmitted(out, String::new()),
    );
    assert!(app.project().outputs[0].destination().is_some());
}

#[test]
fn path_commits_for_every_kind_and_escape_reverts_a_draft() {
    use qtable_engine::model::{Track, TrackId};
    let mut p = crate::files::new_project();
    let member = p.tracks[0].id;
    p.insert_group(
        Track::new_group(TrackId(910), "G", p.scenes.len()),
        &[member],
    );
    let mut app = App::new(Engine::new(p).unwrap(), String::new(), None);
    send(&mut app, OutputMessage::AddOutput);
    let out = app.project().outputs[0].id;
    let main = app.project().main.id;
    for id in [out, main, member, TrackId(910)] {
        send_edit(&mut app, EditMessage::PathChanged(id, "/p".into()));
        send_edit(&mut app, EditMessage::PathSubmitted(id, "/p".into()));
        assert_eq!(app.project().track(id).unwrap().path.as_str(), "/p");
        assert!(
            !app.drafts
                .values
                .contains_key(&DraftKey::Track(id, TrackField::Path))
        );
    }
    let key = DraftKey::Track(main, TrackField::Path);
    send_edit(&mut app, EditMessage::PathChanged(main, "/draft".into()));
    assert_eq!(app.pending_field, Some(key));
    crate::field_focus::cancel(&mut app);
    assert!(!app.drafts.values.contains_key(&key));
    assert_eq!(app.project().main.path.as_str(), "/p");
    // A rejected draft keeps its text and the error look.
    send_edit(&mut app, EditMessage::PathChanged(main, "/bad/".into()));
    send_edit(&mut app, EditMessage::PathSubmitted(main, "/bad/".into()));
    assert_eq!(app.drafts.values[&key], "/bad/");
    assert!(app.drafts.errors.contains_key(&key));
    assert_eq!(app.project().main.path.as_str(), "/p");
}

#[test]
fn submitting_an_empty_path_clears_it_and_forgets_the_draft() {
    let mut app = app_with_outputs(1);
    let out = app.project().outputs[0].id;
    let key = DraftKey::Track(out, TrackField::Path);
    send_edit(&mut app, EditMessage::PathSubmitted(out, "/p".into()));
    send_edit(&mut app, EditMessage::PathChanged(out, String::new()));
    send_edit(&mut app, EditMessage::PathSubmitted(out, String::new()));
    assert!(app.project().track(out).unwrap().path.is_empty());
    assert!(!app.drafts.values.contains_key(&key));
    assert!(!app.drafts.errors.contains_key(&key));
}
