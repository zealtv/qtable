use super::super::parameter_controls::SendValue;
use super::*;

fn setup(delay_ms: f64, looping: bool) -> (Planner, InternalClock, Automation, Instant, FieldRef) {
    let mut project = super::super::tests::project(delay_ms, looping);
    project.tracks[0].mixer_device.volume.output.enabled = true;
    let target = ArgumentRef::volume();
    project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .push(Lane::Envelope(AutomationEnvelope {
            region: None,
            target: target.clone(),
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
    let mut planner = Planner::new(project, Duration::ZERO).unwrap();
    let mut clock = InternalClock::default();
    let now = Instant::now();
    let mut automation = Automation::default();
    automation.sync(&planner, &clock, now);
    planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
    automation.sync(&planner, &clock, now);
    (planner, clock, automation, now, target.on(TrackId(1)))
}
fn value(automation: &Automation, target: &ParameterRef, at: Instant, delay: f64) -> f32 {
    let sent = automation.sample(target, at, delay).unwrap();
    let SendValue::Float(v) = sent.0[0] else {
        panic!("float")
    };
    v
}

#[test]
fn planner_keeps_each_discrete_jump_across_looped_eighth_notes() {
    let mut project = super::super::tests::project(0.0, true);
    let track = &mut project.tracks[0];
    let device = track.first_target_mut_or_insert(|| DeviceId(99));
    let device_id = device.id;
    let target = device.target_mut().unwrap();
    target.parameters.push(ParameterDef {
        key: EntryKey("toggle".into()),
        name: String::new(),
        path: OscAddress::Relative("/toggle".into()),
        fields: vec![FieldDef::new(
            FieldKey("value".into()),
            FieldType::Bool,
            true,
        )],
        send_mode: SendMode::Continuous,
    });
    track.slots[0].clip.as_mut().unwrap().end = BeatTime(3840);
    track.slots[0].clip.as_mut().unwrap().loop_end = BeatTime(3840);
    track.slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .push(Lane::Envelope(AutomationEnvelope {
            region: None,
            target: ArgumentRef {
                device: DeviceRef::Chain(device_id),
                parameter: ParameterKey("toggle".into()),
                field: FieldKey("value".into()),
            },
            points: (0..8)
                .map(|step| Breakpoint {
                    time: BeatTime(step * 480),
                    value: (step % 2) as f64,
                    curve: 0.0,
                })
                .collect(),
        }));
    let mut planner = Planner::new(project, Duration::from_millis(5)).unwrap();
    let mut clock = InternalClock::default();
    let now = Instant::now();
    planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
    let mut jumps = Vec::new();
    for step in 0..=24 {
        let at = now + Duration::from_millis(step * 125);
        for event in planner.plan(&clock, at) {
            if matches!(event.action, super::super::PlannedAction::Automate { .. }) {
                jumps.push(event.at.duration_since(now));
            }
            planner.consume(&event);
        }
    }
    let expected: Vec<_> = (0..=12)
        .map(|step| Duration::from_millis(step * 250))
        .collect();
    assert_eq!(jumps, expected);
}

#[test]
fn frame_samples_fractional_time_and_each_outputs_own_delay() {
    let (_, _, a, now, key) = setup(100.0, true);
    assert!(
        (value(&a, &key.parameter, now + Duration::from_micros(125125), 0.0) - 0.25025).abs()
            < 1e-6
    );
    assert!(
        (value(&a, &key.parameter, now + Duration::from_millis(225), 100.0) - 0.25).abs() < 1e-6
    );
    assert!(
        (value(&a, &key.parameter, now + Duration::from_millis(25), -100.0) - 0.25).abs() < 1e-6
    );
}

#[test]
fn stop_keeps_the_delayed_tail_and_holds_the_boundary_without_editing_base() {
    let (mut p, mut c, mut a, now, key) = setup(200.0, true);
    let stop = now + Duration::from_millis(250);
    p.stop_transport(&c, stop);
    c.stop(stop);
    a.sync(&p, &c, stop);
    assert!(
        (value(&a, &key.parameter, now + Duration::from_millis(300), 200.0) - 0.2).abs() < 1e-6
    );
    assert!(
        (value(&a, &key.parameter, now + Duration::from_millis(450), 200.0) - 0.5).abs() < 1e-6
    );
    assert_eq!(
        a.values(stop + Duration::from_secs(2))[0].state,
        AutomationState::Held
    );
    assert_eq!(p.project.field(&key).unwrap().value, 1.0);
}

#[test]
fn delayed_samples_keep_the_old_tempo_segment() {
    let (mut p, mut c, mut a, now, key) = setup(300.0, true);
    let change = now + Duration::from_millis(250);
    p.set_tempo(&mut c, Tempo::new(60.0).unwrap(), change);
    a.sync(&p, &c, change);
    assert!(
        (value(&a, &key.parameter, now + Duration::from_millis(400), 300.0) - 0.2).abs() < 1e-6
    );
    assert!(
        (value(&a, &key.parameter, now + Duration::from_millis(700), 300.0) - 0.65).abs() < 1e-6
    );
}

#[test]
fn natural_end_holds_closed_endpoint_after_voice_retirement() {
    let (mut p, c, mut a, now, key) = setup(0.0, false);
    let end = now + Duration::from_millis(500);
    p.voices.clear();
    a.sync(&p, &c, end);
    assert_eq!(
        value(&a, &key.parameter, end + Duration::from_secs(1), 0.0),
        1.0
    );
    assert_eq!(a.values(end)[0].state, AutomationState::Held);
    assert!(
        a.next_frame(&key.parameter, end + Duration::from_nanos(1), 0.0)
            .is_none()
    );
}

#[test]
fn empty_replacement_holds_outgoing_value_and_future_trigger_does_not_take_over_early() {
    let (mut p, c, mut a, now, key) = setup(0.0, true);
    p.launch_at(TrackId(1), 1, BeatTime(480)).unwrap();
    a.sync(&p, &c, now + Duration::from_millis(100));
    assert!((value(&a, &key.parameter, now + Duration::from_millis(125), 0.0) - 0.25).abs() < 1e-6);
    assert_eq!(
        value(&a, &key.parameter, now + Duration::from_millis(300), 0.0),
        0.5
    );
}

#[test]
fn flat_curve_has_no_repeated_frame_deadline() {
    let (mut p, c, mut a, now, key) = setup(0.0, true);
    let mut project = (*p.project).clone();
    let Lane::Envelope(e) = project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .last_mut()
        .unwrap()
    else {
        unreachable!()
    };
    e.points[1].value = 0.0;
    p.update_project(project, &c, now).unwrap();
    a.sync(&p, &c, now);
    assert!(
        a.next_frame(&key.parameter, now + Duration::from_millis(1), 0.0)
            .is_none()
    );
}

#[test]
fn mixer_mapping_and_flat_threshold_input_use_the_shared_conversion() {
    let (mut p, c, mut a, now, key) = setup(0.0, true);
    let mut project = (*p.project).clone();
    project.tracks[0].mixer_device.volume.output.mapping =
        Mapping::new([0.0, 1.0], [10.0, 20.0], 1.0, 0.0).unwrap();
    p.update_project(project, &c, now).unwrap();
    a.sync(&p, &c, now);
    assert_eq!(
        value(&a, &key.parameter, now + Duration::from_millis(250), 0.0),
        15.0
    );
    assert_eq!(a.values(now + Duration::from_millis(250))[0].value, 0.5);
}

#[test]
fn point_edits_keep_the_old_curve_until_each_outputs_delay_passes() {
    let (mut p, c, mut a, now, key) = setup(200.0, true);
    let change = now + Duration::from_millis(100);
    let mut project = (*p.project).clone();
    let Lane::Envelope(e) = project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .last_mut()
        .unwrap()
    else {
        unreachable!()
    };
    e.points[0].value = 0.8;
    e.points[1].value = 0.8;
    p.update_project(project, &c, change).unwrap();
    a.sync(&p, &c, change);
    assert!(
        (value(&a, &key.parameter, now + Duration::from_millis(250), 200.0) - 0.1).abs() < 1e-6
    );
    assert!(
        (value(&a, &key.parameter, now + Duration::from_millis(350), 200.0) - 0.8).abs() < 1e-6
    );
}

#[test]
fn history_retains_only_the_delay_window_and_one_preceding_state() {
    let (p, c, mut a, now, _) = setup(200.0, true);
    for i in 1..100 {
        a.sync(&p, &c, now + Duration::from_millis(i * 50));
    }
    assert!(a.history.len() <= 6, "{} histories", a.history.len());
}

#[test]
fn stopping_at_loop_wrap_holds_the_outgoing_endpoint() {
    let (mut p, mut c, mut a, now, key) = setup(0.0, true);
    let stop = now + Duration::from_millis(500);
    // Preserve the outgoing cutoff even when the zero-delay voice is retired immediately.
    p.stop_transport(&c, stop);
    c.stop(stop);
    a.sync(&p, &c, stop);
    assert_eq!(value(&a, &key.parameter, stop, 0.0), 1.0);
}

#[test]
fn unchanged_base_reset_overrides_and_delayed_output_keeps_prior_curve() {
    let (p, c, mut a, now, key) = setup(200.0, true);
    let edit = now + Duration::from_millis(125);
    a.sync(&p, &c, edit);
    a.control(edit, std::slice::from_ref(&key), None);
    assert_eq!(a.values(edit)[0].state, AutomationState::Overridden);
    assert_eq!(value(&a, &key.parameter, edit, 0.0), 1.0);
    assert!(
        (value(&a, &key.parameter, now + Duration::from_millis(250), 200.0) - 0.1).abs() < 1e-6
    );
    a.sync(&p, &c, edit + Duration::from_millis(10));
    assert_eq!(
        a.values(edit + Duration::from_millis(10))[0].state,
        AutomationState::Overridden
    );
    a.control(
        edit + Duration::from_millis(10),
        &[],
        Some(&Some(key.clone())),
    );
    assert!((value(&a, &key.parameter, edit + Duration::from_millis(10), 0.0) - 0.27).abs() < 1e-6);
}

#[test]
fn queued_relaunch_restores_only_at_boundary() {
    let (mut p, c, mut a, now, key) = setup(0.0, true);
    let edit = now + Duration::from_millis(125);
    a.sync(&p, &c, edit);
    a.control(edit, std::slice::from_ref(&key), None);
    p.launch_at(TrackId(1), 0, BeatTime(960)).unwrap();
    a.sync(&p, &c, edit + Duration::from_millis(10));
    assert_eq!(
        a.values(now + Duration::from_millis(400))[0].state,
        AutomationState::Overridden
    );
    assert_eq!(
        a.values(now + Duration::from_millis(500))[0].state,
        AutomationState::Following
    );
    assert_eq!(
        value(&a, &key.parameter, now + Duration::from_millis(500), 0.0),
        0.0
    );
}

#[test]
fn planner_orders_launch_and_pair_jumps_before_hits_and_keeps_delayed_tail() {
    let (mut p, mut c, _, now, _) = setup(200.0, false);
    let mut project = (*p.project).clone();
    let e = project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .iter_mut()
        .find_map(|l| match l {
            Lane::Envelope(e) => Some(e),
            _ => None,
        })
        .unwrap();
    e.points = vec![
        Breakpoint {
            time: BeatTime(0),
            value: 0.0,
            curve: 0.0,
        },
        Breakpoint {
            time: BeatTime(240),
            value: 0.0,
            curve: 0.0,
        },
        Breakpoint {
            time: BeatTime(240),
            value: 1.0,
            curve: 0.0,
        },
    ];
    p.update_project(project, &c, now).unwrap();
    let first = p.plan(&c, now + Duration::from_millis(200));
    assert!(matches!(
        first[0].action,
        super::super::PlannedAction::Automate { .. }
    ));
    assert_eq!(first[0].at, now + Duration::from_millis(200));
    for event in first {
        p.consume(&event);
    }
    p.stop_transport(&c, now + Duration::from_millis(150));
    c.stop(now + Duration::from_millis(150));
    let tail = p.plan(&c, now + Duration::from_millis(400));
    let jumps: Vec<_> = tail
        .iter()
        .filter(|e| matches!(e.action, super::super::PlannedAction::Automate { .. }))
        .collect();
    assert_eq!(jumps.len(), 1);
    assert_eq!(jumps[0].at, now + Duration::from_millis(325));
}

#[test]
fn planner_retains_a_natural_end_jump_after_silence_and_tempo_change() {
    let (mut p, mut c, _, now, _) = setup(200.0, false);
    let mut project = (*p.project).clone();
    let e = project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .iter_mut()
        .find_map(|l| match l {
            Lane::Envelope(e) => Some(e),
            _ => None,
        })
        .unwrap();
    e.points.insert(
        1,
        Breakpoint {
            time: BeatTime(960),
            value: 0.0,
            curve: 0.0,
        },
    );
    p.update_project(project, &c, now).unwrap();
    for event in p.plan(&c, now + Duration::from_millis(500)) {
        p.consume(&event);
    }
    assert!(!p.voices.is_empty());
    p.set_tempo(
        &mut c,
        Tempo::new(60.0).unwrap(),
        now + Duration::from_millis(600),
    );
    let jumps: Vec<_> = p
        .plan(&c, now + Duration::from_millis(700))
        .into_iter()
        .filter(|e| matches!(e.action, super::super::PlannedAction::Automate { .. }))
        .collect();
    assert_eq!(jumps.len(), 1);
    assert_eq!(jumps[0].at, now + Duration::from_millis(700));
}

#[test]
fn manual_edit_replaces_held_value_even_while_an_ended_driver_has_a_tail() {
    let (mut p, c, mut a, now, key) = setup(200.0, false);
    let at = now + Duration::from_millis(600);
    a.sync(&p, &c, at);
    Arc::make_mut(&mut p.project)
        .set_parameter_values(&key.parameter, vec![(key.field.clone(), 0.2)])
        .unwrap();
    a.sync(&p, &c, at);
    a.control(at, std::slice::from_ref(&key), None);
    assert!((value(&a, &key.parameter, at, 0.0) - 0.2).abs() < 1e-6);
    a.sync(&p, &c, at + Duration::from_millis(20));
    assert!((value(&a, &key.parameter, at + Duration::from_millis(20), 0.0) - 0.2).abs() < 1e-6);
    assert_eq!(
        a.values(at + Duration::from_millis(20))[0].state,
        AutomationState::Held
    );
}

#[test]
fn named_updates_match_full_sync_for_following_held_and_delayed_values() {
    for looping in [false, true] {
        let (mut planner, clock, mut narrow, start, field) = setup(200.0, looping);
        let mut full = narrow.clone();
        for ms in [125, 250, 600, 620] {
            let at = start + Duration::from_millis(ms);
            let mut project = (*planner.project).clone();
            project
                .set_parameter_values(&field.parameter, vec![(field.field.clone(), 0.2)])
                .unwrap();
            planner.project = Arc::new(project);
            narrow.update_parameters(&planner.project, at, std::slice::from_ref(&field));
            full.sync(&planner, &clock, at);
            full.control(at, std::slice::from_ref(&field), None);
            for offset in [0, 50, 200, 250] {
                let sample_at = at + Duration::from_millis(offset);
                for delay in [-100.0, 0.0, 200.0] {
                    assert_eq!(
                        narrow.sample(&field.parameter, sample_at, delay),
                        full.sample(&field.parameter, sample_at, delay),
                        "looping={looping}, edit={ms}, sample={offset}, delay={delay}"
                    );
                }
                assert_eq!(narrow.values(sample_at), full.values(sample_at));
            }
        }
        let at = start + Duration::from_millis(650);
        narrow.control(at, &[], Some(&Some(field.clone())));
        full.control(at, &[], Some(&Some(field.clone())));
        assert_eq!(narrow.values(at), full.values(at));
        // New launches regain control even after an explicit override of a held value.
        let mut clock = clock;
        let launch = planner.launch(TrackId(1), 0, &mut clock, at).unwrap();
        narrow.sync(&planner, &clock, at);
        full.sync(&planner, &clock, at);
        let boundary = clock.instant_at(launch).unwrap();
        assert_eq!(narrow.values(boundary), full.values(boundary));
        assert_eq!(narrow.values(boundary)[0].state, AutomationState::Following);
    }
}

#[test]
fn named_updates_share_unrelated_preparation_and_update_undriven_message_arguments() {
    let (mut planner, clock, mut automation, start, volume) = setup(200.0, true);
    let mut project = (*planner.project).clone();
    let track = &mut project.tracks[0];
    let device = track.first_target_mut_or_insert(|| DeviceId(99));
    let id = device.id;
    let target = device.target_mut().unwrap();
    target.parameters.push(ParameterDef {
        key: EntryKey("xy".into()),
        name: String::new(),
        path: OscAddress::Relative("/xy".into()),
        fields: ["x", "y"]
            .into_iter()
            .map(|key| FieldDef::new(FieldKey(key.into()), FieldType::Float, true))
            .collect(),
        send_mode: SendMode::Continuous,
    });
    let x = ArgumentRef {
        device: DeviceRef::Chain(id),
        parameter: ParameterKey("xy".into()),
        field: FieldKey("x".into()),
    }
    .on(TrackId(1));
    let mut y = x.clone();
    y.field = FieldKey("y".into());
    let Lane::Envelope(mut envelope) = track.slots[0].clip.as_ref().unwrap().lanes[0].clone()
    else {
        panic!("envelope")
    };
    envelope.target = ArgumentRef {
        device: DeviceRef::Chain(id),
        parameter: x.parameter.key.clone(),
        field: x.field.clone(),
    };
    track.slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .push(Lane::Envelope(envelope));
    planner.project = Arc::new(project);
    automation.sync(&planner, &clock, start);
    let old = Arc::clone(automation.history.last().unwrap());
    let mut project = (*planner.project).clone();
    project
        .set_parameter_values(&x.parameter, vec![(y.field.clone(), 0.7)])
        .unwrap();
    planner.project = Arc::new(project);
    let at = start + Duration::from_millis(125);
    automation.update_parameters(&planner.project, at, std::slice::from_ref(&y));
    let new = automation.history.last().unwrap();
    assert!(Arc::ptr_eq(&old.fields[&x], &new.fields[&x]));
    assert!(Arc::ptr_eq(&old.fields[&volume], &new.fields[&volume]));
    assert!(Arc::ptr_eq(
        &old.parameters[&volume.parameter],
        &new.parameters[&volume.parameter]
    ));
    assert!(Arc::ptr_eq(
        &old.parameters[&x.parameter].template,
        &new.parameters[&x.parameter].template
    ));
    assert!(Arc::ptr_eq(
        &old.parameters[&x.parameter].positions,
        &new.parameters[&x.parameter].positions
    ));
    assert!(!automation.changes(&planner, &clock, at));
    let sample = automation.sample(&x.parameter, at, 0.0).unwrap();
    assert_eq!(sample.0[0], SendValue::Float(0.25));
    assert_eq!(sample.0[1], SendValue::Float(0.7));
    let delayed = automation.sample(&x.parameter, at, 100.0).unwrap();
    assert_eq!(
        delayed.0[1],
        SendValue::Float(0.0),
        "old complete message retained before edit"
    );
    assert_eq!(
        automation
            .values(at)
            .iter()
            .find(|v| v.field == x)
            .unwrap()
            .state,
        AutomationState::Following
    );
    let mut project = (*planner.project).clone();
    project
        .set_parameter_values(
            &x.parameter,
            vec![(x.field.clone(), 0.1), (y.field.clone(), 0.9)],
        )
        .unwrap();
    planner.project = Arc::new(project);
    let at = at + Duration::from_millis(20);
    automation.update_parameters(&planner.project, at, &[x.clone(), y]);
    let sample = automation.sample(&x.parameter, at, 0.0).unwrap();
    assert_eq!(&*sample.0, &[SendValue::Float(0.1), SendValue::Float(0.9)]);
    let values = automation.values(at);
    assert_eq!(
        values.iter().find(|v| v.field == x).unwrap().state,
        AutomationState::Overridden
    );
    assert_eq!(
        values.iter().find(|v| v.field == volume).unwrap().state,
        AutomationState::Following
    );
}

#[test]
fn jump_catchup_skips_loops_arithmetically_and_does_not_replay_sent_jumps() {
    let (mut p, c, _, now, _) = setup(0.0, true);
    let skipped = p.skip_stale_jumps(
        &c,
        now + Duration::from_secs(1000),
        Duration::from_millis(50),
    );
    assert_eq!(skipped, 2000); // launch and wraps through 999.5 seconds
    assert_eq!(
        p.skip_stale_jumps(
            &c,
            now + Duration::from_secs(1000),
            Duration::from_millis(50)
        ),
        0
    );
    let events = p.plan(&c, now + Duration::from_secs(1000));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e.action, super::super::PlannedAction::Automate { .. }))
            .count(),
        1
    );
}

