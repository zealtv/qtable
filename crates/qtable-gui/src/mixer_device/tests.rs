use super::*;
use crate::control::Text;
use crate::control::mapping::MappingField;
use crate::drafts::MixerField;
use crate::drafts::TrackField;
use qtable_engine::engine::Engine;
use qtable_engine::model::Project;
fn app() -> App {
    App::new(
        Engine::new(Project {
            tracks: vec![Track::new(TrackId(1), "Track", 0)],
            outputs: vec![Track::new_output(TrackId(2), "Out", "127.0.0.1", 9000)],
            ..Project::default()
        })
        .unwrap(),
        String::new(),
        None,
    )
}
fn send(app: &mut App, msg: MixerMessage) {
    let _ = crate::update::update(app, message(msg));
}
#[test]
fn shared_drafts_keep_owners_separate_and_retain_failed_submits() {
    let mut app = app();
    send(&mut app, MixerMessage::Toggle(TrackId(1)));
    let key = DraftKey::Mixer(TrackId(1), MixerField::Volume);
    send(&mut app, MixerMessage::Draft(key, "1.5".into()));
    send(&mut app, MixerMessage::Submit(key));
    assert_eq!(app.project().tracks[0].mixer_device.volume.value, 1.);
    assert!(app.drafts.errors.contains_key(&key));
    assert!(
        !app.drafts
            .values
            .contains_key(&DraftKey::Mixer(TrackId(2), MixerField::Volume))
    );
    send(&mut app, MixerMessage::Draft(key, "0.34567".into()));
    send(&mut app, MixerMessage::Submit(key));
    assert_eq!(app.project().tracks[0].mixer_device.volume.value, 0.34567);
    assert!(!app.drafts.values.contains_key(&key));
    send(&mut app, MixerMessage::Submit(key));
    assert_eq!(app.project().tracks[0].mixer_device.volume.value, 0.34567);
}

#[test]
fn untouched_volume_readout_preserves_the_model_but_a_typed_edit_applies() {
    let mut app = app();
    let id = TrackId(1);
    let key = DraftKey::Mixer(id, MixerField::Volume);
    send(&mut app, MixerMessage::Toggle(id));
    let _ = crate::update::update(
        &mut app,
        Message::Mixer(crate::mixer_band::MixerMessage::SetVolume(id, 0.73456789)),
    );
    assert_eq!(
        app.project().tracks[0]
            .mixer_device
            .volume_parameter()
            .display(),
        "0.7346"
    );
    let revision = app.project_revision();
    send(&mut app, MixerMessage::Submit(key));
    assert_eq!(
        app.project().tracks[0].mixer_device.volume.value,
        0.73456789
    );
    assert_eq!(app.project_revision(), revision);
    send(&mut app, MixerMessage::Draft(key, "0.7346".into()));
    send(&mut app, MixerMessage::Submit(key));
    assert_eq!(app.project().tracks[0].mixer_device.volume.value, 0.7346);
}
#[test]
fn mapping_blur_and_prefix_validation_use_shared_keys() {
    let mut app = app();
    let id = TrackId(2);
    send(&mut app, MixerMessage::Toggle(id));
    let key = DraftKey::Mixer(id, MixerField::Mapping(MappingField::Exponent));
    send(
        &mut app,
        MixerMessage::Mapping(
            id,
            MappingEditorMessage::Changed(MappingField::Exponent, "2.5".into()),
        ),
    );
    app.pending_field = Some(key);
    crate::field_focus::commit(&mut app);
    assert_eq!(
        app.project().outputs[0]
            .mixer_device
            .volume
            .output
            .mapping
            .exponent(),
        2.5
    );
    assert!(!app.drafts.values.contains_key(&key));
    let key = DraftKey::Mixer(id, MixerField::Address);
    send(&mut app, MixerMessage::Draft(key, "bad value".into()));
    send(&mut app, MixerMessage::Submit(key));
    assert!(app.drafts.errors.contains_key(&key));
    assert_eq!(
        app.project().outputs[0].mixer_device.volume.output.address,
        "/volume"
    );
    send(&mut app, MixerMessage::Toggle(id));
    assert!(!app.project().outputs[0].mixer_device.volume.output.enabled);
}

