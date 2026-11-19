use qtable_engine::{
    engine::{Command, Engine},
    model::*,
};
use qtable_test_support::TestReceiver;
use rosc::OscType;
use std::time::{Duration, Instant};

fn project(receiver: &TestReceiver) -> Project {
    let mut track = Track::new(TrackId(1), "Automated", 1);
    track.route = Route::single(TrackId(2));
    let device = track.first_target_mut_or_insert(|| DeviceId(1));
    let target = device.target_mut().unwrap();
    let mut y = FieldDef::new(FieldKey("y".into()), FieldType::Int, true);
    y.range = Some([0.0, 10.0]);
    y.value = Some(7.0);
    target.parameters.push(ParameterDef {
        key: EntryKey("xy".into()),
        name: String::new(),
        path: OscAddress::Relative("/xy".into()),
        fields: vec![
            FieldDef::new(FieldKey("x".into()), FieldType::Float, true),
            y,
        ],
        send_mode: SendMode::OnRelease,
    });
    let mut clip = Clip::new(ClipId(1), "Ramp", BeatTime(960), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.looping = false;
    clip.lanes.push(Lane::Envelope(AutomationEnvelope {
        region: None,
        target: ArgumentRef {
            device: DeviceRef::Chain(DeviceId(1)),
            parameter: ParameterKey("xy".into()),
            field: FieldKey("x".into()),
        },
        points: vec![
            Breakpoint {
                time: BeatTime(0),
                value: 0.0,
                curve: 0.0,
            },
            Breakpoint {
                time: BeatTime(960),
                value: 1.0,
                curve: 0.0,
            },
        ],
    }));
    track.slots[0].clip = Some(clip);
    Project {
        tracks: vec![track],
        outputs: vec![Track::new_output(
            TrackId(2),
            "Out",
            "127.0.0.1",
            receiver.addr().port(),
        )],
        scenes: vec![Scene {
            id: SceneId(1),
            name: "Scene".into(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    }
}

#[test]
fn worker_samples_complete_messages_in_on_release_and_holds_without_saving_values() {
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::new(project(&receiver)).unwrap();
    let before = engine.project().clone();
    let revision = engine.project_revision();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let messages = receiver
        .wait_for_messages(8, Duration::from_secs(2))
        .unwrap();
    let values: Vec<_> = messages
        .iter()
        .map(|(_, message)| {
            assert_eq!(message.addr, "/xy");
            let [OscType::Float(x), OscType::Int(7)] = message.args.as_slice() else {
                panic!("complete xy: {:?}", message.args)
            };
            *x
        })
        .collect();
    assert!(values.windows(2).all(|w| w[1] >= w[0]), "{values:?}");
    assert!(values.last().unwrap() - values[0] > 0.2, "{values:?}");
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let values = engine.snapshot().automation;
        if values
            .iter()
            .any(|v| v.value == 1.0 && v.state == qtable_engine::scheduler::AutomationState::Held)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "did not hold the endpoint: {values:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    loop {
        let messages = receiver
            .wait_for_messages(1, Duration::from_secs(1))
            .unwrap();
        if messages[0].1.args[0] == OscType::Float(1.0) {
            break;
        }
    }
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(90))
            .is_err()
    );
    assert_eq!(engine.project(), &before);
    assert_eq!(engine.project_revision(), revision);
}

#[test]
fn curved_target_and_mapped_mixer_volume_share_the_same_frame_in_on_release() {
    let receiver = TestReceiver::bind().unwrap();
    let mut p = project(&receiver);
    p.outputs[0].output.as_mut().unwrap().stage.bundle = true;
    let volume = &mut p.tracks[0].mixer_device.volume.output;
    volume.enabled = true;
    volume.address = "/volume".into();
    volume.send_mode = SendMode::OnRelease;
    volume.mapping = Mapping::new([0.0, 1.0], [10.0, 20.0], 1.0, 0.0).unwrap();
    let clip = p.tracks[0].slots[0].clip.as_mut().unwrap();
    let Lane::Envelope(target) = &mut clip.lanes[0] else {
        unreachable!()
    };
    // Independent oracle: Target x = phase²; Volume = sqrt(phase), then mapped
    // to 10..20. The two wire values must satisfy x = ((volume - 10) / 10)^4.
    target.points[0].curve = 0.25;
    let mut mixer = target.clone();
    mixer.target = ArgumentRef::volume();
    mixer.points[0].curve = -0.25;
    clip.lanes.push(Lane::Envelope(mixer));
    let mut engine = Engine::new(p).unwrap();
    let before = engine.project().clone();
    let revision = engine.project_revision();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let messages = receiver
        .wait_for_messages(16, Duration::from_secs(2))
        .unwrap();
    let mut interior = 0;
    for pair in messages.chunks_exact(2) {
        // Bundle flattening preserves a common socket-read instant; no wall-clock
        // lateness assertion or assumed phase alignment is needed here.
        assert_eq!(pair[0].0, pair[1].0, "one complete frame bundle");
        let target = pair.iter().find(|(_, m)| m.addr == "/xy").unwrap();
        let mixer = pair.iter().find(|(_, m)| m.addr == "/volume").unwrap();
        let [OscType::Float(x), OscType::Int(7)] = target.1.args.as_slice() else {
            panic!("complete Target arguments: {:?}", target.1.args)
        };
        let [OscType::Float(volume)] = mixer.1.args.as_slice() else {
            panic!("mapped Volume: {:?}", mixer.1.args)
        };
        let normalized = (*volume as f64 - 10.0) / 10.0;
        assert!((0.0..=1.0).contains(x), "{x}");
        assert!((0.0..=1.0).contains(&normalized), "{volume}");
        assert!(
            (*x as f64 - normalized.powi(4)).abs() < 1e-5,
            "curves sampled at different phases or mapped incorrectly: {x}, {volume}"
        );
        interior += usize::from(*x > 0.0 && *x < 1.0);
        println!("curved frame: /xy [{x}, 7], /volume [{volume}]");
    }
    assert!(
        interior >= 4,
        "must exercise curved segments, not just endpoints"
    );
    assert_eq!(engine.project(), &before);
    assert_eq!(engine.project_revision(), revision);
}

#[test]
fn discrete_arguments_and_continuous_values_share_one_complete_message() {
    let receiver = TestReceiver::bind().unwrap();
    let mut p = project(&receiver);
    let target = p.tracks[0].devices[0].target_mut().unwrap();
    target.parameters[0].fields.push(FieldDef::new(
        FieldKey("gate".into()),
        FieldType::Bool,
        true,
    ));
    target.parameters[0].fields.push(FieldDef::new(
        FieldKey("mode".into()),
        FieldType::String,
        true,
    ));
    for field in ["gate", "mode"] {
        p.tracks[0].slots[0]
            .clip
            .as_mut()
            .unwrap()
            .lanes
            .push(Lane::Envelope(AutomationEnvelope {
                region: None,
                target: ArgumentRef {
                    device: DeviceRef::Chain(DeviceId(1)),
                    parameter: ParameterKey("xy".into()),
                    field: FieldKey(field.into()),
                },
                points: vec![
                    Breakpoint {
                        time: BeatTime(0),
                        value: 0.0,
                        curve: 0.0,
                    },
                    Breakpoint {
                        time: BeatTime(480),
                        value: 1.0,
                        curve: 0.0,
                    },
                ],
            }));
    }
    let mut engine = Engine::new(p).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let mut seen_off = false;
    loop {
        let batch = receiver
            .wait_for_messages(1, Duration::from_secs(2))
            .unwrap();
        let args = &batch[0].1.args;
        assert_eq!(args.len(), 4);
        assert_eq!(args[1], OscType::Int(7));
        match (&args[2], &args[3]) {
            (OscType::Bool(false), OscType::String(mode)) if mode == "One" => seen_off = true,
            (OscType::Bool(true), OscType::String(mode)) if mode == "Two" => {
                assert!(seen_off);
                break;
            }
            _ => panic!("partial or wrong discrete message: {args:?}"),
        }
    }
}

#[test]
fn threshold_holds_ramp_samples_but_delivers_the_final_value() {
    let receiver = TestReceiver::bind().unwrap();
    let mut p = project(&receiver);
    let target = p.tracks[0].devices[0].target_mut().unwrap();
    target.parameters[0].fields[0].threshold = 1.0;
    let mut engine = Engine::new(p).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let messages = receiver
        .wait_for_messages(2, Duration::from_secs(2))
        .unwrap();
    let first = match messages[0].1.args[0] {
        OscType::Float(v) => v,
        _ => panic!("float"),
    };
    assert!(first < 0.02, "launch: {first}");
    assert_eq!(messages[1].1.args[0], OscType::Float(1.0));
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(90))
            .is_err()
    );
}

