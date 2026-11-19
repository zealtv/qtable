use super::*;
use crate::edit::{ProjectEdit, apply};
fn project() -> Project {
    let mut track = Track::new(TrackId(1), "Track", 0);
    track.route = Route::Outputs(vec![TrackId(2)]);
    Project {
        tracks: vec![track],
        outputs: vec![Track::new_output(TrackId(2), "Out", "127.0.0.1", 9000)],
        ..Project::default()
    }
}
#[test]
fn defaults_and_roundtrip_are_silent_until_enabled() {
    let mut p = project();
    p.validate().unwrap();
    assert!(p.volume_recipients(TrackId(1)).is_empty());
    let saved = ron::to_string(&p).unwrap();
    assert!(!saved.contains("mixer_device"));
    let loaded: Project = ron::from_str(&saved).unwrap();
    assert_eq!(p, loaded);
    p.tracks[0].mixer_device.volume.value = 0.25;
    p.tracks[0].mixer_device.volume.output.enabled = true;
    p.tracks[0].mixer_device.volume.output.mapping =
        Mapping::new([0., 1.], [10., 0.], 2., 1.).unwrap();
    assert_eq!(p.tracks[0].mixer_device.mapped_volume().unwrap(), 10.375);
    assert_eq!(
        p,
        ron::from_str::<Project>(&ron::to_string(&p).unwrap()).unwrap()
    );
}
#[test]
fn validation_and_edit_failure_preserve_project() {
    let mut p = project();
    let original = p.clone();
    for volume in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        assert!(
            apply(
                &mut p,
                ProjectEdit::set_value(crate::model::ParameterRef::volume(TrackId(1)), volume)
            )
            .is_err()
        );
        assert_eq!(p, original);
    }
    for prefix in ["", "/a\0b", "a b"] {
        let output = ParameterOutput {
            address: prefix.into(),
            ..MixerDevice::default().volume.output
        };
        assert!(
            apply(
                &mut p,
                ProjectEdit::SetParameterOutput {
                    target: crate::model::ParameterRef::volume(TrackId(1)),
                    output
                }
            )
            .is_err()
        );
        assert_eq!(p, original);
    }
    p.tracks[0].mixer_device.volume.output.mapping =
        Mapping::new([0., 1.], [0., f64::MAX / 2.], 1., 0.).unwrap();
    assert!(p.validate().is_err());
}

#[test]
fn stored_values_use_their_specs_even_when_the_controller_is_disabled() {
    let mut p = project();
    p.tracks[0].group = Some(TrackId(3));
    p.tracks[0].route = Route::Group;
    p.tracks.insert(0, Track::new_group(TrackId(3), "Group", 0));
    p.validate().unwrap();
    for id in [TrackId(1), TrackId(3), TrackId(2), p.main.id] {
        for value in [VOLUME.min, VOLUME.max, -0.1, 1.1, f64::NAN, f64::INFINITY] {
            let mut candidate = p.clone();
            candidate.track_mut(id).unwrap().mixer_device.volume.value = value;
            assert_eq!(candidate.validate().is_ok(), VOLUME.check(value).is_ok());
        }
        for hz in [
            STREAM_RATE.min,
            STREAM_RATE.max,
            60.0,
            60.1,
            50.5,
            0.0,
            201.0,
            f64::NAN,
            f64::INFINITY,
        ] {
            for mode in SendMode::ALL {
                let mut candidate = p.clone();
                let output = &mut candidate.track_mut(id).unwrap().mixer_device.volume.output;
                output.stream_rate = hz;
                output.send_mode = mode;
                let output = output.clone();
                assert_eq!(candidate.validate().is_ok(), STREAM_RATE.check(hz).is_ok());
                let mut edited = p.clone();
                assert_eq!(
                    apply(
                        &mut edited,
                        ProjectEdit::SetParameterOutput {
                            target: ParameterRef::volume(id),
                            output,
                        }
                    )
                    .is_ok(),
                    STREAM_RATE.check(hz).is_ok()
                );
                if STREAM_RATE.check(hz).is_err() {
                    assert_eq!(edited, p);
                }
            }
        }
    }
}

#[test]
fn stream_rate_edits_stop_at_sixty_hz() {
    let mut p = project();
    let target = ParameterRef::volume(TrackId(1));
    for mode in SendMode::ALL {
        for hz in [60.1, 61.0, 200.0] {
            let before = p.clone();
            let output = ParameterOutput {
                send_mode: mode,
                stream_rate: hz,
                ..MixerDevice::default().volume.output
            };
            assert!(
                apply(
                    &mut p,
                    ProjectEdit::SetParameterOutput {
                        target: target.clone(),
                        output,
                    }
                )
                .is_err(),
                "accepted {hz} Hz in {mode:?}"
            );
            assert_eq!(p, before);
        }
        let output = ParameterOutput {
            send_mode: mode,
            stream_rate: 60.0,
            ..MixerDevice::default().volume.output
        };
        apply(
            &mut p,
            ProjectEdit::SetParameterOutput {
                target: target.clone(),
                output,
            },
        )
        .unwrap();
        assert_eq!(p.tracks[0].mixer_device.volume.output.stream_rate, 60.0);
    }
}