#[test]
fn all_stored_kinds_share_the_chain_mixer_panel() {
    let mut app = app();
    crate::edit_actions::execute_edit(
        &mut app,
        Command::Edit(ProjectEdit::AddTrack {
            name: "Member".into(),
        }),
    );
    crate::edit_actions::execute_edit(
        &mut app,
        Command::Edit(ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1), TrackId(3)],
        }),
    );
    let group = app.project().group_tracks().next().unwrap().id;
    for id in [TrackId(1), group, TrackId(2)] {
        let _ = crate::update::update(
            &mut app,
            Message::Grid(crate::grid::Action::SelectColumn(
                crate::selection::ColumnOwner::Track(id),
                None,
            )),
        );
        let layout = if id == TrackId(2) {
            crate::output_detail::view(&app)
        } else {
            crate::track_detail::view(&app)
        };
        assert!(layout.content.is_some());
        assert!(layout.sections.iter().all(|s| s.id.0 != "mixer"));
    }
}

#[test]
fn volume_path_prefixes_show_disabled_routes_and_destination_self_targets() {
    use crate::control::path_field::Prefix;
    use qtable_engine::model::PathLevel;
    // Legacy paths also exercise the same resolution used by migrated saved sets.
    let mut project =
        qtable_engine::file::load("../qtable-engine/tests/fixtures/v5-prefixes.ron").unwrap();
    let member = TrackId(1);
    let group = TrackId(10);
    let output = TrackId(201);
    let main = project.main.id;
    project[output].mixer.active = false;
    for id in [member, group, output, main] {
        project[id].mixer_device.volume.output.enabled = false;
    }
    // Each destination's inherited path, with the stored Volume Path piece after it.
    let addresses = |project: &Project, id: TrackId| -> Vec<(String, String)> {
        let piece = &project[id].mixer_device.volume.output.address;
        project
            .inherited_paths(id, PathLevel::Under)
            .into_iter()
            .map(|(d, prefix)| (d.name.clone(), format!("{prefix}{piece}")))
            .collect()
    };
    let one = |name: &str, address: &str| vec![(name.to_owned(), address.to_owned())];
    assert_eq!(addresses(&project, member), one("Bop", "/bop/kites/volume"));
    assert_eq!(addresses(&project, group), one("Bop", "/bop/kites/volume"));
    assert_eq!(addresses(&project, output), one("Bop", "/bop/vol"));
    assert_eq!(addresses(&project, main), one("Main", "/show/mainvol"));
    assert_eq!(
        addresses(&project, TrackId(2)),
        vec![
            ("Main".into(), "/show/volume".into()),
            ("Plain".into(), "/volume".into())
        ]
    );
    // The field shows the first destination's prefix and counts the other distinct one.
    let prefix = Prefix::of(&project, TrackId(2), PathLevel::Under, false);
    assert_eq!((prefix.text().as_str(), prefix.more), ("/show", 1));
    let prefix = Prefix::of(&project, output, PathLevel::Under, false);
    assert_eq!((prefix.text().as_str(), prefix.more), ("/bop", 0));
    project[member].route = qtable_engine::model::Route::empty();
    assert!(addresses(&project, member).is_empty());
    assert_eq!(
        Prefix::of(&project, member, PathLevel::Under, false),
        Prefix::default()
    );
    project.main.output = None;
    assert!(addresses(&project, main).is_empty());
}

