use qtable_engine::edit::{self, ProjectEdit};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::file;
use qtable_engine::model::*;
use qtable_test_support::TestReceiver;
use std::time::Duration;

fn project() -> Project {
    let mut p = file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v6-lanes.ron"
    ))
    .unwrap();
    let device = p.next_device_id();
    let mut clip = Clip::new(
        p.next_clip_id(),
        "Other",
        BeatTime(3840),
        LIVE_CLIP_PALETTE[0],
    )
    .unwrap();
    let track = p.track_mut(TrackId(2)).unwrap();
    clip.events.push(track.hit(
        BeatTime::ZERO,
        OscAddress::Relative("/other".into()),
        vec![],
        || device,
    ));
    track.slots[0].clip = Some(clip);
    p
}

fn add(p: &mut Project, track: TrackId, before: Option<DeviceId>) -> DeviceId {
    let id = p.next_device_id();
    edit::apply(
        p,
        ProjectEdit::AddDevice {
            track,
            kind: NewDevice::Target,
            before,
        },
    )
    .unwrap();
    id
}

fn ids(p: &Project, track: TrackId) -> Vec<DeviceId> {
    p.track(track)
        .unwrap()
        .devices
        .iter()
        .map(|d| d.id)
        .collect()
}

#[test]
fn add_move_and_rename_on_clip_tracks() {
    let mut p = project();
    for track in [TrackId(1), TrackId(2)] {
        let a = add(&mut p, track, None);
        let b = add(&mut p, track, Some(a));
        let c = add(&mut p, track, None);
        let chain = ids(&p, track);
        assert_eq!(&chain[chain.len() - 3..], &[b, a, c]);
        edit::apply(
            &mut p,
            ProjectEdit::MoveDevice {
                device: c,
                before: Some(b),
            },
        )
        .unwrap();
        edit::apply(
            &mut p,
            ProjectEdit::MoveDevice {
                device: b,
                before: None,
            },
        )
        .unwrap();
        let chain = ids(&p, track);
        assert_eq!(&chain[chain.len() - 3..], &[c, a, b]);
        edit::apply(
            &mut p,
            ProjectEdit::MoveDevice {
                device: a,
                before: Some(a),
            },
        )
        .unwrap();
        edit::apply(
            &mut p,
            ProjectEdit::RenameDevice {
                device: a,
                name: "Receiver".into(),
            },
        )
        .unwrap();
        assert_eq!(p.device(a).unwrap().1.name, "Receiver");
    }
}

#[test]
fn duplicate_and_paste_keep_definitions_and_order_with_fresh_ids() {
    let mut p = project();
    let a = p.tracks[0].devices[0].id;
    let b = add(&mut p, TrackId(1), None);
    let c = add(&mut p, TrackId(1), None);
    let source = p.tracks[0].devices[..2].to_vec();
    edit::apply(
        &mut p,
        ProjectEdit::DuplicateDevices {
            devices: vec![b, a, b],
        },
    )
    .unwrap();
    let chain = ids(&p, TrackId(1));
    assert_eq!(&chain[..2], &[a, b]);
    assert_eq!(chain[4], c);
    for (old, new) in source.iter().zip(&p.tracks[0].devices[2..4]) {
        assert_ne!(old.id, new.id);
        assert_eq!(old.name, new.name);
        assert_eq!(old.kind, new.kind);
        assert_eq!(old.active, new.active);
    }
    let before = p.track(TrackId(2)).unwrap().devices[0].id;
    edit::apply(
        &mut p,
        ProjectEdit::PasteDevices {
            track: TrackId(2),
            devices: source.clone(),
            before: Some(before),
        },
    )
    .unwrap();
    for (old, new) in source
        .iter()
        .zip(&p.track(TrackId(2)).unwrap().devices[..2])
    {
        assert_ne!(old.id, new.id);
        assert_eq!(old.kind, new.kind);
    }
    assert_eq!(p.track(TrackId(2)).unwrap().devices[2].id, before);
    p.validate().unwrap();
}

