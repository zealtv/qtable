use super::*;
use crate::inspector::Message as Inspect;
use crate::inspector::event_editor::{Action, PointTarget};
use crate::inspector::lane_model::tests::mixed_project;
use crate::inspector::lane_view::chooser::entries::RowSpec;
use crate::messages::Message as Root;
use crate::overlay::Overlay;
use qtable_engine::model::{
    Axis, BeatTime, Breakpoint, DeviceId, FieldKey, Lane, ParameterKey, Project,
};

fn app_with(project: Project) -> App {
    let engine = qtable_engine::engine::Engine::new(project).unwrap();
    let mut app = App::new(engine, "120".into(), None);
    let track = app.project().tracks[0].id;
    app.selection.clip = Some(crate::grid::Selection { track, slot: 0 });
    app.chrome.detail_open = true;
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    super::super::event_editor::reconcile(&mut app);
    app
}
fn send(app: &mut App, message: Inspect) {
    let _ = crate::update::update(app, Root::Inspector(message));
}
fn bop() -> DeviceRef {
    DeviceRef::Chain(DeviceId(1))
}
fn argument(parameter: &str, field: &str) -> ArgumentRef {
    ArgumentRef {
        device: bop(),
        parameter: ParameterKey(parameter.into()),
        field: FieldKey(field.into()),
    }
}
fn volume() -> ArgumentRef {
    ArgumentRef::volume()
}
fn clip_id(app: &App) -> ClipId {
    app.project().clip(TrackId(1), 0).unwrap().id
}
fn choice(app: &App) -> Choice {
    current(app, TrackId(1), 0)
}
fn lanes(app: &App) -> usize {
    app.project().clip(TrackId(1), 0).unwrap().lanes.len()
}
fn key(app: &App) -> ClipKey {
    ClipKey {
        track: TrackId(1),
        slot: 0,
        id: clip_id(app),
        epoch: app.clip_view.editor.epoch,
    }
}
fn points(app: &App, target: &ArgumentRef) -> Option<Vec<(u64, f64)>> {
    app.project()
        .clip(TrackId(1), 0)
        .unwrap()
        .lanes
        .iter()
        .find_map(|l| l.envelope().filter(|e| e.target == *target))
        .map(|e| e.points.iter().map(|p| (p.time.ticks(), p.value)).collect())
}
fn chooser_rows(app: &App) -> Vec<(String, bool, bool)> {
    let Some(Overlay::Chooser(open)) = app.overlay else {
        panic!("a chooser is open");
    };
    super::chooser::rows(app, open)
        .into_iter()
        .map(
            |RowSpec {
                 label, led, press, ..
             }| (label, led, press.is_some()),
        )
        .collect()
}
fn without_lane(parameter: &str) -> Project {
    let mut project = mixed_project();
    let target = argument(parameter, "value");
    project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .retain(|l| l.envelope().is_none_or(|e| e.target != target));
    project
}

#[test]
fn a_clip_opens_on_overview_and_the_device_chooser_lists_only_devices_in_chain_order() {
    let mut app = app_with(mixed_project());
    assert_eq!(choice(&app), Choice::default());
    assert_eq!(choice(&app).tab, EditorTab::Overview);
    assert_eq!(choice(&app).one_lane(), None);
    send(
        &mut app,
        Inspect::ChooseTab(TrackId(1), 0, EditorTab::Envelopes),
    );
    send(
        &mut app,
        Inspect::OpenChooser(TrackId(1), 0, chooser::Which::Device),
    );
    assert_eq!(
        chooser_rows(&app),
        [
            ("BopOS".to_owned(), false, true),
            ("Mixer".to_owned(), false, true),
        ],
        "the Overview tab replaced All lanes"
    );
}