#[test]
fn disabled_mixer_fields_keep_drafts_and_reenable_with_shared_control_state() {
    use crate::control::{Interaction, LIVE_DARK};
    let mut app = app();
    let id = TrackId(1);
    let volume = DraftKey::Mixer(id, MixerField::Volume);
    let address = DraftKey::Mixer(id, MixerField::Address);
    let exponent = DraftKey::Mixer(id, MixerField::Mapping(MappingField::Exponent));
    // Exercise deferred messages directly: the outer app update commits a valid
    // focused field on blur, before disabling the controller.
    super::update(&mut app, MixerMessage::Toggle(id));
    for (key, value) in [(volume, "0.4"), (address, "/gain"), (exponent, "2.5")] {
        super::update(&mut app, MixerMessage::Draft(key, value.into()));
    }
    super::update(&mut app, MixerMessage::Toggle(id));
    let before = app.project().tracks[0].mixer_device.clone();
    for (key, help) in [(volume, Id::MixerVolume), (address, Id::VolumePath)] {
        let control = field_control(
            &app,
            View::Track,
            key,
            help,
            Text {
                placeholder: "".into(),
            },
        );
        assert!(!control.state.enabled);
        assert_eq!(
            control
                .kind
                .look(&LIVE_DARK, control.state, Interaction::Idle)
                .ink,
            LIVE_DARK.text_disabled
        );
    }
    for key in [volume, address, exponent] {
        super::update(&mut app, MixerMessage::Draft(key, "9".into()));
        super::update(&mut app, MixerMessage::Submit(key));
    }
    super::update(
        &mut app,
        MixerMessage::Mapping(
            id,
            MappingEditorMessage::Changed(MappingField::Exponent, "9".into()),
        ),
    );
    super::update(
        &mut app,
        MixerMessage::Mapping(id, MappingEditorMessage::Submitted(MappingField::Exponent)),
    );
    assert_eq!(app.project().tracks[0].mixer_device, before);
    assert_eq!(app.drafts.values[&volume], "0.4");
    assert_eq!(app.drafts.values[&address], "/gain");
    assert_eq!(app.drafts.values[&exponent], "2.5");
    super::update(&mut app, MixerMessage::Toggle(id));
    let control = field_control(&app, View::Track, volume, Id::MixerVolume, Number::new(""));
    assert!(control.state.enabled);
    assert_eq!(
        control
            .kind
            .look(&LIVE_DARK, control.state, Interaction::Idle)
            .ink,
        LIVE_DARK.control_foreground
    );
    for key in [volume, address, exponent] {
        super::update(&mut app, MixerMessage::Submit(key));
    }
    let device = &app.project().tracks[0].mixer_device;
    assert_eq!(device.volume.value, 0.4);
    assert_eq!(device.volume.output.address, "/gain");
    assert_eq!(device.volume.output.mapping.exponent(), 2.5);
}
#[test]
fn send_mode_changes_only_while_the_slider_is_enabled_and_survives_a_save() {
    let mut app = app();
    let id = TrackId(1);
    let mode = |app: &App| app.project().tracks[0].mixer_device.volume.output.send_mode;
    assert_eq!(mode(&app), SendMode::Continuous);
    super::update(&mut app, MixerMessage::SendMode(id, SendMode::OnRelease));
    assert_eq!(mode(&app), SendMode::Continuous, "disabled with the rest");
    super::update(&mut app, MixerMessage::Toggle(id));
    super::update(&mut app, MixerMessage::SendMode(id, SendMode::OnRelease));
    assert_eq!(mode(&app), SendMode::OnRelease);
    let _ = view(&app, &app.project().tracks[0], View::Track);
    let path = std::env::temp_dir().join(format!("qtable-sendmode-{}.ron", std::process::id()));
    qtable_engine::file::save(&path, app.project()).unwrap();
    let loaded = qtable_engine::file::load(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(
        loaded.tracks[0].mixer_device.volume.output.send_mode,
        SendMode::OnRelease
    );
}
#[test]
fn the_send_mode_dropdown_follows_the_slider_enabled_state() {
    use crate::control::{Dropdown, Interaction, LIVE_DARK};
    assert_eq!(Id::MixerVolumeSend.entry().title, "Send Mode");
    assert!(Id::MixerVolumeSend.entry().text.starts_with("Send Mode"));
    let mut app = app();
    let id = TrackId(1);
    for enabled in [false, true] {
        let control = field_control(
            &app,
            View::Track,
            DraftKey::Mixer(id, MixerField::Volume),
            Id::MixerVolumeSend,
            Dropdown,
        );
        assert_eq!(control.state.enabled, enabled);
        let ink = control
            .kind
            .look(&LIVE_DARK, control.state, Interaction::Idle)
            .ink;
        assert_eq!(ink == LIVE_DARK.text_disabled, !enabled);
        super::update(&mut app, MixerMessage::Toggle(id));
    }
}

#[test]
fn threshold_validates_and_keeps_its_value_across_modes_and_reload() {
    let mut app = app();
    let id = TrackId(1);
    let key = DraftKey::Mixer(id, MixerField::Threshold);
    send(&mut app, MixerMessage::Toggle(id));
    for invalid in ["-1", "NaN", "no"] {
        send(&mut app, MixerMessage::Draft(key, invalid.into()));
        send(&mut app, MixerMessage::Submit(key));
        assert!(app.drafts.errors.contains_key(&key));
        assert!(app.status.message().is_some());
        assert_eq!(app.project()[id].mixer_device.volume.output.threshold, 0.0);
    }
    send(&mut app, MixerMessage::Draft(key, "0.12345678".into()));
    send(&mut app, MixerMessage::Submit(key));
    for mode in SendMode::ALL {
        send(&mut app, MixerMessage::SendMode(id, mode));
        assert_eq!(
            app.project()[id].mixer_device.volume.output.threshold,
            0.12345678
        );
        if mode == SendMode::EveryFrame {
            send(&mut app, MixerMessage::Draft(key, "0.5".into()));
            send(&mut app, MixerMessage::Submit(key));
            assert_eq!(
                app.project()[id].mixer_device.volume.output.threshold,
                0.12345678
            );
        }
    }
    let path = std::env::temp_dir().join(format!("qtable-threshold-{}.ron", std::process::id()));
    qtable_engine::file::save(&path, app.project()).unwrap();
    let loaded = qtable_engine::file::load(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(loaded[id].mixer_device.volume.output.threshold, 0.12345678);
    assert_eq!(loaded[id].mixer_device.volume.output.stream_rate, 30.0);
}

fn mixer_bounds(app: &App) -> Vec<(ControlId, iced::Rectangle)> {
    let mut panel = view(app, &app.project().tracks[0], View::Track);
    crate::control_focus::test_registry::rectangles_in(&mut panel, iced::Size::new(1000.0, 900.0))
}

#[test]
fn send_mode_and_threshold_stack_in_one_column_and_later_controls_stay_still_in_every_mode() {
    let mut app = app();
    let id = TrackId(1);
    let send_id = ControlId::action(View::Track, "send-mode", ControlOwner::Track(id));
    let rate_id = ControlId::field(View::Track, DraftKey::Mixer(id, MixerField::Threshold));
    let mut previous = None;
    for enabled in [false, true] {
        for mode in SendMode::ALL {
            if enabled {
                send(&mut app, MixerMessage::SendMode(id, mode));
            }
            let bounds = mixer_bounds(&app);
            let rect = |id| bounds.iter().find(|(key, _)| *key == id).unwrap().1;
            let rate = rect(rate_id);
            let dropdown = rect(send_id);
            assert_eq!(rate.x, dropdown.x);
            assert!(rate.y > dropdown.y + dropdown.height);
            assert_eq!(rate.width, detail::DEVICE_NUMBER_WIDTH);
            assert_eq!(dropdown.width, 110.0);
            let after: Vec<_> = bounds
                .iter()
                .filter(|(key, _)| *key != send_id && *key != rate_id)
                .cloned()
                .collect();
            if let Some(previous) = &previous {
                assert_eq!(&after, previous, "later controls moved under {mode:?}");
            }
            previous = Some(after);
        }
        send(&mut app, MixerMessage::Toggle(id));
    }
}

#[test]
fn threshold_accepts_focus_except_in_every_frame_with_the_slider_enabled() {
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    use iced::{Event, Size, mouse};
    let mut app = app();
    let id = TrackId(1);
    let rate_id = ControlId::field(View::Track, DraftKey::Mixer(id, MixerField::Threshold));
    for enabled in [true, false] {
        send(&mut app, MixerMessage::Toggle(id));
        for mode in SendMode::ALL {
            // Set up the mode while enabled, then test the actual disabled view too.
            if !enabled {
                send(&mut app, MixerMessage::Toggle(id));
            }
            send(&mut app, MixerMessage::SendMode(id, mode));
            if !enabled {
                send(&mut app, MixerMessage::Toggle(id));
            }
            let bounds = mixer_bounds(&app);
            let rate = bounds.iter().find(|(key, _)| *key == rate_id).unwrap().1;
            let mut panel = view(&app, &app.project().tracks[0], View::Track);
            let renderer = crate::grid::test_renderer();
            let mut tree = Tree::new(&panel);
            let size = Size::new(600.0, 900.0);
            let node = panel.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, size),
            );
            let mut messages = vec![];
            panel.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Layout::new(&node),
                mouse::Cursor::Available(rate.center()),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &iced::Rectangle::with_size(size),
            );
            assert_eq!(
                messages.iter().any(|msg| matches!(msg,
                Message::ControlFocused(key) if *key == rate_id)),
                enabled && mode != SendMode::EveryFrame,
                "Threshold focus under {mode:?}"
            );
        }
    }
}

