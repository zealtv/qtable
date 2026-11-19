use super::*;
use crate::messages::Message;
use iced::Rectangle;
use qtable_engine::engine::Engine;
use qtable_engine::model::*;
use qtable_remote::Endpoint;

pub(super) fn app() -> App {
    let mut project = crate::files::new_project();
    project.tracks[0].slots[0].clip = Some(
        Clip::new(
            ClipId(1),
            "Clip",
            BeatTime::from_beats(4).unwrap(),
            project.tracks[0].colour,
        )
        .unwrap(),
    );
    App::new(Engine::new(project).unwrap(), "120.00".into(), None)
}

pub(super) fn volume() -> ControlRef {
    ControlRef::Argument(FieldRef::volume(TrackId(1)))
}

pub(super) fn slot() -> ControlRef {
    ControlRef::ClipSlot {
        track: TrackId(1),
        scene: SceneId(1),
    }
}

pub(super) fn send(app: &mut App, bytes: &[u8]) {
    app.remote.ingress.push(
        Endpoint::Midi {
            id: "test".into(),
            name: "Controller".into(),
        },
        Input::Midi(bytes.to_vec().into_boxed_slice()),
    );
    let _ = crate::update::update(app, Message::Remote(crate::remote::Action::Wake));
}

pub(super) fn act(app: &mut App, action: Action) {
    let _ = crate::update::update(app, Message::Map(action));
}

fn cc(channel: u8, number: u8) -> Source {
    Source::Midi(MidiSource {
        channel,
        message: MidiMessage::Cc(number),
    })
}

#[test]
fn the_switch_turns_map_mode_on_and_off_and_forgets_the_pending_control() {
    let mut app = app();
    assert!(!app.map_mode.on);
    act(&mut app, Action::Toggle);
    assert!(app.map_mode.on);
    act(&mut app, Action::Pick(volume()));
    assert_eq!(app.map_mode.selected_control, Some(volume()));
    act(&mut app, Action::Toggle);
    assert!(!app.map_mode.on);
    assert_eq!(app.map_mode.selected_control, None);
}

#[test]
fn command_m_is_the_shortcut() {
    use iced::keyboard::{Key, Modifiers};
    assert!(shortcut(
        &Key::Character("m".into()),
        Modifiers::COMMAND,
        false
    ));
    assert!(!shortcut(
        &Key::Character("m".into()),
        Modifiers::COMMAND,
        true
    ));
    assert!(!shortcut(
        &Key::Character("m".into()),
        Modifiers::empty(),
        false
    ));
    assert!(!shortcut(
        &Key::Character("m".into()),
        Modifiers::COMMAND | Modifiers::ALT,
        false
    ));
}

#[test]
fn clicking_a_control_then_moving_a_controller_maps_it_with_the_whole_range() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(volume()));
    send(&mut app, &[0xb0, 21, 64]);
    let [row] = app.project().mappings.as_slice() else {
        panic!("one mapping: {:?}", app.project().mappings);
    };
    assert_eq!(row.source, cc(1, 21));
    assert_eq!(row.control, volume());
    let range = describe_control(app.project(), &volume())
        .unwrap()
        .range
        .unwrap();
    assert_eq!((row.min, row.max), (range[0], range[1]));
    assert_eq!(row.takeover, Takeover::None);
    assert_eq!(app.map_mode.selected_control, None);
    assert_eq!(app.map_mode.selected_row, Some(row.id));
    assert!(app.is_dirty());
    // Learning is over: wiggling the knob again adds nothing.
    send(&mut app, &[0xb0, 21, 90]);
    assert_eq!(app.project().mappings.len(), 1);
}

#[test]
fn a_new_continuous_mapping_operates_immediately_without_a_pickup_crossing() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(volume()));
    send(&mut app, &[0xb0, 21, 64]);
    act(&mut app, Action::Toggle);
    send(&mut app, &[0xb0, 21, 0]);
    assert_eq!(
        describe_control(app.project(), &volume()).unwrap().value,
        Some(0.0)
    );
}

