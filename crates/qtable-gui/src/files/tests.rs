//! Project replacement must not carry drafts or selection across reused IDs.
use crate::app::App;
use crate::drafts::OutputField;
use crate::drafts::TrackField;
use crate::drafts::{ClipField, DraftKey};
use crate::fields::format_tempo;
use crate::files::{Intent, SaveStamp};
use crate::messages::{EditMessage, FileMessage, Message, OutputMessage};
use crate::{grid, inspector_panel, update::update};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::Project;
use qtable_engine::model::{SceneId, TrackId};
use rfd::MessageDialogResult;
use std::path::PathBuf;

fn app_with_stale_ui() -> App {
    let project = Project::default();
    let engine = Engine::new(project.clone()).expect("engine starts");
    let mut app = App::new(engine, "unfinished tempo".into(), Some("old.ron".into()));
    // An accepted edit leaves the project unsaved, as the title and prompts expect.
    app.engine
        .execute(Command::SetTempo(100.0))
        .expect("tempo edit");
    assert!(app.is_dirty());
    app.status.show_error("old error");
    app.drafts.values.insert(
        DraftKey::Track(TrackId(1), TrackField::Delay),
        "unfinished delay".into(),
    );
    app.drafts.values.insert(
        DraftKey::Output(TrackId(201), OutputField::Host),
        "old host".into(),
    );
    app.drafts.values.insert(
        DraftKey::Output(TrackId(201), OutputField::Port),
        "old port".into(),
    );
    app.drafts.values.insert(
        DraftKey::Track(TrackId(201), TrackField::Path),
        "old prefix".into(),
    );
    app.drafts.values.insert(
        DraftKey::Track(TrackId(101), TrackField::Name),
        "old group".into(),
    );
    app.drafts.values.insert(
        DraftKey::clip(TrackId(1), 0, ClipField::Length),
        "old value".into(),
    );
    app.drafts.errors.insert(
        DraftKey::clip(TrackId(1), 0, ClipField::Length),
        "old error".into(),
    );
    app.selection.clip = Some(grid::Selection {
        track: TrackId(1),
        slot: 0,
    });
    app.selection.scene = Some(SceneId(1));
    app.selection.tracks = vec![TrackId(1)];
    app.selection.anchor = Some(TrackId(1));
    app.selection.move_clip = true;
    app.selection.inspector_tab = inspector_panel::Tab::Track;
    app
}

fn assert_ui_reset(app: &App) {
    assert!(app.drafts.values.is_empty());
    assert!(app.drafts.errors.is_empty());
    assert!(app.selection.clip.is_none());
    assert!(app.selection.scene.is_none());
    assert!(app.selection.tracks.is_empty());
    assert!(app.selection.anchor.is_none());
    assert!(!app.selection.move_clip);
    assert_eq!(app.selection.inspector_tab, inspector_panel::Tab::Clip);
    assert!(!app.is_dirty());
    assert!(app.status.message().is_none());
    assert_eq!(
        app.tempo_input,
        format_tempo(app.engine.project().tempo.bpm())
    );
    assert_eq!(app.project(), app.engine.project());
    assert_eq!(app.snapshot.tempo_bpm, app.project().tempo.bpm());
}

fn assert_stale_ui_preserved(app: &App) {
    assert_eq!(
        app.drafts
            .values
            .get(&DraftKey::Output(TrackId(201), OutputField::Host))
            .unwrap(),
        "old host"
    );
    assert_eq!(
        app.drafts
            .values
            .get(&DraftKey::clip(TrackId(1), 0, ClipField::Length))
            .unwrap(),
        "old value"
    );
    assert_eq!(app.selection.clip.unwrap().track, TrackId(1));
    assert_eq!(app.selection.scene, Some(SceneId(1)));
    assert_eq!(app.selection.tracks, vec![TrackId(1)]);
    assert_eq!(app.selection.anchor, Some(TrackId(1)));
    assert!(app.selection.move_clip);
    assert_eq!(app.selection.inspector_tab, inspector_panel::Tab::Track);
    assert_eq!(app.tempo_input, "unfinished tempo");
    assert!(app.is_dirty());
    assert_eq!(app.path, Some(PathBuf::from("old.ron")));
}

