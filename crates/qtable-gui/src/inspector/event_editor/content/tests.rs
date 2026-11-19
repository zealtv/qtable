use super::*;
use crate::inspector::lane_model::tests::mixed_project;
use crate::messages::Message as Root;
use iced::keyboard::{Key, Modifiers, key::Named};

fn setup() -> (App, ClipKey) {
    let engine = qtable_engine::engine::Engine::new(mixed_project()).unwrap();
    let mut app = App::new(engine, "120".into(), None);
    app.selection.clip = Some(crate::grid::Selection {
        track: TrackId(1),
        slot: 0,
    });
    app.chrome.detail_open = true;
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    reconcile(&mut app);
    let key = ClipKey {
        track: TrackId(1),
        slot: 0,
        id: app.project().clip(TrackId(1), 0).unwrap().id,
        epoch: app.clip_view.editor.epoch,
    };
    (app, key)
}
fn all(app: &App, key: ClipKey, range: Range<BeatTime>) -> Selection {
    let clip = app.project().clip(key.track, key.slot).unwrap();
    Selection::time(
        key,
        range,
        lanes::identities(clip).iter().map(|i| i.owned()).collect(),
    )
}
fn send(app: &mut App, action: Action) {
    let _ = crate::update::update(app, Root::Inspector(Message::Timeline(action)));
}
fn keyboard_event(app: &mut App, key: Key, modifiers: Modifiers, ignored: bool) {
    let _ = crate::update::update(
        app,
        Root::KeyPressed {
            key,
            modifiers,
            repeat: false,
            ignored,
        },
    );
}

fn events_tab(app: &mut App) {
    crate::inspector::lane_view::choose_tab(
        app,
        TrackId(1),
        0,
        crate::inspector::lane_view::EditorTab::Events,
    );
}

#[test]
fn events_command_a_and_delete_leave_envelopes_untouched() {
    let (mut app, key) = setup();
    let original = clip_owned(&app, key).unwrap();
    events_tab(&mut app);
    send(&mut app, Action::Select(key, Some(0)));
    app.focus = crate::focus::Focus::Detail;
    keyboard_event(
        &mut app,
        Key::Character("a".into()),
        Modifiers::COMMAND,
        true,
    );
    let selected = app.clip_view.editor.content.as_ref().unwrap();
    assert_eq!(selected.hits.len(), original.events.len());
    assert!(selected.points.is_empty());
    assert!(
        selected
            .lanes
            .iter()
            .all(|id| matches!(id, lanes::Identity::Event(_)))
    );
    super::super::shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    let after = clip_owned(&app, key).unwrap();
    assert!(after.events.is_empty());
    assert_eq!(after.lanes, original.lanes);
}

#[test]
fn entering_events_keeps_hits_in_a_mixed_selection_and_drops_hidden_points() {
    let (mut app, key) = setup();
    let selection = all(&app, key, BeatTime(480)..BeatTime(2880));
    send(&mut app, Action::Content(selection));
    assert!(
        !app.clip_view
            .editor
            .content
            .as_ref()
            .unwrap()
            .points
            .is_empty()
    );
    let hits = app.clip_view.editor.content.as_ref().unwrap().hits.clone();
    events_tab(&mut app);
    let selected = app.clip_view.editor.content.as_ref().unwrap();
    assert_eq!(selected.hits, hits);
    assert!(selected.points.is_empty());
    assert_eq!(selected.range, BeatTime(480)..BeatTime(2880));
    assert!(app.clip_view.editor.point.is_none());
    let envelopes = clip_owned(&app, key).unwrap().lanes;
    super::super::shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    assert_eq!(clip_owned(&app, key).unwrap().lanes, envelopes);
}

#[test]
fn events_drops_an_envelope_only_pick_and_cursor_uses_the_first_visible_lane() {
    let (mut app, key) = setup();
    let target = match all(&app, key, BeatTime(0)..BeatTime(3840)).lanes[1].clone() {
        lanes::Identity::Envelope(t) => t,
        _ => panic!("envelope"),
    };
    pick(
        &mut app,
        key,
        Item {
            lane: lanes::Identity::Envelope(target),
            time: BeatTime(0),
            ordinal: 0,
        },
        false,
    );
    assert!(app.clip_view.editor.point.is_some());
    events_tab(&mut app);
    assert!(app.clip_view.editor.content.is_none());
    assert!(app.clip_view.editor.point.is_none());
    app.focus = crate::focus::Focus::Detail;
    keyboard_event(
        &mut app,
        Key::Named(Named::ArrowRight),
        Modifiers::empty(),
        true,
    );
    let selection = app.clip_view.editor.content.as_ref().unwrap();
    assert_eq!(selection.lanes.len(), 1);
    assert!(matches!(selection.lanes[0], lanes::Identity::Event(_)));
}