#[test]
fn a_controller_does_not_operate_the_set_in_map_mode() {
    let mut app = app();
    // Map CC 21 to Volume with Map Mode on, then move it still in Map Mode.
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(volume()));
    send(&mut app, &[0xb0, 21, 10]);
    let before = describe_control(app.project(), &volume()).unwrap().value;
    let revision = app.project_revision();
    send(&mut app, &[0xb0, 21, 127]);
    assert_eq!(
        describe_control(app.project(), &volume()).unwrap().value,
        before
    );
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn a_second_control_for_the_same_source_fans_out_without_a_prompt() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(volume()));
    send(&mut app, &[0xb0, 22, 1]);
    act(&mut app, Action::Pick(ControlRef::TrackSolo(TrackId(1))));
    send(&mut app, &[0xb0, 22, 1]);
    let rows = &app.project().mappings;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.source == cc(1, 22)));
    // Relearning one target replaces its MIDI source and leaves the fan-out sibling alone.
    act(&mut app, Action::Pick(volume()));
    send(&mut app, &[0xb0, 23, 1]);
    assert_eq!(app.project().mappings.len(), 2);
    let replacement = app.project().mappings.last().unwrap();
    assert_eq!(replacement.source, cc(1, 23));
    assert_eq!(app.map_mode.selected_row, Some(replacement.id));
    assert_eq!(replacement.takeover, Takeover::None);
    let marks = marks(&app);
    assert_eq!(marks[&volume()].label.as_deref(), Some("CC23"));
    assert_eq!(
        marks[&ControlRef::TrackSolo(TrackId(1))].label.as_deref(),
        Some("CC22")
    );
}

#[test]
fn learning_the_same_source_again_selects_its_row_instead_of_duplicating_it() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(volume()));
    send(&mut app, &[0xb0, 21, 1]);
    let id = app.project().mappings[0].id;
    act(&mut app, Action::SelectRow(MappingId(999)));
    act(&mut app, Action::Pick(volume()));
    assert_eq!(
        app.map_mode.selected_row,
        Some(id),
        "a mapped control selects its row"
    );
    send(&mut app, &[0xb0, 21, 1]);
    assert_eq!(app.project().mappings.len(), 1);
    assert_eq!(app.map_mode.selected_row, Some(id));
}

#[test]
fn notes_learn_on_note_on_only_and_a_note_off_teaches_nothing() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(slot()));
    send(&mut app, &[0x80, 60, 0]);
    send(&mut app, &[0x90, 60, 0]);
    assert!(app.project().mappings.is_empty());
    assert_eq!(app.map_mode.selected_control, Some(slot()));
    send(&mut app, &[0x90, 60, 100]);
    let [row] = app.project().mappings.as_slice() else {
        panic!("one mapping");
    };
    assert_eq!(labels::control_cell(&row.source), "Note C3");
    assert_eq!(row.takeover, Takeover::None);
}

#[test]
fn without_a_selected_control_a_controller_teaches_nothing() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    send(&mut app, &[0xb0, 21, 64]);
    assert!(app.project().mappings.is_empty());
}

#[test]
fn an_activator_learns_a_threshold_window_and_a_tempo_live_s_range() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(
        &mut app,
        Action::Pick(ControlRef::TrackActivator(TrackId(1))),
    );
    send(&mut app, &[0xb0, 1, 1]);
    act(
        &mut app,
        Action::Pick(ControlRef::Transport(TransportControl::Tempo)),
    );
    send(&mut app, &[0xb0, 2, 1]);
    let rows = &app.project().mappings;
    assert_eq!(
        (rows[0].min, rows[0].max),
        (qtable_engine::model::ACTIVATOR_WINDOW[0], 1.0)
    );
    assert_eq!((rows[1].min, rows[1].max), (20.0, 999.0));
}

