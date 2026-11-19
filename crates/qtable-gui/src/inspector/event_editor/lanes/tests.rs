use super::*;
use qtable_engine::model::{DeviceId, DeviceRef};
fn setup() -> (App, ClipKey) {
    let project = qtable_engine::file::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lanes.ron"),
    )
    .unwrap();
    setup_project(project)
}
fn setup_project(project: qtable_engine::model::Project) -> (App, ClipKey) {
    let track = project.tracks[0].id;
    let engine = qtable_engine::engine::Engine::new(project).unwrap();
    let mut app = App::new(engine, "120".into(), None);
    app.chrome.detail_open = true;
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    app.selection.clip = Some(crate::grid::Selection { track, slot: 0 });
    app.focus = crate::focus::Focus::Detail;
    super::super::reconcile(&mut app);
    let key = ClipKey {
        track,
        slot: 0,
        id: clip(app.project(), track, 0).unwrap().id,
        epoch: app.clip_view.editor.epoch,
    };
    (app, key)
}
fn keypress(app: &mut App, shortcut: Shortcut) {
    let _ = crate::update::update(app, crate::messages::Message::ClipShortcut(shortcut));
}

fn selected_rows(app: &App, key: ClipKey) -> Vec<usize> {
    rows(
        &app.clip_view.editor,
        key,
        app.project().track(key.track),
        &clip_owned(app, key).unwrap(),
    )
}
fn event_project() -> qtable_engine::model::Project {
    let (app, key) = setup();
    let mut project = app.project().clone();
    let track = project.track_mut(key.track).unwrap();
    track.ensure_event(OscAddress::Relative("/snare".into()), &[], || {
        qtable_engine::model::DeviceId(99)
    });
    let clip = track.slots[0].clip.as_mut().unwrap();
    clip.lanes.retain(|l| l.envelope().is_some());
    project
}

#[test]
fn generated_empty_headers_select_and_shift_select_in_device_order() {
    let (mut app, key) = setup_project(event_project());
    select(&mut app, key, 1, false);
    let selected = app.clip_view.editor.lanes.clone().unwrap();
    assert!(matches!(&selected.items[0], Identity::Event(r) if r.key.0 == "snare"));
    assert_eq!(
        app.clip_view
            .editor
            .lane_for(key, &clip_owned(&app, key).unwrap()),
        None
    );
    app.clip_view.editor.shift_down = true;
    select(&mut app, key, 0, true);
    assert_eq!(selected_rows(&app, key), vec![0, 1]);
    super::super::super::lane_view::choose_tab(
        &mut app,
        key.track,
        key.slot,
        super::super::super::lane_view::EditorTab::Events,
    );
    assert_eq!(app.clip_view.editor.lanes.as_ref().unwrap().items.len(), 2);
}

#[test]
fn generated_headers_offer_no_structural_menu_and_shortcuts_do_not_change_content() {
    let (mut app, key) = setup_project(event_project());
    select(&mut app, key, 0, false);
    let target = Target {
        key,
        revision: app.project_revision(),
        row: 0,
    };
    open_menu(&mut app, target);
    // The menu carries no edit commands, only Go to Device.
    assert_eq!(app.menu_target(), Some(MenuTarget::Lane(target)));
    assert_eq!(
        entry_labels(&crate::context_menu::current_entries(
            &app,
            MenuTarget::Lane(target)
        )),
        ["Go to Device"]
    );
    let before = app.project().clone();
    for command in [
        Shortcut::Copy,
        Shortcut::Cut,
        Shortcut::Delete,
        Shortcut::Duplicate,
        Shortcut::Paste,
    ] {
        keypress(&mut app, command);
        assert_eq!(*app.project(), before);
    }
    move_lanes(&mut app, target, 0..1, 3);
    assert_eq!(*app.project(), before);
}