#[test]
fn the_control_chooser_leads_with_only_adjusted_then_arguments_with_red_dots_and_no_events() {
    let mut app = app_with(mixed_project());
    send(&mut app, Inspect::ChooseDevice(TrackId(1), 0, bop()));
    send(
        &mut app,
        Inspect::OpenChooser(TrackId(1), 0, chooser::Which::Control),
    );
    let rows = chooser_rows(&app);
    let labels: Vec<(&str, bool)> = rows.iter().map(|(l, led, _)| (l.as_str(), *led)).collect();
    assert_eq!(
        labels,
        [
            ("Only show adjusted envelopes", false),
            ("Opacity", true),
            ("Steps", true),
            ("Strobe", true),
            ("Mode", true),
            ("Position · x", false),
            ("Position · y", true),
        ]
    );
    assert!(rows.iter().all(|(_, _, pressable)| *pressable));
}

#[test]
fn the_chooser_faces_carry_the_lists_leds_and_the_control_greys_once_overridden() {
    use crate::control::automation_mark::Mark;
    use crate::control::multi_select::FaceLed::{Lit, Unlit};
    use crate::parameter_gesture::{AutomationState, Effective};
    let mut project = mixed_project();
    let leds = |project: &Project, device, argument, state: Option<AutomationState>| {
        let choice = Choice {
            tab: EditorTab::Envelopes,
            device: Some(device),
            argument,
            only_adjusted: false,
        };
        let live = state.map(|state| Effective { value: 0.5, state });
        chooser::leds(project, (TrackId(1), 0), &choice, live)
    };
    let opacity = Some(argument("opacity", "value"));
    // An argument with a lane lights both faces; one without lights only its device.
    assert_eq!(
        leds(
            &project,
            bop(),
            opacity.clone(),
            Some(AutomationState::Following)
        ),
        [Lit(Mark::Following), Lit(Mark::Following)]
    );
    assert_eq!(
        leds(&project, bop(), Some(argument("position", "x")), None),
        [Lit(Mark::Following), Unlit]
    );
    // A manual change greys the control's LED, as it greys the curve.
    assert_eq!(
        leds(&project, bop(), opacity, Some(AutomationState::Overridden)),
        [Lit(Mark::Following), Lit(Mark::Overridden)]
    );
    // Once the clip has no lanes, both faces are unlit, with their room kept.
    project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .clear();
    assert_eq!(
        leds(&project, DeviceRef::Mixer, Some(volume()), None),
        [Unlit, Unlit]
    );
}

#[test]
fn only_show_adjusted_envelopes_keeps_the_list_open_and_drops_entries_without_a_lane() {
    let mut app = app_with(mixed_project());
    send(&mut app, Inspect::ChooseDevice(TrackId(1), 0, bop()));
    send(
        &mut app,
        Inspect::OpenChooser(TrackId(1), 0, chooser::Which::Control),
    );
    send(&mut app, Inspect::OnlyAdjusted(TrackId(1), 0));
    assert!(matches!(app.overlay, Some(Overlay::Chooser(_))));
    assert!(choice(&app).only_adjusted);
    let labels: Vec<String> = chooser_rows(&app).into_iter().map(|r| r.0).collect();
    assert!(!labels.contains(&"Position · x".to_owned()));
    assert!(labels.contains(&"Position · y".to_owned()));
    send(&mut app, Inspect::OnlyAdjusted(TrackId(1), 0));
    assert!(!choice(&app).only_adjusted);
}

#[test]
fn choosing_a_device_picks_its_first_lane_else_its_first_argument_and_never_adds_a_lane() {
    let mut app = app_with(mixed_project());
    let before = lanes(&app);
    send(&mut app, Inspect::ChooseDevice(TrackId(1), 0, bop()));
    assert_eq!(choice(&app).one_lane(), Some(&argument("opacity", "value")));
    send(
        &mut app,
        Inspect::ChooseDevice(TrackId(1), 0, DeviceRef::Mixer),
    );
    assert_eq!(choice(&app).one_lane(), Some(&volume()));
    assert_eq!(choice(&app).tab, EditorTab::Envelopes);
    assert_eq!(lanes(&app), before);

    // No lane for Opacity or any other argument on the device: its first argument still shows.
    let mut project = mixed_project();
    project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .retain(|l| l.envelope().is_none());
    let mut app = app_with(project);
    send(&mut app, Inspect::ChooseDevice(TrackId(1), 0, bop()));
    assert_eq!(choice(&app).one_lane(), Some(&argument("opacity", "value")));
}

