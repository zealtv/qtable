use super::*;
use crate::clip_shortcuts::Shortcut;
use crate::messages::Message as Root;
use qtable_engine::model::{LIVE_CLIP_PALETTE, OscArgument};
fn setup() -> (App, ClipKey) {
    let mut p = crate::files::new_project();
    let track = p.tracks[0].id;
    let mut c = Clip::new(ClipId(1), "Hits", BeatTime(3840), LIVE_CLIP_PALETTE[0]).unwrap();
    let arguments = vec![
        OscArgument::Int(3),
        OscArgument::Float(0.123),
        OscArgument::String("payload".into()),
        OscArgument::Bool(true),
    ];
    let path = OscAddress::Absolute("/hit".into());
    c.events
        .push(p.hit(track, BeatTime(960), path, arguments).unwrap());
    p.tracks[0].slots[0].clip = Some(c);
    let mut app = App::new(
        qtable_engine::engine::Engine::new(p).unwrap(),
        "120".into(),
        None,
    );
    app.selection.clip = Some(crate::grid::Selection { track, slot: 0 });
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    app.focus = crate::focus::Focus::Detail;
    reconcile(&mut app);
    let key = ClipKey {
        track,
        slot: 0,
        id: ClipId(1),
        epoch: app.clip_view.editor.epoch,
    };
    (app, key)
}
fn target(app: &App, key: ClipKey) -> HitTarget {
    HitTarget {
        key,
        index: 0,
        revision: app.project_revision(),
    }
}
fn click(app: &App, key: ClipKey) -> menu::Click {
    menu::Click {
        key,
        revision: app.project_revision(),
        lane: lanes::Identity::Event(clip_owned(app, key).unwrap().events[0].definition.clone()),
        under: menu::Under::Hit(0),
    }
}
/// Right-click the first hit and return the menu that opened.
fn open(app: &mut App, key: ClipKey) -> menu::ContentTarget {
    send(app, Action::ContentMenu(click(app, key)));
    let Some(crate::context_menu::MenuTarget::Content(target)) = app.menu_target() else {
        panic!("a content menu should be open");
    };
    target
}
fn send(app: &mut App, action: Action) {
    let _ = crate::update::update(app, Root::Inspector(Message::Timeline(action)));
}
fn key(app: &mut App, shortcut: Shortcut) {
    let _ = crate::update::update(app, Root::ClipShortcut(shortcut));
}
#[test]
fn revision_and_clip_changes_reject_stale_menu_move_and_duplicate() {
    let (mut app, owner) = setup();
    let hit = target(&app, owner);
    let menu = open(&mut app, owner);
    assert!(app.overlay.is_some());
    run_edit(
        &mut app,
        Command::Edit(ProjectEdit::RenameClip {
            track: owner.track,
            slot: 0,
            name: "new".into(),
        }),
    );
    app.validate_overlay();
    assert!(app.overlay.is_none());
    let original = app.project().clone();
    send(&mut app, Action::Move(hit, BeatTime(1920)));
    send(&mut app, Action::Duplicate(hit, BeatTime(1920)));
    send(
        &mut app,
        Action::ContentCommand(menu, menu::ContentCommand::Key(Shortcut::Delete)),
    );
    assert_eq!(app.project(), &original);
    app.reset_for_project();
    assert!(!valid_target(&app, hit));
}
#[test]
fn cursor_is_gui_state_survives_marker_shortening_and_resets_on_selection() {
    let (mut app, owner) = setup();
    send(&mut app, Action::Cursor(owner, BeatTime(3000), Some(0)));
    assert_eq!(app.project_revision(), 0);
    assert_eq!(
        app.clip_view.editor.content.as_ref().map(|s| s.range.start),
        Some(BeatTime(3000))
    );
    run_edit(
        &mut app,
        Command::Edit(ProjectEdit::SetClipLength {
            track: owner.track,
            slot: 0,
            length: BeatTime(1920),
        }),
    );
    reconcile(&mut app);
    assert_eq!(
        app.clip_view.editor.content.as_ref().map(|s| s.range.start),
        Some(BeatTime(3000))
    );
    app.selection.clip = None;
    reconcile(&mut app);
    assert_eq!(
        app.clip_view.editor.content.as_ref().map(|s| s.range.start),
        None
    );
}
#[test]
fn text_and_grid_focus_do_not_use_hit_clipboard() {
    let (mut app, owner) = setup();
    send(&mut app, Action::Select(owner, Some(0)));
    let field = DraftKey::clip(
        owner.track,
        0,
        ClipField::EventArgument {
            event: 0,
            argument: 0,
        },
    );
    let _ = crate::update::update(&mut app, Root::Inspector(Message::Draft(field, "4".into())));
    let revision = app.project_revision();
    key(&mut app, Shortcut::Copy);
    key(&mut app, Shortcut::Paste);
    key(&mut app, Shortcut::Delete);
    assert_eq!(app.project_revision(), revision);
    assert!(app.clip_view.editor.clipboard.is_none());
    app.pending_field = None;
    app.focus = crate::focus::Focus::Grid;
    key(&mut app, Shortcut::Copy);
    assert!(app.clip_actions.clipboard.is_some());
    assert!(app.clip_view.editor.clipboard.is_none());
}
#[test]
fn hit_menu_uses_the_one_slot_and_paste_enablement() {
    use crate::context_menu::{Entry, MenuTarget, OpenMenu, current_entries};
    use crate::overlay::Overlay;
    let (mut app, owner) = setup();
    app.open_overlay(Overlay::SectionsMenu);
    let target = open(&mut app, owner);
    assert!(matches!(
        &app.overlay,
        Some(Overlay::Context(OpenMenu { target: MenuTarget::Content(t), .. })) if *t == target
    ));
    let entries = current_entries(&app, MenuTarget::Content(target));
    assert!(matches!(
        &entries[1],
        Entry::Command {
            label: "Paste",
            shortcut: "⌘V",
            enabled: false,
            ..
        }
    ));
    let _ = crate::update::update(&mut app, Root::CloseOverlay);
    assert!(app.overlay.is_none());
}