#[test]
fn stored_stream_rates_above_sixty_survive_loading_and_unrelated_edits() {
    let path = std::env::temp_dir().join(format!(
        "qtable-legacy-stream-rate-{}.ron",
        std::process::id()
    ));
    for hz in [60.1, 100.0, 200.0] {
        for mode in SendMode::ALL {
            let mut p = project();
            let output = &mut p.tracks[0].mixer_device.volume.output;
            output.stream_rate = hz;
            output.send_mode = mode;
            crate::file::save(&path, &p).unwrap();
            let mut loaded = crate::file::load(&path).unwrap();
            assert_eq!(loaded, p);
            let target = ParameterRef::volume(TrackId(1));
            apply(&mut loaded, ProjectEdit::set_value(target.clone(), 0.5)).unwrap();
            let mut output = loaded
                .parameter(&target)
                .unwrap()
                .output
                .clone()
                .into_owned();
            output.enabled = true;
            apply(
                &mut loaded,
                ProjectEdit::SetParameterOutput { target, output },
            )
            .unwrap();
            assert_eq!(loaded.tracks[0].mixer_device.volume.output.stream_rate, hz);
            crate::file::save(&path, &loaded).unwrap();
            assert_eq!(crate::file::load(&path).unwrap(), loaded);
        }
    }
    std::fs::remove_file(path).unwrap();
}
#[test]
fn output_self_route_and_source_flags_do_not_gate_control() {
    let mut p = project();
    p.tracks[0].mixer_device.volume.output.enabled = true;
    p.tracks[0].mixer.active = false;
    p.outputs[0].mixer.solo = true;
    assert_eq!(p.volume_recipients(TrackId(1)).len(), 1);
    p.outputs[0].mixer_device.volume.output.enabled = true;
    p.outputs[0].delay_ms = 4.;
    p.main.delay_ms = 3.;
    assert_eq!(p.volume_recipients(TrackId(2))[0].id, TrackId(2));
    assert_eq!(p.volume_delay_ms(TrackId(2), TrackId(2)), 7.);
    p.outputs[0].mixer.active = false;
    assert!(p.volume_recipients(TrackId(1)).is_empty());
    assert!(p.volume_recipients(TrackId(2)).is_empty());
}
#[test]
fn ancestor_prefixes_and_delay_use_common_resolution() {
    let mut p = project();
    let mut group = Track::new_group(TrackId(3), "Group", 0);
    group.route = Route::Outputs(vec![TrackId(2)]);
    group = group.with_path("/group");
    group.delay_ms = 2.;
    p.tracks[0].group = Some(group.id);
    p.tracks[0].route = Route::Group;
    p.tracks.insert(0, group);
    p.tracks[1].mixer_device.volume.output.enabled = true;
    p.tracks[1].delay_ms = 1.;
    p.main.delay_ms = 3.;
    p.outputs[0].delay_ms = 4.;
    p.outputs[0] = p.outputs[0].clone().with_path("/out");
    p.validate().unwrap();
    assert_eq!(
        p.volume_address(TrackId(1), TrackId(2)).unwrap(),
        "/out/group/volume"
    );
    assert_eq!(
        p.volume_address(TrackId(3), TrackId(2)).unwrap(),
        "/out/group/volume"
    );
    assert_eq!(p.volume_delay_ms(TrackId(1), TrackId(2)), 10.);
    p.tracks[1].route = Route::empty();
    assert!(p.volume_recipients(TrackId(1)).is_empty());
}

#[test]
fn file_roundtrip_and_output_delay_validation() {
    let mut p = project();
    p.outputs[0].mixer_device.volume.output.enabled = true;
    p.outputs[0].mixer_device.volume.value = 0.123456;
    let path =
        std::env::temp_dir().join(format!("qtable-mixer-roundtrip-{}.ron", std::process::id()));
    crate::file::save(&path, &p).unwrap();
    assert_eq!(crate::file::load(&path).unwrap(), p);
    std::fs::remove_file(path).unwrap();
    p.outputs[0].mixer.active = false;
    p.outputs[0].delay_ms = 600.;
    p.main.delay_ms = 500.;
    p.tracks[0].route = Route::empty();
    assert!(p.validate().is_err());
}

#[test]
fn main_volume_self_targets_and_uses_global_delay_once() {
    let mut p = project();
    let id = p.main.id;
    p.main.mixer_device.volume.output.enabled = true;
    p.main.delay_ms = 25.0;
    assert_eq!(
        p.volume_recipients(id)
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>(),
        [id]
    );
    assert_eq!(p.volume_address(id, id), Some("/volume".into()));
    assert_eq!(p.volume_delay_ms(id, id), 25.0);
    p.main.output = None;
    assert!(p.volume_recipients(id).is_empty());
}
#[test]
fn send_mode_defaults_to_continuous_for_files_written_before_it_existed() {
    let old = r#"(enabled: true, address: "/volume", mapping: (input: (0.0, 1.0), output: (0.0, 1.0), exponent: 1.0, bias: 0.0))"#;
    let loaded: ParameterOutput = ron::from_str(old).unwrap();
    assert_eq!(loaded.send_mode, SendMode::Continuous);
    assert!(loaded.enabled);
    let loaded: ParameterOutput = ron::from_str("()").unwrap();
    assert_eq!(loaded, MixerDevice::default().volume.output);
}
#[test]
fn send_mode_round_trips_through_ron() {
    let output = ParameterOutput {
        send_mode: SendMode::OnRelease,
        ..MixerDevice::default().volume.output
    };
    let text = ron::to_string(&output).unwrap();
    assert_eq!(ron::from_str::<ParameterOutput>(&text).unwrap(), output);
}
