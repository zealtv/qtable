//! Compare sampled values, not just launch positions, under queued command history.
use super::*;
use crate::scheduler::{SchedulerCommand, commands::EffectiveFloor};

fn update(
    planner: &mut Planner,
    automation: &mut Automation,
    floor: &mut EffectiveFloor,
    key: &FieldRef,
    value: f64,
    enqueued: Instant,
    dequeued: Instant,
) {
    let mut project = (*planner.project).clone();
    project
        .set_parameter_values(&key.parameter, vec![(key.field.clone(), value)])
        .unwrap();
    let envelope = SchedulerCommand::UpdateParameters(crate::engine::ValueEdit::for_test(
        project.clone(),
        vec![key.clone()],
    ))
    .enqueued(enqueued);
    let effective = floor.advance(&envelope, dequeued);
    planner.project = Arc::new(project);
    automation.update_parameters(&planner.project, effective, std::slice::from_ref(key));
}

#[test]
fn queued_values_keep_delayed_history_and_same_time_fifo_through_reenable() {
    let (mut p, c, mut a, start, key) = setup(500.0, true);
    let project = Arc::make_mut(&mut p.project);
    project.outputs[0].delay_ms = 300.0;
    let mut second = Track::new_output(TrackId(202), "early", "127.0.0.1", 9998);
    second.delay_ms = -600.0;
    project.outputs.push(second);
    project.tracks[0].route = Route::Outputs(vec![TrackId(201), TrackId(202)]);
    a.sync(&p, &c, start);
    assert_eq!(p.project.send_delay_ms(TrackId(1), TrackId(201)), 800.0);
    assert_eq!(p.project.send_delay_ms(TrackId(1), TrackId(202)), -100.0);
    let ms = |n| start + Duration::from_millis(n);
    let mut floor = EffectiveFloor::default();
    // A half-second queue backlog, same-time writes, and a reversed producer stamp.
    for (posted, value) in [(100, 0.2), (200, 0.4), (200, 0.6), (150, 0.7), (500, 0.9)] {
        update(&mut p, &mut a, &mut floor, &key, value, ms(posted), ms(600));
    }
    // Every positive-delay sample below is still pending at dequeue (600 ms).
    for (musical, expected) in [
        (50, 0.1),
        (100, 0.2),
        (199, 0.2),
        (200, 0.7),
        (499, 0.7),
        (500, 0.9),
    ] {
        assert!(
            (value(&a, &key.parameter, ms(musical + 800), 800.0) - expected).abs() < 1e-6,
            "musical {musical}"
        );
    }
    assert_eq!(a.values(ms(600))[0].state, AutomationState::Overridden);
    assert!((value(&a, &key.parameter, ms(600), -100.0) - 0.9).abs() < 1e-6);
    // Re-enable remains D-based and becomes the floor for an older queued manual edit.
    let reenable = SchedulerCommand::ReEnableAutomation(Some(key.clone())).enqueued(ms(50));
    let effective = floor.advance(&reenable, ms(700));
    assert_eq!(effective, ms(700));
    a.sync(&p, &c, effective);
    a.control(effective, &[], Some(&Some(key.clone())));
    assert_eq!(a.values(ms(700))[0].state, AutomationState::Following);
    update(&mut p, &mut a, &mut floor, &key, 0.3, ms(350), ms(750));
    assert!((value(&a, &key.parameter, ms(1499), 800.0) - 0.9).abs() < 1e-6);
    assert!((value(&a, &key.parameter, ms(1500), 800.0) - 0.3).abs() < 1e-6);
    assert!((value(&a, &key.parameter, ms(750), -100.0) - 0.3).abs() < 1e-6);
    assert_eq!(a.values(ms(750))[0].state, AutomationState::Overridden);
    // A same-time re-enable wins FIFO, with older history intact.
    a.sync(&p, &c, ms(700));
    a.control(ms(700), &[], Some(&Some(key.clone())));
    assert_eq!(a.values(ms(700))[0].state, AutomationState::Following);
    assert!((value(&a, &key.parameter, ms(1500), 800.0) - 0.4).abs() < 1e-6);
    assert!((value(&a, &key.parameter, ms(1499), 800.0) - 0.9).abs() < 1e-6);
}

