//! Update-loop flows for the name fields and the drafts that the inspector keeps.
use super::update;
use crate::app::App;
use crate::drafts::OutputField;
use crate::drafts::TrackField;
use crate::drafts::{ClipField, DraftKey, SceneField};
use crate::edit_actions::execute_structural_edit;
use crate::files::SaveStamp;
use crate::inspector_update::execute_inspector_edit;
use crate::messages::{EditMessage, FileMessage, Message};
use crate::view::{clip_name_key, name_field_value};
use crate::{grid, inspector};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::Project;
use qtable_engine::model::{SceneId, TrackId};
use std::path::PathBuf;

fn app_with_two_tracks_and_clips() -> App {
    let project = Project::default();
    let engine = Engine::new(project.clone()).expect("engine starts");
    let mut app = App::new(engine, String::new(), None);
    send(&mut app, Message::Edit(EditMessage::AddTrack));
    send(&mut app, Message::Edit(EditMessage::AddTrack));
    send(&mut app, Message::Edit(EditMessage::AddScene));
    send(&mut app, Message::Edit(EditMessage::AddScene));
    for track in track_ids(&app) {
        select(&mut app, track, 0);
        send(&mut app, Message::Edit(EditMessage::CreateSelectedClip));
    }
    app
}

fn send(app: &mut App, message: Message) {
    let _ = update(app, message);
}

fn select(app: &mut App, track: TrackId, slot: usize) {
    send(app, Message::Grid(grid::Action::Select(track, slot)));
}

fn track_ids(app: &App) -> Vec<TrackId> {
    app.project().tracks.iter().map(|t| t.id).collect()
}

fn model_clip_name(app: &App, track: TrackId, slot: usize) -> String {
    app.project()
        .clip(track, slot)
        .expect("clip exists")
        .name
        .clone()
}

fn shown_clip_name(app: &App, track: TrackId, slot: usize) -> String {
    name_field_value(
        &app.drafts.values,
        &clip_name_key(track, slot),
        &model_clip_name(app, track, slot),
    )
}

#[test]
fn clip_name_commits_when_another_cell_is_selected() {
    let mut app = app_with_two_tracks_and_clips();
    let [first, second] = [track_ids(&app)[0], track_ids(&app)[1]];

    select(&mut app, first, 0);
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(
            DraftKey::clip(first, 0, ClipField::Name),
            "Intro".into(),
        )),
    );
    select(&mut app, second, 0);
    assert_eq!(
        shown_clip_name(&app, second, 0),
        model_clip_name(&app, second, 0)
    );
    assert!(app.drafts.values.is_empty());

    select(&mut app, first, 0);
    assert_eq!(
        shown_clip_name(&app, first, 0),
        model_clip_name(&app, first, 0)
    );
}

#[test]
fn clip_name_submit_renames_only_the_selected_clip() {
    let mut app = app_with_two_tracks_and_clips();
    let [first, second] = [track_ids(&app)[0], track_ids(&app)[1]];
    let second_name = model_clip_name(&app, second, 0);

    select(&mut app, first, 0);
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(
            DraftKey::clip(first, 0, ClipField::Name),
            "Intro".into(),
        )),
    );
    send(
        &mut app,
        Message::Inspector(inspector::Message::ClipName(first, 0, "Intro".into())),
    );

    assert_eq!(model_clip_name(&app, first, 0), "Intro");
    assert_eq!(model_clip_name(&app, second, 0), second_name);
}

#[test]
fn scene_name_commits_when_another_scene_is_selected() {
    let mut app = app_with_two_tracks_and_clips();
    let scenes: Vec<SceneId> = app.project().scenes.iter().map(|s| s.id).collect();
    let [first, second] = [scenes[0], scenes[1]];
    let key = inspector::scene_key(first, SceneField::Name);

    send(&mut app, Message::Grid(grid::Action::SelectScene(first)));
    send(
        &mut app,
        Message::Inspector(inspector::Message::SceneDraft(key, "Verse".into())),
    );
    assert!(app.drafts.values.contains_key(&key));

    send(&mut app, Message::Grid(grid::Action::SelectScene(second)));
    assert!(!app.drafts.values.contains_key(&key));
    assert_eq!(app.project().scenes[0].name, "Verse");
}