#[test]
fn insert_position_is_independent_of_engine_playback_marker() {
    let (mut app, owner) = setup();
    app.snapshot.clip_positions = vec![qtable_engine::scheduler::ClipPosition {
        track: owner.track,
        clip: owner.id,
        position: BeatTime(1440),
    }];
    send(&mut app, Action::Cursor(owner, BeatTime(480), Some(0)));
    assert_eq!(
        app.clip_view.editor.content.as_ref().map(|s| s.range.start),
        Some(BeatTime(480))
    );
    assert_eq!(app.clip_view.editor.playhead, Some(BeatTime(1440)));
    assert_eq!(app.project_revision(), 0);
    assert!(!app.snapshot.playing);
}
#[test]
fn hidden_editor_or_another_detail_tab_closes_the_hit_menu() {
    let (mut app, owner) = setup();
    let hit = target(&app, owner);
    open(&mut app, owner);
    app.chrome.detail_open = false;
    app.validate_overlay();
    assert!(app.overlay.is_none());
    assert!(!valid_target(&app, hit));
    app.chrome.detail_open = true;
    open(&mut app, owner);
    app.selection.inspector_tab = crate::inspector_panel::Tab::Track;
    app.validate_overlay();
    assert!(app.overlay.is_none());
    assert!(!valid_target(&app, hit));
}
#[test]
fn lane_repoint_shows_the_engine_conversion_notice() {
    let (mut app, owner) = setup();
    let device = clip(app.project(), owner.track, 0).unwrap().events[0]
        .definition
        .device;
    run_edit(
        &mut app,
        Command::Edit(ProjectEdit::EditTarget {
            device,
            edit: qtable_engine::edit::TargetEdit::AddEvent {
                path: OscAddress::Absolute("/bang".into()),
                name: String::new(),
            },
        }),
    );
    run_edit(
        &mut app,
        Command::Edit(ProjectEdit::EditLane {
            track: owner.track,
            slot: 0,
            edit: LaneEdit::SetPath {
                lane: 0,
                path: "/bang".into(),
            },
        }),
    );
    assert_eq!(
        app.status.message(),
        Some("Converted 1 hit to no arguments")
    );
    assert!(
        clip(app.project(), owner.track, 0).unwrap().events[0]
            .arguments
            .is_empty()
    );
}

#[test]
fn hit_clipboard_pastes_at_content_start_and_overwrites_on_repeat() {
    let (mut app, owner) = setup();
    let source = clip_owned(&app, owner).unwrap().events[0].clone();
    open(&mut app, owner);
    key(&mut app, Shortcut::Copy);
    assert_eq!(app.project_revision(), 0);
    assert!(app.clip_actions.clipboard.is_none());
    send(&mut app, Action::Cursor(owner, BeatTime(1920), Some(0)));
    key(&mut app, Shortcut::Paste);
    key(&mut app, Shortcut::Paste);
    let clip = clip_owned(&app, owner).unwrap();
    assert_eq!(clip.events.len(), 2);
    assert_eq!(clip.events[1].position, BeatTime(1920));
    assert_eq!(clip.events[1].arguments, source.arguments);
    assert_eq!(clip.events[1].definition, source.definition);
}

#[test]
fn a_hit_menu_ends_with_go_to_device_which_reveals_the_hit_s_definition() {
    use crate::context_menu::{Entry, current_entries};
    let (mut app, owner) = setup();
    let menu = open(&mut app, owner);
    let entries = current_entries(&app, crate::context_menu::MenuTarget::Content(menu));
    let labels: Vec<_> = entries
        .iter()
        .filter_map(|e| match e {
            Entry::Command { label, .. } => Some(*label),
            _ => None,
        })
        .collect();
    assert_eq!(
        labels,
        ["Copy", "Paste", "Delete", "Draw Mode", "Go to Device"]
    );
    assert!(matches!(entries[entries.len() - 2], Entry::Separator));
    let definition = clip_owned(&app, owner).unwrap().events[0]
        .definition
        .clone();
    let Some(Entry::Command {
        message,
        enabled: true,
        ..
    }) = entries.last().cloned()
    else {
        panic!("Go to Device should be enabled");
    };
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    let _ = crate::update::update(&mut app, message);
    assert_eq!(
        app.selection.inspector_tab,
        crate::inspector_panel::Tab::Track
    );
    assert_eq!(app.selection.devices, vec![definition.device]);
    assert_eq!(
        app.device_chain.entry.get(&definition.device),
        Some(&definition.key)
    );
}