#[test]
fn deletion_cleans_declared_and_derived_lanes_in_all_clips_only_on_its_track() {
    let mut p = project();
    let other = p.track(TrackId(2)).unwrap().clone();
    let device = p.tracks[0].devices[0].id;
    let retained = add(&mut p, TrackId(1), None);
    let reference = EventRef {
        device: retained,
        key: EntryKey("keep".into()),
    };
    let target = p.device_mut(retained).unwrap().target_mut().unwrap();
    target.events.push(EventDef {
        key: reference.key.clone(),
        name: String::new(),
        path: OscAddress::Relative("/keep".into()),
        fields: vec![],
    });
    let clip = p.tracks[0].slots[0].clip.as_mut().unwrap();
    clip.lanes
        .push(qtable_engine::model::Lane::Events(EventLane {
            definition: reference.clone(),
        }));
    clip.events.push(Event {
        position: BeatTime::ZERO,
        definition: reference.clone(),
        arguments: vec![],
    });
    let slots = p.tracks[0].slots.len();
    edit::apply(
        &mut p,
        ProjectEdit::RemoveDevices {
            devices: vec![device],
        },
    )
    .unwrap();
    assert_eq!(p.track(TrackId(2)).unwrap(), &other);
    assert_eq!(p.tracks[0].slots.len(), slots);
    assert_eq!(
        p.tracks[0].slots[0].clip.as_ref().unwrap().lane_refs(),
        vec![reference]
    );
    for clip in p.tracks[0]
        .slots
        .iter()
        .skip(1)
        .filter_map(|s| s.clip.as_ref())
    {
        assert!(clip.lane_refs().is_empty());
        assert!(clip.events.is_empty());
    }
    p.validate().unwrap();
}

#[test]
fn invalid_device_edits_reject_the_whole_change() {
    let original = project();
    let a = original.tracks[0].devices[0].id;
    let b = original.track(TrackId(2)).unwrap().devices[0].id;
    let missing = DeviceId(9999);
    for devices in [vec![a, b], vec![a, missing], vec![]] {
        for edit in [
            ProjectEdit::RemoveDevices {
                devices: devices.clone(),
            },
            ProjectEdit::DuplicateDevices {
                devices: devices.clone(),
            },
            ProjectEdit::SetDevicesActive {
                devices,
                active: false,
            },
        ] {
            let mut p = original.clone();
            assert!(edit::apply(&mut p, edit).is_err());
            assert_eq!(p, original);
        }
    }
    for edit in [
        ProjectEdit::MoveDevice {
            device: a,
            before: Some(b),
        },
        ProjectEdit::AddDevice {
            track: TrackId(1),
            kind: NewDevice::Target,
            before: Some(missing),
        },
        ProjectEdit::PasteDevices {
            track: TrackId(1),
            devices: original.tracks[0].devices.clone(),
            before: Some(b),
        },
        ProjectEdit::PasteDevices {
            track: TrackId(1),
            devices: vec![],
            before: None,
        },
        ProjectEdit::RenameDevice {
            device: missing,
            name: "x".into(),
        },
    ] {
        let mut p = original.clone();
        assert!(edit::apply(&mut p, edit).is_err());
        assert_eq!(p, original);
    }
    let mut invalid = original.tracks[0].devices.clone();
    let target = invalid[0].target_mut().unwrap();
    target.events[0].key = EntryKey(DEVICE_ON.into());
    let mut p = original.clone();
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::PasteDevices {
                track: TrackId(1),
                devices: invalid,
                before: None
            }
        )
        .is_err()
    );
    assert_eq!(p, original);
}

