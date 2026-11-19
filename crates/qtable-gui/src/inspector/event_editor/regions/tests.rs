use super::*;
use crate::inspector::event_editor::content::{Item, Move, Selection};
use crate::inspector::lane_model::tests::mixed_project;
use qtable_engine::model::AutomationEnvelope;

fn setup() -> (App, ClipKey) {
    setup_project(mixed_project())
}
fn setup_project(project: qtable_engine::model::Project) -> (App, ClipKey) {
    let mut app = App::new(
        qtable_engine::engine::Engine::new(project).unwrap(),
        "120".into(),
        None,
    );
    app.selection.clip = Some(crate::grid::Selection {
        track: TrackId(1),
        slot: 0,
    });
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    app.chrome.detail_open = true;
    app.focus = crate::focus::Focus::Detail;
    reconcile(&mut app);
    let key = ClipKey {
        track: TrackId(1),
        slot: 0,
        id: app.project().clip(TrackId(1), 0).unwrap().id,
        epoch: app.clip_view.editor.epoch,
    };
    (app, key)
}
fn range(app: &mut App, key: ClipKey, span: Range<BeatTime>) {
    let clip = clip_owned(app, key).unwrap();
    let lanes = lanes::identities(&clip).iter().map(|i| i.owned()).collect();
    content::apply(app, Selection::time(key, span, lanes));
}
fn gesture(app: &App, key: ClipKey, dt: f32, dv: f32, copy: bool) -> Move {
    Move {
        key,
        revision: app.project_revision(),
        from: iced::Point::ORIGIN,
        to: iced::Point::new(dt, dv),
        copy,
        moved: true,
        time_scale: 1.0,
        value_scale: 0.01,
    }
}