fn off_grid_jump_project(receiver: &TestReceiver) -> Project {
    let mut p = project(receiver);
    p.outputs[0].output.as_mut().unwrap().stage.frame_rate = 1.0;
    let target = p.tracks[0].devices[0].target_mut().unwrap();
    target.parameters[0].fields.push(FieldDef::new(
        FieldKey("gate".into()),
        FieldType::Bool,
        true,
    ));
    p.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .push(Lane::Envelope(AutomationEnvelope {
            region: None,
            target: ArgumentRef {
                device: DeviceRef::Chain(DeviceId(1)),
                parameter: ParameterKey("xy".into()),
                field: FieldKey("gate".into()),
            },
            points: vec![
                Breakpoint {
                    time: BeatTime(0),
                    value: 0.0,
                    curve: 0.0,
                },
                Breakpoint {
                    time: BeatTime(240),
                    value: 1.0,
                    curve: 0.0,
                },
            ],
        }));
    p
}

#[test]
fn off_grid_jump_samples_other_arguments_and_deduplicates_transport_start() {
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::new(off_grid_jump_project(&receiver)).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let messages = receiver
        .wait_for_messages(2, Duration::from_secs(2))
        .unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0].1.args,
        vec![OscType::Float(0.0), OscType::Int(7), OscType::Bool(false)]
    );
    assert_eq!(
        messages[1].1.args,
        vec![OscType::Float(0.25), OscType::Int(7), OscType::Bool(true)]
    );
}