#[test]
fn mixed_header_range_cannot_remove_or_copy_generated_rows() {
    let (mut app, key) = setup();
    select(&mut app, key, 0, false);
    app.clip_view.editor.shift_down = true;
    select(&mut app, key, 2, true);
    app.clip_view.editor.shift_down = false;
    assert_eq!(selected_rows(&app, key), vec![0, 1, 2]);
    let target = Target {
        key,
        revision: app.project_revision(),
        row: 1,
    };
    let entries = crate::context_menu::current_entries(&app, MenuTarget::Lane(target));
    assert!(!entries.iter().any(|entry| matches!(
        entry,
        crate::context_menu::Entry::Command {
            label: "Copy" | "Paste" | "Delete",
            ..
        }
    )));
    let before = app.project().clone();
    keypress(&mut app, Shortcut::Delete);
    keypress(&mut app, Shortcut::Copy);
    assert_eq!(*app.project(), before);
    assert!(app.clip_view.editor.lane_clipboard.is_none());
    move_lanes(&mut app, target, 0..3, 3);
    assert_eq!(*app.project(), before);
}

#[test]
fn envelope_header_edits_map_display_order_back_to_stored_lanes() {
    let mut project = event_project();
    // Stored event lane order differs from the generated prefix, and one row has no stored lane.
    project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .rotate_left(2);
    let (mut app, key) = setup_project(project);
    let before = clip_owned(&app, key).unwrap();
    let selected = displayed_in(&app, key, &before)[2].clone();
    select(&mut app, key, 2, false);
    keypress(&mut app, Shortcut::Copy);
    assert!(app.clip_view.editor.lane_clipboard.is_some());
    keypress(&mut app, Shortcut::Delete);
    let after = clip_owned(&app, key).unwrap();
    assert_eq!(after.events, before.events);
    assert_eq!(
        after.lanes.iter().filter_map(Lane::envelope).count(),
        before.lanes.iter().filter_map(Lane::envelope).count() - 1
    );
    assert!(!identities(&after).iter().any(|id| id.matches(&selected)));
    select(&mut app, key, 2, false);
    keypress(&mut app, Shortcut::Paste);
    assert!(
        identities(&clip_owned(&app, key).unwrap())
            .iter()
            .any(|id| id.matches(&selected))
    );
}

#[test]
fn envelope_drag_keeps_generated_order_and_rejects_event_destinations_and_stale_drops() {
    let (mut app, key) = setup_project(event_project());
    let target = Target {
        key,
        revision: app.project_revision(),
        row: 2,
    };
    let before = clip_owned(&app, key).unwrap();
    move_lanes(&mut app, target, 2..3, 0);
    assert_eq!(clip_owned(&app, key).unwrap(), before);
    let moved = displayed_in(&app, key, &before)[2].clone();
    move_lanes(&mut app, target, 2..3, 4);
    let after = clip_owned(&app, key).unwrap();
    assert_eq!(displayed_in(&app, key, &after)[4], moved);
    assert_eq!(after.events, before.events);
    assert_eq!(selected_rows(&app, key), vec![4]);
    move_lanes(&mut app, target, 2..3, 3);
    assert_eq!(clip_owned(&app, key).unwrap(), after);
}

#[test]
fn selected_event_survives_an_earlier_definition_removal_and_keeps_its_hits() {
    let mut project = event_project();
    let reference =
        project.tracks[0].ensure_event(OscAddress::Relative("/later".into()), &[], || {
            qtable_engine::model::DeviceId(99)
        });
    project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .events
        .push(Event {
            definition: reference.clone(),
            position: BeatTime(480),
            arguments: vec![],
        });
    let (mut app, key) = setup_project(project);
    select(&mut app, key, 2, false);
    let selected = app.clip_view.editor.lanes.clone().unwrap();
    let target = app.project().tracks[0].devices[0].target().unwrap();
    let removed = target.events[1].key.clone();
    app.engine
        .execute(Command::Edit(ProjectEdit::EditTarget {
            device: reference.device,
            edit: qtable_engine::edit::TargetEdit::RemoveEntries {
                keys: vec![removed],
            },
        }))
        .unwrap();
    app.snapshot = app.engine.snapshot();
    super::super::reconcile(&mut app);
    assert_eq!(app.clip_view.editor.lanes.as_ref(), Some(&selected));
    assert_eq!(selected_rows(&app, key), vec![1]);
    assert!(
        clip_owned(&app, key)
            .unwrap()
            .events
            .iter()
            .any(|e| e.definition == reference)
    );
}