#[test]
fn events_paste_from_a_mixed_clipboard_pastes_hits_only() {
    let (mut app, key) = setup();
    range(&mut app, key, BeatTime(480)..BeatTime(2880));
    shortcut(&mut app, Shortcut::Copy);
    let original = clip_owned(&app, key).unwrap();
    crate::inspector::lane_view::choose_tab(
        &mut app,
        key.track,
        key.slot,
        crate::inspector::lane_view::EditorTab::Events,
    );
    let reference = original.events[0].definition.clone();
    content::apply(
        &mut app,
        Selection::time(
            key,
            BeatTime(0)..BeatTime(0),
            vec![lanes::Identity::Event(reference)],
        ),
    );
    shortcut(&mut app, Shortcut::Paste);
    let after = clip_owned(&app, key).unwrap();
    assert_ne!(after.events, original.events);
    assert_eq!(after.lanes, original.lanes);
    assert!(
        app.clip_view
            .editor
            .content
            .as_ref()
            .unwrap()
            .points
            .is_empty()
    );
}
#[test]
fn range_delete_and_cut_keep_edge_points_and_add_none_in_one_revision() {
    for start in [BeatTime::ZERO, BeatTime(960)] {
        for command in [Shortcut::Delete, Shortcut::Cut] {
            let (mut app, key) = setup();
            let before = clip_owned(&app, key).unwrap();
            range(&mut app, key, start..BeatTime(2880));
            shortcut(&mut app, command);
            assert_eq!(app.project_revision(), 1);
            let after = clip_owned(&app, key).unwrap();
            assert!(
                after
                    .events
                    .iter()
                    .all(|e| !(start..BeatTime(2880)).contains(&e.position))
            );
            for old in before.lanes.iter().filter_map(|l| l.envelope()) {
                assert_eq!(
                    points(&after, &old.target),
                    old.points
                        .iter()
                        .filter(|p| p.time <= start || p.time >= BeatTime(2880))
                        .cloned()
                        .collect::<Vec<_>>()
                );
            }
            assert_eq!(
                app.clip_view.editor.clipboard.is_some(),
                command == Shortcut::Cut
            );
        }
    }
}
#[test]
fn duplicate_overwrites_following_range_and_selects_copy_without_changing_clipboard() {
    let (mut app, key) = setup();
    range(&mut app, key, BeatTime(960)..BeatTime(1920));
    let selection = app.clip_view.editor.content.clone().unwrap();
    let before = clip_owned(&app, key).unwrap();
    shortcut(&mut app, Shortcut::Duplicate);
    assert_eq!(app.project_revision(), 1);
    let after = clip_owned(&app, key).unwrap();
    assert_eq!(
        after
            .events
            .iter()
            .filter(|e| (BeatTime(1920)..BeatTime(2880)).contains(&e.position))
            .map(|e| (&e.definition, e.position, e.arguments.clone()))
            .collect::<Vec<_>>(),
        selection
            .hits
            .iter()
            .map(|i| {
                let e = &before.events[*i];
                (
                    &e.definition,
                    BeatTime(e.position.ticks() + 960),
                    e.arguments.clone(),
                )
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().range,
        BeatTime(1920)..BeatTime(2880)
    );
    assert!(app.clip_view.editor.clipboard.is_none());
}
#[test]
fn duplicate_and_paste_over_a_jump_pair_keep_the_arriving_and_incoming_values() {
    for command in [Shortcut::Duplicate, Shortcut::Paste] {
        let (mut app, key) = setup();
        range(&mut app, key, BeatTime::ZERO..BeatTime(1920));
        if command == Shortcut::Paste {
            shortcut(&mut app, Shortcut::Copy);
            range(&mut app, key, BeatTime(1920)..BeatTime(1920));
        }
        shortcut(&mut app, command);
        assert_eq!(app.project_revision(), 1, "{command:?}");
        let after = clip_owned(&app, key).unwrap();
        after.validate().unwrap();
        let mode = after
            .lanes
            .iter()
            .filter_map(|l| l.envelope())
            .find(|e| e.target.parameter.0 == "mode")
            .unwrap();
        assert_eq!(
            mode.points
                .iter()
                .filter(|p| p.time == BeatTime(1920))
                .map(|p| p.value)
                .collect::<Vec<_>>(),
            vec![2.0, 0.0]
        );
        assert_eq!(
            app.clip_view.editor.content.as_ref().unwrap().range,
            BeatTime(1920)..BeatTime(3840)
        );
    }
}

#[test]
fn repeated_duplicate_on_volume_collapses_equal_value_pairs() {
    let (mut app, key) = setup();
    let clip = clip_owned(&app, key).unwrap();
    let target = clip
        .lanes
        .iter()
        .filter_map(|l| l.envelope())
        .find(|e| e.target.device == qtable_engine::model::DeviceRef::Mixer)
        .unwrap()
        .target
        .clone();
    content::apply(
        &mut app,
        Selection::time(
            key,
            BeatTime::ZERO..BeatTime(960),
            vec![lanes::Identity::Envelope(target.clone())],
        ),
    );
    for revision in 1..=4 {
        shortcut(&mut app, Shortcut::Duplicate);
        assert_eq!(app.project_revision(), revision);
        let clip = clip_owned(&app, key).unwrap();
        let points = points(&clip, &target);
        clip.validate().unwrap();
        assert!(
            points
                .windows(2)
                .all(|w| w[0].time != w[1].time || w[0].value != w[1].value)
        );
        if revision == 4 {
            // Each copy overwrites what follows and steps back at its end, as Live's ⌘D does:
            // the flat 0.7 runs on to 4800, then jumps back to the held 0.9.
            let at = |time| {
                points
                    .iter()
                    .filter(|p| p.time == BeatTime(time))
                    .map(|p| p.value)
                    .collect::<Vec<_>>()
            };
            assert_eq!(at(3840), vec![0.7]);
            assert_eq!(at(4800), vec![0.7, 0.9]);
        }
        assert_eq!(
            app.clip_view.editor.content.as_ref().unwrap().range,
            BeatTime(revision * 960)..BeatTime((revision + 1) * 960)
        );
    }
}

#[test]
fn one_lane_envelope_copy_pastes_onto_parameter_in_view() {
    let (mut app, key) = setup();
    let clip = clip_owned(&app, key).unwrap();
    let targets: Vec<_> = clip
        .lanes
        .iter()
        .filter_map(|l| l.envelope())
        .map(|e| e.target.clone())
        .collect();
    let source = targets[0].clone();
    // Position y has an empty, continuous 0..100 envelope.
    let destination = targets[4].clone();
    content::apply(
        &mut app,
        Selection::time(
            key,
            BeatTime(960)..BeatTime(2880),
            vec![lanes::Identity::Envelope(source.clone())],
        ),
    );
    shortcut(&mut app, Shortcut::Copy);
    crate::inspector::lane_view::choose_control(&mut app, key.track, key.slot, destination.clone());
    let key = ClipKey {
        epoch: app.clip_view.editor.epoch,
        ..key
    };
    content::apply(
        &mut app,
        Selection::time(
            key,
            BeatTime(3840)..BeatTime(3840),
            vec![lanes::Identity::Envelope(destination.clone())],
        ),
    );
    shortcut(&mut app, Shortcut::Paste);
    let after = clip_owned(&app, key).unwrap();
    assert_eq!(points(&after, &source), points(&clip, &source));
    assert_eq!(
        points(&after, &destination)
            .iter()
            .map(|p| p.time)
            .collect::<Vec<_>>(),
        // Besides its two points, the copy carries the line's values at the range edges (Live).
        vec![
            BeatTime(3840),
            BeatTime(4080),
            BeatTime(5280),
            BeatTime(5760)
        ]
    );
    assert_eq!(app.project_revision(), 1);
}
#[test]
fn picked_delete_removes_only_one_equal_time_point_and_one_hit() {
    let (mut app, key) = setup();
    let clip = clip_owned(&app, key).unwrap();
    let picks: Vec<Item> = Selection::items(&clip)
        .into_iter()
        .map(|(i, _)| i)
        .filter(|i| {
            matches!(&i.lane,lanes::Identity::Envelope(t) if t.parameter.0=="mode")
                && i.time == BeatTime(1920)
                && i.ordinal == 1
        })
        .collect();
    assert_eq!(picks.len(), 1);
    let target = match &picks[0].lane {
        lanes::Identity::Envelope(t) => t.clone(),
        _ => unreachable!(),
    };
    let mut selection = Selection::time(key, BeatTime(1920)..BeatTime(1921), vec![]);
    selection.picks = Some(picks);
    content::apply(&mut app, selection);
    shortcut(&mut app, Shortcut::Delete);
    let after = clip_owned(&app, key).unwrap();
    assert_eq!(after.events, clip.events);
    assert_eq!(
        points(&after, &target)
            .iter()
            .filter(|p| p.time == BeatTime(1920))
            .map(|p| p.value)
            .collect::<Vec<_>>(),
        vec![2.0]
    );
    assert_eq!(app.project_revision(), 1);
}
#[test]
fn mixed_move_preview_matches_release_preserves_gap_and_is_one_edit() {
    let (mut app, key) = setup();
    range(&mut app, key, BeatTime(960)..BeatTime(1500));
    let clip = clip_owned(&app, key).unwrap();
    let selection = app.clip_view.editor.content.clone().unwrap();
    let model = LaneModel::build(app.project(), key.track, &clip);
    let drag = gesture(&app, key, 1920.0, -10.0, false);
    let (preview, _, _) = super::preview(&model, &clip, &selection, &drag, Grid::Off, false);
    assert_eq!(app.project_revision(), 0);
    app.clip_view.editor.grid = Grid::Off;
    commit_move(&mut app, drag);
    assert_eq!(clip_owned(&app, key).unwrap(), preview);
    assert_eq!(app.project_revision(), 1);
    // Untouched hits between source and destination survive the union write.
    for e in clip
        .events
        .iter()
        .filter(|e| e.position >= BeatTime(1500) && e.position < BeatTime(2880))
    {
        assert!(preview.events.contains(e));
    }
}
#[test]
fn alt_drag_keeps_source_and_leftward_motion_clamps_selection_as_a_unit() {
    let (mut app, key) = setup();
    range(&mut app, key, BeatTime(960)..BeatTime(1500));
    let before = clip_owned(&app, key).unwrap();
    let drag = gesture(&app, key, 1920.0, 0.0, true);
    commit_move(&mut app, drag);
    let after = clip_owned(&app, key).unwrap();
    for e in before
        .events
        .iter()
        .filter(|e| (BeatTime(960)..BeatTime(1500)).contains(&e.position))
    {
        assert!(after.events.contains(e));
    }
    let selection = app.clip_view.editor.content.as_ref().unwrap();
    let drag = gesture(&app, key, -99999.0, 0.0, false);
    assert_eq!(deltas(selection, &drag, Grid::Sixteenth, false).0, -2880);
}
#[test]
fn invalid_destination_is_atomic_and_reports_only_status() {
    let (mut app, key) = setup();
    range(&mut app, key, BeatTime(960)..BeatTime(2880));
    shortcut(&mut app, Shortcut::Copy);
    // Invalid identity cannot create a new event definition through ReplaceRange.
    let clipboard = app.clip_view.editor.clipboard.as_mut().unwrap();
    for (key, data) in &mut clipboard.lanes {
        if let (LaneKey::Events(r), LaneContent::Hits(hits)) = (key, data) {
            r.key = qtable_engine::model::EntryKey("gone".into());
            for h in hits {
                h.definition = r.clone();
            }
        }
    }
    let before = app.project().clone();
    let selection = app.clip_view.editor.content.clone();
    shortcut(&mut app, Shortcut::Paste);
    assert_eq!(app.project(), &before);
    assert_eq!(app.clip_view.editor.content, selection);
    assert!(app.status.message().is_some());
}

#[test]
fn picked_zero_point_delete_and_cut_succeed_in_one_mixed_transaction() {
    for command in [Shortcut::Delete, Shortcut::Cut] {
        let (mut app, key) = setup();
        let clip = clip_owned(&app, key).unwrap();
        let mut picks: Vec<_> = Selection::items(&clip)
            .into_iter()
            .map(|(i, _)| i)
            .filter(|i| matches!(i.lane, lanes::Identity::Envelope(_)) && i.time == BeatTime::ZERO)
            .take(1)
            .collect();
        let target = match &picks[0].lane {
            lanes::Identity::Envelope(t) => t.clone(),
            _ => unreachable!(),
        };
        picks.push(
            Selection::items(&clip)
                .into_iter()
                .map(|(i, _)| i)
                .find(|i| matches!(i.lane, lanes::Identity::Event(_)))
                .unwrap(),
        );
        let mut selection = Selection::time(key, BeatTime::ZERO..BeatTime(1), vec![]);
        selection.picks = Some(picks);
        content::apply(&mut app, selection);
        let selection = app.clip_view.editor.content.clone().unwrap();
        let copied = copy(&clip, &selection, app.clip_view.editor.grid.ticks(), |_| {
            qtable_engine::model::Axis::Continuous
        });
        shortcut(&mut app, command);
        assert_eq!(app.project_revision(), 1);
        let mut expected = clip.clone();
        expected.events.remove(selection.hits[0]);
        let envelope = expected
            .lanes
            .iter_mut()
            .find_map(|l| match l {
                qtable_engine::model::Lane::Envelope(e) if e.target == target => Some(e),
                _ => None,
            })
            .unwrap();
        envelope.points.remove(0);
        assert_eq!(clip_owned(&app, key).unwrap(), expected);
        assert_eq!(
            app.clip_view.editor.clipboard.as_ref(),
            (command == Shortcut::Cut).then_some(&copied)
        );
        assert_eq!(
            app.clip_view.editor.content.as_ref().unwrap().insert_mark(),
            Some(BeatTime::ZERO)
        );
    }
}

#[test]
fn picked_zero_point_move_preview_matches_release_and_preserves_other_content() {
    // Move away from zero, back to zero, vertically at zero, and copy from zero.
    for (from, dt, dv, copy) in [
        (0, 960.0, 0.0, false),
        (1200, -1200.0, 0.0, false),
        (0, 0.0, -10.0, false),
        (0, 960.0, 0.0, true),
    ] {
        let (mut app, key) = setup();
        let clip = clip_owned(&app, key).unwrap();
        let item = Selection::items(&clip)
            .into_iter()
            .map(|(i, _)| i)
            .find(|i| {
                matches!(&i.lane, lanes::Identity::Envelope(t) if t.parameter.0 == "opacity")
                    && i.time == BeatTime(from)
            })
            .unwrap();
        let target = match &item.lane {
            lanes::Identity::Envelope(t) => t.clone(),
            _ => unreachable!(),
        };
        let mut selection = Selection::time(key, item.time..BeatTime(from + 1), vec![]);
        selection.picks = Some(vec![item]);
        content::apply(&mut app, selection);
        let selection = app.clip_view.editor.content.clone().unwrap();
        let model = LaneModel::build(app.project(), key.track, &clip);
        let drag = gesture(&app, key, dt, dv, copy);
        let (preview, _, next) = super::preview(&model, &clip, &selection, &drag, Grid::Off, false);
        app.clip_view.editor.grid = Grid::Off;
        commit_move(&mut app, drag);
        assert_eq!(app.project_revision(), 1);
        assert_eq!(clip_owned(&app, key).unwrap(), preview);
        assert_eq!(app.clip_view.editor.content.as_ref().unwrap(), &next);
        assert_eq!(next.count(), 1);
        assert_eq!(next.range.start, BeatTime((from as i64 + dt as i64) as u64));
        let mut expected = points(&clip, &target);
        let mut moved = expected
            .iter()
            .find(|p| p.time == BeatTime(from))
            .unwrap()
            .clone();
        if !copy {
            expected.retain(|p| p.time != moved.time);
        }
        moved.time = next.range.start;
        moved.value -= f64::from(dv) * 0.01;
        expected.push(moved);
        expected.sort_by_key(|p| p.time);
        assert_eq!(points(&preview, &target), expected);
        assert_eq!(preview.events, clip.events);
        for e in clip
            .lanes
            .iter()
            .filter_map(|l| l.envelope())
            .filter(|e| e.target != target)
        {
            assert_eq!(points(&preview, &e.target), e.points);
        }
    }
}

#[test]
fn moving_second_equal_time_pick_selects_its_new_ordinal() {
    let (mut app, key) = setup();
    let clip = clip_owned(&app, key).unwrap();
    let item = Selection::items(&clip)
        .into_iter()
        .map(|(i, _)| i)
        .find(|i| {
            matches!(&i.lane,lanes::Identity::Envelope(t) if t.parameter.0=="mode")
                && i.time == BeatTime(1920)
                && i.ordinal == 1
        })
        .unwrap();
    let mut selection = Selection::time(key, item.time..BeatTime(1921), vec![]);
    selection.picks = Some(vec![item]);
    content::apply(&mut app, selection);
    let drag = gesture(&app, key, 480.0, 0.0, false);
    commit_move(&mut app, drag);
    let selection = app.clip_view.editor.content.as_ref().unwrap();
    assert_eq!(selection.count(), 1);
    assert_eq!(selection.picks.as_ref().unwrap()[0].ordinal, 0);
    assert_eq!(selection.range.start, BeatTime(2400));
}

#[test]
fn hit_menu_keeps_the_selected_region_for_copy_and_delete() {
    let (mut app, key) = setup();
    range(&mut app, key, BeatTime(960)..BeatTime(2880));
    let selection = app.clip_view.editor.content.clone().unwrap();
    let clip = clip_owned(&app, key).unwrap();
    let click = menu::Click {
        key,
        revision: app.project_revision(),
        lane: lanes::Identity::Event(clip.events[selection.hits[0]].definition.clone()),
        under: menu::Under::Hit(selection.hits[0]),
    };
    let open = |app: &mut App| {
        let target = menu::open(app, click.clone()).unwrap();
        assert!(app.overlay.is_some());
        target
    };
    let target = open(&mut app);
    assert_eq!(app.clip_view.editor.content, Some(selection.clone()));
    assert_eq!(target.clicked, menu::Clicked::TimeSelection);
    menu::command(&mut app, target, menu::ContentCommand::Key(Shortcut::Copy));
    assert_eq!(
        app.clip_view.editor.clipboard.as_ref().unwrap().lanes.len(),
        selection.lanes.len()
    );
    let target = open(&mut app);
    menu::command(
        &mut app,
        target,
        menu::ContentCommand::Key(Shortcut::Delete),
    );
    assert_eq!(app.project_revision(), 1);
    let clip = clip_owned(&app, key).unwrap();
    assert!(
        clip.events
            .iter()
            .all(|e| !selection.range.contains(&e.position))
    );
}

#[test]
fn all_lanes_paste_filters_unlinked_destinations_and_reports_when_none_remain() {
    let mut project = mixed_project();
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    let region = clip.region();
    let envelope = clip
        .lanes
        .iter_mut()
        .find_map(|l| match l {
            qtable_engine::model::Lane::Envelope(e) => Some(e),
            _ => None,
        })
        .unwrap();
    envelope.region = Some(region);
    let target = envelope.target.clone();
    let (mut app, key) = setup_project(project);
    crate::inspector::lane_view::choose_control(&mut app, key.track, key.slot, target.clone());
    let key = ClipKey {
        epoch: app.clip_view.editor.epoch,
        ..key
    };
    content::apply(
        &mut app,
        Selection::time(
            key,
            BeatTime::ZERO..BeatTime(960),
            vec![lanes::Identity::Envelope(target.clone())],
        ),
    );
    shortcut(&mut app, Shortcut::Copy);
    let copied = app.clip_view.editor.clipboard.clone().unwrap();
    assert_eq!(copied.lanes.len(), 1);
    crate::inspector::lane_view::choose_tab(
        &mut app,
        key.track,
        key.slot,
        crate::inspector::lane_view::EditorTab::Overview,
    );
    let key = ClipKey {
        epoch: app.clip_view.editor.epoch,
        ..key
    };
    range(&mut app, key, BeatTime(2880)..BeatTime(2880));
    let before = app.project().clone();
    let selection = app.clip_view.editor.content.clone();
    shortcut(&mut app, Shortcut::Paste);
    assert_eq!(app.project(), &before);
    assert_eq!(app.project_revision(), 0);
    assert_eq!(app.clip_view.editor.content, selection);
    assert_eq!(app.clip_view.editor.clipboard, Some(copied.clone()));
    assert_eq!(
        app.status.message(),
        Some("Unlinked lanes can only be pasted in their One lane view")
    );
    assert_eq!(app.status.severity(), crate::status::Severity::Notice);

    // A mixed clipboard still pastes its editable lanes, without touching the unlinked one.
    let clip = clip_owned(&app, key).unwrap();
    range(&mut app, key, BeatTime::ZERO..BeatTime(960));
    shortcut(&mut app, Shortcut::Copy);
    app.clip_view
        .editor
        .clipboard
        .as_mut()
        .unwrap()
        .lanes
        .extend(copied.lanes);
    range(&mut app, key, BeatTime(2880)..BeatTime(2880));
    shortcut(&mut app, Shortcut::Paste);
    assert_eq!(app.project_revision(), 1);
    assert_eq!(
        points(&clip_owned(&app, key).unwrap(), &target),
        points(&clip, &target)
    );
    assert!(
        !app.clip_view
            .editor
            .content
            .as_ref()
            .unwrap()
            .lanes
            .contains(&lanes::Identity::Envelope(target))
    );
}

/// The mixed project with Volume as `points` (time, value, curve), and that lane alone in a
/// time selection over `span`.
fn volume_ramp(points: &[(u64, f64, f32)], span: Range<BeatTime>) -> (App, ClipKey, ArgumentRef) {
    let mut project = mixed_project();
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    let volume = clip
        .lanes
        .iter_mut()
        .filter_map(|l| match l {
            qtable_engine::model::Lane::Envelope(e) => Some(e),
            _ => None,
        })
        .find(|e| e.target.device == qtable_engine::model::DeviceRef::Mixer)
        .unwrap();
    volume.points = points
        .iter()
        .map(|&(t, value, curve)| Breakpoint {
            time: BeatTime(t),
            value,
            curve,
        })
        .collect();
    let target = volume.target.clone();
    let (mut app, key) = setup_project(project);
    content::apply(
        &mut app,
        Selection::time(key, span, vec![lanes::Identity::Envelope(target.clone())]),
    );
    (app, key, target)
}
fn listed(app: &App, key: ClipKey, target: &ArgumentRef) -> Vec<(u64, f64)> {
    points(&clip_owned(app, key).unwrap(), target)
        .iter()
        .map(|p| (p.time.ticks(), (p.value * 1e6).round() / 1e6))
        .collect()
}

#[test]
fn a_range_between_points_copies_the_line_and_repeated_duplicate_builds_a_sawtooth() {
    // Live 12 (recording): ⌘D on a 16th in
    // the middle of a ramp inserts a copy rising from the curve's value at the selection start
    // to its value at the end, then steps back to the curve.
    let (mut app, key, volume) = volume_ramp(&[(0, 0.0, 0.0), (3840, 1.0, 0.0)], {
        BeatTime(960)..BeatTime(1200)
    });
    shortcut(&mut app, Shortcut::Copy);
    let copied = app.clip_view.editor.clipboard.clone().unwrap();
    assert_eq!(
        copied.lanes[0].1,
        LaneContent::Points(vec![
            Breakpoint {
                time: BeatTime(0),
                value: 0.25,
                curve: 0.0
            },
            Breakpoint {
                time: BeatTime(240),
                value: 0.3125,
                curve: 0.0
            },
        ])
    );
    for revision in 1..=3 {
        shortcut(&mut app, Shortcut::Duplicate);
        assert_eq!(app.project_revision(), revision);
        clip_owned(&app, key).unwrap().validate().unwrap();
    }
    assert_eq!(
        listed(&app, key, &volume),
        [
            (0, 0.0),
            (1200, 0.3125),
            (1200, 0.25),
            (1440, 0.3125),
            (1440, 0.25),
            (1680, 0.3125),
            (1680, 0.25),
            (1920, 0.3125),
            (1920, 0.5),
            (3840, 1.0),
        ]
    );
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().range,
        BeatTime(1680)..BeatTime(1920)
    );
}

#[test]
fn a_curved_segment_cut_by_the_range_keeps_its_shape_on_both_sides() {
    let (mut app, key, volume) = volume_ramp(
        &[(0, 0.0, 0.5), (3840, 1.0, 0.0)],
        BeatTime(960)..BeatTime(1920),
    );
    let before = AutomationEnvelope {
        region: None,
        target: volume.clone(),
        points: points(&clip_owned(&app, key).unwrap(), &volume),
    };
    shortcut(&mut app, Shortcut::Copy);
    content::apply(
        &mut app,
        Selection::time(
            key,
            BeatTime(2880)..BeatTime(2880),
            vec![lanes::Identity::Envelope(volume.clone())],
        ),
    );
    shortcut(&mut app, Shortcut::Paste);
    let after = AutomationEnvelope {
        region: None,
        target: volume.clone(),
        points: points(&clip_owned(&app, key).unwrap(), &volume),
    };
    let value = |e: &AutomationEnvelope, t: u64| e.value_at(t as f64, Axis::Continuous).unwrap();
    // Before the paste the curve is untouched: its first point still shapes it exactly.
    for t in (0..2880).step_by(120) {
        assert!((value(&after, t) - value(&before, t)).abs() < 1e-12, "{t}");
    }
    // The pasted copy follows the copied stretch of the curve (a fitted split, within 3%).
    for x in (0..960).step_by(60) {
        let error = (value(&after, 2880 + x) - value(&before, 960 + x)).abs();
        assert!(error < 0.03, "{x}: {error}");
    }
    // It ends on the copied end value, then steps back to the lane's own end point.
    let at_end: Vec<_> = after
        .points
        .iter()
        .filter(|p| p.time == BeatTime(3840))
        .map(|p| p.value)
        .collect();
    assert_eq!(at_end, vec![value(&before, 1920), 1.0]);
}

#[test]
fn a_range_paste_puts_back_hits_and_points_at_its_end() {
    // The write reaches one tick past the paste end; whatever stood there stays.
    let (mut app, key) = setup();
    let before = clip_owned(&app, key).unwrap();
    range(&mut app, key, BeatTime::ZERO..BeatTime(960));
    shortcut(&mut app, Shortcut::Copy);
    let lanes = lanes::identities(&before)
        .iter()
        .map(|i| i.owned())
        .collect();
    content::apply(
        &mut app,
        Selection::time(key, BeatTime(960)..BeatTime(960), lanes),
    );
    shortcut(&mut app, Shortcut::Paste);
    let after = clip_owned(&app, key).unwrap();
    after.validate().unwrap();
    let at = |clip: &Clip| {
        clip.events
            .iter()
            .filter(|e| e.position == BeatTime(1920))
            .count()
    };
    assert_eq!(at(&after), at(&before));
    // Mode's jump pair at 1920 survives whole.
    let mode = |clip: &Clip| {
        clip.lanes
            .iter()
            .filter_map(|l| l.envelope())
            .find(|e| e.target.parameter.0 == "mode")
            .unwrap()
            .points
            .iter()
            .filter(|p| p.time == BeatTime(1920))
            .map(|p| p.value)
            .collect::<Vec<_>>()
    };
    assert_eq!(mode(&after).last(), mode(&before).last());
}