#[test]
fn open_resets_previous_project_ui_and_refreshes_model() {
    let mut app = app_with_stale_ui();
    // Populate a replacement through the update loop so its IDs restart at 1.
    let mut replacement = App::new(
        Engine::new(Project::default()).expect("engine starts"),
        String::new(),
        None,
    );
    let _ = update(&mut replacement, Message::Edit(EditMessage::AddTrack));
    let _ = update(&mut replacement, Message::Edit(EditMessage::AddScene));
    let _ = update(&mut replacement, Message::Outputs(OutputMessage::AddOutput));
    let _ = update(&mut replacement, Message::Outputs(OutputMessage::AddOutput));
    let _ = update(&mut replacement, Message::TempoChanged("93.5".into()));
    let _ = update(&mut replacement, Message::TempoSubmitted);
    let project = replacement.project().clone();
    let expected = project.clone();
    let path = PathBuf::from("new.ron");

    let _ = update(
        &mut app,
        Message::Files(FileMessage::ProjectLoaded(Ok((
            path.clone(),
            Box::new(project),
        )))),
    );

    assert_ui_reset(&app);
    assert_eq!(app.project(), &expected);
    assert_eq!(app.path, Some(path));
}

#[test]
fn new_after_discard_resets_previous_project_ui() {
    let mut app = app_with_stale_ui();
    let _ = update(
        &mut app,
        Message::Files(FileMessage::DiscardChoice(
            Intent::New,
            MessageDialogResult::No,
        )),
    );

    assert_ui_reset(&app);
    assert_eq!(app.project(), &super::new_project());
    assert!(app.path.is_none());
}

#[test]
fn failed_open_preserves_previous_project_ui() {
    let mut app = app_with_stale_ui();
    let old_project = app.project().clone();
    let _ = update(
        &mut app,
        Message::Files(FileMessage::ProjectLoaded(Err("read failed".into()))),
    );

    assert_stale_ui_preserved(&app);
    assert_eq!(app.project(), &old_project);
    assert_eq!(app.status.message(), Some("read failed"));

    let mut invalid = Project::default();
    // Duplicate IDs are representable in the model but rejected by the engine.
    invalid.scenes.push(qtable_engine::model::Scene {
        id: SceneId(1),
        name: "One".into(),
        follow_actions: Default::default(),
    });
    invalid.scenes.push(invalid.scenes[0].clone());
    let _ = update(
        &mut app,
        Message::Files(FileMessage::ProjectLoaded(Ok((
            "invalid.ron".into(),
            Box::new(invalid),
        )))),
    );

    assert_stale_ui_preserved(&app);
    assert_eq!(app.project(), &old_project);
    assert!(app.status.message().is_some());
}

#[test]
fn cancelled_open_or_discard_preserves_previous_project_ui() {
    let mut app = app_with_stale_ui();
    let _ = update(&mut app, Message::Files(FileMessage::OpenPicked(None)));
    assert_stale_ui_preserved(&app);
    let _ = update(
        &mut app,
        Message::Files(FileMessage::DiscardChoice(
            Intent::New,
            MessageDialogResult::Cancel,
        )),
    );
    assert_stale_ui_preserved(&app);
}

#[test]
fn save_completion_preserves_edits_and_pending_intent_rechecks_dirty_state() {
    let mut app = app_with_stale_ui();
    let stamp = SaveStamp::capture(&app);
    let _ = update(&mut app, Message::Outputs(OutputMessage::AddOutput));
    let project = app.project().clone();
    let _ = update(
        &mut app,
        Message::Files(FileMessage::ProjectSaved(
            Ok("saved.ron".into()),
            stamp,
            Some(Intent::New),
        )),
    );
    assert!(app.is_dirty());
    assert_eq!(app.project(), &project);
    assert_eq!(app.path, Some("saved.ron".into()));

    let stamp = SaveStamp::capture(&app);
    let _ = update(
        &mut app,
        Message::Files(FileMessage::ProjectSaved(
            Ok("saved.ron".into()),
            stamp,
            None,
        )),
    );
    assert!(!app.is_dirty());
}