#[test]
fn empty_generated_row_keeps_insert_selection_and_accepts_double_click_or_draw_edits() {
    let (mut app, key) = setup_project(event_project());
    let Identity::Event(reference) =
        displayed_in(&app, key, &clip_owned(&app, key).unwrap())[1].clone()
    else {
        panic!()
    };
    handle(
        &mut app,
        Action::Content(content::Selection::time(
            key,
            BeatTime(240)..BeatTime(240),
            vec![Identity::Event(reference.clone())],
        )),
    );
    super::super::reconcile(&mut app);
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().lanes,
        vec![Identity::Event(reference.clone())]
    );
    handle(&mut app, Action::Add(key, BeatTime(240), reference.clone()));
    assert!(
        clip_owned(&app, key)
            .unwrap()
            .events
            .iter()
            .any(|e| e.definition == reference && e.position == BeatTime(240))
    );
    let revision = app.project_revision();
    handle(
        &mut app,
        Action::Draw(Draw::Hits {
            key,
            revision,
            lane: reference.clone(),
            span: BeatTime(480)..BeatTime(960),
            positions: vec![BeatTime(480), BeatTime(720)],
        }),
    );
    assert_eq!(
        clip_owned(&app, key)
            .unwrap()
            .events
            .iter()
            .filter(|e| e.definition == reference)
            .count(),
        3
    );
}

#[test]
fn envelope_menu_keeps_reenable_and_field_focus_blocks_header_delete() {
    let (mut app, key) = setup();
    select(&mut app, key, 1, false);
    let target = Target {
        key,
        revision: app.project_revision(),
        row: 1,
    };
    open_menu(&mut app, target);
    assert_eq!(app.menu_target(), Some(MenuTarget::Lane(target)));
    let labels: Vec<_> = crate::context_menu::current_entries(&app, MenuTarget::Lane(target))
        .into_iter()
        .filter_map(|entry| match entry {
            crate::context_menu::Entry::Command { label, .. } => Some(label),
            _ => None,
        })
        .collect();
    assert_eq!(
        labels,
        vec![
            "Copy",
            "Paste",
            "Delete",
            "Re-Enable Automation",
            "Go to Device"
        ]
    );
    let before = app.project().clone();
    app.pending_field = Some(DraftKey::clip(key.track, key.slot, ClipField::Name));
    keypress(&mut app, Shortcut::Delete);
    assert_eq!(*app.project(), before);
}

