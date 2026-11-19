//! Exercise focus transitions through update without launching a GUI.
use super::update;
use crate::app::App;
use crate::drafts::MixerField;
use crate::drafts::OutputField;
use crate::drafts::TrackField;
use crate::drafts::{ClipField, DraftKey, SceneField};
use crate::inspector;
use crate::messages::{EditMessage, Message, OutputMessage};
use crate::{files, grid};
use qtable_engine::engine::Engine;
use qtable_engine::model::{BeatTime, OscAddress, OscArgument, PathLevel};

fn send(app: &mut App, message: Message) {
    let _ = update(app, message);
}
fn app() -> App {
    let engine = Engine::new(files::new_project()).unwrap();
    let mut app = App::new(engine, "120".into(), None);
    let track = app.project().tracks[0].id;
    send(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    send(&mut app, Message::Edit(EditMessage::CreateSelectedClip));
    crate::inspector::event_editor::add_hit(&mut app, track, 0, BeatTime::ZERO);
    let int = [qtable_engine::model::FieldType::Int];
    crate::inspector::event_editor::sign_lane(&mut app, track, 0, 0, &int);
    app
}
fn blur(app: &mut App) {
    let key = app.pending_field.unwrap();
    let value = crate::field_focus::value(app, key).unwrap();
    send(
        app,
        Message::FieldFocusChecked(key, value, app.project_generation, false),
    );
}

#[test]
fn group_path_blur_updates_preview_and_keeps_rejected_draft() {
    use qtable_engine::model::{Route, Track, TrackId};
    let mut p = crate::files::new_project();
    let member = p.tracks[0].id;
    let mut group = Track::new_group(TrackId(10), "Ensemble", p.scenes.len());
    group.route = Route::single(TrackId(201));
    p.insert_group(group, &[member]);
    p[member].route = Route::Group;
    let mut output = Track::new_output(TrackId(201), "Stage", "127.0.0.1", 9000);
    output = output.with_path("/stage");
    p.outputs.push(output);
    let mut other = Track::new_output(TrackId(202), "Media", "127.0.0.1", 9001);
    other = other.with_path("/media");
    other.mixer.active = false;
    p.outputs.push(other);
    p[TrackId(10)].route = Route::Outputs(vec![TrackId(201), TrackId(202)]);
    let mut app = App::new(Engine::new(p).unwrap(), "120".into(), None);
    app.selection.tracks = vec![TrackId(10)];
    app.selection.anchor = Some(TrackId(10));
    app.selection.inspector_tab = crate::inspector_panel::Tab::Track;
    let key = DraftKey::Track(TrackId(10), TrackField::Path);
    for (prefix, valid) in [
        ("/ensemble", true),
        ("/bad/", false),
        ("/edited", true),
        ("", true),
    ] {
        let before = app.project().clone();
        send(
            &mut app,
            Message::Edit(EditMessage::PathChanged(TrackId(10), prefix.into())),
        );
        assert_eq!(app.project(), &before);
        assert_eq!(app.pending_field, Some(key));
        blur(&mut app);
        if valid {
            assert_eq!(app.project()[TrackId(10)].path.as_str(), prefix);
            assert!(!app.drafts.values.contains_key(&key));
            assert!(!app.drafts.errors.contains_key(&key));
            let address = OscAddress::Relative("/play".into());
            let piece = "/play";
            for (output, prefix) in app.project().inherited_paths(member, PathLevel::Under) {
                let resolved = app
                    .project()
                    .resolve_event_address(member, output.id, &address)
                    .unwrap();
                assert_eq!(format!("{prefix}{piece}"), resolved);
            }
            assert!(
                app.project()
                    .path_destinations(member)
                    .iter()
                    .any(|d| !d.mixer.active)
            );
        } else {
            assert_eq!(app.project(), &before);
            assert_eq!(app.drafts.values[&key], prefix);
            assert!(app.drafts.errors.contains_key(&key));
        }
    }
}

#[test]
fn every_clip_and_scene_field_commits_on_focus_loss() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let scene = app.project().scenes[0].id;
    let cases = [
        (DraftKey::clip(track, 0, ClipField::Length), "8"),
        (DraftKey::clip(track, 0, ClipField::FollowTime), "3"),
        (DraftKey::clip(track, 0, ClipField::ChanceA), "7"),
        (DraftKey::clip(track, 0, ClipField::ChanceB), "9"),
        (DraftKey::clip(track, 0, ClipField::Multiplier), "2/3"),
        (
            DraftKey::clip(
                track,
                0,
                ClipField::EventArgument {
                    event: 0,
                    argument: 0,
                },
            ),
            "42",
        ),
        (DraftKey::scene(scene, SceneField::Name), "Verse"),
        (DraftKey::scene(scene, SceneField::FollowTime), "6"),
        (DraftKey::scene(scene, SceneField::ChanceA), "11"),
        (DraftKey::scene(scene, SceneField::ChanceB), "13"),
    ];
    for (key, value) in cases {
        let revision = app.project_revision();
        send(
            &mut app,
            Message::Inspector(inspector::Message::Draft(key, value.into())),
        );
        assert_eq!(
            app.project_revision(),
            revision,
            "typing {key:?} commits nothing"
        );
        blur(&mut app);
        assert!(app.project_revision() > revision, "blur commits {key:?}");
        assert!(!app.drafts.values.contains_key(&key));
        assert!(!app.drafts.errors.contains_key(&key));
    }
    let clip = app.project().tracks[0].slots[0].clip.as_ref().unwrap();
    assert_eq!(clip.length(), BeatTime::from_beats(32).unwrap());
    assert_eq!(clip.follow_actions.time, BeatTime::from_beats(12).unwrap());
    assert_eq!(clip.follow_actions.actions.chance_a, 7);
    assert_eq!(clip.follow_actions.actions.chance_b, 9);
    assert_eq!(clip.follow_actions.multiplier.numerator(), 2);
    assert_eq!(clip.follow_actions.multiplier.denominator(), 3);
    assert_eq!(clip.events[0].arguments[0], OscArgument::Int(42));
    assert_eq!(app.project().scenes[0].name, "Verse");
    assert_eq!(
        app.project().scenes[0].follow_actions.time,
        BeatTime::from_beats(24).unwrap()
    );
    assert_eq!(app.project().scenes[0].follow_actions.actions.chance_a, 11);
    assert_eq!(app.project().scenes[0].follow_actions.actions.chance_b, 13);
}

