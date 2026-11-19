use super::*;
use crate::inspector::event_editor;
use qtable_engine::model::BeatTime;

fn setup() -> (App, PointTarget) {
    let project = crate::inspector::lane_model::tests::mixed_project();
    let argument = project
        .clip(TrackId(1), 0)
        .unwrap()
        .lanes
        .iter()
        .find_map(|l| l.envelope().map(|e| e.target.clone()))
        .unwrap();
    let engine = qtable_engine::engine::Engine::new(project).unwrap();
    let mut app = App::new(engine, "120".into(), None);
    app.selection.clip = Some(crate::grid::Selection {
        track: TrackId(1),
        slot: 0,
    });
    app.chrome.detail_open = true;
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    event_editor::reconcile(&mut app);
    choose_control(&mut app, TrackId(1), 0, argument.clone());
    let target = PointTarget {
        key: key_for(&app, TrackId(1), 0).unwrap(),
        revision: app.project_revision(),
        target: argument,
    };
    (app, target)
}
fn fresh(app: &App, target: &PointTarget) -> PointTarget {
    PointTarget {
        revision: app.project_revision(),
        ..target.clone()
    }
}
#[test]
fn unlinked_bar_copies_markers_edits_region_and_relinks_without_touching_points() {
    let (mut app, target) = setup();
    let clip = app.project().clip(target.key.track, 0).unwrap();
    let original = clip.clone();
    submit(&mut app, target.clone(), Some(original.region()), None);
    assert_eq!(
        lane(&app, &fresh(&app, &target)).unwrap(),
        Some(original.region())
    );
    let next = fresh(&app, &target);
    text(&mut app, next, Field::Position, "1. 2. 1".into());
    let region = lane(&app, &fresh(&app, &target)).unwrap().unwrap();
    assert_eq!(region.loop_start, BeatTime(960));
    assert_eq!(region.loop_end, BeatTime(original.loop_end.0 + 960));
    assert_eq!(
        app.project().clip(target.key.track, 0).unwrap().region(),
        original.region()
    );
    let next = fresh(&app, &target);
    submit(&mut app, next, None, None);
    assert_eq!(app.project().clip(target.key.track, 0).unwrap(), &original);
}
#[test]
fn unlinked_bar_rejects_stale_targets_and_invalid_text() {
    let (mut app, target) = setup();
    let region = app.project().clip(target.key.track, 0).unwrap().region();
    submit(&mut app, target.clone(), Some(region), None);
    let revision = app.project_revision();
    submit(&mut app, target.clone(), None, None);
    assert_eq!(app.project_revision(), revision);
    let next = fresh(&app, &target);
    let message = Message::Timeline(Action::LaneRegionText(next, Field::Start, "bad".into()));
    let _ = crate::update::update(&mut app, crate::messages::Message::Inspector(message));
    assert_eq!(app.project_revision(), revision);
    assert!(app.status.message().unwrap().contains("position"));
    let next = fresh(&app, &target);
    let _ = crate::update::update(
        &mut app,
        crate::messages::Message::Inspector(Message::Timeline(Action::LaneRegionText(
            next,
            Field::Length,
            "0. 0. 0".into(),
        ))),
    );
    assert_eq!(app.project_revision(), revision);
    assert!(app.status.message().is_some());
}
#[test]
fn unlinked_overview_select_all_excludes_own_region_but_one_lane_includes_it() {
    let (mut app, target) = setup();
    let region = app.project().clip(target.key.track, 0).unwrap().region();
    submit(&mut app, target.clone(), Some(region), None);
    choose_tab(&mut app, target.key.track, 0, EditorTab::Overview);
    let key = key_for(&app, target.key.track, 0).unwrap();
    event_editor::handle(
        &mut app,
        Action::Content(event_editor::content::Selection::time(
            key,
            BeatTime::ZERO..BeatTime(3840),
            vec![event_editor::lanes::Identity::Envelope(
                target.target.clone(),
            )],
        )),
    );
    assert!(
        app.clip_view
            .editor
            .content
            .as_ref()
            .unwrap()
            .points
            .is_empty()
    );
    choose_control(&mut app, target.key.track, 0, target.target.clone());
    let key = key_for(&app, target.key.track, 0).unwrap();
    event_editor::handle(
        &mut app,
        Action::Content(event_editor::content::Selection::time(
            key,
            BeatTime::ZERO..BeatTime(3840),
            vec![event_editor::lanes::Identity::Envelope(
                target.target.clone(),
            )],
        )),
    );
    assert!(
        !app.clip_view
            .editor
            .content
            .as_ref()
            .unwrap()
            .points
            .is_empty()
    );
}

#[test]
fn linked_region_controls_leave_the_widget_and_focus_tree_until_unlinked() {
    fn controls(app: &App, target: &PointTarget) -> Vec<ControlId> {
        let focus = crate::drafts::ControlFocus::default();
        let controls = Controls::new(crate::control::LIVE_DARK, &focus);
        let mut widget = bar(
            app.project(),
            target.key,
            app.project_revision(),
            &target.target,
            &controls,
            &app.drafts.values,
            &app.drafts.errors,
        );
        crate::control_focus::test_registry::controls(&mut widget)
    }
    let (mut app, target) = setup();
    let link = ControlId::action(
        View::Clip,
        "lane-link",
        ControlOwner::ClipSlot(target.key.track, 0),
    );
    assert_eq!(controls(&app, &target), vec![link]);
    let region = app.project().clip(target.key.track, 0).unwrap().region();
    submit(&mut app, target.clone(), Some(region), None);
    let shown = controls(&app, &target);
    assert!(shown.contains(&link));
    assert!(shown.contains(&ControlId::action(
        View::Clip,
        "lane-loop",
        ControlOwner::ClipSlot(target.key.track, 0)
    )));
    for field in Field::ALL {
        assert!(shown.contains(&ControlId::field(
            View::Clip,
            DraftKey::clip(target.key.track, 0, ClipField::LaneMarker(field))
        )));
    }
    let next = fresh(&app, &target);
    submit(&mut app, next, None, None);
    assert_eq!(controls(&app, &target), vec![link]);
}

#[test]
fn region_whole_text_uses_musical_defaults_for_all_four_fields() {
    for field in Field::ALL {
        for input in ["2", "2.2", "2.2.3"] {
            let (mut app, target) = setup();
            let mut region = app.project().clip(target.key.track, 0).unwrap().region();
            region.end = BeatTime(38400);
            region.looping = false;
            submit(&mut app, target.clone(), Some(region), None);
            let next = fresh(&app, &target);
            text(&mut app, next, field, input.into());
            assert!(app.drafts.errors.is_empty(), "{field:?} {input}");
            let actual = lane(&app, &fresh(&app, &target)).unwrap().unwrap();
            let expected = if field == Field::Length {
                musical_time::parse_duration(input, app.project().time_signature).unwrap()
            } else {
                musical_time::parse_position(input, app.project().time_signature).unwrap()
            };
            assert_eq!(
                match field {
                    Field::Start => actual.start,
                    Field::End => actual.end,
                    Field::Position => actual.loop_start,
                    Field::Length => BeatTime(actual.loop_end.0 - actual.loop_start.0),
                },
                expected,
                "{field:?} {input}"
            );
        }
    }
}