#[test]
fn retiring_an_ended_voice_needs_no_sync_and_skipping_it_keeps_every_value() {
    let (mut p, c, a, now, key) = setup(0.0, false);
    let end = now + Duration::from_millis(500);
    p.voices.clear();
    assert!(!a.changes(&p, &c, end));
    let mut synced = a.clone();
    synced.sync(&p, &c, end);
    for ms in [500, 600, 1500] {
        let at = now + Duration::from_millis(ms);
        assert_eq!(
            value(&a, &key.parameter, at, 0.0),
            value(&synced, &key.parameter, at, 0.0)
        );
        assert_eq!(a.values(at), synced.values(at));
    }
}

#[test]
fn a_live_voice_ending_early_or_a_new_launch_changes_automation() {
    let (mut p, c, a, now, _) = setup(0.0, true);
    let at = now + Duration::from_millis(100);
    assert!(!a.changes(&p, &c, at));
    let mut launched = p.clone();
    launched.launch_at(TrackId(1), 0, BeatTime(960)).unwrap();
    assert!(a.changes(&launched, &c, at));
    p.voices.clear();
    assert!(a.changes(&p, &c, at));
}

/// Frozen L4 sampler, including its original wire conversion, for a deterministic cost
/// comparison and an oracle independent of the new template's conversion code.
impl Automation {
    fn allocating_sample(
        &self,
        target: &ParameterRef,
        at: Instant,
        delay_ms: f64,
    ) -> Option<SendValues> {
        let musical = delay::shift(at, -delay_ms)?;
        let state = self.state(musical)?;
        let indexed = state.parameters.get(target)?;
        let mut parameter = state.project.parameter(target)?;
        let mut driven = false;
        for field in &mut parameter.fields {
            let Some(key) = indexed.fields.iter().find(|key| key.field == *field.key) else {
                continue;
            };
            if let Some((value, _)) = state.value(key, musical) {
                field.value = field
                    .spec
                    .check(value.clamp(field.spec.min, field.spec.max))
                    .ok()?;
                driven = true;
            }
        }
        if !driven {
            return None;
        }
        Some(match target.device {
            DeviceRef::Mixer => SendValue::Float(
                parameter_value_flow(
                    &parameter.fields[0].spec,
                    parameter.fields[0].value,
                    parameter.output.mapping,
                )
                .ok()?,
            )
            .into(),
            DeviceRef::Chain(_) => SendValues(
                parameter
                    .fields
                    .iter()
                    .map(|field| SendValue::from(field.argument()))
                    .collect::<Vec<_>>()
                    .into(),
            ),
        })
    }
}