#[test]
fn removing_an_event_drops_event_drafts_on_that_clip() {
    let mut app = app_with_two_tracks_and_clips();
    let track = track_ids(&app)[0];

    select(&mut app, track, 0);
    for _ in 0..2 {
        inspector::event_editor::add_hit(&mut app, track, 0, qtable_engine::model::BeatTime::ZERO);
    }
    let key = inspector::field_key(
        track,
        0,
        ClipField::EventArgument {
            event: 1,
            argument: 0,
        },
    );
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(key, "2".into())),
    );

    send(
        &mut app,
        Message::Inspector(inspector::Message::RemoveEvent(track, 0, 0)),
    );

    assert_eq!(events_on(&app, track, 0), 1);
    assert!(app.drafts.values.is_empty());
}

#[test]
fn rejected_structural_edit_keeps_drafts() {
    let mut app = app_with_two_tracks_and_clips();
    let track = track_ids(&app)[0];

    select(&mut app, track, 0);
    let key = inspector::field_key(
        track,
        0,
        ClipField::EventArgument {
            event: 7,
            argument: 0,
        },
    );
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(key, "2".into())),
    );
    send(
        &mut app,
        Message::Inspector(inspector::Message::RemoveEvent(track, 0, 7)),
    );

    assert_eq!(app.drafts.values.get(&key).map(String::as_str), Some("2"));
    assert!(app.status.message().is_some());
}

fn events_on(app: &App, track: TrackId, slot: usize) -> usize {
    app.project()
        .clip(track, slot)
        .map(|c| c.events.len())
        .unwrap_or_default()
}

#[test]
fn musical_sixteenth_fractions_reach_clip_and_scene_fields() {
    use inspector::Message as Inspector;
    use qtable_engine::model::BeatTime;
    let mut app = app_with_two_tracks_and_clips();
    let track = track_ids(&app)[0];
    let scene = app.project().scenes[0].id;
    for message in [
        Inspector::Length(track, 0, "0.2.1+1/3".into()),
        Inspector::FollowTime(track, 0, "0.0.1+1/3".into()),
        Inspector::SceneTime(scene, "0.0.1+1/3".into()),
    ] {
        send(&mut app, Message::Inspector(message));
        assert!(app.drafts.errors.is_empty());
        assert!(app.status.message().is_none());
    }
    let clip = app.project().tracks[0].slots[0].clip.as_ref().unwrap();
    assert_eq!(clip.length(), BeatTime(2240));
    assert_eq!(clip.follow_actions.time, BeatTime(320));
    assert_eq!(app.project().scenes[0].follow_actions.time, BeatTime(320));
}

#[test]
fn successful_commands_and_save_clear_status_messages() {
    let mut app = app_with_two_tracks_and_clips();
    for message in [
        Message::Stop,
        Message::Edit(EditMessage::AddTrack),
        Message::TempoSubmitted,
    ] {
        app.status.show_error("previous failure");
        app.tempo_input = "120".into();
        send(&mut app, message);
        assert!(app.status.message().is_none());
    }
    app.status.show_error("previous failure");
    let stamp = SaveStamp::capture(&app);
    send(
        &mut app,
        Message::Files(FileMessage::ProjectSaved(
            Ok(PathBuf::from("saved.ron")),
            stamp,
            None,
        )),
    );
    assert!(app.status.message().is_none());
    app.status.show_error("previous failure");
    let track = track_ids(&app)[0];
    execute_inspector_edit(
        &mut app,
        DraftKey::Track(track, TrackField::Name),
        ProjectEdit::RenameTrack {
            track,
            name: "Renamed".into(),
        },
    );
    assert!(app.status.message().is_none());
}

#[test]
fn invalid_input_survives_typing_until_dismissed_or_successful() {
    let mut app = app_with_two_tracks_and_clips();
    send(&mut app, Message::TempoChanged("invalid".into()));
    send(&mut app, Message::TempoSubmitted);
    assert!(app.status.message().is_some());
    send(&mut app, Message::TempoChanged("120".into()));
    assert!(app.status.message().is_some());
    send(&mut app, Message::DismissStatus);
    assert!(app.status.message().is_none());
}