#[test]
fn queued_value_before_autonomous_follow_matches_chronological_delayed_samples() {
    let (mut p, mut c, mut queued, start, key) = setup(400.0, true);
    let project = Arc::make_mut(&mut p.project);
    project.follow_actions_enabled = true;
    project.scenes[0].follow_actions.actions.enabled = true;
    project.scenes[0].follow_actions.actions.action_a = FollowAction::Next;
    project.scenes[0].follow_actions.time = BeatTime(960);
    let mut incoming = project.tracks[0].slots[0].clip.as_ref().unwrap().lanes[0].clone();
    let Lane::Envelope(envelope) = &mut incoming else {
        panic!("envelope")
    };
    for point in &mut envelope.points {
        point.value = 0.3;
    }
    project.tracks[0].slots[1]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .push(incoming);
    p.launch_scene(SceneId(1), &mut c, start).unwrap();
    queued.sync(&p, &c, start);
    let mut chronological = queued.clone();
    let mut reference = p.clone();
    let ms = |n| start + Duration::from_millis(n);
    let mut queued_floor = EffectiveFloor::default();
    let mut reference_floor = EffectiveFloor::default();
    update(
        &mut reference,
        &mut chronological,
        &mut reference_floor,
        &key,
        0.8,
        ms(250),
        ms(250),
    );
    // The worker may consume a follow and publish its transition while a command waits.
    for (planner, automation) in [(&mut p, &mut queued), (&mut reference, &mut chronological)] {
        let event = planner.scene_event(&c, ms(500)).unwrap();
        assert_eq!(event.at, ms(500));
        planner.consume(&event);
        assert_eq!(planner.last_triggered_scene, Some(SceneId(2)));
        automation.sync(planner, &c, event.at);
    }
    update(
        &mut p,
        &mut queued,
        &mut queued_floor,
        &key,
        0.8,
        ms(250),
        ms(600),
    );
    // Still-pending positive-delay samples straddle the override and follow boundary.
    for send_ms in [601, 649, 650, 700, 899, 900, 950, 1100] {
        assert_eq!(
            queued.sample(&key.parameter, ms(send_ms), 400.0),
            chronological.sample(&key.parameter, ms(send_ms), 400.0),
            "send {send_ms}"
        );
    }
    assert!((value(&queued, &key.parameter, ms(850), 400.0) - 0.8).abs() < 1e-6);
    assert!((value(&queued, &key.parameter, ms(950), 400.0) - 0.3).abs() < 1e-6);
    for delay in [-100.0, 0.0] {
        assert_eq!(
            queued.sample(&key.parameter, ms(601), delay),
            chronological.sample(&key.parameter, ms(601), delay)
        );
    }
}

#[test]
fn admitted_launch_keeps_preceding_values_and_delayed_tails_on_both_outputs() {
    let (mut p, mut c, mut a, start, key) = setup(500.0, true);
    let project = Arc::make_mut(&mut p.project);
    project.outputs[0].delay_ms = 300.0;
    let mut output = Track::new_output(TrackId(202), "early", "127.0.0.1", 9998);
    output.delay_ms = -600.0;
    project.outputs.push(output);
    project.tracks[0].route = Route::Outputs(vec![TrackId(201), TrackId(202)]);
    let mut incoming = project.tracks[0].slots[0].clip.as_ref().unwrap().lanes[0].clone();
    let Lane::Envelope(envelope) = &mut incoming else {
        panic!("envelope")
    };
    for point in &mut envelope.points {
        point.value = 0.3;
    }
    project.tracks[0].slots[1]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .push(incoming);
    a.sync(&p, &c, start);
    let ms = |n| start + Duration::from_millis(n);
    let mut floor = EffectiveFloor::default();
    update(&mut p, &mut a, &mut floor, &key, 0.8, ms(1998), ms(2001));
    update(&mut p, &mut a, &mut floor, &key, 0.7, ms(1999), ms(2001));
    let mut command = SchedulerCommand::Launch(TrackId(1), 1).enqueued(ms(1998));
    floor.admit(&mut command, ms(2001), &p, &c);
    let effective = floor.advance(&command, ms(2001));
    assert_eq!(effective, ms(1999));
    assert_eq!(
        p.launch(TrackId(1), 1, &mut c, effective).unwrap(),
        BeatTime(3840)
    );
    a.sync(&p, &c, effective);
    for (musical, expected) in [
        (1997, 0.994),
        (1998, 0.8),
        (1999, 0.7),
        (2000, 0.3),
        (2050, 0.3),
    ] {
        assert!(
            (value(&a, &key.parameter, ms(musical + 800), 800.0) - expected).abs() < 1e-5,
            "musical {musical}"
        );
    }
    assert!((value(&a, &key.parameter, ms(2001), -100.0) - 0.3).abs() < 1e-6);
    // Same-time edits still follow channel FIFO after launch, and re-enable
    // stays at D. Neither can erase the positive-delay pre-boundary samples.
    update(&mut p, &mut a, &mut floor, &key, 0.6, effective, ms(2001));
    assert!((value(&a, &key.parameter, ms(2798), 800.0) - 0.8).abs() < 1e-6);
    let reenable = SchedulerCommand::ReEnableAutomation(Some(key.clone())).enqueued(ms(1999));
    let effective = floor.advance(&reenable, ms(2001));
    a.sync(&p, &c, effective);
    a.control(effective, &[], Some(&Some(key.clone())));
    assert!((value(&a, &key.parameter, ms(2801), 800.0) - 0.3).abs() < 1e-6);
}