#[test]
fn track_and_output_paste_renew_devices_and_clip_refs() {
    let mut p = project();
    for track in [TrackId(1), TrackId(201)] {
        let source = p.track(track).unwrap().clone();
        edit::apply(
            &mut p,
            ProjectEdit::PasteTracks {
                tracks: vec![source.clone()],
                before: None,
                group: None,
            },
        )
        .unwrap();
        let copy = if track == TrackId(1) {
            p.tracks.last().unwrap()
        } else {
            p.outputs.last().unwrap()
        };
        for (old, new) in source.devices.iter().zip(&copy.devices) {
            assert_ne!(old.id, new.id);
            assert_eq!(old.kind, new.kind);
        }
        for clip in copy.slots.iter().filter_map(|s| s.clip.as_ref()) {
            for reference in clip.lane_refs() {
                assert!(copy.event_def(&reference).is_some());
                assert!(source.event_def(&reference).is_none());
            }
        }
    }
    p.validate().unwrap();
}

#[test]
fn device_edits_round_trip_on_clip_tracks() {
    let mut p = project();
    for track in [TrackId(1), TrackId(2)] {
        let device = add(&mut p, track, None);
        edit::apply(
            &mut p,
            ProjectEdit::SetDevicesActive {
                devices: vec![device],
                active: false,
            },
        )
        .unwrap();
    }
    let path =
        std::env::temp_dir().join(format!("qtable-chain-edits-{}.qtable", std::process::id()));
    file::save(&path, &p).unwrap();
    assert_eq!(file::load(&path).unwrap(), p);
    std::fs::remove_file(path).unwrap();
    let text = ron::ser::to_string(&Project::default()).unwrap();
    assert!(!text.contains("devices"));
}

#[test]
fn inactive_target_resumes_on_time_without_replaying_muted_hits() {
    let receiver = TestReceiver::bind().unwrap();
    let mut p = project();
    p.tempo = Tempo::new(120.0).unwrap();
    p.outputs = vec![Track::new_output(
        TrackId(201),
        "receiver",
        "127.0.0.1",
        receiver.addr().port(),
    )];
    p.tracks = vec![Track::new(TrackId(1), "source", p.scenes.len())];
    p.tracks[0].route = Route::single(TrackId(201));
    let mut clip = Clip::new(ClipId(1), "ticks", BeatTime(9600), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.quantisation = LaunchQuantisation::None;
    clip.looping = false;
    for index in 0..20 {
        clip.events.push(p.tracks[0].hit(
            BeatTime(index * 384),
            OscAddress::Relative("/tick".into()),
            vec![OscArgument::Int(index as i32)],
            || DeviceId(1),
        ));
    }
    p.tracks[0].slots[0].clip = Some(clip);
    let mut engine = Engine::new(p).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let first = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(first[0].1.args, vec![rosc::OscType::Int(0)]);
    engine
        .execute(Command::Edit(ProjectEdit::SetDevicesActive {
            devices: vec![DeviceId(1)],
            active: false,
        }))
        .unwrap();
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(650))
            .is_err()
    );
    engine
        .execute(Command::Edit(ProjectEdit::SetDevicesActive {
            devices: vec![DeviceId(1)],
            active: true,
        }))
        .unwrap();
    let resumed = receiver
        .wait_for_messages(2, Duration::from_secs(2))
        .unwrap();
    let rosc::OscType::Int(index) = resumed[0].1.args[0] else {
        panic!("expected tick index")
    };
    assert!(index >= 4, "muted hit replayed: {index}");
    assert_eq!(resumed[1].1.args, vec![rosc::OscType::Int(index + 1)]);
    assert!(resumed[1].0.duration_since(resumed[0].0) > Duration::from_millis(100));
}