#[test]
fn project_reload_clears_persistent_scheduler_failure() {
    let mut app = app_with_two_tracks_and_clips();
    app.status.poll(
        &qtable_engine::status::EngineStatus {
            scheduler_failure: Some("thread exited".into()),
            ..Default::default()
        },
        std::time::Instant::now(),
    );
    send(&mut app, Message::Stop);
    assert_eq!(
        app.status.message(),
        Some("Scheduler stopped: thread exited")
    );
    app.mark_saved();
    send(&mut app, Message::Files(FileMessage::New));
    assert!(app.status.message().is_none());
}

#[test]
fn scene_reorder_updates_selection_only_after_acceptance() {
    let mut app = app_with_two_tracks_and_clips();
    let track = track_ids(&app)[0];
    select(&mut app, track, 0);
    let scene = app.project().scenes[0].id;
    send(&mut app, Message::Edit(EditMessage::ReorderScene(1)));
    assert_eq!(app.selection.clip.unwrap().slot, 1);
    assert_eq!(app.project().scenes[1].id, scene);

    app.drafts.values.insert(
        DraftKey::clip(track, 0, ClipField::Length),
        "unfinished".into(),
    );
    let revision = app.project_revision();
    assert!(!execute_structural_edit(
        &mut app,
        Command::Edit(ProjectEdit::MoveScene { from: 1, to: 99 }),
    ));
    assert_eq!(app.selection.clip.unwrap().slot, 1);
    assert_eq!(app.project_revision(), revision);
    assert_eq!(
        app.drafts
            .values
            .get(&DraftKey::clip(track, 0, ClipField::Length))
            .unwrap(),
        "unfinished"
    );
    assert!(app.status.message().is_some());
}

#[test]
fn submitted_tempo_refreshes_gui_model_and_rejected_input_preserves_it() {
    let mut app = app_with_two_tracks_and_clips();
    send(&mut app, Message::TempoChanged("93.5".into()));
    send(&mut app, Message::TempoSubmitted);
    assert_eq!(app.project().tempo.bpm(), 93.5);
    assert_eq!(app.project(), app.engine.project());
    send(&mut app, Message::TempoChanged("invalid".into()));
    send(&mut app, Message::TempoSubmitted);
    assert_eq!(app.project().tempo.bpm(), 93.5);
}

#[test]
fn selection_and_structural_edits_clear_inspector_drafts_but_keep_output_drafts() {
    use crate::messages::OutputMessage;
    use qtable_engine::model::TrackId;
    let mut app = app_with_two_tracks_and_clips();
    let [first, second] = [track_ids(&app)[0], track_ids(&app)[1]];
    let host = DraftKey::Output(TrackId(201), OutputField::Host);
    send(
        &mut app,
        Message::Outputs(OutputMessage::OutputHostChanged(
            TrackId(201),
            "unfinished host".into(),
        )),
    );
    select(&mut app, first, 0);
    inspector::event_editor::add_hit(&mut app, first, 0, qtable_engine::model::BeatTime::ZERO);
    let key = inspector::field_key(first, 0, ClipField::Length);
    for structural in [false, true] {
        select(&mut app, first, 0);
        send(
            &mut app,
            Message::Inspector(inspector::Message::Draft(key, "unfinished length".into())),
        );
        app.drafts.errors.insert(key, "old error".into());
        if structural {
            send(
                &mut app,
                Message::Inspector(inspector::Message::RemoveEvent(first, 0, 0)),
            );
        } else {
            select(&mut app, second, 0);
        }
        assert_eq!(app.drafts.values.contains_key(&key), !structural);
        assert_eq!(app.drafts.errors.contains_key(&key), !structural);
        assert_eq!(
            app.drafts.values.get(&host).map(String::as_str),
            Some("unfinished host")
        );
    }
}