#[test]
fn delete_removes_the_selected_list_row_and_ignores_a_key_a_field_took() {
    use iced::keyboard::{Key, Modifiers, key::Named};
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(volume()));
    send(&mut app, &[0xb0, 21, 1]);
    let delete = Key::Named(Named::Delete);
    let id = app.project().mappings[0].id;
    app.focus = crate::focus::Focus::Browser;
    assert!(keyboard(&app, &delete, Modifiers::empty(), false).is_none());
    assert!(keyboard(&app, &delete, Modifiers::COMMAND, true).is_none());
    act(&mut app, Action::SelectRow(id));
    let action = keyboard(&app, &delete, Modifiers::empty(), true).expect("delete from the list");
    act(&mut app, action);
    assert!(app.project().mappings.is_empty());
    assert_eq!(app.map_mode.selected_row, None);
}

fn mapped(app: &mut App, control: ControlRef) -> MappingId {
    act(app, Action::Toggle);
    act(app, Action::Pick(control));
    send(app, &[0xb0, 21, 1]);
    app.project().mappings[0].id
}

#[test]
fn min_and_max_are_typed_in_the_units_of_the_control_and_refused_outside_them() {
    let mut app = app();
    let id = mapped(&mut app, volume());
    act(&mut app, Action::FieldChanged(id, Cell::Max, "0.5".into()));
    assert_eq!(app.pending_field, Some(DraftKey::Mapping(id, Cell::Max)));
    act(&mut app, Action::FieldSubmitted(id, Cell::Max));
    assert_eq!(app.pending_field, None);
    assert_eq!(app.project().mappings[0].max, 0.5);
    assert!(
        !app.drafts
            .values
            .contains_key(&DraftKey::Mapping(id, Cell::Max))
    );

    act(&mut app, Action::FieldChanged(id, Cell::Min, "7".into()));
    act(&mut app, Action::FieldSubmitted(id, Cell::Min));
    assert_eq!(
        app.project().mappings[0].min,
        0.0,
        "outside the range: refused"
    );
    assert!(app.status.message().is_some());
    assert!(
        app.drafts
            .errors
            .contains_key(&DraftKey::Mapping(id, Cell::Min))
    );
}

#[test]
fn an_enum_and_a_switch_choose_their_ends_from_a_list() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(
        &mut app,
        Action::Pick(ControlRef::Transport(TransportControl::Quantisation)),
    );
    send(&mut app, &[0xb0, 21, 1]);
    let id = app.project().mappings[0].id;
    act(&mut app, Action::LimitChosen(id, Cell::Max, 4.0));
    assert_eq!(app.project().mappings[0].max, 4.0);
    let limits = row_limits(&app, &app.project().mappings[0]).unwrap();
    assert!(matches!(limits, labels::Limits::Choice(_)));
    assert_eq!(labels::format_limit(&limits, 4.0), "1 Bar");
}

#[test]
fn mode_and_takeover_are_edited_in_the_inspector() {
    let mut app = app();
    let id = mapped(&mut app, volume());
    let twos = SourceMode::Relative(RelativeEncoding::TwosComplement);
    act(&mut app, Action::SetMode(id, twos));
    assert_eq!(app.project().mappings[0].mode, twos);
    assert!(!app.project().mappings[0].source.uses_takeover(twos));
    act(&mut app, Action::SetMode(id, SourceMode::Absolute));
    act(&mut app, Action::SetTakeover(id, Takeover::PickUp));
    assert_eq!(app.project().mappings[0].takeover, Takeover::PickUp);
}

#[test]
fn a_note_cannot_be_made_relative() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(slot()));
    send(&mut app, &[0x90, 60, 100]);
    let id = app.project().mappings[0].id;
    act(
        &mut app,
        Action::SetMode(id, SourceMode::Relative(RelativeEncoding::SignedBit)),
    );
    assert_eq!(app.project().mappings[0].mode, SourceMode::Absolute);
    assert!(app.status.message().is_some());
}