#[test]
fn an_envelope_lane_menu_offers_re_enable_only_while_that_argument_is_overridden() {
    use qtable_engine::scheduler::{AutomationState, AutomationValue};
    let (mut app, key) = setup();
    let owned = clip_owned(&app, key).unwrap();
    let ids = identities(&owned);
    let volume = crate::mixer_device::volume_field(key.track);
    let revision = app.project_revision();
    let at = |row| Target { key, revision, row };
    let rows: Vec<_> = (0..ids.len())
        .map(|row| (row, envelope_field(&app, at(row))))
        .collect();
    let volume_row = rows
        .iter()
        .find_map(|(row, field)| (field.as_ref() == Some(&volume)).then_some(*row))
        .expect("the fixture has a Mixer Volume envelope");
    let event_row = rows.iter().find(|(_, field)| field.is_none()).unwrap().0;
    let entry = |app: &App, row| {
        crate::context_menu::current_entries(app, MenuTarget::Lane(at(row)))
            .into_iter()
            .find_map(|entry| match entry {
                crate::context_menu::Entry::Command {
                    label: "Re-Enable Automation",
                    enabled,
                    message,
                    ..
                } => Some((enabled, message)),
                _ => None,
            })
    };
    assert!(
        entry(&app, event_row).is_none(),
        "an event lane has no envelope"
    );
    let drive = |app: &mut App, state| {
        app.snapshot.automation = vec![AutomationValue {
            field: volume.clone(),
            value: 0.5,
            state,
        }];
    };
    assert!(!entry(&app, volume_row).unwrap().0, "nothing is driving it");
    drive(&mut app, AutomationState::Following);
    assert!(
        !entry(&app, volume_row).unwrap().0,
        "following needs nothing"
    );
    drive(&mut app, AutomationState::Overridden);
    let (enabled, message) = entry(&app, volume_row).unwrap();
    assert!(enabled);
    assert!(matches!(
        message,
        crate::messages::Message::ReEnableAutomation(Some(ref sent)) if *sent == volume
    ));
    // Another lane's entry stays disabled: the override belongs to Volume alone.
    let other = rows
        .iter()
        .find(|(row, field)| field.is_some() && *row != volume_row)
        .unwrap()
        .0;
    assert!(!entry(&app, other).unwrap().0);
    // A stale target offers nothing.
    let stale = Target {
        revision: revision + 1,
        ..at(volume_row)
    };
    assert_eq!(envelope_field(&app, stale), None);
}

#[test]
fn identities_borrow_declared_and_implicit_refs_without_copying_envelopes() {
    let project = crate::inspector::lane_model::tests::mixed_project();
    let mut clip = project.clip(TrackId(1), 0).unwrap().clone();
    let mut implicit = clip.events[0].definition.clone();
    implicit.key.0 = "implicit".into();
    clip.events.push(Event {
        definition: implicit,
        position: BeatTime::ZERO,
        arguments: vec![],
    });
    clip.events.push(clip.events.last().unwrap().clone());
    let ids = identities(&clip);
    assert_eq!(ids.len(), clip.lanes.len() + 1);
    for (lane, id) in clip.lanes.iter().zip(&ids) {
        match (lane, id) {
            (Lane::Events(lane), IdentityRef::Event(reference)) => {
                assert!(std::ptr::eq(&lane.definition, *reference))
            }
            (Lane::Envelope(lane), IdentityRef::Envelope(reference)) => {
                assert!(std::ptr::eq(&lane.target, *reference))
            }
            _ => panic!("lane order changed"),
        }
    }
    assert!(
        matches!(ids.last(), Some(IdentityRef::Event(reference)) if std::ptr::eq(*reference, &clip.events[clip.events.len() - 2].definition))
    );
}

#[test]
fn events_tab_does_not_paste_an_envelope_header_clipboard_without_a_header_selection() {
    let (mut app, key) = setup();
    select(&mut app, key, 1, false);
    keypress(&mut app, Shortcut::Cut);
    assert!(app.clip_view.editor.lane_clipboard.is_some());
    super::super::super::lane_view::choose_tab(
        &mut app,
        key.track,
        key.slot,
        super::super::super::lane_view::EditorTab::Events,
    );
    content::clear(&mut app.clip_view.editor);
    app.clip_view.editor.focused = true;
    let before = app.project().clone();
    keypress(&mut app, Shortcut::Paste);
    assert_eq!(*app.project(), before);
}

fn entry_labels(entries: &[crate::context_menu::Entry]) -> Vec<&'static str> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            crate::context_menu::Entry::Command { label, .. } => Some(*label),
            _ => None,
        })
        .collect()
}

/// The first Target device's events, copied onto a second Target device, so two devices on
/// the track define events with the same names.
fn two_targets_defining_the_same_names() -> (
    App,
    ClipKey,
    qtable_engine::model::DeviceId,
    qtable_engine::model::DeviceId,
) {
    let mut project = event_project();
    let track = project.tracks[0].id;
    let owner = project.track_mut(track).unwrap();
    let first = owner.devices[0].clone();
    let mut second = first.clone();
    second.id = qtable_engine::model::DeviceId(98);
    second.name = "Second".into();
    second.expanded = false;
    let ids = (first.id, second.id);
    owner.devices.push(second);
    let (app, key) = setup_project(project);
    (app, key, ids.0, ids.1)
}