#[test]
fn picking_an_argument_without_a_lane_browses_without_adding_one() {
    let mut app = app_with(mixed_project());
    let before = lanes(&app);
    send(
        &mut app,
        Inspect::OpenChooser(TrackId(1), 0, chooser::Which::Control),
    );
    send(
        &mut app,
        Inspect::ChooseControl(TrackId(1), 0, argument("position", "x")),
    );
    assert!(app.overlay.is_none());
    assert_eq!(choice(&app).one_lane(), Some(&argument("position", "x")));
    assert_eq!(choice(&app).device, Some(bop()));
    assert_eq!(lanes(&app), before);
    assert_eq!(app.clip_view.editor.envelope_for(key(&app)), None);
}

#[test]
fn picking_a_laned_argument_selects_its_lane_on_the_envelopes_tab() {
    let mut app = app_with(mixed_project());
    send(&mut app, Inspect::ChooseControl(TrackId(1), 0, volume()));
    assert_eq!(app.clip_view.editor.envelope_for(key(&app)), Some(volume()));
    assert_eq!(choice(&app).device, Some(DeviceRef::Mixer));
    assert_eq!(choice(&app).tab, EditorTab::Envelopes);
}

#[test]
fn each_clip_keeps_its_own_choice_and_a_removed_clip_is_forgotten() {
    let mut app = app_with(mixed_project());
    send(&mut app, Inspect::ChooseDevice(TrackId(1), 0, bop()));
    let id = clip_id(&app);
    // Another clip's key finds All lanes; this clip's choice is still there.
    let views = &app.clip_view.editor.views;
    assert_eq!(
        views.choice(app.project(), TrackId(1), ClipId(99)),
        Choice::default()
    );
    assert!(
        views
            .choice(app.project(), TrackId(1), id)
            .one_lane()
            .is_some()
    );
    // Deselecting and reselecting the clip (reconcile's owner change) keeps it.
    app.selection.clip = None;
    super::super::event_editor::reconcile(&mut app);
    app.selection.clip = Some(crate::grid::Selection {
        track: TrackId(1),
        slot: 0,
    });
    super::super::event_editor::reconcile(&mut app);
    assert!(choice(&app).one_lane().is_some());
    // The clip going away drops its entry.
    let mut project = app.project().clone();
    project.tracks[0].slots[0].clip = None;
    app.clip_view.editor.views.retain_existing(&project);
    assert_eq!(
        app.clip_view.editor.views.choice(&project, TrackId(1), id),
        Choice::default()
    );
}

#[test]
fn a_choice_naming_a_device_that_is_gone_forgets_it_and_envelopes_falls_back_to_the_first_pick() {
    let project = mixed_project();
    let stale = Choice {
        tab: EditorTab::Overview,
        device: Some(DeviceRef::Chain(DeviceId(77))),
        argument: Some(argument("opacity", "value")),
        only_adjusted: true,
    };
    let resolved = stale.resolved(&project, TrackId(1), Some(0));
    assert_eq!(resolved.tab, EditorTab::Overview);
    assert_eq!(resolved.device, None);
    assert_eq!(resolved.argument, None);
    assert!(
        resolved.only_adjusted,
        "the filter is a preference, not a lane"
    );
    // On the Envelopes tab the clip's first envelope stands in, as when the tab first opens.
    let on_envelopes = Choice {
        tab: EditorTab::Envelopes,
        ..stale
    };
    let resolved = on_envelopes.resolved(&project, TrackId(1), Some(0));
    assert_eq!(resolved.one_lane(), Some(&argument("opacity", "value")));
    // An argument of another device does not stand for this one.
    let mixed_up = Choice {
        tab: EditorTab::Envelopes,
        device: Some(DeviceRef::Mixer),
        argument: Some(argument("opacity", "value")),
        only_adjusted: false,
    };
    assert_eq!(
        mixed_up.resolved(&project, TrackId(1), Some(0)).argument,
        None
    );
}