#[test]
fn closed_sources_send_nothing_while_automation_runs_and_reopen_with_the_current_sample() {
    use qtable_engine::edit::ProjectEdit;
    for soloed_away in [false, true] {
        let receiver = TestReceiver::bind().unwrap();
        let mut p = off_grid_jump_project(&receiver);
        p.outputs[0].output.as_mut().unwrap().stage.frame_rate = 30.0;
        p.tracks[0].slots[0].clip.as_mut().unwrap().looping = true;
        let (close, reopen) = if soloed_away {
            let mut other = Track::new(TrackId(3), "Solo", p.scenes.len());
            other.mixer.solo = true;
            p.tracks.push(other);
            let solo = |solo| ProjectEdit::SetSolo {
                strips: vec![TrackId(3)],
                solo,
                exclusive: false,
            };
            (solo(true), solo(false))
        } else {
            p.tracks[0].mixer.active = false;
            let active = |active| ProjectEdit::SetActive {
                strips: vec![TrackId(1)],
                active,
            };
            (active(false), active(true))
        };
        let mut engine = Engine::new(p).unwrap();
        let launched = Instant::now();
        engine
            .execute(Command::LaunchClip {
                track: TrackId(1),
                slot: 0,
            })
            .unwrap();
        // The launch, the ramp's frames and the 125 ms Toggle step all stay home.
        assert!(
            receiver
                .wait_for_messages(1, Duration::from_millis(300))
                .is_err()
        );
        let x = |engine: &Engine| {
            engine
                .snapshot()
                .automation
                .iter()
                .find(|value| value.field.field.0 == "x")
                .map(|value| value.value)
        };
        assert!(x(&engine).is_some_and(|x| x > 0.4), "{:?}", x(&engine));
        engine.execute(Command::Edit(reopen)).unwrap();
        let first = receiver
            .wait_for_messages(1, Duration::from_millis(500))
            .unwrap();
        // One complete current message: no launch value, no replayed step.
        let [OscType::Float(sent), OscType::Int(7), OscType::Bool(true)] = first[0].1.args[..]
        else {
            panic!("{:?}", first[0].1.args)
        };
        let elapsed = first[0].0 - launched;
        assert!(
            sent > 0.4 && f64::from(sent) <= elapsed.as_secs_f64() / 0.5 + 0.05,
            "{sent} after {elapsed:?}"
        );
        engine.execute(Command::Edit(close)).unwrap();
        // Frames already owed before the close may still arrive; then nothing does.
        std::thread::sleep(Duration::from_millis(100));
        while receiver
            .wait_for_messages(1, Duration::from_millis(1))
            .is_ok()
        {}
        let before = x(&engine);
        assert!(
            receiver
                .wait_for_messages(1, Duration::from_millis(300))
                .is_err()
        );
        assert_ne!(x(&engine), before, "automation keeps moving while closed");
    }
}

