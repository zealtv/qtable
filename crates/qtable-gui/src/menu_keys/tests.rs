//! An open menu holds the keyboard: Space never reaches the transport, Up and Down move the
//! highlight, Enter sends the row's own message, Escape closes (2026-11-15, L1).
use crate::app::App;
use crate::grid;
use crate::inspector::Message as Inspect;
use crate::inspector::lane_model::tests::mixed_project;
use crate::messages::{EditMessage, Message};
use crate::overlay::Overlay;
use iced::keyboard::{Key, Modifiers, key::Named};
use qtable_engine::engine::Engine;
use qtable_engine::model::TrackId;

fn app() -> App {
    let engine = Engine::new(crate::files::new_project()).expect("engine starts");
    let mut app = App::new(engine, String::new(), None);
    for _ in 0..2 {
        send(&mut app, Message::Edit(EditMessage::AddTrack));
    }
    let track = app.project().tracks[0].id;
    send(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    send(&mut app, Message::Edit(EditMessage::CreateSelectedClip));
    app
}

fn send(app: &mut App, message: Message) {
    let _ = crate::update::update(app, message);
}

fn press(app: &mut App, key: Key, modifiers: Modifiers, ignored: bool) {
    send(
        app,
        Message::KeyPressed {
            key,
            modifiers,
            repeat: false,
            ignored,
        },
    );
}

fn named(app: &mut App, named: Named) {
    press(app, Key::Named(named), Modifiers::NONE, true);
}

fn clip_menu(app: &mut App) {
    let track = app.project().tracks[0].id;
    send(app, Message::Grid(grid::Action::ContextMenu(track, 0)));
    assert!(matches!(app.overlay, Some(Overlay::Context(_))));
}

fn chooser_app() -> App {
    let engine = Engine::new(mixed_project()).unwrap();
    let mut app = App::new(engine, "120".into(), None);
    let track = app.project().tracks[0].id;
    app.selection.clip = Some(grid::Selection { track, slot: 0 });
    app.chrome.detail_open = true;
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    crate::inspector::event_editor::reconcile(&mut app);
    send(
        &mut app,
        Message::Inspector(Inspect::OpenChooser(
            TrackId(1),
            0,
            crate::inspector::lane_view::chooser::Which::Device,
        )),
    );
    assert!(matches!(app.overlay, Some(Overlay::Chooser(_))));
    app
}

fn playing_after_space(app: &mut App) -> bool {
    let started = std::time::Instant::now();
    press(app, Key::Named(Named::Space), Modifiers::NONE, true);
    // Give a (wrongly) started transport time to report back.
    while started.elapsed() < std::time::Duration::from_millis(150) {
        app.engine.poll_events();
        app.snapshot = app.engine.snapshot();
        if app.snapshot.playing {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

#[test]
fn space_does_not_start_the_transport_while_a_menu_is_open() {
    let mut app = app();
    clip_menu(&mut app);
    assert!(!playing_after_space(&mut app));
    assert!(
        matches!(app.overlay, Some(Overlay::Context(_))),
        "still open"
    );

    app.close_overlay();
    send(&mut app, Message::ToggleSectionsMenu);
    assert!(!playing_after_space(&mut app));

    let mut app = chooser_app();
    assert!(!playing_after_space(&mut app));
}

#[test]
fn space_starts_the_transport_again_once_the_menu_closes() {
    let mut app = app();
    clip_menu(&mut app);
    named(&mut app, Named::Escape);
    assert!(app.overlay.is_none());
    assert!(playing_after_space(&mut app));
}

#[test]
fn the_transport_key_itself_refuses_while_a_menu_is_open() {
    let mut app = app();
    let space = Key::Named(Named::Space);
    assert!(crate::shortcuts::transport_key(
        &app,
        &space,
        Modifiers::NONE,
        false,
        true
    ));
    clip_menu(&mut app);
    assert!(!crate::shortcuts::transport_key(
        &app,
        &space,
        Modifiers::NONE,
        false,
        true
    ));
}

#[test]
fn escape_closes_each_kind_of_menu() {
    let mut app = app();
    clip_menu(&mut app);
    named(&mut app, Named::Escape);
    assert!(app.overlay.is_none());

    send(&mut app, Message::ToggleSectionsMenu);
    assert_eq!(app.overlay, Some(Overlay::SectionsMenu));
    named(&mut app, Named::Escape);
    assert!(app.overlay.is_none());

    let mut app = chooser_app();
    named(&mut app, Named::Escape);
    assert!(app.overlay.is_none());
}

#[test]
fn up_and_down_step_through_the_enabled_rows_and_stop_at_the_ends() {
    let mut app = app();
    clip_menu(&mut app);
    let Some(Overlay::Context(open)) = app.overlay.clone() else {
        unreachable!()
    };
    let count = crate::context_menu::rows(&app, &open).presses.len();
    assert!(count > 2, "the clip menu lists several enabled commands");

    assert_eq!(app.menu_highlight, None);
    named(&mut app, Named::ArrowDown);
    assert_eq!(app.menu_highlight, Some(0));
    named(&mut app, Named::ArrowDown);
    assert_eq!(app.menu_highlight, Some(1));
    named(&mut app, Named::ArrowUp);
    named(&mut app, Named::ArrowUp);
    assert_eq!(app.menu_highlight, Some(0), "stops at the top");
    for _ in 0..count + 3 {
        named(&mut app, Named::ArrowDown);
    }
    assert_eq!(app.menu_highlight, Some(count - 1), "stops at the bottom");
}

#[test]
fn up_from_nothing_lands_on_the_last_row() {
    let mut app = app();
    clip_menu(&mut app);
    named(&mut app, Named::ArrowUp);
    let Some(Overlay::Context(open)) = app.overlay.clone() else {
        unreachable!()
    };
    let count = crate::context_menu::rows(&app, &open).presses.len();
    assert_eq!(app.menu_highlight, Some(count - 1));
}

#[test]
fn enter_with_nothing_highlighted_chooses_nothing() {
    let mut app = app();
    clip_menu(&mut app);
    named(&mut app, Named::Enter);
    assert!(matches!(app.overlay, Some(Overlay::Context(_))));
}

#[test]
fn enter_sends_the_highlighted_rows_own_message() {
    let mut app = app();
    clip_menu(&mut app);
    let Some(Overlay::Context(open)) = app.overlay.clone() else {
        unreachable!()
    };
    let rows = crate::context_menu::rows(&app, &open);
    // Cut is the first row of a clip's menu, and removes the clip.
    assert!(matches!(
        rows.presses[0],
        Message::Edit(EditMessage::CutSelectedClip)
    ));
    let track = app.project().tracks[0].id;
    assert!(app.project().clip(track, 0).is_some());
    named(&mut app, Named::ArrowDown);
    named(&mut app, Named::Enter);
    assert!(app.overlay.is_none(), "choosing closes the menu");
    assert!(app.project().clip(track, 0).is_none(), "Cut ran");
}

#[test]
fn disabled_rows_are_skipped() {
    let mut app = app();
    clip_menu(&mut app);
    let Some(Overlay::Context(open)) = app.overlay.clone() else {
        unreachable!()
    };
    let all = crate::context_menu::current_entries(&app, open.target.clone());
    let commands = all
        .iter()
        .filter(|e| matches!(e, crate::context_menu::Entry::Command { .. }))
        .count();
    let reachable = crate::context_menu::rows(&app, &open).presses.len();
    assert!(
        reachable < commands,
        "Paste is disabled with an empty clipboard ({reachable} of {commands})"
    );
}

#[test]
fn the_sections_menu_toggles_the_highlighted_section_on_enter() {
    let mut app = app();
    send(&mut app, Message::ToggleSectionsMenu);
    let before = app.chrome.section(crate::grid::Row::Path);
    named(&mut app, Named::ArrowDown);
    named(&mut app, Named::ArrowDown);
    assert_eq!(app.menu_highlight, Some(1), "Path is the second line");
    named(&mut app, Named::Enter);
    assert!(app.overlay.is_none());
    assert_ne!(app.chrome.section(crate::grid::Row::Path), before);
}

#[test]
fn a_chooser_starts_from_its_chosen_entry_and_enter_picks_the_next() {
    let mut app = chooser_app();
    let Some(Overlay::Chooser(open)) = app.overlay else {
        unreachable!()
    };
    let rows = crate::inspector::lane_view::chooser::key_rows(&app, open);
    assert_eq!(rows.presses.len(), 2, "BopOS and Mixer");
    // Nothing is chosen yet, so Down lands on the first and Down again on the second.
    named(&mut app, Named::ArrowDown);
    named(&mut app, Named::ArrowDown);
    assert_eq!(app.menu_highlight, Some(1));
    named(&mut app, Named::Enter);
    let choice = crate::inspector::lane_view::current(&app, TrackId(1), 0);
    assert_eq!(
        choice.device,
        Some(qtable_engine::model::DeviceRef::Mixer),
        "Enter chose Mixer"
    );
}

#[test]
fn other_plain_keys_are_swallowed_but_chords_pass() {
    let mut app = app();
    clip_menu(&mut app);
    press(&mut app, Key::Character("b".into()), Modifiers::NONE, true);
    assert!(matches!(app.overlay, Some(Overlay::Context(_))));
    assert_eq!(app.menu_highlight, None);
    // Cmd-, still opens Settings, which replaces the menu.
    press(
        &mut app,
        Key::Character(",".into()),
        Modifiers::COMMAND,
        true,
    );
    assert!(matches!(app.overlay, Some(Overlay::Settings(_))));
}

#[test]
fn opening_or_closing_a_menu_clears_the_highlight() {
    let mut app = app();
    clip_menu(&mut app);
    named(&mut app, Named::ArrowDown);
    assert!(app.menu_highlight.is_some());
    send(&mut app, Message::ToggleSectionsMenu);
    assert_eq!(app.menu_highlight, None);
}