#[test]
fn move_mode_moves_once_and_rejected_destination_preserves_source_and_drafts() {
    let mut app = app_with_two_tracks_and_clips();
    let track = track_ids(&app)[0];
    select(&mut app, track, 0);
    let original = app.project().tracks[0].slots[0].clip.as_ref().unwrap().id;
    let key = inspector::field_key(track, 0, ClipField::Length);
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(key, "unfinished".into())),
    );
    send(&mut app, Message::Edit(EditMessage::ToggleMoveClipMode));
    select(&mut app, TrackId(999), 1);
    assert!(!app.selection.move_clip);
    assert_eq!(app.selection.clip, Some(grid::Selection { track, slot: 0 }));
    assert!(app.drafts.values.contains_key(&key));
    assert_eq!(
        app.project().tracks[0].slots[0].clip.as_ref().unwrap().id,
        original
    );

    send(&mut app, Message::Edit(EditMessage::ToggleMoveClipMode));
    select(&mut app, track, 1);
    assert!(!app.selection.move_clip);
    assert!(app.project().tracks[0].slots[0].clip.is_none());
    assert_eq!(
        app.project().tracks[0].slots[1].clip.as_ref().unwrap().id,
        original
    );
    assert_eq!(app.selection.clip, Some(grid::Selection { track, slot: 1 }));
    assert!(!app.drafts.values.contains_key(&key));
    assert!(app.status.message().is_none());
    // The next click selects rather than moving the clip again.
    select(&mut app, track, 1);
    assert_eq!(app.selection.clip, Some(grid::Selection { track, slot: 1 }));
    assert_eq!(
        app.project().tracks[0].slots[1].clip.as_ref().unwrap().id,
        original
    );
}

#[test]
fn selecting_a_scene_cancels_move_mode() {
    let mut app = app_with_two_tracks_and_clips();
    let track = track_ids(&app)[0];
    select(&mut app, track, 0);
    send(&mut app, Message::Edit(EditMessage::ToggleMoveClipMode));
    send(
        &mut app,
        Message::Grid(grid::Action::SelectScene(SceneId(1))),
    );
    assert!(!app.selection.move_clip);
}

#[test]
fn unsaved_state_follows_the_engine_revision_for_every_edit_path() {
    let mut app = app_with_two_tracks_and_clips();
    assert!(app.is_dirty());
    app.mark_saved();
    send(&mut app, Message::TempoChanged("93.5".into()));
    send(&mut app, Message::TempoSubmitted);
    assert!(app.is_dirty());
    app.mark_saved();
    send(&mut app, Message::TempoChanged("invalid".into()));
    send(&mut app, Message::TempoSubmitted);
    assert!(!app.is_dirty());
}

#[test]
fn musical_time_whole_text_reaches_clip_markers_follow_time_and_scene_duration() {
    use inspector::Message as I;
    use qtable_engine::model::BeatTime;
    for (text, ticks) in [("5", 19200), ("5.2", 21120), ("5.2.3", 21840)] {
        let mut app = app_with_two_tracks_and_clips();
        let track = track_ids(&app)[0];
        let scene = app.project().scenes[0].id;
        for message in [
            I::Length(track, 0, text.into()),
            I::FollowTime(track, 0, text.into()),
            I::SceneTime(scene, text.into()),
        ] {
            send(&mut app, Message::Inspector(message));
            assert!(app.drafts.errors.is_empty(), "{text}");
        }
        let clip = app.project().clip(track, 0).unwrap();
        assert_eq!(clip.length(), BeatTime(ticks));
        assert_eq!(clip.follow_actions.time, BeatTime(ticks));
        assert_eq!(app.project().scenes[0].follow_actions.time, BeatTime(ticks));
    }
    for field in [ClipField::Start, ClipField::End, ClipField::Position] {
        for (text, ticks) in [("2", 3840), ("2.2", 4800), ("2.2.3", 5280)] {
            let mut app = app_with_two_tracks_and_clips();
            let track = track_ids(&app)[0];
            send(&mut app, Message::Inspector(I::ToggleLoop(track, 0)));
            // Leave a generous valid region for each independently edited marker.
            send(
                &mut app,
                Message::Inspector(I::Marker(track, 0, ClipField::End, "9".into())),
            );
            send(
                &mut app,
                Message::Inspector(I::Marker(track, 0, field, text.into())),
            );
            assert!(app.drafts.errors.is_empty(), "{field:?}: {text}");
            let clip = app.project().clip(track, 0).unwrap();
            assert_eq!(
                match field {
                    ClipField::Start => clip.start,
                    ClipField::End => clip.end,
                    _ => clip.loop_start,
                },
                BeatTime(ticks)
            );
        }
    }
}