fn row_of(app: &App, key: ClipKey, device: qtable_engine::model::DeviceId, name: &str) -> usize {
    let clip = clip(app.project(), key.track, key.slot).unwrap();
    displayed_in(app, key, clip)
        .iter()
        .position(|id| matches!(id, Identity::Event(r) if r.device == device && r.key.0 == name))
        .unwrap()
}

fn press_go_to_device(app: &mut App, target: Target) {
    open_menu(app, target);
    let entries = crate::context_menu::current_entries(app, MenuTarget::Lane(target));
    let message = entries
        .iter()
        .find_map(|entry| match entry {
            crate::context_menu::Entry::Command {
                label: "Go to Device",
                message,
                enabled: true,
                ..
            } => Some(message.clone()),
            _ => None,
        })
        .expect("an enabled Go to Device");
    let _ = crate::update::update(app, message);
}

#[test]
fn go_to_device_on_an_event_header_reveals_the_definition_on_its_own_device() {
    let (mut app, key, first, second) = two_targets_defining_the_same_names();
    for (device, other) in [(second, first), (first, second)] {
        let row = row_of(&app, key, device, "snare");
        let target = Target {
            key,
            revision: app.project_revision(),
            row,
        };
        app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
        press_go_to_device(&mut app, target);
        assert_eq!(
            app.selection.inspector_tab,
            crate::inspector_panel::Tab::Track
        );
        assert!(app.project().device(device).unwrap().1.expanded);
        assert_eq!(app.selection.devices, vec![device]);
        assert_ne!(app.selection.devices, vec![other]);
        assert_eq!(
            app.device_chain.entry.get(&device).map(|k| k.0.clone()),
            Some("snare".to_string())
        );
        assert!(app.overlay.is_none());
    }
}

fn envelope_row(app: &App, key: ClipKey, device: DeviceRef, parameter: &str) -> usize {
    let clip = clip(app.project(), key.track, key.slot).unwrap();
    displayed_in(app, key, clip)
        .iter()
        .position(|id| {
            matches!(id, Identity::Envelope(a) if a.device == device && a.parameter.0 == parameter)
        })
        .unwrap_or_else(|| panic!("no envelope row for {parameter}"))
}

/// The fixture's Target copied onto a second device, so both define `opacity`, and the clip
/// automates `opacity` on each.
fn two_targets_defining_the_same_parameter() -> (App, ClipKey, DeviceId, DeviceId) {
    let (app, key) = setup();
    let mut project = app.project().clone();
    let owner = project.track_mut(key.track).unwrap();
    let first = owner.devices[0].clone();
    let mut second = first.clone();
    second.id = DeviceId(98);
    second.name = "Second".into();
    second.expanded = false;
    let ids = (first.id, second.id);
    owner.devices.push(second);
    let clip = owner.slots[0].clip.as_mut().unwrap();
    let copied: Vec<_> = clip
        .lanes
        .iter()
        .filter_map(|lane| lane.envelope())
        .filter(|envelope| envelope.target.device == DeviceRef::Chain(ids.0))
        .cloned()
        .map(|mut envelope| {
            envelope.target.device = DeviceRef::Chain(ids.1);
            envelope
        })
        .collect();
    assert!(!copied.is_empty());
    clip.lanes.extend(copied.into_iter().map(Lane::Envelope));
    let (app, key) = setup_project(project);
    (app, key, ids.0, ids.1)
}

fn press_go_to_device_on_row(app: &mut App, key: ClipKey, row: usize) {
    press_go_to_device(
        app,
        Target {
            key,
            revision: app.project_revision(),
            row,
        },
    );
}