#[test]
fn old_project_save_completion_cannot_change_replacement_path_or_dirty_state() {
    let mut app = app_with_stale_ui();
    let stamp = SaveStamp::capture(&app);
    let _ = update(
        &mut app,
        Message::Files(FileMessage::ProjectLoaded(Ok((
            "replacement.ron".into(),
            Box::new(Project::default()),
        )))),
    );
    let _ = update(&mut app, Message::Edit(EditMessage::AddScene));
    let project = app.project().clone();
    for result in [Ok("old-save.ron".into()), Err("old write failure".into())] {
        let _ = update(
            &mut app,
            Message::Files(FileMessage::ProjectSaved(result, stamp, Some(Intent::New))),
        );
        assert_eq!(app.path, Some("replacement.ron".into()));
        assert!(app.is_dirty());
        assert_eq!(app.project(), &project);
        assert!(app.status.message().is_none());
    }
}

#[test]
fn completed_save_continues_pending_new_and_resets_the_saved_project_ui() {
    let mut app = app_with_stale_ui();
    let stamp = SaveStamp::capture(&app);
    let _ = update(
        &mut app,
        Message::Files(FileMessage::ProjectSaved(
            Ok("saved.ron".into()),
            stamp,
            Some(Intent::New),
        )),
    );
    assert_ui_reset(&app);
    assert_eq!(app.project(), &super::new_project());
    assert!(app.path.is_none());
}
#[test]
fn new_set_consumes_the_first_track_colour_before_later_creations() {
    use qtable_engine::edit::{ProjectEdit, apply};
    use qtable_engine::model::LIVE_CLIP_PALETTE;
    let mut project = super::new_project();
    assert_eq!(project.tracks[0].colour, LIVE_CLIP_PALETTE[0]);
    apply(
        &mut project,
        ProjectEdit::AddTrack {
            name: "Next".into(),
        },
    )
    .unwrap();
    assert_eq!(project.tracks[1].colour, LIVE_CLIP_PALETTE[1]);
}

#[test]
fn repeated_close_is_ignored_until_prompt_or_save_finishes() {
    let mut app = app_with_stale_ui();
    let id = iced::window::Id::unique();
    let close = || Message::Files(FileMessage::CloseRequested(id));
    assert!(update(&mut app, close()).units() > 0);
    assert!(app.file_intent_pending);
    assert_eq!(update(&mut app, close()).units(), 0);
    let _ = update(
        &mut app,
        Message::Files(FileMessage::DiscardChoice(
            Intent::Close(id),
            MessageDialogResult::Cancel,
        )),
    );
    assert!(!app.file_intent_pending);
    assert!(update(&mut app, close()).units() > 0);
    let _ = update(
        &mut app,
        Message::Files(FileMessage::DiscardChoice(
            Intent::Close(id),
            MessageDialogResult::Yes,
        )),
    );
    assert!(app.file_intent_pending);
    assert_eq!(update(&mut app, close()).units(), 0);
    let _ = update(
        &mut app,
        Message::Files(FileMessage::SavePicked(None, Some(Intent::Close(id)))),
    );
    assert!(!app.file_intent_pending);
    assert!(update(&mut app, close()).units() > 0);
    let stamp = SaveStamp::capture(&app);
    let _ = update(
        &mut app,
        Message::Files(FileMessage::ProjectSaved(
            Err("write failed".into()),
            stamp,
            Some(Intent::Close(id)),
        )),
    );
    assert!(!app.file_intent_pending);
}

#[test]
fn a_fresh_set_routes_its_first_track_to_the_localhost_main() {
    let p = super::new_project();
    p.validate().unwrap();
    assert_eq!(p.resolved_route(p.tracks[0].id), [p.main.id]);
    assert_eq!(p.main.destination().unwrap().host, "127.0.0.1");
    assert_eq!(p.main.destination().unwrap().port, 9000);
}
