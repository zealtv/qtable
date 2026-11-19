//! Space, Cmd-S, Cmd-Shift-S and Cmd-, against the focus table (undo design, section 9).
use super::*;
use crate::drafts::DraftKey;
use crate::overlay::Overlay;
use crate::status::Severity;
use qtable_engine::engine::Engine;
use std::time::{Duration, Instant};

fn app() -> App {
    let engine = Engine::new(crate::files::new_project()).expect("engine starts");
    App::new(engine, String::new(), None)
}

/// One key press as the window sends it; the task it returns says whether anything was started.
fn press(app: &mut App, key: Key, modifiers: Modifiers, ignored: bool) -> Task<Message> {
    let task = crate::update::update(
        app,
        Message::KeyPressed {
            key,
            modifiers,
            repeat: false,
            ignored,
        },
    );
    crate::gesture_steps::boundary(app);
    task
}

fn chr(app: &mut App, character: &str, modifiers: Modifiers) -> Task<Message> {
    press(app, Key::Character(character.into()), modifiers, true)
}

fn cmd_s(app: &mut App) -> Task<Message> {
    chr(app, "s", Modifiers::COMMAND)
}

fn cmd_shift_s(app: &mut App) -> Task<Message> {
    chr(app, "S", Modifiers::COMMAND | Modifiers::SHIFT)
}

fn cmd_comma(app: &mut App) -> Task<Message> {
    chr(app, ",", Modifiers::COMMAND)
}

fn space(app: &mut App, ignored: bool) -> Task<Message> {
    press(app, Key::Named(Named::Space), Modifiers::NONE, ignored)
}

fn type_tempo(app: &mut App, text: &str) {
    let _ = crate::update::update(app, Message::TempoChanged(text.into()));
    assert_eq!(app.pending_field, Some(DraftKey::Tempo));
}

fn tempo(app: &App) -> f64 {
    app.project().tempo.bpm()
}

fn settings_open(app: &App) -> bool {
    matches!(app.overlay, Some(Overlay::Settings(_)))
}