#[test]
fn targets_on_non_clip_kinds_reject_all_edit_paths_atomically() {
    let original = project();
    let source = original.tracks[0].devices.clone();
    for track in [TrackId(10), TrackId(201), original.main.id] {
        assert!(original.track(track).unwrap().capabilities().device_chain);
        let mut engine = Engine::new(original.clone()).unwrap();
        for edit in [
            ProjectEdit::AddDevice {
                track,
                kind: NewDevice::Target,
                before: None,
            },
            ProjectEdit::PasteDevices {
                track,
                devices: source.clone(),
                before: None,
            },
        ] {
            let revision = engine.project_revision();
            let error = engine.execute(Command::Edit(edit)).unwrap_err();
            assert!(error.to_string().contains("Targets require a Track"));
            assert_eq!(engine.project(), &original);
            assert_eq!(engine.project_revision(), revision);
        }
        // Such an owner cannot enter an Engine. Also guard direct edits of raw model data.
        let mut invalid = original.clone();
        let device = NewDevice::Target.build(invalid.next_device_id());
        let id = device.id;
        invalid.track_mut(track).unwrap().devices.push(device);
        assert!(Engine::new(invalid.clone()).is_err());
        for edit in [
            ProjectEdit::DuplicateDevices { devices: vec![id] },
            ProjectEdit::MoveDevice {
                device: id,
                before: None,
            },
            ProjectEdit::MoveDevice {
                device: id,
                before: Some(id),
            },
            ProjectEdit::PasteTracks {
                tracks: vec![invalid.track(track).unwrap().clone()],
                before: None,
                group: None,
            },
        ] {
            let mut p = invalid.clone();
            assert!(edit::apply(&mut p, edit).is_err());
            assert_eq!(p, invalid);
        }
        assert!(
            invalid
                .validate()
                .unwrap_err()
                .0
                .contains("Targets require a Track")
        );
        let path = std::env::temp_dir().join(format!(
            "qtable-invalid-target-{}-{}.qtable",
            std::process::id(),
            track.0
        ));
        let text = format!(
            "(version:{},project:{})",
            file::FORMAT_VERSION,
            ron::to_string(&invalid).unwrap()
        );
        std::fs::write(&path, text).unwrap();
        assert!(
            file::load(&path)
                .unwrap_err()
                .to_string()
                .contains("Targets require a Track")
        );
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn grouping_and_ungrouping_keep_targets_on_their_member_tracks() {
    let mut p = project();
    let devices = p.tracks[0].devices.clone();
    edit::apply(
        &mut p,
        ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1)],
        },
    )
    .unwrap();
    let group = p.track(TrackId(1)).unwrap().group.unwrap();
    assert!(p.track(group).unwrap().devices.is_empty());
    assert_eq!(p.track(TrackId(1)).unwrap().devices, devices);
    edit::apply(&mut p, ProjectEdit::Ungroup { group }).unwrap();
    assert!(p.track(group).is_none());
    assert_eq!(p.track(TrackId(1)).unwrap().devices, devices);
    p.validate().unwrap();
}

#[test]
fn fold_state_defaults_collapsed_and_round_trips_without_changing_definitions() {
    let mut p = project();
    let migrated = p.tracks[0].devices[0].id;
    assert!(!p.device(migrated).unwrap().1.expanded);
    let id = add(&mut p, TrackId(1), None);
    assert!(p.device(id).unwrap().1.expanded);
    let before = p.device(id).unwrap().1.kind.clone();
    let report = edit::apply(
        &mut p,
        ProjectEdit::SetDeviceExpanded {
            device: id,
            expanded: false,
        },
    )
    .unwrap();
    assert_eq!(report, edit::EditReport::default());
    assert_eq!(p.device(id).unwrap().1.kind, before);
    let device = p.device(id).unwrap().1;
    let text = ron::to_string(device).unwrap();
    assert!(!text.contains("expanded"));
    assert!(!ron::from_str::<Device>(&text).unwrap().expanded);
    edit::apply(
        &mut p,
        ProjectEdit::SetDeviceExpanded {
            device: id,
            expanded: true,
        },
    )
    .unwrap();
    let text = ron::to_string(p.device(id).unwrap().1).unwrap();
    assert!(ron::from_str::<Device>(&text).unwrap().expanded);
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::SetDeviceExpanded {
                device: DeviceId(9999),
                expanded: false
            }
        )
        .is_err()
    );
}