#[test]
fn outputs_and_tempo_commit_without_enter() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    send(&mut app, Message::Outputs(OutputMessage::AddOutput));
    let output = app.project().outputs[0].id;
    for message in [
        Message::TempoChanged("93.5".into()),
        Message::Outputs(OutputMessage::OutputHostChanged(output, "localhost".into())),
        Message::Outputs(OutputMessage::OutputPortChanged(output, "9001".into())),
        Message::Edit(EditMessage::PathChanged(output, "/test".into())),
        Message::Outputs(OutputMessage::TrackDelayChanged(track, "-12.5".into())),
    ] {
        send(&mut app, message);
        blur(&mut app);
        assert!(app.drafts.errors.is_empty());
    }
    assert_eq!(app.project().tempo.bpm(), 93.5);
    assert_eq!(
        app.project().outputs[0].destination().unwrap().host,
        "localhost"
    );
    assert_eq!(app.project().outputs[0].destination().unwrap().port, 9001);
    assert_eq!(app.project().outputs[0].path.as_str(), "/test");
    assert_eq!(app.project().tracks[0].delay_ms, -12.5);
}

#[test]
fn changing_fields_commits_only_the_previous_draft_and_stale_checks_are_ignored() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let generation = app.project_generation;
    let key = DraftKey::clip(track, 0, ClipField::Name);
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(key, "/first".into())),
    );
    send(
        &mut app,
        Message::FieldFocusChecked(key, "/first".into(), generation, true),
    );
    assert!(app.drafts.values.contains_key(&key));
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(key, "/second".into())),
    );
    send(
        &mut app,
        Message::FieldFocusChecked(key, "/first".into(), generation, false),
    );
    assert_eq!(app.drafts.values[&key], "/second");
    send(&mut app, Message::TempoChanged("90".into()));
    assert!(!app.drafts.values.contains_key(&key));
    assert_eq!(app.project().tempo.bpm(), 120.0);
    let track = app.project().tracks[0].id;
    assert_eq!(app.project().clip(track, 0).unwrap().name, "/second");
    send(
        &mut app,
        Message::FieldFocusChecked(
            DraftKey::Tempo,
            "90".into(),
            generation.wrapping_add(1),
            false,
        ),
    );
    assert_eq!(app.project().tempo.bpm(), 120.0);
    blur(&mut app);
    assert_eq!(app.project().tempo.bpm(), 90.0);
}