#[test]
fn changing_the_chosen_lane_makes_every_earlier_key_stale_like_a_clip_change() {
    let mut app = app_with(mixed_project());
    send(&mut app, Inspect::ChooseDevice(TrackId(1), 0, bop()));
    let old = PointTarget {
        key: key(&app),
        revision: app.project_revision(),
        target: argument("opacity", "value"),
    };
    let before = points(&app, &old.target).unwrap();
    send(
        &mut app,
        Inspect::ChooseControl(TrackId(1), 0, argument("steps", "value")),
    );
    // A release that began on the earlier lane commits nothing.
    send(
        &mut app,
        Inspect::Timeline(Action::SetPoints(old.clone(), vec![], None)),
    );
    assert_eq!(points(&app, &old.target).unwrap(), before);
    assert_eq!(app.clip_view.editor.point_for(key(&app)), None);
}

fn first_point(value: f64) -> Vec<Breakpoint> {
    vec![Breakpoint {
        time: BeatTime(480),
        value,
        curve: 0.0,
    }]
}

#[test]
fn the_first_point_creates_the_lane_in_two_edits_and_selects_it() {
    let mut app = app_with(mixed_project());
    let target = argument("position", "x");
    send(
        &mut app,
        Inspect::ChooseControl(TrackId(1), 0, target.clone()),
    );
    let (before, revision) = (lanes(&app), app.project_revision());
    let lane = PointTarget {
        key: key(&app),
        revision,
        target: target.clone(),
    };
    send(
        &mut app,
        Inspect::Timeline(Action::StartEnvelope(lane, first_point(40.0), Some(0))),
    );
    assert_eq!(lanes(&app), before + 1);
    assert_eq!(points(&app, &target), Some(vec![(480, 40.0)]));
    // `EnvelopeEdit::Add` and `SetPoints`: two revisions.
    assert_eq!(app.project_revision(), revision + 2);
    assert_eq!(
        app.clip_view.editor.envelope_for(key(&app)),
        Some(target.clone())
    );
    assert_eq!(app.clip_view.editor.point_for(key(&app)), Some(0));
    assert!(matches!(
        app.project().clip(TrackId(1), 0).unwrap().lanes.last(),
        Some(Lane::Envelope(e)) if e.target == target
    ));
}

#[test]
fn a_refused_second_edit_leaves_an_empty_lane_and_an_error_in_the_status_bar() {
    let mut app = app_with(mixed_project());
    let target = argument("position", "x");
    send(
        &mut app,
        Inspect::ChooseControl(TrackId(1), 0, target.clone()),
    );
    let lane = PointTarget {
        key: key(&app),
        revision: app.project_revision(),
        target: target.clone(),
    };
    // Position x runs 0 to 100.
    send(
        &mut app,
        Inspect::Timeline(Action::StartEnvelope(lane, first_point(500.0), Some(0))),
    );
    assert_eq!(points(&app, &target), Some(vec![]));
    assert_eq!(app.status.severity(), crate::status::Severity::Error);
    assert!(app.status.message().is_some());
    assert_eq!(app.clip_view.editor.envelope_for(key(&app)), Some(target));
}