fn wait(app: &mut App, condition: impl Fn(&qtable_engine::engine::Snapshot) -> bool) {
    let started = Instant::now();
    loop {
        app.engine.poll_events();
        app.snapshot = app.engine.snapshot();
        if condition(&app.snapshot) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "scheduler state did not arrive"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn the_chords_are_recognised_with_cmd_alone_or_with_shift() {
    let s = Key::Character("s".into());
    let capital = Key::Character("S".into());
    let comma = Key::Character(",".into());
    assert_eq!(chord(&s, Modifiers::COMMAND), Some(Shortcut::Save));
    assert_eq!(chord(&capital, Modifiers::COMMAND), Some(Shortcut::Save));
    let shifted = Modifiers::COMMAND | Modifiers::SHIFT;
    assert_eq!(chord(&capital, shifted), Some(Shortcut::SaveAs));
    assert_eq!(chord(&s, shifted), Some(Shortcut::SaveAs));
    assert_eq!(chord(&comma, Modifiers::COMMAND), Some(Shortcut::Settings));
    // Undo and redo stay on the same path.
    let z = Key::Character("z".into());
    assert!(matches!(
        chord(&z, Modifiers::COMMAND),
        Some(Shortcut::History(crate::undo::Step::Undo))
    ));
    assert!(matches!(
        chord(&z, shifted),
        Some(Shortcut::History(crate::undo::Step::Redo))
    ));
}

#[test]
fn plain_and_other_modified_keys_are_not_chords() {
    let s = Key::Character("s".into());
    let comma = Key::Character(",".into());
    for modifiers in [
        Modifiers::NONE,
        Modifiers::SHIFT,
        Modifiers::ALT,
        Modifiers::COMMAND | Modifiers::ALT,
    ] {
        assert_eq!(chord(&s, modifiers), None, "{modifiers:?}");
        assert_eq!(chord(&comma, modifiers), None, "{modifiers:?}");
    }
    assert_eq!(
        chord(&comma, Modifiers::COMMAND | Modifiers::SHIFT),
        None,
        "Cmd-Shift-comma is not Settings"
    );
}

#[test]
fn space_starts_and_stops_the_transport() {
    let mut app = app();
    assert!(!app.snapshot.playing);
    let _ = space(&mut app, true);
    wait(&mut app, |snapshot| snapshot.playing);
    let _ = space(&mut app, true);
    wait(&mut app, |snapshot| !snapshot.playing);
}

#[test]
fn space_types_a_space_while_a_text_field_has_the_keyboard() {
    let mut app = app();
    // The field captured the key, so the window hears it as not ignored.
    let _ = space(&mut app, false);
    assert!(app.fast_poll_until.is_none(), "no transport command ran");
    assert!(!app.snapshot.playing);
    // Typing waiting for its commit keeps the key even if a space reaches the window.
    type_tempo(&mut app, "93");
    let _ = space(&mut app, true);
    assert!(app.fast_poll_until.is_none(), "no transport command ran");
    assert_eq!(app.pending_field, Some(DraftKey::Tempo), "the draft stays");
}

#[test]
fn a_held_space_or_a_modified_one_does_nothing() {
    let mut app = app();
    let _ = crate::update::update(
        &mut app,
        Message::KeyPressed {
            key: Key::Named(Named::Space),
            modifiers: Modifiers::NONE,
            repeat: true,
            ignored: true,
        },
    );
    for modifiers in [Modifiers::SHIFT, Modifiers::ALT, Modifiers::COMMAND] {
        let _ = press(&mut app, Key::Named(Named::Space), modifiers, true);
    }
    assert!(app.fast_poll_until.is_none());
}

#[test]
fn space_picks_in_an_open_route_list_and_does_not_start_the_transport() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    app.open_route_dropdown(track);
    assert!(matches!(app.overlay, Some(Overlay::Dropdown(_))));
    let _ = space(&mut app, true);
    assert!(app.fast_poll_until.is_none(), "the list took the key");
}

#[test]
fn space_works_in_key_map_mode_because_it_is_not_a_typed_character() {
    let mut app = app();
    app.map_mode.on = true;
    app.map_mode.mode = crate::map_mode::Mode::Key;
    let _ = space(&mut app, true);
    wait(&mut app, |snapshot| snapshot.playing);
}

#[test]
fn cmd_s_saves_and_cmd_shift_s_asks_where() {
    let mut app = app();
    app.path = Some("set.ron".into());
    assert!(cmd_s(&mut app).units() > 0, "Save starts a write");
    assert!(
        cmd_shift_s(&mut app).units() > 0,
        "Save As opens the picker"
    );
    // A held chord does not save again.
    let held = crate::update::update(
        &mut app,
        Message::KeyPressed {
            key: Key::Character("s".into()),
            modifiers: Modifiers::COMMAND,
            repeat: true,
            ignored: true,
        },
    );
    assert_eq!(held.units(), 0);
}

#[test]
fn the_save_chords_work_whether_or_not_a_widget_took_the_key() {
    let mut app = app();
    let task = press(
        &mut app,
        Key::Character("s".into()),
        Modifiers::COMMAND,
        false,
    );
    assert!(task.units() > 0);
}

#[test]
fn cmd_s_commits_unsaved_typing_first_and_then_saves() {
    for chord_key in [cmd_s as fn(&mut App) -> Task<Message>, cmd_shift_s] {
        let mut app = app();
        type_tempo(&mut app, "93");
        assert_ne!(tempo(&app), 93.0);
        let task = chord_key(&mut app);
        assert_eq!(tempo(&app), 93.0, "the typed value is in the set");
        assert_eq!(app.pending_field, None);
        assert!(task.units() > 0, "and the save started");
    }
}

#[test]
fn the_typing_commits_as_a_step_of_its_own() {
    let mut app = app();
    type_tempo(&mut app, "93");
    let _ = cmd_s(&mut app);
    assert!(app.engine.can_undo());
    let _ = chr(&mut app, "z", Modifiers::COMMAND);
    assert_ne!(tempo(&app), 93.0, "one undo takes the typed value back");
}

#[test]
fn a_rejected_value_shows_its_error_and_nothing_saves() {
    for chord_key in [cmd_s as fn(&mut App) -> Task<Message>, cmd_shift_s] {
        let mut app = app();
        let before = tempo(&app);
        type_tempo(&mut app, "fast");
        let task = chord_key(&mut app);
        assert_eq!(task.units(), 0, "no save, no picker");
        assert_eq!(tempo(&app), before);
        assert_eq!(app.status.severity(), Severity::Error);
        assert!(app.status.message().is_some());
        assert!(
            app.drafts.errors.contains_key(&DraftKey::Tempo),
            "the field keeps its error"
        );
    }
}

#[test]
fn a_rejected_value_keeps_blocking_every_save_until_it_is_dropped() {
    let mut app = app();
    let before = tempo(&app);
    type_tempo(&mut app, "fast");
    assert_eq!(cmd_s(&mut app).units(), 0, "the first press refuses");
    // The field still shows the rejected text, so the draft is still waiting.
    assert_eq!(app.pending_field, Some(DraftKey::Tempo));
    assert_eq!(app.tempo_input, "fast");
    assert_eq!(cmd_s(&mut app).units(), 0, "so does a second Cmd-S");
    assert_eq!(cmd_shift_s(&mut app).units(), 0, "and Cmd-Shift-S");
    assert_eq!(tempo(&app), before);
    assert_eq!(
        app.status.severity(),
        Severity::Error,
        "each refusal shows its error"
    );
    // Escape drops the draft; with nothing typed the chord saves again.
    let _ = crate::update::update(&mut app, Message::CancelField);
    assert_eq!(app.pending_field, None);
    assert!(cmd_s(&mut app).units() > 0);
}

#[test]
fn a_rename_in_progress_is_committed_before_the_save() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    crate::header_actions::begin_rename(&mut app, crate::grid::RenameTarget::Header(track));
    app.header_actions.draft = "Drums".into();
    let task = cmd_s(&mut app);
    assert_eq!(app.project().track(track).unwrap().name, "Drums");
    assert!(app.header_actions.renaming.is_none());
    assert!(task.units() > 0);
}