#[test]
fn invalid_blur_keeps_draft_error_and_model_across_selection_changes() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let key = DraftKey::clip(track, 0, ClipField::Length);
    let length = app.project().tracks[0].slots[0]
        .clip
        .as_ref()
        .unwrap()
        .length();
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(key, "invalid".into())),
    );
    send(&mut app, Message::Grid(grid::Action::Select(track, 1)));
    assert_eq!(app.drafts.values[&key], "invalid");
    assert!(app.drafts.errors.contains_key(&key));
    assert!(app.status.message().is_some());
    assert_eq!(
        app.project().tracks[0].slots[0]
            .clip
            .as_ref()
            .unwrap()
            .length(),
        length
    );
    send(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    assert_eq!(app.drafts.values[&key], "invalid");
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(key, "5".into())),
    );
    blur(&mut app);
    assert!(!app.drafts.values.contains_key(&key));
    assert!(!app.drafts.errors.contains_key(&key));
}

#[test]
fn escape_cancels_and_enter_and_tab_still_commit() {
    let mut app = app();
    send(&mut app, Message::TempoChanged("invalid".into()));
    send(&mut app, Message::TempoSubmitted);
    assert!(app.drafts.errors.contains_key(&DraftKey::Tempo));
    send(&mut app, Message::CancelField);
    assert_eq!(app.tempo_input, "120.00");
    send(&mut app, Message::TempoChanged("95".into()));
    send(&mut app, Message::TempoSubmitted);
    assert_eq!(app.project().tempo.bpm(), 95.0);
    assert!(app.drafts.values.is_empty());
}

#[test]
fn group_name_and_clip_name_commit_before_other_actions() {
    let mut app = app();
    let first = app.project().tracks[0].id;
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(
            DraftKey::clip(first, 0, ClipField::Name),
            "Intro".into(),
        )),
    );
    send(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(first, false)),
    );
    assert_eq!(
        app.project().tracks[0].slots[0].clip.as_ref().unwrap().name,
        "Intro"
    );
    send(&mut app, Message::Edit(EditMessage::AddTrack));
    let second = app.project().tracks[1].id;
    send(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(first, false)),
    );
    send(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(second, true)),
    );
    send(
        &mut app,
        Message::Outputs(OutputMessage::CreateGroupAroundSelected),
    );
    let group = app.project().group_tracks().next().unwrap().id;
    send(
        &mut app,
        Message::Edit(EditMessage::BeginRenameHeader(group)),
    );
    send(
        &mut app,
        Message::Edit(EditMessage::RenameHeaderDraftChanged("Band".into())),
    );
    send(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(first, false)),
    );
    assert_eq!(app.project().group_tracks().next().unwrap().name, "Band");
}

