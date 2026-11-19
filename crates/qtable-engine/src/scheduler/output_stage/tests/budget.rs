use super::*;

#[test]
fn a_budget_defers_changes_in_turn_until_every_value_has_arrived() {
    // 10 messages a second at 50 Hz: a bucket of one message, refilled every 100 ms.
    let mut p = with_stage(many(4, SendMode::Continuous), |stage| {
        stage.budget_on = true;
        stage.budget = 10;
    });
    let mut rig = Rig::new(&p);
    rig.update(&p, 0);
    volumes(&mut p, 0.5);
    rig.update(&p, 1);
    let mut arrived = vec![];
    for at in (20..=500).step_by(20) {
        arrived.push((at, rig.values(&p, at)));
    }
    let sent: Vec<_> = arrived.iter().filter(|(_, v)| !v.is_empty()).collect();
    // One message per 100 ms, each parameter once, none starved.
    assert_eq!(sent.len(), 4, "{arrived:?}");
    assert!(sent.windows(2).all(|w| w[1].0 - w[0].0 >= 100));
    let mut values: Vec<_> = sent.iter().map(|(_, v)| v[0]).collect();
    values.sort_by(f32::total_cmp);
    assert_eq!(values, [0.51, 0.52, 0.53, 0.54]);
    let counts = rig.stages.counts(TrackId(100)).unwrap();
    assert!(counts.deferred > 0);
    assert_eq!(counts.messages, 4);
    // Nothing owed once the last change went out.
    assert_eq!(rig.deadline(&p), None);
}

#[test]
fn events_take_tokens_without_waiting_and_parameters_wait_for_the_debt() {
    // 100 messages a second at 50 Hz: a bucket of four, two tokens per frame.
    let mut p = with_stage(many(1, SendMode::Continuous), |stage| {
        stage.budget_on = true;
        stage.budget = 100;
    });
    let mut rig = Rig::new(&p);
    rig.update(&p, 0);
    for index in 0..6 {
        send_event(&mut rig, &p, index as f32, 0);
    }
    // Every event went out at once: six tokens from a bucket of four.
    assert_eq!(rig.read().len(), 6);
    volumes(&mut p, 0.5);
    rig.update(&p, 1);
    assert!(rig.values(&p, 20).is_empty());
    assert_eq!(rig.values(&p, 40), vec![0.51]);
    let counts = rig.stages.counts(TrackId(100)).unwrap();
    assert_eq!((counts.messages, counts.deferred), (7, 1));
}

#[test]
fn transport_start_sends_are_never_deferred_but_take_tokens() {
    let mut p = with_stage(many(3, SendMode::Continuous), |stage| {
        stage.budget_on = true;
        stage.budget = 10;
    });
    let mut rig = Rig::new(&p);
    rig.update(&p, 0);
    rig.stages.start(ms(rig.anchor, 5));
    let mut started = rig.values(&p, 5);
    started.sort_by(f32::total_cmp);
    assert_eq!(started, [0.1, 0.2, 0.3]);
    // The bucket holds one token and debt is capped at one: a change waits for recovery.
    volumes(&mut p, 0.5);
    rig.update(&p, 6);
    assert!(rig.values(&p, 20).is_empty());
    assert!(rig.values(&p, 200).is_empty());
    assert_eq!(rig.values(&p, 220).len(), 1);
}

#[test]
fn every_frame_repeats_are_skipped_under_a_budget_and_changes_win() {
    let p = with_stage(many(3, SendMode::EveryFrame), |stage| {
        stage.budget_on = true;
        stage.budget = 50;
    });
    let mut rig = Rig::new(&p);
    rig.update(&p, 0);
    rig.stages.start(ms(rig.anchor, 0));
    // Three start sends take the bucket of two into debt.
    assert_eq!(rig.values(&p, 0).len(), 3);
    // One token per frame afterwards: the repeats of three streams share it.
    let mut total = 0;
    for at in (20..=200).step_by(20) {
        total += rig.values(&p, at).len();
    }
    assert!((9..=10).contains(&total), "{total}");
    assert!(rig.stages.counts(TrackId(100)).unwrap().skipped > 0);
}

#[test]
fn refresh_resends_every_value_round_robin_while_stopped() {
    let p = with_stage(many(2, SendMode::Continuous), |stage| {
        stage.refresh_on = true;
        stage.refresh = 1.0;
    });
    let mut rig = Rig::new(&p);
    rig.update(&p, 0);
    let mut sent = vec![];
    for at in (0..=2000).step_by(20) {
        for value in rig.values(&p, at) {
            sent.push((at, value));
        }
    }
    assert_eq!(sent, [(500, 0.1), (1000, 0.2), (1500, 0.1), (2000, 0.2)]);
    // Off again: nothing is owed.
    let off = many(2, SendMode::Continuous);
    rig.update(&off, 2001);
    assert_eq!(rig.deadline(&off), None);
}

#[test]
fn refresh_steps_skipped_under_a_saturated_budget_are_not_queued() {
    let p = with_stage(many(5, SendMode::Continuous), |stage| {
        stage.budget_on = true;
        stage.budget = 50;
        stage.refresh_on = true;
        stage.refresh = 0.1;
    });
    let mut rig = Rig::new(&p);
    for index in 0..4 {
        send_event(&mut rig, &p, index as f32, 0);
    }
    assert_eq!(rig.read().len(), 4);
    // One Refresh step per frame; the first two find the bucket still in debt.
    assert!(rig.values(&p, 20).is_empty());
    assert!(rig.values(&p, 40).is_empty());
    // Recovery goes on to the third slot, without retrying the skipped steps.
    for (at, expected) in [(60, 0.3), (80, 0.4), (100, 0.5), (120, 0.1), (140, 0.2)] {
        assert_eq!(rig.values(&p, at), vec![expected]);
    }
    let counts = rig.stages.counts(TrackId(100)).unwrap();
    assert_eq!(
        (counts.messages, counts.skipped, counts.deferred),
        (9, 2, 0)
    );
}
