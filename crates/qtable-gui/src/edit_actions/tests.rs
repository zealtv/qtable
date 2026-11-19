use super::*;
use crate::drafts::TrackField;
use crate::status::Severity;
use qtable_engine::engine::Engine;
use qtable_engine::model::{FieldType, OscArgument};

fn app() -> App {
    App::new(
        Engine::new(crate::files::new_project()).unwrap(),
        "120".into(),
        None,
    )
}

#[test]
fn rejected_manual_events_do_not_flash() {
    use qtable_engine::model::{DeviceId, EntryKey, EventRef};
    let mut app = app();
    let track = app.project().tracks[0].id;
    let event = EventRef {
        device: DeviceId(u64::MAX),
        key: EntryKey("missing".into()),
    };
    execute(
        &mut app,
        Command::FireEvent {
            track,
            event: event.clone(),
        },
    );
    assert!(app.status.message().is_some());
    assert!(
        !app.event_feedback
            .lit(track, &event, std::time::Instant::now())
    );
}

#[test]
fn rejected_field_keeps_its_draft_and_snapshot_then_success_clears_only_that_field() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let key = DraftKey::Track(track, TrackField::Path);
    let other = DraftKey::Track(track, TrackField::Name);
    app.drafts.values.insert(key, "/bad path".into());
    app.drafts.values.insert(other, "unfinished name".into());
    let revision = app.project_revision();
    let before = app.project().clone();
    app.snapshot.tempo_bpm = -1.0;
    assert!(
        run_edit_report(
            &mut app,
            Command::Edit(ProjectEdit::SetTrackPath {
                track,
                path: "/bad path".into(),
            }),
            Some(key),
        )
        .is_none()
    );
    assert_eq!(app.project_revision(), revision);
    assert_eq!(app.project(), &before);
    assert_eq!(app.snapshot.tempo_bpm, -1.0);
    assert_eq!(app.drafts.values[&key], "/bad path");
    assert_eq!(app.status.message(), Some(app.drafts.errors[&key].as_str()));
    assert_eq!(app.status.severity(), Severity::Error);

    assert!(
        run_edit_report(
            &mut app,
            Command::Edit(ProjectEdit::SetTrackPath {
                track,
                path: "/fixed".into(),
            }),
            Some(key),
        )
        .is_some()
    );
    assert_eq!(app.snapshot.tempo_bpm, app.project().tempo.bpm());
    assert!(!app.drafts.values.contains_key(&key));
    assert!(!app.drafts.errors.contains_key(&key));
    assert_eq!(app.drafts.values[&other], "unfinished name");
    assert!(app.status.message().is_none());
}

#[test]
fn unkeyed_edit_keeps_drafts_and_returns_conversion_details_with_a_notice() {
    let project = qtable_engine::file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/hit-arguments.ron"
    ))
    .unwrap();
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    let track = &app.project().tracks[0];
    let device = track.devices[0].id;
    let target = track.devices[0].target().unwrap();
    let entry = target.events[0].key.clone();
    let field = target.events[0].fields[0].key.clone();
    let key = DraftKey::Track(track.id, TrackField::Name);
    app.drafts.values.insert(key, "unfinished".into());
    app.drafts.errors.insert(key, "previous rejection".into());
    app.status.show_error("old error");
    let report = run_edit_report(
        &mut app,
        Command::Edit(ProjectEdit::EditTarget {
            device,
            edit: qtable_engine::edit::TargetEdit::SetFieldType {
                entry,
                field,
                value_type: FieldType::Float,
            },
        }),
        None,
    )
    .unwrap();
    assert_eq!(report.converted_hits, 1);
    assert_eq!(report.signature.as_ref().unwrap()[0], FieldType::Float);
    assert_eq!(
        app.project().tracks[0].slots[0]
            .clip
            .as_ref()
            .unwrap()
            .events[0]
            .arguments[0],
        OscArgument::Float(0.0)
    );
    assert!(
        app.status
            .message()
            .unwrap()
            .starts_with("Converted 1 hit to ")
    );
    assert_eq!(app.status.severity(), Severity::Notice);
    assert_eq!(app.drafts.values[&key], "unfinished");
    assert_eq!(app.drafts.errors[&key], "previous rejection");
}
