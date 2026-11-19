use super::*;
use crate::inspector::Message as Inspect;
use crate::inspector::lane_model::tests::mixed_project;
use crate::messages::Message as Root;
use qtable_engine::model::{DeviceRef, FieldKey, ParameterKey, Project};

fn app_with(project: Project) -> App {
    let engine = qtable_engine::engine::Engine::new(project).unwrap();
    let mut app = App::new(engine, "120".into(), None);
    let track = app.project().tracks[0].id;
    app.selection.clip = Some(crate::grid::Selection { track, slot: 0 });
    app.chrome.detail_open = true;
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    reconcile(&mut app);
    app
}
fn send(app: &mut App, message: Inspect) {
    let _ = crate::update::update(app, Root::Inspector(message));
}
fn action(app: &mut App, action: Action) {
    send(app, Inspect::Timeline(action));
}
fn key(app: &App) -> ClipKey {
    let clip = app.project().clip(TrackId(1), 0).unwrap();
    ClipKey {
        track: TrackId(1),
        slot: 0,
        id: clip.id,
        epoch: app.clip_view.editor.epoch,
    }
}
fn argument(parameter: &str) -> ArgumentRef {
    ArgumentRef {
        device: DeviceRef::Chain(qtable_engine::model::DeviceId(1)),
        parameter: ParameterKey(parameter.into()),
        field: FieldKey("value".into()),
    }
}
fn target(app: &App, parameter: &str) -> PointTarget {
    PointTarget {
        key: key(app),
        revision: app.project_revision(),
        target: argument(parameter),
    }
}
fn points(app: &App, parameter: &str) -> Vec<(u64, f64)> {
    let target = argument(parameter);
    app.project()
        .clip(TrackId(1), 0)
        .unwrap()
        .lanes
        .iter()
        .find_map(|l| l.envelope().filter(|e| e.target == target))
        .unwrap()
        .points
        .iter()
        .map(|p| (p.time.ticks(), p.value))
        .collect()
}
fn curve(app: &App, parameter: &str, index: usize) -> f32 {
    let target = argument(parameter);
    app.project()
        .clip(TrackId(1), 0)
        .unwrap()
        .lanes
        .iter()
        .find_map(|l| l.envelope().filter(|e| e.target == target))
        .unwrap()
        .points[index]
        .curve
}
/// Selects `parameter`'s lane and point `index` as a released gesture would.
fn select(app: &mut App, parameter: &str, index: usize) {
    let lane = target(app, parameter);
    action(app, Action::PickPoint(lane, index));
}
fn field(app: &App, field: ClipField) -> DraftKey {
    DraftKey::clip(key(app).track, 0, field)
}
fn typed(app: &mut App, field: ClipField, text: &str) {
    let key = DraftKey::clip(TrackId(1), 0, field);
    send(app, Inspect::Draft(key, text.into()));
    send(app, Inspect::PointText(TrackId(1), 0, field, text.into()));
}

#[test]
fn a_released_gesture_is_one_edit_that_selects_the_lane_and_the_point() {
    let mut app = app_with(mixed_project());
    let before = app.project_revision();
    let lane = target(&app, "opacity");
    let mut list = app
        .project()
        .clip(TrackId(1), 0)
        .unwrap()
        .lanes
        .iter()
        .find_map(|l| l.envelope().filter(|e| e.target == lane.target))
        .unwrap()
        .points
        .clone();
    list[1].time = qtable_engine::model::BeatTime(1440);
    action(&mut app, Action::SetPoints(lane.clone(), list, Some(1)));
    assert_eq!(app.project_revision(), before + 1);
    assert_eq!(points(&app, "opacity")[1], (1440, 0.9));
    assert_eq!(app.clip_view.editor.point_for(key(&app)), Some(1));
    assert_eq!(
        app.clip_view.editor.envelope_for(key(&app)),
        Some(lane.target)
    );
    assert!(app.clip_view.editor.hover.is_none());
}

#[test]
fn a_click_that_selects_a_point_fills_the_fields_without_an_edit() {
    let mut app = app_with(mixed_project());
    let before = app.project_revision();
    let lane = target(&app, "opacity");
    let list = app
        .project()
        .clip(TrackId(1), 0)
        .unwrap()
        .lanes
        .iter()
        .find_map(|l| l.envelope().filter(|e| e.target == lane.target))
        .unwrap()
        .points
        .clone();
    action(&mut app, Action::SetPoints(lane, list, Some(1)));
    assert_eq!(app.project_revision(), before, "selecting is not an edit");
    let point = app.clip_view.editor.point.as_ref().unwrap();
    assert_eq!((point.index, &point.lane.target), (1, &argument("opacity")));
    assert!(selected(&app).is_some());
}