#[test]
fn the_window_declares_the_controls_a_controller_can_drive() {
    use iced::Size;
    let mut app = app();
    act(&mut app, Action::Toggle);
    let mut view = crate::view::view(&app);
    let renderer = crate::grid::test_renderer();
    let mut tree = iced::advanced::widget::Tree::new(&view);
    let node = view.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &iced::advanced::layout::Limits::new(Size::ZERO, Size::new(1600.0, 900.0)),
    );
    let found = crate::control::map_layer::collect(|operation| {
        view.as_widget_mut().operate(
            &mut tree,
            iced::advanced::Layout::new(&node),
            &renderer,
            operation,
        );
    });
    let controls: Vec<_> = found.iter().map(|(_, control)| control).collect();
    for expected in [
        volume(),
        ControlRef::TrackSolo(TrackId(1)),
        slot(),
        ControlRef::Scene(SceneId(1)),
        ControlRef::Transport(TransportControl::Play),
        ControlRef::Transport(TransportControl::Tempo),
        ControlRef::Transport(TransportControl::StopAll),
        ControlRef::TrackStop(TrackId(1)),
    ] {
        assert!(controls.contains(&&expected), "{expected:?} is lit");
    }
    let bounds = Rectangle::with_size(Size::new(1600.0, 900.0));
    let outside: Vec<_> = found
        .iter()
        .filter(|(rect, _)| {
            rect.x < -0.01
                || rect.y < -0.01
                || rect.x + rect.width > bounds.width + 0.01
                || rect.y + rect.height > bounds.height + 0.01
        })
        .collect();
    assert!(
        outside.is_empty(),
        "regions outside the window: {outside:?}"
    );
}

#[test]
fn the_inspector_shows_limits_in_their_own_form_and_none_for_a_slot() {
    use crate::control_focus::{View, test_registry::controls};
    let mut app = app();
    act(&mut app, Action::Toggle);
    for (control, cc) in [
        (volume(), 1),
        (slot(), 2),
        (ControlRef::Transport(TransportControl::Quantisation), 3),
        (ControlRef::TrackActivator(TrackId(1)), 4),
    ] {
        act(&mut app, Action::Pick(control));
        send(&mut app, &[0xb0, cc, 1]);
    }
    let ids: Vec<_> = app.project().mappings.iter().map(|row| row.id).collect();
    let mut has = |id: MappingId, cell: Cell| {
        act(&mut app, Action::SelectRow(id));
        let mut list = list_view(&app);
        let focus: Vec<crate::drafts::ControlId> = controls(&mut list);
        focus.iter().any(|c| {
            *c == crate::drafts::ControlId::field(View::Mappings, DraftKey::Mapping(id, cell))
        })
    };
    // A float and an Activator window are Number fields; a slot has none.
    for cell in [Cell::Min, Cell::Max] {
        assert!(has(ids[0], cell), "volume {cell:?}");
        assert!(!has(ids[1], cell), "slot {cell:?}");
        assert!(has(ids[3], cell), "activator {cell:?}");
        // An Enum picks its ends from a list instead.
        assert!(!has(ids[2], cell), "enum {cell:?}");
    }
}

#[test]
fn the_list_lays_out_with_no_mappings_and_with_selected_rows() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    let size = iced::Size::new(crate::style::detail::DEFAULT_BROWSER_WIDTH, 300.0);
    let _: Vec<(crate::drafts::ControlId, Rectangle)> =
        crate::control_focus::test_registry::rectangles_in(&mut list_view(&app), size);
    act(&mut app, Action::Pick(volume()));
    send(&mut app, &[0xb0, 21, 1]);
    let id = app.project().mappings[0].id;
    act(
        &mut app,
        Action::SetMode(id, SourceMode::Relative(RelativeEncoding::SignedBit)),
    );
    let found: Vec<(crate::drafts::ControlId, Rectangle)> =
        crate::control_focus::test_registry::rectangles_in(&mut list_view(&app), size);
    assert!(!found.is_empty());
    // A window too short for the inspector still lays out: it scrolls inside itself.
    let short = iced::Size::new(120.0, 90.0);
    let _: Vec<(crate::drafts::ControlId, Rectangle)> =
        crate::control_focus::test_registry::rectangles_in(&mut list_view(&app), short);
}

#[test]
fn invert_swaps_min_and_max_and_swaps_them_back() {
    let mut app = app();
    let id = mapped(&mut app, volume());
    assert_eq!(
        (app.project().mappings[0].min, app.project().mappings[0].max),
        (0.0, 1.0)
    );
    act(&mut app, Action::Invert(id));
    assert_eq!(
        (app.project().mappings[0].min, app.project().mappings[0].max),
        (1.0, 0.0)
    );
    act(&mut app, Action::Invert(id));
    assert_eq!(
        (app.project().mappings[0].min, app.project().mappings[0].max),
        (0.0, 1.0)
    );
}