#[test]
fn go_to_device_on_a_parameter_header_reveals_the_definition_on_its_own_device() {
    let (mut app, key, first, second) = two_targets_defining_the_same_parameter();
    for (device, other) in [(second, first), (first, second)] {
        let row = envelope_row(&app, key, DeviceRef::Chain(device), "opacity");
        app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
        press_go_to_device_on_row(&mut app, key, row);
        assert_eq!(
            app.selection.inspector_tab,
            crate::inspector_panel::Tab::Track
        );
        assert!(app.project().device(device).unwrap().1.expanded);
        assert_eq!(app.selection.devices, vec![device]);
        assert_ne!(app.selection.devices, vec![other]);
        assert_eq!(
            app.device_chain.entry.get(&device).map(|k| k.0.clone()),
            Some("opacity".to_string())
        );
        assert!(app.overlay.is_none());
    }
}

#[test]
fn go_to_device_on_a_parameter_header_selects_the_automated_field() {
    let (mut app, key) = setup();
    // The fixture automates `position`'s second field, `y`.
    let row = envelope_row(&app, key, DeviceRef::Chain(DeviceId(1)), "position");
    press_go_to_device_on_row(&mut app, key, row);
    assert_eq!(
        app.device_chain
            .entry
            .get(&DeviceId(1))
            .map(|k| k.0.clone()),
        Some("position".to_string())
    );
    assert_eq!(
        app.device_chain.field.get(&DeviceId(1)),
        Some(&qtable_engine::model::FieldKey("y".into()))
    );
}

#[test]
fn go_to_device_on_the_volume_header_shows_the_mixer() {
    let (mut app, key) = setup();
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    app.selection.devices.clear();
    let volume = ArgumentRef::volume();
    // Called directly: `update` turns the scroll request into a task at once.
    crate::device_chain::go_to_argument(&mut app, key.track, &volume);
    assert_eq!(
        app.selection.inspector_tab,
        crate::inspector_panel::Tab::Track
    );
    assert!(app.chrome.detail_open);
    assert!(app.overlay.is_none());
    // The Mixer is not a chain device: nothing is selected, and the tab scrolls to its end.
    assert!(app.selection.devices.is_empty());
    assert!(crate::browser::reveal_is_mixer(&app));
    // Through the menu, the same item switches to the Track tab.
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    let row = envelope_row(&app, key, DeviceRef::Mixer, "volume");
    press_go_to_device_on_row(&mut app, key, row);
    assert_eq!(
        app.selection.inspector_tab,
        crate::inspector_panel::Tab::Track
    );
}

#[test]
fn a_parameter_header_menu_ends_with_go_to_device_after_a_separator() {
    let (app, key) = setup();
    let revision = app.project_revision();
    for (device, parameter) in [
        (DeviceRef::Chain(DeviceId(1)), "opacity"),
        (DeviceRef::Mixer, "volume"),
    ] {
        let row = envelope_row(&app, key, device, parameter);
        let entries = crate::context_menu::current_entries(
            &app,
            MenuTarget::Lane(Target { key, revision, row }),
        );
        assert_eq!(entry_labels(&entries).last(), Some(&"Go to Device"));
        assert!(matches!(
            entries[entries.len() - 2],
            crate::context_menu::Entry::Separator
        ));
        assert!(matches!(
            entries.last(),
            Some(crate::context_menu::Entry::Command { enabled: true, .. })
        ));
    }
}

#[test]
fn go_to_device_goes_nowhere_when_the_definition_is_gone() {
    let (mut app, key) = setup();
    // A reference whose definition is gone does nothing.
    let gone = qtable_engine::model::EventRef {
        device: DeviceId(777),
        key: qtable_engine::model::EntryKey("none".into()),
    };
    let before = app.selection.inspector_tab;
    crate::device_chain::go_to_event(&mut app, key.track, gone);
    let missing = ArgumentRef {
        device: DeviceRef::Chain(DeviceId(777)),
        parameter: qtable_engine::model::ParameterKey("none".into()),
        field: qtable_engine::model::FieldKey("value".into()),
    };
    crate::device_chain::go_to_argument(&mut app, key.track, &missing);
    assert_eq!(app.selection.inspector_tab, before);
}