#[test]
fn range_and_headers_are_exclusive_and_the_empty_range_is_an_insert_marker() {
    let (mut app, key) = setup();
    send(&mut app, Action::Header(key, 0));
    assert!(app.clip_view.editor.lanes.as_ref().unwrap().headers);
    let s = all(&app, key, BeatTime(960)..BeatTime(2880));
    send(&mut app, Action::Content(s));
    let editor = &app.clip_view.editor;
    let content = editor.content.as_ref().unwrap();
    assert_eq!(content.hits, vec![0, 1]);
    assert!(content.points.len() > 2);
    assert!(
        content
            .summary(app.project().time_signature)
            .unwrap()
            .starts_with("2 hits, ")
    );
    assert!(editor.selected.is_none() && editor.point.is_none() && editor.lanes.is_none());
    send(&mut app, Action::Header(key, 1));
    assert!(app.clip_view.editor.content.is_none());
    let lane = app.clip_view.editor.lanes.as_ref().unwrap().items[0].clone();
    send(
        &mut app,
        Action::Content(Selection::time(
            key,
            BeatTime(1440)..BeatTime(1440),
            vec![lane],
        )),
    );
    let content = app.clip_view.editor.content.as_ref().unwrap();
    assert!(content.range.is_empty());
    assert_eq!(content.count(), 0);
    assert_eq!(
        app.clip_view.editor.content.as_ref().map(|s| s.range.start),
        Some(BeatTime(1440))
    );
    assert!(app.clip_view.editor.lanes.is_none());
}