#[test]
fn volume_slider_columns_align_fields_and_keep_compact_mapping_pairs_adjacent() {
    let mut app = app();
    let id = TrackId(1);
    for enabled in [false, true] {
        let bounds = mixer_bounds(&app);
        let rect = |key| bounds.iter().find(|(id, _)| *id == key).unwrap().1;
        let field = |key| rect(ControlId::field(View::Track, key));
        let toggle = rect(ControlId::action(
            View::Track,
            "emit-volume",
            ControlOwner::Track(id),
        ));
        let send_mode = rect(ControlId::action(
            View::Track,
            "send-mode",
            ControlOwner::Track(id),
        ));
        let path = field(DraftKey::Mixer(id, MixerField::Address));
        let threshold = field(DraftKey::Mixer(id, MixerField::Threshold));
        let volume = field(DraftKey::Mixer(id, MixerField::Volume));
        let delay = field(DraftKey::Track(id, TrackField::Delay));
        assert_eq!(toggle.y, send_mode.y);
        assert_eq!(delay.y, send_mode.y);
        assert_eq!(path.y, threshold.y);
        assert_eq!(path.y - toggle.y, 38.0);
        assert_eq!(volume.y - path.y, 38.0);
        for (pair, y) in mapping_keys(id)
            .chunks_exact(2)
            .zip([toggle.y, path.y, volume.y])
        {
            let start = field(pair[0]);
            let end = field(pair[1]);
            assert_eq!(start.y, y);
            assert_eq!(end.y, y);
            assert_eq!(end.x - (start.x + start.width), 4.0);
            assert_eq!(start.width, detail::DEVICE_NUMBER_WIDTH);
            assert_eq!(end.width, detail::DEVICE_NUMBER_WIDTH);
        }
        for field in [toggle, send_mode, path] {
            assert_eq!(field.width, 110.0);
            assert!(field.height <= detail::CONTROL_HEIGHT);
        }
        for field in [threshold, volume, delay] {
            assert_eq!(field.width, detail::DEVICE_NUMBER_WIDTH);
        }
        assert!(delay.x > field(DraftKey::Mixer(id, MixerField::Mapping(MappingField::Bias))).x);
        assert_eq!(
            enabled,
            app.project()[id].mixer_device.volume.output.enabled
        );
        send(&mut app, MixerMessage::Toggle(id));
    }
}