#[test]
fn starting_an_envelope_is_refused_when_the_lane_exists_or_the_revision_moved() {
    let mut app = app_with(mixed_project());
    let target = argument("opacity", "value");
    let have = points(&app, &target).unwrap();
    let mut lane = PointTarget {
        key: key(&app),
        revision: app.project_revision(),
        target,
    };
    send(
        &mut app,
        Inspect::Timeline(Action::StartEnvelope(
            lane.clone(),
            first_point(0.5),
            Some(0),
        )),
    );
    assert_eq!(
        points(&app, &lane.target).unwrap(),
        have,
        "an existing lane is not restarted"
    );

    let mut app = app_with(without_lane("steps"));
    lane.target = argument("steps", "value");
    lane.revision = app.project_revision() + 1;
    lane.key = key(&app);
    let before = lanes(&app);
    send(
        &mut app,
        Inspect::Timeline(Action::StartEnvelope(lane, first_point(3.0), Some(0))),
    );
    assert_eq!(lanes(&app), before, "a stale revision publishes no edit");
}

#[test]
fn removing_the_chosen_lane_keeps_the_argument_chosen_so_the_view_invites_a_new_envelope() {
    let mut app = app_with(mixed_project());
    send(&mut app, Inspect::ChooseControl(TrackId(1), 0, volume()));
    crate::edit_actions::execute_structural_edit(
        &mut app,
        qtable_engine::engine::Command::Edit(qtable_engine::edit::ProjectEdit::EditEnvelope {
            track: TrackId(1),
            slot: 0,
            edit: qtable_engine::edit::EnvelopeEdit::Remove { target: volume() },
        }),
    );
    assert_eq!(points(&app, &volume()), None);
    assert_eq!(choice(&app).one_lane(), Some(&volume()));
}

#[test]
fn the_chosen_row_is_the_clips_lane_or_an_empty_one_of_the_same_axis_and_range() {
    let project = mixed_project();
    let clip = project.clip(TrackId(1), 0).unwrap();
    let model = super::super::lane_model::LaneModel::build(&project, TrackId(1), clip);
    let opacity = chosen_row(&project, TrackId(1), &model, &argument("opacity", "value"));
    assert_eq!(opacity.points.len(), 4);
    let empty = chosen_row(&project, TrackId(1), &model, &argument("position", "x"));
    assert!(empty.points.is_empty());
    assert_eq!((empty.min, empty.max), (0.0, 100.0));
    assert_eq!(
        (empty.axis, opacity.axis),
        (Axis::Continuous, Axis::Continuous)
    );
    assert_eq!((opacity.min, opacity.max), (0.0, 1.0));
    let steps = chosen_row(&project, TrackId(1), &model, &argument("steps", "value"));
    assert_eq!(
        (steps.axis, steps.min, steps.max),
        (Axis::Stepped, 0.0, 8.0)
    );
    let strobe = chosen_row(&project, TrackId(1), &model, &argument("strobe", "value"));
    assert_eq!(strobe.axis, Axis::Discrete);
    let mode = chosen_row(&project, TrackId(1), &model, &argument("mode", "value"));
    assert_eq!((mode.axis, mode.labels.len()), (Axis::Discrete, 4));
}

#[test]
fn the_choosers_close_when_the_clip_selection_moves_away() {
    let mut app = app_with(mixed_project());
    send(
        &mut app,
        Inspect::OpenChooser(TrackId(1), 0, chooser::Which::Device),
    );
    assert!(app.overlay.is_some());
    app.selection.clip = None;
    app.validate_overlay();
    assert!(app.overlay.is_none());
}

/// The *Show Envelope* entry of the control menu for `subject`: whether it is enabled, its
/// message and its help id; `None` when the menu does not list it.
fn show_envelope_entry(
    app: &App,
    subject: &crate::info::Subject,
) -> Option<(bool, Root, crate::info::Id)> {
    use crate::context_menu::{Entry, MenuTarget, current_entries};
    let revision = app.project_revision();
    current_entries(
        app,
        MenuTarget::Argument {
            subject: subject.clone(),
            revision,
        },
    )
    .into_iter()
    .find_map(|entry| match entry {
        Entry::Command {
            label: "Show Envelope",
            enabled,
            message,
            info,
            ..
        } => Some((enabled, message, info)),
        _ => None,
    })
}