#[test]
fn sensitivity_is_typed_as_a_percentage_and_refused_outside_1_to_1000() {
    let mut app = app();
    let id = mapped(&mut app, volume());
    let key = DraftKey::Mapping(id, Cell::Sensitivity);
    act(
        &mut app,
        Action::FieldChanged(id, Cell::Sensitivity, "250".into()),
    );
    assert_eq!(app.pending_field, Some(key));
    act(&mut app, Action::FieldSubmitted(id, Cell::Sensitivity));
    assert_eq!(app.project().mappings[0].sensitivity, 2.5);
    assert!(!app.drafts.values.contains_key(&key));
    act(
        &mut app,
        Action::FieldChanged(id, Cell::Sensitivity, "0".into()),
    );
    act(&mut app, Action::FieldSubmitted(id, Cell::Sensitivity));
    assert_eq!(app.project().mappings[0].sensitivity, 2.5);
    assert!(app.drafts.errors.contains_key(&key));
    assert_eq!(
        app.status.message(),
        Some("Enter a percentage between 1 and 1000")
    );
    // A blur commits the draft as Return does.
    act(
        &mut app,
        Action::FieldChanged(id, Cell::Sensitivity, "50 %".into()),
    );
    commit(&mut app, id, Cell::Sensitivity);
    assert_eq!(app.project().mappings[0].sensitivity, 0.5);
}

#[test]
fn a_note_chooses_latch_or_gate_and_a_cc_takes_value_scaling() {
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(ControlRef::TrackSolo(TrackId(1))));
    send(&mut app, &[0x90, 60, 100]);
    let note = app.project().mappings[0].id;
    act(&mut app, Action::SetMode(note, SourceMode::Gate));
    assert_eq!(app.project().mappings[0].mode, SourceMode::Gate);
    // Latch is stored as format 11 had it.
    act(&mut app, Action::SetMode(note, SourceMode::Absolute));
    assert_eq!(app.project().mappings[0].mode, SourceMode::Absolute);
    act(&mut app, Action::Pick(volume()));
    send(&mut app, &[0xb0, 21, 1]);
    let cc = app.project().mappings[1].id;
    act(&mut app, Action::SetTakeover(cc, Takeover::ValueScaling));
    assert_eq!(app.project().mappings[1].takeover, Takeover::ValueScaling);
}

/// Delete or Backspace as the window delivers it when no widget took the key.
fn press_delete(app: &mut App, key: iced::keyboard::key::Named, ignored: bool) {
    let _ = crate::update::update(
        app,
        Message::KeyPressed {
            key: iced::keyboard::Key::Named(key),
            modifiers: iced::keyboard::Modifiers::empty(),
            repeat: false,
            ignored,
        },
    );
}

/// The grid holds focus with the first clip and its track selected, as after clicking a cell.
fn grid_with_clip(app: &mut App) {
    app.focus = crate::focus::Focus::Grid;
    app.selection.clip = Some(crate::grid::Selection {
        track: TrackId(1),
        slot: 0,
    });
    app.selection.tracks = vec![TrackId(1)];
}

fn bind_key(app: &mut App, text: &str, control: ControlRef) {
    act(app, Action::ToggleKey);
    act(app, Action::Pick(control));
    let _ = crate::update::update(
        app,
        Message::MappedKey(crate::control::key_gate::KeyInput {
            key: iced::keyboard::Key::Character(text.into()),
            modified_key: iced::keyboard::Key::Character(text.into()),
            modifiers: iced::keyboard::Modifiers::empty(),
            pressed: true,
            repeat: false,
        }),
    );
    let _ = crate::update::update(app, Message::ReleaseKeys);
}