#[test]
fn mixer_cells_fit_the_fixed_body_for_every_track_kind() {
    use iced::Size;
    use iced::advanced::{layout, widget::Tree};
    let mut app = app();
    crate::edit_actions::execute_edit(
        &mut app,
        Command::Edit(ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1)],
        }),
    );
    let group = app.project().group_tracks().next().unwrap().id;
    for id in [TrackId(1), group, TrackId(2), app.project().main.id] {
        let track = &app.project()[id];
        let mut widget = view(&app, track, View::Track);
        let renderer = crate::grid::test_renderer();
        let mut tree = Tree::new(&widget);
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(1000.0, detail::DEVICE_BODY_HEIGHT)),
        );
        let cells = node.children();
        assert_eq!(
            cells.len(),
            if track.capabilities().destination {
                1
            } else {
                2
            }
        );
        assert_eq!(
            width(track),
            if track.capabilities().destination {
                361.0
            } else {
                483.0
            }
        );
        assert_eq!(node.bounds().width, width(track) - detail::SCROLLBAR_GUTTER);
        for (index, cell) in cells.iter().enumerate() {
            assert_eq!(
                cell.bounds().width,
                if index == 0 {
                    VOLUME_WIDTH
                } else {
                    detail::DEVICE_CELL_WIDTH
                }
            );
            assert_eq!(cell.bounds().y, detail::DEVICE_BODY_INSET);
            assert_eq!(cell.bounds().height, 137.0);
            assert!(
                cell.bounds().y + cell.bounds().height
                    <= detail::DEVICE_BODY_HEIGHT - detail::DEVICE_BODY_INSET
            );
            assert!(
                cell.bounds().x + cell.bounds().width
                    <= node.bounds().width - detail::DEVICE_BODY_INSET
            );
            if index > 0 {
                let previous = cells[index - 1].bounds();
                assert_eq!(
                    cell.bounds().x - (previous.x + previous.width),
                    detail::CONTROL_GAP
                );
            }
        }
    }
}