/// The subject of the Target face's first argument, Opacity.
fn face_subject() -> crate::info::Subject {
    let field = field_of(&argument("opacity", "value"));
    crate::info::Subject::TargetField(
        crate::info::EntryRef::Parameter(field.parameter),
        field.field,
    )
}

fn field_of(argument: &ArgumentRef) -> qtable_engine::model::FieldRef {
    argument.on(TrackId(1))
}

#[test]
fn show_envelope_opens_the_selected_clip_s_lane_from_the_face_and_the_mixer() {
    use crate::info::{Id, Subject};
    let cases = [
        (face_subject(), argument("opacity", "value")),
        (Subject::Volume(TrackId(1)), volume()),
    ];
    for (subject, target) in cases {
        let mut app = app_with(mixed_project());
        app.selection.inspector_tab = crate::inspector_panel::Tab::Track;
        app.chrome.detail_open = false;
        let (enabled, message, info) = show_envelope_entry(&app, &subject).expect("listed");
        assert!(enabled && info == Id::ShowEnvelope);
        assert!(matches!(&message, Root::ShowEnvelope(f) if *f == field_of(&target)));
        let _ = crate::update::update(&mut app, message);
        assert_eq!(choice(&app).one_lane(), Some(&target));
        assert_eq!(
            app.selection.inspector_tab,
            crate::inspector_panel::Tab::Clip
        );
        assert!(app.chrome.detail_open);
        assert_eq!(
            app.selection.clip.map(|s| (s.track, s.slot)),
            Some((TrackId(1), 0))
        );
    }
}

#[test]
fn show_envelope_closes_the_menu_and_adds_no_lane_for_an_argument_without_one() {
    let mut app = app_with(without_lane("opacity"));
    let before = lanes(&app);
    let revision = app.project_revision();
    app.open_overlay(Overlay::Context(crate::context_menu::OpenMenu {
        at: iced::Point::ORIGIN,
        target: crate::context_menu::MenuTarget::Argument {
            subject: face_subject(),
            revision,
        },
    }));
    let _ = crate::update::update(
        &mut app,
        Root::ShowEnvelope(field_of(&argument("opacity", "value"))),
    );
    assert!(app.overlay.is_none());
    assert_eq!(choice(&app).one_lane(), Some(&argument("opacity", "value")));
    assert_eq!((lanes(&app), app.project_revision()), (before, revision));
}

#[test]
fn show_envelope_is_disabled_with_a_reason_without_the_selected_clip_on_the_track() {
    use crate::info::{Id, Subject};
    let mut project = mixed_project();
    project
        .tracks
        .push(qtable_engine::model::Track::new(TrackId(2), "Other", 1));
    let mut app = app_with(project);
    // Another slot playing does not stand in for a selection.
    app.snapshot
        .tracks
        .push(qtable_engine::engine::TrackSnapshot {
            id: TrackId(1),
            playing_slot: Some(0),
            triggered: None,
            play_position_beats: Some(0.0),
            pass_progress: Some(0.0),
            next_follow_beat: None,
            queued_follow_slot: None,
            empty_route: false,
        });
    for subject in [face_subject(), Subject::Volume(TrackId(1))] {
        app.selection.clip = None;
        let (enabled, message, info) = show_envelope_entry(&app, &subject).expect("listed");
        assert!(!enabled && info == Id::ShowEnvelopeNoClip);
        assert!(matches!(message, Root::CloseOverlay));

        app.selection.clip = Some(crate::grid::Selection {
            track: TrackId(2),
            slot: 0,
        });
        let (enabled, _, info) = show_envelope_entry(&app, &subject).expect("listed");
        assert!(!enabled && info == Id::ShowEnvelopeOtherTrack);
    }
    // Were it pressed anyway, nothing opens and no clip gets selected.
    let selected = app.selection.clip;
    let _ = crate::update::update(
        &mut app,
        Root::ShowEnvelope(field_of(&argument("opacity", "value"))),
    );
    assert_eq!(choice(&app), Choice::default());
    assert_eq!(app.selection.clip, selected);
}