#[test]
fn delete_on_a_picked_mapped_control_clears_its_midi_mapping_and_nothing_else() {
    use iced::keyboard::key::Named;
    for key in [Named::Delete, Named::Backspace] {
        let mut app = app();
        bind_key(&mut app, "a", volume());
        act(&mut app, Action::Toggle);
        act(&mut app, Action::Pick(volume()));
        send(&mut app, &[0xb0, 21, 1]);
        assert_eq!(app.project().mappings.len(), 2);
        grid_with_clip(&mut app);
        let (tracks, clip) = (
            app.project().tracks.len(),
            app.project().tracks[0].slots[0].clip.clone(),
        );
        act(&mut app, Action::Pick(volume()));
        press_delete(&mut app, key, true);
        let rows = &app.project().mappings;
        assert_eq!(rows.len(), 1, "{key:?}: only the MIDI mapping goes");
        assert!(matches!(rows[0].source, Source::Key(_)));
        assert_eq!(app.map_mode.selected_row, None);
        assert_eq!(app.project().tracks.len(), tracks);
        assert_eq!(
            app.project().tracks[0].slots[0].clip,
            clip,
            "the clip stays"
        );
    }
}

#[test]
fn delete_on_a_picked_mapped_control_clears_its_key_mapping_and_keeps_the_midi_one() {
    use iced::keyboard::key::Named;
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(volume()));
    send(&mut app, &[0xb0, 21, 1]);
    bind_key(&mut app, "a", volume());
    assert_eq!(app.project().mappings.len(), 2);
    grid_with_clip(&mut app);
    act(&mut app, Action::Pick(volume()));
    press_delete(&mut app, Named::Delete, true);
    let rows = &app.project().mappings;
    assert_eq!(rows.len(), 1);
    assert!(matches!(rows[0].source, Source::Midi(_)));
    assert!(app.project().tracks[0].slots[0].clip.is_some());
}

#[test]
fn delete_in_map_mode_never_falls_through_to_editing() {
    use iced::keyboard::key::Named;
    for mode in [Action::Toggle, Action::ToggleKey] {
        // A picked control with no mapping, then no pick at all, with a clip selected.
        for picked in [true, false] {
            let mut app = app();
            act(&mut app, mode.clone());
            grid_with_clip(&mut app);
            if picked {
                act(&mut app, Action::Pick(volume()));
            }
            press_delete(&mut app, Named::Delete, true);
            assert!(app.project().tracks[0].slots[0].clip.is_some(), "{picked}");
            assert_eq!(app.project().tracks.len(), 1);
            // Track-header selection, with no clip.
            app.selection.clip = None;
            press_delete(&mut app, Named::Backspace, true);
            assert!(!app.project().tracks.is_empty(), "{picked}");
        }
    }
}

#[test]
fn delete_with_no_control_for_this_mode_keeps_the_other_modes_mapping() {
    use iced::keyboard::key::Named;
    let mut app = app();
    bind_key(&mut app, "a", volume());
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(volume()));
    assert_eq!(app.map_mode.selected_row, None, "no MIDI mapping on it");
    press_delete(&mut app, Named::Delete, true);
    assert_eq!(app.project().mappings.len(), 1);
}

#[test]
fn a_field_that_took_delete_keeps_it() {
    use iced::keyboard::key::Named;
    let mut app = app();
    act(&mut app, Action::Toggle);
    act(&mut app, Action::Pick(volume()));
    send(&mut app, &[0xb0, 21, 1]);
    act(&mut app, Action::Pick(volume()));
    press_delete(&mut app, Named::Delete, false);
    assert_eq!(
        app.project().mappings.len(),
        1,
        "a focused field consumed it"
    );
    app.pending_field = Some(DraftKey::Mapping(app.project().mappings[0].id, Cell::Min));
    press_delete(&mut app, Named::Delete, true);
    assert_eq!(app.project().mappings.len(), 1, "a field is being edited");
}

#[test]
fn delete_outside_map_mode_keeps_its_editing_meaning() {
    use iced::keyboard::key::Named;
    let mut app = app();
    grid_with_clip(&mut app);
    press_delete(&mut app, Named::Delete, true);
    assert!(app.project().tracks[0].slots[0].clip.is_none());
}
