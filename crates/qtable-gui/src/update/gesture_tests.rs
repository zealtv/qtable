//! Which edits make one undo step, through
//! the public update router and the remote host. Input boundaries arrive as the messages the
//! window subscription sends; a widget's own messages for an event come before them.
use super::update;
use crate::app::App;
use crate::browser;
use crate::drafts::{ClipField, DraftKey};
use crate::messages::{EditMessage, Message};
use crate::mixer_band::MixerMessage;
use iced::keyboard::{Key, key::Named};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use qtable_remote::Endpoint;

fn volume(track: u64) -> FieldRef {
    crate::mixer_device::volume_field(TrackId(track))
}

/// Three tracks with a clip on the first, each volume mapped to its own CC (1, 2, 3, absolute,
/// no pick-up) and track 1's Solo to note 60.
fn app() -> App {
    let mut tracks: Vec<_> = (1..=3)
        .map(|n| Track::new(TrackId(n), format!("T{n}"), 1))
        .collect();
    tracks[0].slots[0].clip = Some(
        Clip::new(
            ClipId(1),
            "Clip",
            BeatTime::from_beats(4).unwrap(),
            tracks[0].colour,
        )
        .unwrap(),
    );
    let mut project = Project {
        tracks,
        scenes: vec![Scene {
            id: SceneId(10),
            name: "Scene".into(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    };
    let mut map = |message, control| {
        let mut mapping = RemoteMapping::new(
            MappingId(project.mappings.len() as u64 + 1),
            Source::Midi(MidiSource {
                channel: 1,
                message,
            }),
            control,
            0.0,
            1.0,
        );
        mapping.takeover = Takeover::None;
        project.mappings.push(mapping);
    };
    for n in 1..=3 {
        map(MidiMessage::Cc(n), ControlRef::Argument(volume(n.into())));
    }
    map(MidiMessage::Note(60), ControlRef::TrackSolo(TrackId(1)));
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    app.status.clear_transient();
    app
}

fn send(app: &mut App, message: impl Into<Message>) {
    let _ = update(app, message.into());
}

/// The messages the subscription sends when the pointer is released, a key is released, and
/// the window loses focus.
fn release(app: &mut App) {
    send(app, Message::Browser(browser::Action::Release));
}
fn key_up(app: &mut App) {
    send(app, Message::KeyReleased(Key::Named(Named::ArrowUp)));
}

fn midi(app: &mut App, bytes: [u8; 3]) {
    app.remote.ingress.push_midi(
        &Endpoint::Midi {
            id: "test".into(),
            name: "Controller".into(),
        },
        &bytes,
    );
    send(app, Message::Remote(crate::remote::Action::Wake));
}

fn value(app: &App, track: u64) -> f64 {
    app.project().field(&volume(track)).unwrap().value
}

/// How many undo steps the history holds. It counts by undoing and redoing, which closes the
/// engine's open run (an undo does, and a redo lands closed steps), so call it last in a test,
/// never between two inputs whose joining the test is about.
fn steps(app: &mut App) -> usize {
    let mut count = 0;
    while app.engine.can_undo() {
        app.engine.execute(Command::Undo).unwrap();
        count += 1;
    }
    for _ in 0..count {
        app.engine.execute(Command::Redo).unwrap();
    }
    count
}

fn undo(app: &mut App) {
    app.engine.execute(Command::Undo).unwrap();
}

/// The subscription's report of a pointer button going down.
fn press(app: &mut App) {
    send(app, Message::PointerPressed);
}

/// ⌘Z through the key path, as the window sends it.
fn cmd_z(app: &mut App) {
    send(
        app,
        Message::KeyPressed {
            key: Key::Character("z".into()),
            modifiers: iced::keyboard::Modifiers::COMMAND,
            repeat: false,
            ignored: true,
        },
    );
}

fn drag(app: &mut App, track: u64, value: f64) {
    send(
        app,
        Message::Mixer(MixerMessage::DragVolume(TrackId(track), value)),
    );
}

fn drag_fader(app: &mut App, track: u64, values: &[f64]) {
    for &value in values {
        send(
            app,
            Message::Mixer(MixerMessage::DragVolume(TrackId(track), value)),
        );
    }
    send(
        app,
        Message::Mixer(MixerMessage::ReleaseVolume(
            TrackId(track),
            *values.last().unwrap(),
        )),
    );
    release(app);
}

fn clip_length(app: &mut App, text: &str) {
    let key = DraftKey::clip(TrackId(1), 0, ClipField::Length);
    send(
        app,
        Message::Inspector(crate::inspector::Message::Draft(key, text.into())),
    );
    send(
        app,
        Message::Inspector(crate::inspector::Message::CommitNumber),
    );
}

#[test]
fn a_fader_drag_is_one_step_and_one_undo_returns_the_start() {
    let mut app = app();
    let start = value(&app, 1);
    drag_fader(&mut app, 1, &[0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
    assert_eq!(value(&app, 1), 0.6);
    assert_eq!(steps(&mut app), 1);
    undo(&mut app);
    assert_eq!(value(&app, 1), start);
}

#[test]
fn two_drags_of_one_fader_make_two_steps() {
    let mut app = app();
    let start = value(&app, 1);
    drag_fader(&mut app, 1, &[0.1, 0.2, 0.3]);
    drag_fader(&mut app, 1, &[0.4, 0.5, 0.6]);
    assert_eq!(steps(&mut app), 2);
    undo(&mut app);
    assert_eq!(value(&app, 1), 0.3);
    undo(&mut app);
    assert_eq!(value(&app, 1), start);
}

#[test]
fn a_number_field_drag_is_one_step() {
    let mut app = app();
    let start = app.project().clip(TrackId(1), 0).unwrap().length();
    // Press, then one changed-and-commit pair per drag step, then release.
    for beats in ["5", "6", "7", "8"] {
        clip_length(&mut app, beats);
    }
    release(&mut app);
    assert_eq!(
        app.project().clip(TrackId(1), 0).unwrap().length(),
        BeatTime::from_beats(32).unwrap(),
        "the field counts bars"
    );
    assert_eq!(steps(&mut app), 1);
    undo(&mut app);
    assert_eq!(app.project().clip(TrackId(1), 0).unwrap().length(), start);
}

#[test]
fn a_held_arrow_key_is_one_step_and_separate_taps_are_separate_steps() {
    let mut app = app();
    // Held: the first press and its repeats nudge; one release ends it.
    for beats in ["5", "6", "7"] {
        clip_length(&mut app, beats);
    }
    key_up(&mut app);
    // Taps: a release after each.
    for beats in ["8", "9", "10"] {
        clip_length(&mut app, beats);
        key_up(&mut app);
    }
    // The held key's one step and the three taps; counting between them would close the run.
    assert_eq!(steps(&mut app), 4);
}

#[test]
fn two_clicks_on_two_toggles_make_two_steps() {
    let mut app = app();
    send(
        &mut app,
        Message::Mixer(MixerMessage::Activator(TrackId(1))),
    );
    release(&mut app);
    send(
        &mut app,
        Message::Mixer(MixerMessage::Activator(TrackId(2))),
    );
    release(&mut app);
    assert_eq!(steps(&mut app), 2);
    undo(&mut app);
    assert!(!app.project().track_facts(TrackId(1)).unwrap().mixer.active);
    assert!(app.project().track_facts(TrackId(2)).unwrap().mixer.active);
}

#[test]
fn several_edits_from_one_click_make_one_step() {
    let mut app = app();
    // A click whose messages edit twice (a Solo click that also clears another Solo).
    send(&mut app, Message::Mixer(MixerMessage::Solo(TrackId(1))));
    send(
        &mut app,
        Message::Mixer(MixerMessage::Activator(TrackId(2))),
    );
    release(&mut app);
    assert_eq!(steps(&mut app), 1);
}

#[test]
fn typing_a_name_then_clicking_a_toggle_makes_two_steps() {
    let mut app = app();
    let key = DraftKey::clip(TrackId(1), 0, ClipField::Name);
    send(
        &mut app,
        Message::Inspector(crate::inspector::Message::Draft(key, "Renamed".into())),
    );
    assert_eq!(app.pending_field, Some(key));
    // The click's message commits the draft first, in the same update.
    send(
        &mut app,
        Message::Mixer(MixerMessage::Activator(TrackId(2))),
    );
    release(&mut app);
    assert_eq!(app.project().clip(TrackId(1), 0).unwrap().name, "Renamed");
    assert_eq!(steps(&mut app), 2);
    undo(&mut app);
    assert!(app.project().track_facts(TrackId(2)).unwrap().mixer.active);
    assert_eq!(app.project().clip(TrackId(1), 0).unwrap().name, "Renamed");
    undo(&mut app);
    assert_eq!(app.project().clip(TrackId(1), 0).unwrap().name, "Clip");
}

#[test]
fn a_typed_value_and_its_enter_make_one_step_and_the_next_typed_value_another() {
    let mut app = app();
    let start = app.project().clip(TrackId(1), 0).unwrap().length();
    // Typing changes the draft only; Enter commits, and its key's release ends the span.
    let key = DraftKey::clip(TrackId(1), 0, ClipField::Length);
    for text in ["5", "56", "6"] {
        send(
            &mut app,
            Message::Inspector(crate::inspector::Message::Draft(key, text.into())),
        );
    }
    send(
        &mut app,
        Message::Inspector(crate::inspector::Message::CommitNumber),
    );
    key_up(&mut app);
    let first = app.project().clip(TrackId(1), 0).unwrap().length();
    assert_ne!(first, start);
    clip_length(&mut app, "8");
    key_up(&mut app);
    // Two typed values, two steps: the second undo returns to the start.
    undo(&mut app);
    assert_eq!(app.project().clip(TrackId(1), 0).unwrap().length(), first);
    undo(&mut app);
    assert_eq!(app.project().clip(TrackId(1), 0).unwrap().length(), start);
    assert!(!app.engine.can_undo());
}

// Live check, 2026-11-11, through the remote path.

#[test]
fn one_mapped_control_moved_paused_and_moved_again_is_one_step() {
    let mut app = app();
    let start = value(&app, 1);
    midi(&mut app, [0xb0, 1, 30]);
    midi(&mut app, [0xb0, 1, 31]);
    // A pause, with the GUI doing other things that make no step.
    send(&mut app, Message::Tick);
    send(&mut app, Message::Play);
    midi(&mut app, [0xb0, 1, 90]);
    assert_eq!(steps(&mut app), 1);
    undo(&mut app);
    assert_eq!(value(&app, 1), start);
}

#[test]
fn two_mapped_controls_in_turn_make_two_steps_a_drag_a_third_and_the_first_again_a_fourth() {
    let mut app = app();
    let start = [value(&app, 1), value(&app, 2), value(&app, 3)];
    midi(&mut app, [0xb0, 1, 30]);
    midi(&mut app, [0xb0, 1, 60]);
    midi(&mut app, [0xb0, 2, 40]);
    midi(&mut app, [0xb0, 2, 70]);
    // A click-and-drag on a third control, then the first control again.
    drag_fader(&mut app, 3, &[0.1, 0.2, 0.9]);
    midi(&mut app, [0xb0, 1, 100]);
    // Four steps, undone newest first. Nothing counted them beforehand: counting by undoing
    // would have closed the run the test is about.
    undo(&mut app);
    assert_eq!(value(&app, 1), 60.0 / 127.0, "the first control's new step");
    assert_eq!(value(&app, 3), 0.9);
    undo(&mut app);
    assert_eq!(value(&app, 3), start[2], "the drag");
    assert_ne!(value(&app, 2), start[1]);
    undo(&mut app);
    assert_eq!(value(&app, 2), start[1], "the second control, both moves");
    assert_ne!(value(&app, 1), start[0]);
    undo(&mut app);
    assert_eq!(value(&app, 1), start[0], "the first control, both moves");
    assert!(!app.engine.can_undo());
}

#[test]
fn a_mapped_switch_press_makes_its_own_step_and_ends_the_controls_step() {
    let mut app = app();
    midi(&mut app, [0xb0, 1, 30]);
    midi(&mut app, [0x90, 60, 100]);
    assert!(app.project().track_facts(TrackId(1)).unwrap().mixer.solo);
    // The control moved after the switch starts a step above it.
    midi(&mut app, [0xb0, 1, 90]);
    assert_eq!(steps(&mut app), 3);
}

#[test]
fn a_mapped_control_keeps_its_step_until_the_set_is_loaded_again() {
    let mut app = app();
    midi(&mut app, [0xb0, 1, 30]);
    let project = app.project().clone();
    app.engine.execute(Command::LoadProject(project)).unwrap();
    assert_eq!(crate::gesture_steps::control_tokens(&app), 1);
    app.reset_for_project();
    assert_eq!(
        crate::gesture_steps::control_tokens(&app),
        0,
        "a load drops every control's token"
    );
    midi(&mut app, [0xb0, 1, 90]);
    assert_eq!(steps(&mut app), 1);
    assert_eq!(app.engine.undo_label(), Some("Set Value"));
}

#[test]
fn four_alternating_mapped_moves_undo_b2_a2_b1_a1() {
    // Live check, 2026-11-11: fader A, B, A, B, through the remote path. Live undoes
    // B's last move, A's last, B's first, A's first.
    let mut app = app();
    let (a0, b0) = (value(&app, 1), value(&app, 2));
    midi(&mut app, [0xb0, 1, 30]);
    midi(&mut app, [0xb0, 2, 40]);
    midi(&mut app, [0xb0, 1, 60]);
    midi(&mut app, [0xb0, 2, 70]);
    let cc = |v: f64| v / 127.0;
    undo(&mut app);
    assert_eq!((value(&app, 1), value(&app, 2)), (cc(60.0), cc(40.0)), "B2");
    undo(&mut app);
    assert_eq!((value(&app, 1), value(&app, 2)), (cc(30.0), cc(40.0)), "A2");
    undo(&mut app);
    assert_eq!((value(&app, 1), value(&app, 2)), (cc(30.0), b0), "B1");
    undo(&mut app);
    assert_eq!((value(&app, 1), value(&app, 2)), (a0, b0), "A1");
    assert!(!app.engine.can_undo());
}

#[test]
fn alternating_mapped_controls_make_a_step_for_each_move_after_the_first_pair() {
    let mut app = app();
    for n in 0..20u8 {
        midi(&mut app, [0xb0, 1, 10 + n]);
        midi(&mut app, [0xb0, 2, 100 - n]);
    }
    assert_eq!(steps(&mut app), 40);
}

#[test]
fn a_gui_edit_closes_the_run_so_a_controller_move_after_it_starts_a_step() {
    let mut app = app();
    midi(&mut app, [0xb0, 1, 30]);
    // A click: the edit and then the release.
    send(&mut app, Message::Edit(EditMessage::AddScene));
    release(&mut app);
    midi(&mut app, [0xb0, 1, 90]);
    assert_eq!(steps(&mut app), 3);
}

#[test]
fn a_launch_between_two_moves_of_one_control_does_not_split_its_step() {
    let mut app = app();
    midi(&mut app, [0xb0, 1, 30]);
    send(&mut app, Message::Play);
    send(&mut app, Message::Stop);
    release(&mut app);
    midi(&mut app, [0xb0, 1, 90]);
    assert_eq!(steps(&mut app), 1);
}

// Input arriving in the middle of a gesture.

#[test]
fn a_mapped_control_a_mapped_switch_an_undo_and_the_control_again_do_not_share_a_step() {
    let mut app = app();
    let start = value(&app, 1);
    midi(&mut app, [0xb0, 1, 30]);
    midi(&mut app, [0x90, 60, 100]);
    cmd_z(&mut app);
    assert!(!app.project().track_facts(TrackId(1)).unwrap().mixer.solo);
    midi(&mut app, [0xb0, 1, 90]);
    undo(&mut app);
    assert_eq!(
        value(&app, 1),
        30.0 / 127.0,
        "only the later move is taken back"
    );
    undo(&mut app);
    assert_eq!(value(&app, 1), start);
    assert!(!app.engine.can_undo());
}

#[test]
fn letting_go_of_a_modifier_before_the_key_does_not_end_the_held_keys_step() {
    // A fine nudge with an arrow held under Shift, Shift released first.
    let mut app = app();
    let start = app.project().clip(TrackId(1), 0).unwrap().length();
    clip_length(&mut app, "5");
    send(&mut app, Message::KeyReleased(Key::Named(Named::Shift)));
    send(&mut app, Message::KeyReleased(Key::Named(Named::Super)));
    clip_length(&mut app, "6");
    key_up(&mut app);
    undo(&mut app);
    assert_eq!(
        app.project().clip(TrackId(1), 0).unwrap().length(),
        start,
        "one undo takes back every nudge"
    );
    assert!(!app.engine.can_undo());
}

#[test]
fn a_key_released_while_a_pointer_button_is_down_does_not_split_the_drag() {
    let mut app = app();
    let start = value(&app, 1);
    press(&mut app);
    drag(&mut app, 1, 0.2);
    key_up(&mut app);
    drag(&mut app, 1, 0.3);
    release(&mut app);
    undo(&mut app);
    assert_eq!(value(&app, 1), start);
    assert!(!app.engine.can_undo());
    // With no button down the same release is a boundary: two taps, two steps.
    clip_length(&mut app, "5");
    key_up(&mut app);
    clip_length(&mut app, "6");
    key_up(&mut app);
    assert_eq!(steps(&mut app), 2);
}

#[test]
fn a_pointer_release_after_a_key_release_in_a_drag_still_ends_the_drag() {
    let mut app = app();
    press(&mut app);
    drag(&mut app, 1, 0.2);
    key_up(&mut app);
    release(&mut app);
    // The next span: its edits are a step of their own.
    drag(&mut app, 1, 0.5);
    release(&mut app);
    assert_eq!(steps(&mut app), 2);
}

#[test]
fn the_subscriptions_press_after_a_widgets_press_edit_does_not_split_the_click() {
    // A Solo click acts on the press (the widget's message comes first) and the subscription
    // reports the same press afterwards; the click's other edit follows. Were the press a
    // boundary, the click's edits would fall in two steps.
    let mut app = app();
    send(&mut app, Message::Mixer(MixerMessage::Solo(TrackId(1))));
    press(&mut app);
    send(
        &mut app,
        Message::Mixer(MixerMessage::Activator(TrackId(2))),
    );
    release(&mut app);
    assert_eq!(steps(&mut app), 1);
}

#[test]
fn undo_in_the_middle_of_a_fader_drag_lets_the_drag_carry_on_as_one_new_step() {
    let mut app = app();
    let start = value(&app, 1);
    press(&mut app);
    drag(&mut app, 1, 0.1);
    drag(&mut app, 1, 0.2);
    cmd_z(&mut app);
    assert_eq!(value(&app, 1), start);
    drag(&mut app, 1, 0.3);
    // Z and Cmd go up while the button is still down.
    send(&mut app, Message::KeyReleased(Key::Character("z".into())));
    send(&mut app, Message::KeyReleased(Key::Named(Named::Super)));
    drag(&mut app, 1, 0.4);
    release(&mut app);
    assert_eq!(value(&app, 1), 0.4);
    assert!(!app.engine.can_redo());
    undo(&mut app);
    assert_eq!(value(&app, 1), start);
    assert!(!app.engine.can_undo(), "the rest of the drag is one step");
}

#[test]
fn undo_while_a_key_is_held_lets_the_repeats_carry_on_as_one_new_step() {
    let mut app = app();
    let start = app.project().clip(TrackId(1), 0).unwrap().length();
    clip_length(&mut app, "5");
    clip_length(&mut app, "6");
    cmd_z(&mut app);
    assert_eq!(app.project().clip(TrackId(1), 0).unwrap().length(), start);
    clip_length(&mut app, "7");
    clip_length(&mut app, "8");
    key_up(&mut app);
    undo(&mut app);
    assert_eq!(app.project().clip(TrackId(1), 0).unwrap().length(), start);
    assert!(!app.engine.can_undo());
}

#[test]
fn undo_in_the_middle_of_a_controller_run_starts_a_new_step_for_the_next_move() {
    let mut app = app();
    let start = value(&app, 1);
    midi(&mut app, [0xb0, 1, 30]);
    midi(&mut app, [0xb0, 1, 60]);
    cmd_z(&mut app);
    assert_eq!(value(&app, 1), start);
    midi(&mut app, [0xb0, 1, 90]);
    midi(&mut app, [0xb0, 1, 91]);
    assert!(!app.engine.can_redo());
    undo(&mut app);
    assert_eq!(value(&app, 1), start);
    assert!(!app.engine.can_undo(), "the later moves are one step");
}

#[test]
fn a_fold_with_the_mouse_between_two_moves_of_a_control_does_not_close_its_step() {
    let mut app = app();
    app.engine
        .execute(Command::Edit(
            qtable_engine::edit::ProjectEdit::CreateGroup {
                tracks: vec![TrackId(2), TrackId(3)],
            },
        ))
        .unwrap();
    let group = app
        .project()
        .tracks
        .iter()
        .find(|track| track.is_group())
        .unwrap()
        .id;
    let start = value(&app, 1);
    midi(&mut app, [0xb0, 1, 30]);
    send(
        &mut app,
        Message::Grid(crate::grid::Action::ToggleGroup(group)),
    );
    release(&mut app);
    midi(&mut app, [0xb0, 1, 90]);
    undo(&mut app);
    assert_eq!(value(&app, 1), start, "both moves are one step");
    assert!(app.engine.can_undo(), "the group's creation remains");
}

// Decided 2026-11-12: mappings stay outside the undo history.

fn map_cc(app: &mut App, number: u8, track: u64) {
    app.engine
        .execute(Command::Edit(
            qtable_engine::edit::ProjectEdit::EditMapping(qtable_engine::edit::MappingEdit::Add {
                source: Source::Midi(MidiSource {
                    channel: 1,
                    message: MidiMessage::Cc(number),
                }),
                control: ControlRef::Argument(volume(track)),
                min: 0.0,
                max: 1.0,
            }),
        ))
        .unwrap();
}

#[test]
fn map_a_control_move_it_undo_and_undo_again_leaves_the_mapping_driving_the_control() {
    let mut app = app();
    let third = value(&app, 3);
    // A step before the mapping.
    drag(&mut app, 3, 0.25);
    release(&mut app);
    map_cc(&mut app, 9, 2);
    let mapped = app.project().mappings.clone();
    let start = value(&app, 2);
    midi(&mut app, [0xb0, 9, 30]);
    assert_ne!(value(&app, 2), start, "the new mapping moves the control");
    cmd_z(&mut app);
    assert_eq!(value(&app, 2), start, "the first undo puts the value back");
    assert_eq!(app.project().mappings, mapped);
    cmd_z(&mut app);
    assert_eq!(
        value(&app, 3),
        third,
        "the second undo takes the step before"
    );
    assert_eq!(app.project().mappings, mapped, "the mapping is still there");
    midi(&mut app, [0xb0, 9, 30]);
    assert_ne!(value(&app, 2), start, "and it still drives the control");
}

#[test]
fn a_mapping_edit_marks_the_set_unsaved_and_makes_no_step() {
    let mut app = app();
    app.mark_saved();
    assert!(!app.is_dirty());
    map_cc(&mut app, 9, 2);
    assert!(app.is_dirty(), "mapping changes the set as Live marks it");
    assert_eq!(steps(&mut app), 0);
}

// As of 2026-11-12, Live brings a deleted track's MIDI mapping back with it.

#[test]
fn delete_a_mapped_track_then_cmd_z_and_the_knob_drives_it_again() {
    let mut app = app();
    let mapped = app.project().mappings.clone();
    app.engine
        .execute(Command::Edit(
            qtable_engine::edit::ProjectEdit::RemoveTracks {
                tracks: vec![TrackId(2)],
            },
        ))
        .unwrap();
    assert_eq!(app.project().mappings.len(), mapped.len() - 1);
    // The knob has nothing to drive while the track is gone, and the others still answer.
    midi(&mut app, [0xb0, 2, 30]);
    cmd_z(&mut app);
    assert!(app.project().track(TrackId(2)).is_some());
    assert_eq!(
        app.project().mappings,
        mapped,
        "the mapping came back with the track"
    );
    let start = value(&app, 2);
    midi(&mut app, [0xb0, 2, 100]);
    assert_ne!(value(&app, 2), start, "the knob drives the returned track");
}