#[test]
fn a_rejected_rename_does_not_save() {
    let mut app = app();
    // Main's name is fixed, so the engine refuses a rename of it.
    let track = app.project().main.id;
    let before = app.project().track(track).unwrap().name.clone();
    app.header_actions.renaming = Some(crate::grid::RenameTarget::Header(track));
    app.header_actions.draft = "Mix".into();
    let task = cmd_s(&mut app);
    assert_eq!(app.project().track(track).unwrap().name, before);
    assert_eq!(task.units(), 0, "the engine refused the name");
    assert_eq!(app.status.severity(), Severity::Error);
}

#[test]
fn an_earlier_error_in_the_status_bar_does_not_block_saving() {
    let mut app = app();
    type_tempo(&mut app, "93");
    app.status.show_error("an earlier problem");
    assert!(cmd_s(&mut app).units() > 0);
}

/// The status bar as the engine leaves it when the scheduler has stopped: its message is
/// persistent, so it drops every message that follows.
fn with_scheduler_failure(app: &mut App) {
    let status = qtable_engine::status::EngineStatus {
        scheduler_failure: Some("boom".into()),
        ..Default::default()
    };
    app.status.poll(&status, Instant::now());
    assert_eq!(app.status.message(), Some("Scheduler stopped: boom"));
}

#[test]
fn a_rejected_value_blocks_saving_while_the_scheduler_failure_shows() {
    for chord_key in [cmd_s as fn(&mut App) -> Task<Message>, cmd_shift_s] {
        let mut app = app();
        with_scheduler_failure(&mut app);
        let before = tempo(&app);
        type_tempo(&mut app, "fast");
        assert_eq!(chord_key(&mut app).units(), 0, "no save, no picker");
        assert_eq!(
            chord_key(&mut app).units(),
            0,
            "and not on the second press"
        );
        assert_eq!(tempo(&app), before);
        assert_eq!(app.pending_field, Some(DraftKey::Tempo));
        assert!(app.drafts.errors.contains_key(&DraftKey::Tempo));
        assert_eq!(
            app.status.message(),
            Some("Scheduler stopped: boom"),
            "the persistent message stays"
        );
    }
}