#[test]
fn frame_sample_plan_cost_has_zero_allocations_for_mixer_and_max_arity_target() {
    use super::super::plan_cost_tests::allocations;
    let (mut p, c, mut a, now, mixer) = setup(200.0, true);
    let mut project = (*p.project).clone();
    let track = &mut project.tracks[0];
    let device = track.first_target_mut_or_insert(|| DeviceId(99));
    let device_id = device.id;
    let target = device.target_mut().unwrap();
    target.parameters.push(ParameterDef {
        key: EntryKey("mixed".into()),
        name: "Mixed".into(),
        path: OscAddress::Relative("/mixed".into()),
        send_mode: SendMode::Continuous,
        fields: (0..FieldDef::MAX_ARGUMENTS)
            .map(|i| {
                let kind = [
                    FieldType::Float,
                    FieldType::Int,
                    FieldType::Bool,
                    FieldType::String,
                ][i % 4];
                let mut field = FieldDef::new(FieldKey(format!("arg{i}")), kind, true);
                if kind == FieldType::Int {
                    field.range = Some([-20_000_000.0, 20_000_000.0]);
                    field.value = Some(16_777_217.0);
                }
                field
            })
            .collect(),
    });
    // Leave one field of every type undriven: complete messages must retain their bases.
    for i in 0..12 {
        track.slots[0]
            .clip
            .as_mut()
            .unwrap()
            .lanes
            .push(Lane::Envelope(AutomationEnvelope {
                region: None,
                target: ArgumentRef {
                    device: DeviceRef::Chain(device_id),
                    parameter: ParameterKey("mixed".into()),
                    field: FieldKey(format!("arg{i}")),
                },
                points: vec![
                    Breakpoint {
                        time: BeatTime::ZERO,
                        value: 0.0,
                        curve: 0.0,
                    },
                    Breakpoint {
                        time: BeatTime(960),
                        value: 1.0,
                        curve: 0.5,
                    },
                ],
            }));
    }
    p.update_project(project, &c, now).unwrap();
    a.sync(&p, &c, now);
    let target = ParameterRef {
        track: TrackId(1),
        device: DeviceRef::Chain(device_id),
        key: ParameterKey("mixed".into()),
    };
    for key in [&mixer.parameter, &target] {
        let baseline = allocations(|| {
            std::hint::black_box(a.allocating_sample(key, now, 0.0).unwrap());
        });
        let sampled = allocations(|| {
            for i in 0..1024 {
                let sample = a
                    .sample(key, now + Duration::from_micros(i * 1000), 0.0)
                    .unwrap();
                // Cloning retained snapshots must also remain allocation-free.
                std::hint::black_box(sample.clone());
                std::hint::black_box(sample);
            }
        });
        assert!(baseline > 0);
        assert_eq!(sampled, 0);
        println!(
            "frame_sample_plan_cost {:?}: original {baseline} allocations/sample; prepared {sampled} allocations/1024 samples and clones",
            key.device
        );
        for delay in [-200.0, 0.0, 200.0] {
            for ms in [200, 225, 499, 500, 700, 1200] {
                let at = now + Duration::from_millis(ms);
                assert_eq!(
                    a.sample(key, at, delay),
                    a.allocating_sample(key, at, delay)
                );
            }
        }
    }
    let retained = a.sample(&target, now, 0.0).unwrap();
    let later = a
        .sample(&target, now + Duration::from_millis(400), 0.0)
        .unwrap();
    assert!(!retained.same(&later));
    assert_eq!(retained.0[13], SendValue::Int(16_777_217));
    // Reorder tracks and fields and alter an enum wire value: the delayed history keeps
    // its old template and the new state must rebuild every positional lookup.
    let edit = now + Duration::from_millis(250);
    let mut project = (*p.project).clone();
    project
        .tracks
        .insert(0, Track::new(TrackId(88), "before", 2));
    let definition = project
        .track_mut(TrackId(1))
        .unwrap()
        .devices
        .iter_mut()
        .find(|d| d.id == device_id)
        .unwrap()
        .target_mut()
        .unwrap();
    let definition = definition
        .parameters
        .iter_mut()
        .find(|d| d.key.0 == "mixed")
        .unwrap();
    definition.fields.reverse();
    let field = definition
        .fields
        .iter_mut()
        .find(|f| f.key.0 == "arg3")
        .unwrap();
    field.options[0].label = "edited".into();
    field.options[0].value = OscArgument::String("edited".into());
    field.default = field.options[0].value.clone();
    p.update_project(project, &c, edit).unwrap();
    a.sync(&p, &c, edit);
    for delay in [0.0, 200.0] {
        let at = now + Duration::from_millis(400);
        assert_eq!(
            a.sample(&target, at, delay),
            a.allocating_sample(&target, at, delay)
        );
    }
    assert_eq!(retained.0[13], SendValue::Int(16_777_217));
}

mod receive_time;