#[test]
fn tracks_without_clip_slots_do_not_offer_show_envelope() {
    let app = app_with(mixed_project());
    let main = app.project().main.id;
    assert!(show_envelope_entry(&app, &crate::info::Subject::Volume(main)).is_none());
}

#[test]
fn each_show_envelope_help_text_gives_its_reason() {
    use crate::info::Id;
    assert!(
        Id::ShowEnvelopeNoClip
            .entry()
            .text
            .contains("no clip is selected")
    );
    assert!(
        Id::ShowEnvelopeOtherTrack
            .entry()
            .text
            .contains("another track")
    );
}

fn tab(app: &mut App, tab: EditorTab) {
    send(app, Inspect::ChooseTab(TrackId(1), 0, tab));
}

#[test]
fn envelopes_opened_without_a_pick_shows_the_clips_first_envelope_and_adds_no_lane() {
    // The clip's first envelope, here Volume after its event lane: not the first device's.
    let mut project = mixed_project();
    project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .retain(|l| l.envelope().is_none_or(|e| e.target == volume()));
    let mut app = app_with(project);
    let (before, revision) = (lanes(&app), app.project_revision());
    tab(&mut app, EditorTab::Envelopes);
    assert_eq!(choice(&app).tab, EditorTab::Envelopes);
    assert_eq!(choice(&app).one_lane(), Some(&volume()));
    assert_eq!(app.clip_view.editor.envelope_for(key(&app)), Some(volume()));
    assert_eq!((lanes(&app), app.project_revision()), (before, revision));
}

#[test]
fn envelopes_without_any_envelope_shows_the_first_devices_first_control_else_the_mixers() {
    let mut project = mixed_project();
    project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .retain(|l| l.envelope().is_none());
    let mut app = app_with(project.clone());
    tab(&mut app, EditorTab::Envelopes);
    assert_eq!(choice(&app).one_lane(), Some(&argument("opacity", "value")));
    assert_eq!(app.clip_view.editor.envelope_for(key(&app)), None);

    // A Target with no parameters has nothing to automate; the Mixer's Volume is next.
    let target = project.tracks[0].devices[0].target_mut().unwrap();
    target.parameters.clear();
    let mut app = app_with(project);
    tab(&mut app, EditorTab::Envelopes);
    assert_eq!(choice(&app).one_lane(), Some(&volume()));
}

#[test]
fn overview_and_events_share_the_hit_selection_and_keep_every_key() {
    let mut app = app_with(mixed_project());
    let start = key(&app);
    send(&mut app, Inspect::Timeline(Action::Select(start, Some(1))));
    assert_eq!(app.clip_view.editor.selected_for(start), Some(1));
    tab(&mut app, EditorTab::Events);
    assert_eq!(choice(&app).tab, EditorTab::Events);
    assert_eq!(choice(&app).one_lane(), None);
    assert_eq!(key(&app), start, "no gesture or selection goes stale");
    assert_eq!(app.clip_view.editor.selected_for(start), Some(1));
    tab(&mut app, EditorTab::Overview);
    assert_eq!(choice(&app), Choice::default());
    assert_eq!(app.clip_view.editor.selected_for(start), Some(1));
}