#[test]
fn shift_picks_mix_hits_and_points_and_toggle_without_editing() {
    let (mut app, key) = setup();
    let revision = app.project_revision();
    send(&mut app, Action::Select(key, Some(0)));
    app.clip_view.editor.shift_down = true;
    send(&mut app, Action::Select(key, Some(2)));
    let point = Selection::items(app.project().clip(key.track, 0).unwrap())
        .into_iter()
        .find(|(i, _)| matches!(i.lane, lanes::Identity::Envelope(_)))
        .unwrap()
        .0;
    let lanes::Identity::Envelope(target) = point.lane else {
        unreachable!()
    };
    let target = PointTarget {
        key,
        revision,
        target,
    };
    send(&mut app, Action::PickPoint(target.clone(), 0));
    assert_eq!(app.clip_view.editor.content.as_ref().unwrap().count(), 3);
    assert!(app.clip_view.editor.point.is_none() && app.clip_view.editor.selected.is_none());
    send(&mut app, Action::Select(key, Some(0)));
    assert_eq!(app.clip_view.editor.content.as_ref().unwrap().count(), 2);
    send(&mut app, Action::PickPoint(target, 0));
    assert_eq!(app.clip_view.editor.selected_for(key), Some(2));
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn range_items_resolve_again_after_index_and_lane_order_changes() {
    let (mut app, key) = setup();
    let s = all(&app, key, BeatTime(960)..BeatTime(2880));
    send(&mut app, Action::Content(s));
    let original = app.clip_view.editor.content.clone().unwrap();
    run_edit(
        &mut app,
        Command::Edit(ProjectEdit::RemoveEvent {
            track: key.track,
            slot: 0,
            event: 0,
        }),
    );
    reconcile(&mut app);
    let selected = app.clip_view.editor.content.as_ref().unwrap();
    assert_eq!(selected.range, original.range);
    assert_eq!(selected.hits, vec![0]);
    let count = app.project().clip(key.track, 0).unwrap().lanes.len();
    run_edit(
        &mut app,
        Command::Edit(ProjectEdit::EditLane {
            track: key.track,
            slot: 0,
            edit: LaneEdit::Move {
                from: 0..1,
                to: count - 1,
            },
        }),
    );
    reconcile(&mut app);
    let selected = app.clip_view.editor.content.as_ref().unwrap();
    assert_eq!(selected.lanes, original.lanes);
    assert_eq!(selected.hits, vec![0]);
}

#[test]
fn actual_command_a_and_escape_route_only_while_the_editor_owns_keys() {
    let (mut app, key) = setup();
    send(&mut app, Action::Select(key, Some(0)));
    app.focus = crate::focus::Focus::Detail;
    keyboard_event(
        &mut app,
        Key::Character("a".into()),
        Modifiers::COMMAND,
        true,
    );
    let content = app.clip_view.editor.content.as_ref().unwrap();
    assert_eq!(content.hits.len(), 3);
    // Includes a point exactly at the clip's end.
    assert!(content.points.iter().any(|(target, index)| {
        app.project()
            .clip(key.track, 0)
            .unwrap()
            .lanes
            .iter()
            .filter_map(|l| l.envelope())
            .find(|e| e.target == *target)
            .unwrap()
            .points[*index]
            .time
            == BeatTime(3840)
    }));
    let before = content.clone();
    keyboard_event(
        &mut app,
        Key::Character("a".into()),
        Modifiers::COMMAND,
        false,
    );
    assert_eq!(app.clip_view.editor.content.as_ref(), Some(&before));
    keyboard_event(&mut app, Key::Named(Named::Escape), Modifiers::NONE, true);
    assert!(app.clip_view.editor.content.is_none() && app.clip_view.editor.lanes.is_none());
    assert!(
        app.clip_view
            .editor
            .content
            .as_ref()
            .map(|s| s.range.start)
            .is_none()
    );
}

#[test]
fn region_delete_is_one_edit_and_stale_moves_are_rejected() {
    let (mut app, key) = setup();
    let s = all(&app, key, BeatTime(960)..BeatTime(2880));
    send(&mut app, Action::Content(s));
    let revision = app.project_revision();
    super::super::shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    assert_eq!(app.project_revision(), revision + 1);
    let before = app.project().clone();
    send(
        &mut app,
        Action::RegionMove(Move {
            key,
            revision,
            from: iced::Point::ORIGIN,
            to: iced::Point::new(30.0, 5.0),
            copy: true,
            moved: true,
            time_scale: 1.0,
            value_scale: 0.0,
        }),
    );
    assert_eq!(app.project(), &before);
}

#[test]
fn background_drag_keeps_crossed_lanes_after_doubling_back_and_uses_strict_threshold() {
    let (app, key) = setup();
    let lanes = all(&app, key, BeatTime(0)..BeatTime(1)).lanes;
    let mut drag = Drag::new(key, iced::Point::ORIGIN, BeatTime(480), lanes[0].clone());
    assert!(
        drag.update(iced::Point::new(3.0, 0.0), BeatTime(960), vec![])
            .range
            .is_empty()
    );
    let s = drag.update(
        iced::Point::new(30.0, 50.0),
        BeatTime(1440),
        lanes[..3].to_vec(),
    );
    assert_eq!(s.range, BeatTime(480)..BeatTime(1440));
    let s = drag.update(
        iced::Point::new(-10.0, 0.0),
        BeatTime(240),
        vec![lanes[0].clone()],
    );
    assert_eq!(s.range, BeatTime(240)..BeatTime(480));
    assert_eq!(s.lanes, lanes[..3]);
}

#[test]
fn selecting_a_different_point_drops_the_previous_points_draft_and_error() {
    let (mut app, key) = setup();
    let item = Selection::items(app.project().clip(key.track, 0).unwrap())
        .into_iter()
        .find(|(i, _)| matches!(i.lane, lanes::Identity::Envelope(_)))
        .unwrap()
        .0;
    let lanes::Identity::Envelope(target) = item.lane else {
        unreachable!()
    };
    let target = PointTarget {
        key,
        revision: app.project_revision(),
        target,
    };
    send(&mut app, Action::PickPoint(target.clone(), 0));
    let draft = DraftKey::clip(key.track, key.slot, ClipField::PointValue);
    app.drafts.values.insert(draft, "bad value".into());
    app.drafts.errors.insert(draft, "Refused".into());
    send(&mut app, Action::PickPoint(target, 1));
    assert_eq!(app.clip_view.editor.point_for(key), Some(1));
    assert!(!app.drafts.values.contains_key(&draft));
    assert!(!app.drafts.errors.contains_key(&draft));
}

#[test]
fn a_point_edit_inside_the_range_keeps_the_range_and_resolves_its_members() {
    let (mut app, key) = setup();
    let selection = all(&app, key, BeatTime(960)..BeatTime(2880));
    send(&mut app, Action::Content(selection));
    let selection = app.clip_view.editor.content.clone().unwrap();
    let target = selection.points[0].0.clone();
    let index = selection.points[0].1;
    let mut points = app
        .project()
        .clip(key.track, 0)
        .unwrap()
        .lanes
        .iter()
        .filter_map(|l| l.envelope())
        .find(|e| e.target == target)
        .unwrap()
        .points
        .clone();
    points.remove(index);
    let lane = PointTarget {
        key,
        revision: app.project_revision(),
        target,
    };
    send(&mut app, Action::SetPoints(lane, points, None));
    let after = app
        .clip_view
        .editor
        .content
        .as_ref()
        .expect("point deletion keeps the selected range");
    assert_eq!(after.range, selection.range);
    assert_eq!(after.lanes, selection.lanes);
    assert_eq!(after.count() + 1, selection.count());
    assert!(app.clip_view.editor.lanes.is_none());
}

#[test]
fn insert_keys_status_and_paste_follow_the_empty_selection() {
    let (mut app, key) = setup();
    let revision = app.project_revision();
    let selection = all(&app, key, BeatTime(1440)..BeatTime(1440));
    send(&mut app, Action::Content(selection));
    assert_eq!(app.status.message(), Some("Insert Mark 1.2.3"));
    let lanes = app.clip_view.editor.content.as_ref().unwrap().lanes.clone();
    keyboard_event(
        &mut app,
        Key::Named(Named::ArrowRight),
        Modifiers::empty(),
        true,
    );
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().insert_mark(),
        Some(BeatTime(1680))
    );
    keyboard_event(
        &mut app,
        Key::Named(Named::ArrowLeft),
        Modifiers::empty(),
        true,
    );
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().insert_mark(),
        Some(BeatTime(1440))
    );
    keyboard_event(&mut app, Key::Named(Named::End), Modifiers::empty(), true);
    let end = app.project().clip(key.track, key.slot).unwrap().end;
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().insert_mark(),
        Some(end)
    );
    // The compatibility adapter still clamps legacy single-hit commands; drawing and keys do not.
    assert_eq!(app.clip_view.editor.content.as_ref().unwrap().lanes, lanes);
    keyboard_event(&mut app, Key::Named(Named::Home), Modifiers::empty(), true);
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().insert_mark(),
        Some(BeatTime::ZERO)
    );
    keyboard_event(
        &mut app,
        Key::Named(Named::ArrowLeft),
        Modifiers::empty(),
        true,
    );
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().insert_mark(),
        Some(BeatTime::ZERO)
    );
    assert_eq!(app.project_revision(), revision);

    // Copy a hit, then paste at the marker moved by the ordinary key path.
    send(&mut app, Action::Select(key, Some(0)));
    super::super::shortcut(&mut app, crate::clip_shortcuts::Shortcut::Copy);
    let selection = all(&app, key, BeatTime(240)..BeatTime(240));
    send(&mut app, Action::Content(selection));
    keyboard_event(
        &mut app,
        Key::Named(Named::ArrowRight),
        Modifiers::empty(),
        true,
    );
    super::super::shortcut(&mut app, crate::clip_shortcuts::Shortcut::Paste);
    assert!(
        app.project()
            .clip(key.track, key.slot)
            .unwrap()
            .events
            .iter()
            .any(|e| e.position == BeatTime(480))
    );
}