#[test]
fn rejected_output_blur_preserves_input_and_escape_does_not_commit_it() {
    let mut app = app();
    send(&mut app, Message::Outputs(OutputMessage::AddOutput));
    let id = app.project().outputs[0].id;
    let key = DraftKey::Output(id, OutputField::Port);
    send(
        &mut app,
        Message::Outputs(OutputMessage::OutputPortChanged(id, "0".into())),
    );
    blur(&mut app);
    assert_eq!(app.drafts.values[&key], "0");
    assert!(app.drafts.errors.contains_key(&key));
    assert!(app.status.message().is_some());
    assert_eq!(app.project().outputs[0].destination().unwrap().port, 9000);
    send(
        &mut app,
        Message::Outputs(OutputMessage::OutputPortChanged(id, "9002".into())),
    );
    send(&mut app, Message::CancelField);
    assert!(!app.drafts.values.contains_key(&key));
    assert!(!app.drafts.errors.contains_key(&key));
    assert_eq!(app.project().outputs[0].destination().unwrap().port, 9000);
}

#[test]
fn invalid_multiplier_is_red_and_reports_an_error() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let key = DraftKey::clip(track, 0, ClipField::Multiplier);
    send(
        &mut app,
        Message::Inspector(inspector::Message::Draft(key, "1/0".into())),
    );
    blur(&mut app);
    assert_eq!(app.drafts.values[&key], "1/0");
    assert!(app.drafts.errors.contains_key(&key));
    assert!(app.status.message().is_some());
    assert_eq!(
        app.project().tracks[0].slots[0]
            .clip
            .as_ref()
            .unwrap()
            .follow_actions
            .multiplier
            .denominator(),
        1
    );
}

#[test]
fn refocusing_the_pending_field_preserves_drafts_and_the_project() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    send(
        &mut app,
        Message::Edit(EditMessage::MixerDevice(
            crate::mixer_device::MixerMessage::Toggle(track),
        )),
    );
    for key in [
        DraftKey::Mixer(track, MixerField::Address),
        DraftKey::clip(track, 0, ClipField::Name),
    ] {
        for draft in ["dr", "a b"] {
            if matches!(key, DraftKey::Mixer(_, MixerField::Address)) {
                send(
                    &mut app,
                    Message::Edit(EditMessage::MixerDevice(
                        crate::mixer_device::MixerMessage::Draft(key, draft.into()),
                    )),
                );
            } else {
                send(
                    &mut app,
                    Message::Inspector(inspector::Message::Draft(key, draft.into())),
                );
            }
            let before = app.project().clone();
            for _ in 0..2 {
                let message = Message::Edit(EditMessage::FocusField(key));
                let task = update(&mut app, message);
                assert!(task.units() > 0);
                assert_eq!(app.project(), &before);
                assert_eq!(app.pending_field, Some(key));
                assert_eq!(app.drafts.values[&key], draft);
                assert!(!app.drafts.errors.contains_key(&key));
                assert!(app.status.message().is_none());
            }
            crate::field_focus::cancel(&mut app);
        }
    }
}

#[test]
fn bool_argument_commits_once_per_toggle_after_switching_type() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let field = DraftKey::clip(
        track,
        0,
        ClipField::EventArgument {
            event: 0,
            argument: 0,
        },
    );
    let before = app.project_revision();
    let bool = [qtable_engine::model::FieldType::Bool];
    crate::inspector::event_editor::sign_lane(&mut app, track, 0, 0, &bool);
    assert!(app.project_revision() > before);
    assert_eq!(
        app.project()[track].slots[0].clip.as_ref().unwrap().events[0].arguments[0],
        OscArgument::Bool(false)
    );
    for on in [true, false, true] {
        let before = app.project_revision();
        let mut expected = app.project().clone();
        expected[track].slots[0].clip.as_mut().unwrap().events[0].arguments[0] =
            OscArgument::Bool(on);
        send(
            &mut app,
            Message::Inspector(inspector::Message::Argument(
                track,
                0,
                0,
                0,
                "bool".into(),
                on.to_string(),
            )),
        );
        assert_eq!(app.project_revision(), before + 1);
        assert_eq!(app.project(), &expected);
        assert!(app.pending_field.is_none());
        assert!(!app.drafts.values.contains_key(&field));
        assert!(!app.drafts.errors.contains_key(&field));
        send(&mut app, Message::CheckFieldFocus);
        send(&mut app, Message::CommitField);
        assert_eq!(app.project_revision(), before + 1);
    }
}