#[test]
fn a_gesture_against_a_stale_lane_publishes_no_edit() {
    let mut app = app_with(mixed_project());
    let stale = target(&app, "opacity");
    // Any other edit changes the revision, as do deleting the argument's lane or the clip.
    send(&mut app, Inspect::ClipName(TrackId(1), 0, "Renamed".into()));
    let revision = app.project_revision();
    action(&mut app, Action::SetPoints(stale.clone(), vec![], None));
    assert_eq!(app.project_revision(), revision);
    assert_eq!(points(&app, "opacity").len(), 4);
    // The lane is gone: nothing to replace, and nothing fails.
    crate::edit_actions::execute_structural_edit(
        &mut app,
        qtable_engine::engine::Command::Edit(qtable_engine::edit::ProjectEdit::EditEnvelope {
            track: TrackId(1),
            slot: 0,
            edit: qtable_engine::edit::EnvelopeEdit::Remove {
                target: argument("opacity"),
            },
        }),
    );
    let revision = app.project_revision();
    let gone = PointTarget {
        revision,
        ..stale.clone()
    };
    action(&mut app, Action::SetPoints(gone, vec![], None));
    assert_eq!(app.project_revision(), revision);
    // Another selected clip (a new epoch) is not this gesture's clip.
    let mut other = target(&app, "steps");
    other.key.epoch += 1;
    action(&mut app, Action::SetPoints(other, vec![], None));
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn delete_and_backspace_remove_the_selected_point_and_only_it() {
    let mut app = app_with(mixed_project());
    // With nothing selected the shortcut falls through to hits, which have none selected.
    let before = app.project_revision();
    shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    assert_eq!(app.project_revision(), before);
    select(&mut app, "opacity", 1);
    shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    assert_eq!(
        points(&app, "opacity"),
        [(0, 0.2), (2400, 0.3), (3840, 0.6)]
    );
    assert_eq!(app.clip_view.editor.point_for(key(&app)), None);
    assert_eq!(app.project_revision(), before + 1);
}

#[test]
fn the_selected_point_resolves_by_identity_after_an_unrelated_edit() {
    let mut app = app_with(mixed_project());
    select(&mut app, "opacity", 1);
    reconcile(&mut app);
    assert_eq!(app.clip_view.editor.point_for(key(&app)), Some(1));
    // Content picks resolve again by lane, time and ordinal after a revision.
    let draft = field(&app, ClipField::PointValue);
    app.drafts.values.insert(draft, "0.3".into());
    send(&mut app, Inspect::ClipName(TrackId(1), 0, "Renamed".into()));
    assert_eq!(app.clip_view.editor.point_for(key(&app)), Some(1));
    assert_eq!(
        app.clip_view.editor.point.as_ref().unwrap().lane.revision,
        app.project_revision()
    );
}

#[test]
fn a_typed_position_removes_the_neighbours_it_passes_and_clamps_to_the_loop() {
    let mut app = app_with(mixed_project());
    select(&mut app, "opacity", 1);
    // Bar 2 beat 1 is tick 3840, the loop end: it passes the point at 2400 and steps with the
    // last point, staying before it as it has not passed it.
    typed(&mut app, ClipField::PointPosition, "2. 1. 1");
    assert_eq!(
        points(&app, "opacity"),
        [(0, 0.2), (3840, 0.9), (3840, 0.6)]
    );
    assert_eq!(app.clip_view.editor.point_for(key(&app)), Some(1));
    // A position past the end lands on it.
    let mut app = app_with(mixed_project());
    select(&mut app, "opacity", 1);
    typed(&mut app, ClipField::PointPosition, "9. 1. 1");
    assert_eq!(points(&app, "opacity")[2].0, 3840);
    // A position inside moves it.
    let mut app = app_with(mixed_project());
    select(&mut app, "opacity", 1);
    typed(&mut app, ClipField::PointPosition, "1. 2. 1");
    assert_eq!(points(&app, "opacity")[1], (960, 0.9));
}

#[test]
fn typed_values_validate_as_the_face_does_and_escape_restores_the_shown_value() {
    let mut app = app_with(mixed_project());
    select(&mut app, "opacity", 1);
    let value = field(&app, ClipField::PointValue);
    typed(&mut app, ClipField::PointValue, "0.5");
    assert_eq!(points(&app, "opacity")[1], (1200, 0.5));
    let revision = app.project_revision();
    // Out of the argument's range, and not a number: no edit, the draft stays flagged, and
    // the message is in the status bar.
    for bad in ["1.5", "loud"] {
        typed(&mut app, ClipField::PointValue, bad);
        assert_eq!(app.project_revision(), revision, "{bad}");
        assert!(app.drafts.errors.contains_key(&value), "{bad}");
        assert!(app.status.message().is_some(), "{bad}");
    }
    // The refusal names the argument, as the lane header does.
    typed(&mut app, ClipField::PointValue, "1.5");
    assert_eq!(
        app.status.message(),
        Some("Opacity must be between 0.00 and 1.00")
    );
    // Escape drops the draft and its error without an edit.
    send(&mut app, Inspect::Draft(value, "0.9".into()));
    let _ = crate::update::update(&mut app, Root::CancelField);
    assert!(!app.drafts.values.contains_key(&value));
    assert!(!app.drafts.errors.contains_key(&value));
    assert_eq!(app.project_revision(), revision);
    // Leaving the field commits, as Enter does.
    send(&mut app, Inspect::Draft(value, "0.75".into()));
    let _ = crate::update::update(&mut app, Root::CommitField);
    assert_eq!(points(&app, "opacity")[1].1, 0.75);
    // Typing what is already there is no edit.
    let revision = app.project_revision();
    typed(&mut app, ClipField::PointValue, "0.75");
    assert_eq!(app.project_revision(), revision);
    assert!(!app.drafts.values.contains_key(&value));
}

#[test]
fn stepped_values_round_and_discrete_levels_are_chosen() {
    let mut app = app_with(mixed_project());
    select(&mut app, "steps", 1);
    typed(&mut app, ClipField::PointValue, "3.4");
    assert_eq!(points(&app, "steps")[1], (1920, 3.0));
    typed(&mut app, ClipField::PointValue, "9");
    assert_eq!(points(&app, "steps")[1].1, 3.0, "8 is the top of the range");
    // A Toggle's press and an Enum's option set a level.
    select(&mut app, "strobe", 0);
    send(&mut app, Inspect::PointValue(TrackId(1), 0, 1.0));
    assert_eq!(points(&app, "strobe")[0], (0, 1.0));
    select(&mut app, "mode", 0);
    send(&mut app, Inspect::PointValue(TrackId(1), 0, 3.0));
    assert_eq!(points(&app, "mode")[0], (0, 3.0));
}

#[test]
fn curvature_is_typed_in_range_and_refused_for_the_last_point_and_discrete_arguments() {
    let mut app = app_with(mixed_project());
    select(&mut app, "opacity", 1);
    typed(&mut app, ClipField::PointCurve, "-0.4");
    assert_eq!(curve(&app, "opacity", 1), -0.4);
    let revision = app.project_revision();
    let key = field(&app, ClipField::PointCurve);
    typed(&mut app, ClipField::PointCurve, "1.5");
    assert!(app.drafts.errors.contains_key(&key));
    assert_eq!(app.project_revision(), revision);
    // The last point has no leaving segment.
    select(&mut app, "opacity", 3);
    typed(&mut app, ClipField::PointCurve, "0.5");
    assert_eq!(curve(&app, "opacity", 3), 0.0);
    // A Toggle's curvature is disabled.
    select(&mut app, "strobe", 0);
    typed(&mut app, ClipField::PointCurve, "0.5");
    assert_eq!(curve(&app, "strobe", 0), 0.0);
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn point_position_whole_text_accepts_one_two_and_three_parts() {
    for (input, ticks) in [("1", 0), ("1.2", 960), ("1.2.3", 1440)] {
        let mut app = app_with(mixed_project());
        select(&mut app, "opacity", 1);
        typed(&mut app, ClipField::PointPosition, input);
        assert!(app.drafts.errors.is_empty(), "{input}");
        assert!(
            points(&app, "opacity")
                .iter()
                .any(|(time, value)| *time == ticks && (*value - 0.9).abs() < 1e-9),
            "{input}"
        );
    }
}