#[test]
fn insert_keys_leave_ranges_picks_headers_and_fields_alone() {
    let (mut app, key) = setup();
    let selection = all(&app, key, BeatTime(480)..BeatTime(2880));
    send(&mut app, Action::Content(selection));
    let before = app.clip_view.editor.content.clone();
    assert!(!keyboard(
        &mut app,
        &Key::Named(Named::Home),
        Modifiers::empty(),
        false,
        true
    ));
    assert_eq!(app.clip_view.editor.content, before);
    send(&mut app, Action::Select(key, Some(0)));
    assert!(!keyboard(
        &mut app,
        &Key::Named(Named::ArrowRight),
        Modifiers::empty(),
        false,
        true
    ));
    send(&mut app, Action::Header(key, 0));
    assert!(!keyboard(
        &mut app,
        &Key::Named(Named::End),
        Modifiers::empty(),
        false,
        true
    ));
    let selection = all(&app, key, BeatTime(480)..BeatTime(480));
    send(&mut app, Action::Content(selection));
    assert!(!keyboard(
        &mut app,
        &Key::Named(Named::ArrowRight),
        Modifiers::empty(),
        false,
        false
    ));
    app.pending_field = Some(DraftKey::clip(key.track, key.slot, ClipField::Length));
    assert!(!keyboard(
        &mut app,
        &Key::Named(Named::ArrowRight),
        Modifiers::empty(),
        false,
        true
    ));
    app.pending_field = None;
    app.clip_view.editor.focused = false;
    assert!(!keyboard(
        &mut app,
        &Key::Named(Named::ArrowRight),
        Modifiers::empty(),
        false,
        true
    ));
}