#[test]
fn volume_slider_help_titles_match_the_short_cell_labels() {
    for (id, title) in [
        (Id::MixerVolumeOutput, "Enable"),
        (Id::VolumePath, "Path"),
        (Id::MixerVolume, "Value"),
    ] {
        assert_eq!(id.entry().title, title);
        assert!(!id.entry().text.is_empty());
    }
}

#[test]
fn a_double_click_on_threshold_restores_its_default_and_drops_the_draft() {
    let mut app = app();
    let id = TrackId(1);
    let key = DraftKey::Mixer(id, MixerField::Threshold);
    send(&mut app, MixerMessage::Toggle(id));
    send(&mut app, MixerMessage::Draft(key, "0.2".into()));
    send(&mut app, MixerMessage::Submit(key));
    assert_eq!(
        app.project().tracks[0].mixer_device.volume.output.threshold,
        0.2
    );
    send(&mut app, MixerMessage::Draft(key, "0.7".into()));
    send(&mut app, MixerMessage::ResetThreshold(id));
    assert_eq!(
        app.project().tracks[0].mixer_device.volume.output.threshold,
        THRESHOLD.default
    );
    assert!(!app.drafts.values.contains_key(&key));
}

#[test]
fn a_double_click_on_the_volume_box_sends_the_fader_default_and_ends_a_hold() {
    let mut app = app();
    let id = TrackId(1);
    send(&mut app, MixerMessage::Toggle(id));
    let band = |m| Message::Mixer(crate::mixer_band::MixerMessage::SetVolume(id, m));
    let _ = crate::update::update(&mut app, band(0.3));
    app.parameter_gesture.hold(&super::volume_field(id), 0.6);
    app.drafts
        .values
        .insert(DraftKey::Mixer(id, MixerField::Volume), "0.9".into());
    let _ = crate::update::update(&mut app, band(VOLUME.default));
    assert_eq!(
        app.project().tracks[0].mixer_device.volume.value,
        VOLUME.default
    );
    assert!(app.parameter_gesture.is_empty());
    assert!(
        !app.drafts
            .values
            .contains_key(&DraftKey::Mixer(id, MixerField::Volume))
    );
}

#[test]
fn the_volume_mapping_fields_keep_their_info_entries() {
    for field in [MappingField::InputStart, MappingField::InputEnd] {
        assert_eq!(mapping_help(field).entry().title, "Mapping Input Range");
    }
    for field in [MappingField::OutputStart, MappingField::OutputEnd] {
        assert_eq!(mapping_help(field).entry().title, "Mapping Output Range");
    }
}

#[test]
fn the_volume_field_declares_the_argument_it_drives() {
    let app = app();
    let mut panel = view(&app, &app.project().tracks[0], View::Track);
    let declared =
        crate::control_focus::test_registry::declared(&mut panel, iced::Size::new(1000.0, 900.0));
    let volume = qtable_engine::model::ControlRef::Argument(volume_field(TrackId(1)));
    assert!(qtable_engine::model::describe_control(app.project(), &volume).is_some());
    assert_eq!(declared, [volume]);
}