#[test]
fn a_rejected_rename_blocks_saving_while_the_scheduler_failure_shows() {
    let mut app = app();
    with_scheduler_failure(&mut app);
    let track = app.project().main.id;
    app.header_actions.renaming = Some(crate::grid::RenameTarget::Header(track));
    app.header_actions.draft = "Mix".into();
    assert_eq!(cmd_s(&mut app).units(), 0, "the engine refused the name");
}

#[test]
fn a_valid_value_saves_while_the_scheduler_failure_shows() {
    let mut app = app();
    with_scheduler_failure(&mut app);
    type_tempo(&mut app, "93");
    assert!(cmd_s(&mut app).units() > 0);
    assert_eq!(tempo(&app), 93.0);
}

#[test]
fn cmd_comma_opens_settings() {
    let mut app = app();
    assert!(!settings_open(&app));
    let _ = cmd_comma(&mut app);
    assert!(settings_open(&app));
    // The shortcut opens; a second press leaves the pane open rather than closing it.
    let _ = cmd_comma(&mut app);
    assert!(settings_open(&app));
}

#[test]
fn cmd_comma_replaces_another_popover() {
    let mut app = app();
    app.open_overlay(Overlay::SectionsMenu);
    let _ = cmd_comma(&mut app);
    assert!(settings_open(&app));
}

#[test]
fn cmd_comma_commits_unsaved_typing_and_then_opens_settings() {
    let mut app = app();
    type_tempo(&mut app, "93");
    let _ = cmd_comma(&mut app);
    assert_eq!(tempo(&app), 93.0);
    assert_eq!(app.pending_field, None);
    assert!(settings_open(&app));
}

#[test]
fn cmd_comma_with_a_rejected_value_reverts_the_field_and_opens_settings() {
    let mut app = app();
    let before = tempo(&app);
    type_tempo(&mut app, "fast");
    let _ = cmd_comma(&mut app);
    assert_eq!(tempo(&app), before, "the set keeps its tempo");
    assert_eq!(
        app.pending_field, None,
        "the draft is dropped, as Escape drops it"
    );
    assert!(!app.drafts.errors.contains_key(&DraftKey::Tempo));
    assert_eq!(
        app.tempo_input,
        crate::fields::format_tempo(before),
        "the field shows the set's value again"
    );
    assert!(settings_open(&app));
    // Nothing is left to hold Space, so it reaches the transport.
    assert!(crate::shortcuts::transport_key(
        &app,
        &Key::Named(Named::Space),
        Modifiers::NONE,
        false,
        true
    ));
}

/// The tasks a chord's own work starts, and one more for letting go of the field.
fn units_with_release(app: &mut App, message: Message) -> usize {
    crate::update::update(app, message).units() + 1
}

#[test]
fn a_chord_that_goes_ahead_also_releases_the_field() {
    let mut app = app();
    app.path = Some("set.ron".into());
    let save = units_with_release(&mut app, Message::Files(FileMessage::Save));
    assert_eq!(cmd_s(&mut app).units(), save, "Cmd-S saves and releases");
    let save_as = units_with_release(&mut app, Message::Files(FileMessage::SaveAs));
    assert_eq!(cmd_shift_s(&mut app).units(), save_as, "Cmd-Shift-S too");
}

#[test]
fn cmd_comma_releases_the_field_whether_the_value_commits_or_not() {
    use crate::settings_pane::Action;
    let open = units_with_release(&mut app(), Message::Settings(Action::Toggle));
    let mut committed = app();
    type_tempo(&mut committed, "93");
    assert_eq!(cmd_comma(&mut committed).units(), open);
    let mut rejected = app();
    type_tempo(&mut rejected, "fast");
    assert_eq!(cmd_comma(&mut rejected).units(), open);
    // A second press, with the pane already open, still lets go of the field.
    assert!(cmd_comma(&mut rejected).units() >= 1);
}

#[test]
fn a_refused_save_keeps_the_keyboard_in_the_field() {
    let mut app = app();
    type_tempo(&mut app, "fast");
    for task in [cmd_s(&mut app), cmd_shift_s(&mut app)] {
        assert_eq!(task.units(), 0, "no release: the value still needs fixing");
    }
    assert_eq!(app.pending_field, Some(DraftKey::Tempo));
}