#[test]
fn leaving_envelopes_keeps_its_pick_for_next_time_and_starts_a_new_epoch_each_way() {
    let mut app = app_with(mixed_project());
    send(
        &mut app,
        Inspect::ChooseControl(TrackId(1), 0, argument("steps", "value")),
    );
    let on_envelopes = key(&app);
    tab(&mut app, EditorTab::Overview);
    assert_ne!(
        key(&app),
        on_envelopes,
        "a gesture on the lane can't commit"
    );
    assert_eq!(choice(&app).one_lane(), None);
    assert_eq!(choice(&app).argument, Some(argument("steps", "value")));
    let on_overview = key(&app);
    tab(&mut app, EditorTab::Events);
    tab(&mut app, EditorTab::Envelopes);
    assert_ne!(key(&app), on_overview);
    assert_eq!(choice(&app).one_lane(), Some(&argument("steps", "value")));
    assert_eq!(
        app.clip_view.editor.envelope_for(key(&app)),
        Some(argument("steps", "value"))
    );
}

#[test]
fn pressing_the_shown_tab_changes_nothing_and_any_press_closes_an_open_chooser() {
    let mut app = app_with(mixed_project());
    tab(&mut app, EditorTab::Envelopes);
    let shown = (choice(&app), key(&app));
    send(
        &mut app,
        Inspect::OpenChooser(TrackId(1), 0, chooser::Which::Control),
    );
    tab(&mut app, EditorTab::Envelopes);
    assert!(app.overlay.is_none());
    assert_eq!((choice(&app), key(&app)), shown);
}

#[test]
fn show_envelope_from_overview_or_events_lands_on_the_envelopes_tab() {
    for from in [EditorTab::Overview, EditorTab::Events] {
        let mut app = app_with(mixed_project());
        tab(&mut app, from);
        let _ = crate::update::update(&mut app, Root::ShowEnvelope(field_of(&volume())));
        assert_eq!(choice(&app).tab, EditorTab::Envelopes);
        assert_eq!(choice(&app).one_lane(), Some(&volume()));
    }
}

#[test]
fn each_clip_keeps_its_own_tab() {
    let mut app = app_with(mixed_project());
    tab(&mut app, EditorTab::Events);
    let views = &app.clip_view.editor.views;
    assert_eq!(
        views.choice(app.project(), TrackId(1), ClipId(99)).tab,
        EditorTab::Overview,
        "a clip seen for the first time opens on Overview"
    );
    assert_eq!(
        views.choice(app.project(), TrackId(1), clip_id(&app)).tab,
        EditorTab::Events
    );
}

#[test]
fn the_tab_capture_hook_presses_each_tab_through_its_handler() {
    let mut app = app_with(mixed_project());
    for tab in EditorTab::ALL {
        capture(&mut app, tab.name());
        assert_eq!(choice(&app).tab, tab);
    }
    assert_eq!(choice(&app).one_lane(), None);
    capture(&mut app, "tab-envelopes");
    assert_eq!(choice(&app).one_lane(), Some(&argument("opacity", "value")));
}

#[test]
fn the_chooser_capture_hooks_open_each_chooser_on_the_envelopes_tab() {
    for (scenario, which) in [
        ("device-chooser", chooser::Which::Device),
        ("control-chooser", chooser::Which::Control),
    ] {
        let mut app = app_with(mixed_project());
        capture(&mut app, scenario);
        assert_eq!(choice(&app).one_lane(), Some(&argument("opacity", "value")));
        assert!(matches!(app.overlay, Some(Overlay::Chooser(open)) if open.which == which));
    }
}

#[test]
fn each_tab_has_its_own_help() {
    let help: Vec<_> = EditorTab::ALL.iter().map(|t| t.help()).collect();
    assert_eq!(
        help,
        [
            crate::info::Id::EditorTabEvents,
            crate::info::Id::EditorTabEnvelopes,
            crate::info::Id::EditorTabOverview
        ]
    );
    for tab in EditorTab::ALL {
        assert!(tab.help().entry().title.starts_with(tab.label()));
    }
}