#[test]
fn command_override_and_reenable_are_runtime_only_and_per_argument() {
    use qtable_engine::{edit::ProjectEdit, scheduler::AutomationState};
    let receiver = TestReceiver::bind().unwrap();
    let mut p = project(&receiver);
    p.tracks[0].slots[0].clip.as_mut().unwrap().looping = true;
    p.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .push(Lane::Envelope(AutomationEnvelope {
            region: None,
            target: ArgumentRef {
                device: DeviceRef::Chain(DeviceId(1)),
                parameter: ParameterKey("xy".into()),
                field: FieldKey("y".into()),
            },
            points: vec![
                Breakpoint {
                    time: BeatTime(0),
                    value: 7.0,
                    curve: 0.0,
                },
                Breakpoint {
                    time: BeatTime(960),
                    value: 10.0,
                    curve: 0.0,
                },
            ],
        }));
    let mut engine = Engine::new(p).unwrap();
    let key = FieldRef {
        parameter: ParameterRef {
            track: TrackId(1),
            device: DeviceRef::Chain(DeviceId(1)),
            key: ParameterKey("xy".into()),
        },
        field: FieldKey("x".into()),
    };
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    receiver
        .wait_for_messages(3, Duration::from_secs(2))
        .unwrap();
    let base = engine.project().field(&key).unwrap().value;
    engine
        .execute(Command::Edit(ProjectEdit::SetParameter {
            target: key.parameter.clone(),
            values: vec![(key.field.clone(), base)],
        }))
        .unwrap();
    let wait = |engine: &Engine, wanted| {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if engine
                .snapshot()
                .automation
                .iter()
                .any(|v| v.field == key && v.state == wanted)
            {
                break;
            }
            assert!(Instant::now() < deadline, "missing state {wanted:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    wait(&engine, AutomationState::Overridden);
    assert!(
        engine
            .snapshot()
            .automation
            .iter()
            .any(|v| v.field.field.0 == "y" && v.state == AutomationState::Following)
    );
    let revision = engine.project_revision();
    engine
        .execute(Command::ReEnableAutomation {
            field: Some(key.clone()),
        })
        .unwrap();
    wait(&engine, AutomationState::Following);
    assert_eq!(engine.project_revision(), revision);
    assert_eq!(engine.project().field(&key).unwrap().value, base);
}

#[test]
fn exact_jump_frames_reuse_the_jump_sample() {
    use qtable_engine::scheduler::SchedulerEvent;
    let receiver = TestReceiver::bind().unwrap();
    let mut engine = Engine::with_diagnostics(off_grid_jump_project(&receiver), true).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    receiver
        .wait_for_messages(2, Duration::from_secs(2))
        .unwrap();
    let jumps: Vec<_> = engine
        .scheduler_events()
        .try_iter()
        .filter_map(|event| match event {
            SchedulerEvent::FrameSent { profile, .. } if profile.jump.is_some() => Some(profile),
            _ => None,
        })
        .collect();
    assert!(!jumps.is_empty());
    for profile in jumps {
        assert_eq!((profile.considered, profile.sampled), (1, 0), "{profile:?}");
    }
}

#[test]
fn worker_sends_unlinked_wraps_then_holds_own_end_while_clip_keeps_playing() {
    use qtable_engine::edit::{LaneEdit, ProjectEdit};
    use qtable_engine::model::region::Region;
    let receiver = TestReceiver::bind().unwrap();
    let mut p = project(&receiver);
    let clip = p.clip_mut(TrackId(1), 0).unwrap();
    clip.looping = true;
    clip.end = BeatTime(16 * 960);
    clip.loop_end = clip.end;
    let own = Region {
        start: BeatTime(0),
        end: BeatTime(240),
        loop_start: BeatTime(0),
        loop_end: BeatTime(240),
        looping: true,
    };
    let Lane::Envelope(envelope) = &mut clip.lanes[0] else {
        unreachable!()
    };
    envelope.region = Some(own);
    let lane = qtable_engine::edit::LaneKey::Envelope(envelope.target.clone());
    let mut engine = Engine::new(p).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let messages = receiver
        .wait_for_messages(20, Duration::from_secs(3))
        .unwrap();
    let values: Vec<_> = messages
        .iter()
        .map(|(_, message)| {
            assert_eq!(message.addr, "/xy");
            let [OscType::Float(x), OscType::Int(7)] = message.args.as_slice() else {
                panic!("complete xy: {:?}", message.args)
            };
            *x
        })
        .collect();
    assert!(
        values.windows(2).filter(|w| w[0] - w[1] > 0.05).count() >= 2,
        "{values:?}"
    );
    engine
        .execute(Command::Edit(ProjectEdit::EditLane {
            track: TrackId(1),
            slot: 0,
            edit: LaneEdit::SetRegion {
                lane,
                region: Some(Region {
                    looping: false,
                    ..own
                }),
            },
        }))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let snapshot = engine.snapshot();
        if snapshot
            .automation
            .iter()
            .any(|v| v.value == 0.25 && v.state == qtable_engine::scheduler::AutomationState::Held)
        {
            assert!(
                !snapshot.clip_positions.is_empty(),
                "clip continues after lane end"
            );
            break;
        }
        assert!(Instant::now() < deadline, "lane did not hold its end");
        std::thread::sleep(Duration::from_millis(10));
    }
    loop {
        let messages = receiver
            .wait_for_messages(1, Duration::from_secs(1))
            .unwrap();
        if messages[0].1.args[0] == OscType::Float(0.25) {
            break;
        }
    }
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(100))
            .is_err()
    );
}
