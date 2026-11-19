use super::*;

#[test]
fn a_new_set_owes_nothing_and_unrelated_edits_send_nothing() {
    let mut p = project(SendMode::Continuous, 0.0);
    let mut rig = Rig::new(&p);
    assert_eq!(rig.deadline(&p), None);
    p.tracks[0].name = "Renamed".into();
    rig.update(&p, 5);
    assert_eq!(rig.deadline(&p), None);
    let mut on_release = p.clone();
    on_release.tracks[0].mixer_device.volume.output.send_mode = SendMode::OnRelease;
    on_release.tracks[0].mixer_device.volume.output.threshold = 0.5;
    rig.update(&on_release, 6);
    assert_eq!(rig.deadline(&on_release), None);
}

#[test]
fn edits_inside_one_frame_merge_and_wait_for_the_frame() {
    let mut p = at_50_hz(project(SendMode::Continuous, 0.0));
    let mut rig = Rig::new(&p);
    for (index, value) in [0.2, 0.4, 0.7].into_iter().enumerate() {
        set_volume(&mut p, value);
        rig.update(&p, 3 + index as u64);
    }
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 20)));
    assert!(rig.values(&p, 19).is_empty());
    assert_eq!(rig.values(&p, 20), vec![0.7]);
    assert_eq!(rig.deadline(&p), None);
    let counts = rig.stages.counts(TrackId(2)).unwrap();
    assert_eq!((counts.messages, counts.merged), (1, 2));
    assert_eq!(rig.activity.active_tracks(ms(rig.anchor, 20)), [TrackId(1)]);
}

#[test]
fn disabling_or_rerouting_cancels_and_enabling_sends_the_current_value() {
    let mut p = at_50_hz(project(SendMode::Continuous, 0.0));
    let mut rig = Rig::new(&p);
    set_volume(&mut p, 0.4);
    rig.update(&p, 1);
    let mut disabled = p.clone();
    disabled.tracks[0].mixer_device.volume.output.enabled = false;
    rig.update(&disabled, 2);
    assert_eq!(rig.deadline(&disabled), None);
    rig.update(&p, 3);
    assert_eq!(rig.values(&p, 20), vec![0.4]);
    let mut rerouted = p.clone();
    rerouted.tracks[0].route = Route::empty();
    set_volume(&mut rerouted, 0.5);
    rig.update(&rerouted, 21);
    assert_eq!(rig.deadline(&rerouted), None);
}

#[test]
fn an_unresolved_output_waits_then_sends_only_the_latest_value() {
    let mut p = at_50_hz(project(SendMode::Continuous, 0.0));
    p.outputs[0].destination_mut().unwrap().host = "pending.invalid".into();
    let mut rig = Rig::new(&p);
    rig.sender = OscSender::new().unwrap();
    for edit in 1..=10_000 {
        set_volume(&mut p, edit as f64 / 20_000.0);
        rig.update(&p, edit);
    }
    let stage = rig
        .stages
        .stages
        .iter()
        .find(|stage| stage.output == TrackId(2))
        .unwrap();
    assert_eq!(stage.slots[0].1.history_len(), 1);
    assert_eq!(rig.deadline(&p), None);
    assert!(rig.values(&p, 10_020).is_empty());
    rig.sender
        .insert_resolved(&p.outputs[0], rig.receiver.local_addr().unwrap());
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 10_000)));
    // The late frame goes once, as the latest tick.
    assert_eq!(rig.values(&p, 10_035), vec![0.5]);
    assert_eq!(rig.deadline(&p), None);
    let counts = rig.stages.counts(TrackId(2)).unwrap();
    assert_eq!((counts.messages, counts.merged), (1, 9_999));
}

#[test]
fn delay_shifts_eligibility_and_negative_delay_sends_on_the_next_frame() {
    let mut p = at_50_hz(project(SendMode::Continuous, 30.0));
    p.outputs[0].delay_ms = 10.0;
    p.main.delay_ms = 5.0;
    let mut rig = Rig::new(&p);
    set_volume(&mut p, 0.4);
    rig.update(&p, 5);
    // Eligible at 50 ms; the next frame is at 60 ms.
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 60)));
    assert!(rig.values(&p, 40).is_empty());
    assert_eq!(rig.values(&p, 60), vec![0.4]);
    p.tracks[0].delay_ms = -100.0;
    set_volume(&mut p, 0.5);
    rig.update(&p, 61);
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 80)));
}

#[test]
fn transport_start_resends_every_value_at_once_and_stop_sends_nothing() {
    let p = at_50_hz(project(SendMode::Continuous, 0.0));
    let mut rig = Rig::new(&p);
    rig.stages.start(ms(rig.anchor, 7));
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 7)));
    assert_eq!(rig.values(&p, 7), vec![1.0]);
    assert_eq!(rig.deadline(&p), None);
    rig.stages.stop(ms(rig.anchor, 30));
    assert_eq!(rig.deadline(&p), None);
    // With a positive delay, Play then a quick Stop still delivers the start value.
    let mut delayed = p.clone();
    delayed.tracks[0].delay_ms = 50.0;
    rig.update(&delayed, 31);
    rig.stages.start(ms(rig.anchor, 40));
    rig.stages.stop(ms(rig.anchor, 41));
    assert_eq!(rig.deadline(&delayed), Some(ms(rig.anchor, 90)));
    assert_eq!(rig.values(&delayed, 90), vec![1.0]);
}

#[test]
fn a_grid_lead_reanchors_the_frame_grid_at_transport_start_and_none_keeps_it() {
    let p = at_50_hz(project(SendMode::EveryFrame, 0.0));
    for (lead, next) in [(None, [20, 40]), (Some(3), [22, 42])] {
        let mut rig = Rig::new(&p);
        rig.stages.grid_lead = lead.map(Duration::from_millis);
        rig.stages.start(ms(rig.anchor, 5));
        assert_eq!(rig.values(&p, 5), vec![1.0]);
        for at in next {
            assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, at)), "lead {lead:?}");
            assert_eq!(rig.values(&p, at), vec![1.0]);
        }
    }
}

#[test]
fn deferred_replan_keeps_the_next_delayed_start_visible() {
    let mut p = at_50_hz(project(SendMode::Continuous, 0.0));
    let mut second = p.tracks[0].clone();
    second.id = TrackId(3);
    second.delay_ms = 50.0;
    p.tracks.push(second);
    let mut rig = Rig::new(&p);
    rig.stages.live_timing = true;
    rig.stages.start(ms(rig.anchor, 40));
    assert_eq!(rig.values(&p, 40), vec![1.0]);
    // The full plan waits for adjacent clip hits, but the next off-grid start
    // must remain a deadline during that wait.
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 90)));
    assert_eq!(rig.values(&p, 90), vec![1.0]);
}

#[test]
fn a_frame_lets_clip_events_due_within_a_millisecond_go_first_but_start_sends_do_not() {
    let mut p = at_50_hz(project(SendMode::Continuous, 0.0));
    let mut rig = Rig::new(&p);
    set_volume(&mut p, 0.4);
    rig.update(&p, 3);
    let micros = |rig: &Rig, us: u64| Some(rig.anchor + Duration::from_micros(us));
    // A clip event due 0.5 ms after the tick goes first; the frame stays owed.
    assert!(rig.values_before(&p, 20, micros(&rig, 20_500)).is_empty());
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 20)));
    // One due more than a millisecond after the tick doesn't hold the frame.
    assert_eq!(rig.values_before(&p, 20, micros(&rig, 21_100)), vec![0.4]);
    // Transport start values still go ahead of a clip event at the same instant.
    rig.stages.start(ms(rig.anchor, 47));
    assert_eq!(rig.values_before(&p, 47, micros(&rig, 47_000)), vec![0.4]);
}

#[test]
fn a_destination_unset_since_the_edit_drops_its_slot() {
    let mut p = project(SendMode::Continuous, 0.0);
    p.tracks[0].route = Route::single(p.main.id);
    let mut rig = Rig::new(&p);
    rig.stages.start(rig.anchor);
    p.main.output = None;
    rig.update(&p, 1);
    assert_eq!(rig.deadline(&p), None);
    assert!(rig.stages.stages.iter().all(|s| s.output != p.main.id));
}

#[test]
fn a_moved_receiver_or_address_gets_the_current_value_again() {
    let mut p = at_50_hz(project(SendMode::Continuous, 0.0));
    let mut rig = Rig::new(&p);
    set_volume(&mut p, 0.3);
    rig.update(&p, 1);
    assert_eq!(rig.values(&p, 20), vec![0.3]);
    p.outputs[0].destination_mut().unwrap().port = 9001;
    rig.sender
        .insert_resolved(&p.outputs[0], rig.receiver.local_addr().unwrap());
    rig.update(&p, 21);
    assert_eq!(rig.values(&p, 40), vec![0.3]);
    p.tracks[0].mixer_device.volume.output.address = "/level".into();
    rig.update(&p, 41);
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 60)));
}

#[test]
fn every_frame_sends_each_frame_of_the_output_and_late_frames_skip_ahead() {
    let p = at_50_hz(project(SendMode::EveryFrame, 0.0));
    let mut rig = Rig::new(&p);
    rig.stages.start(ms(rig.anchor, 5));
    assert_eq!(rig.values(&p, 5), vec![1.0]);
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 20)));
    assert_eq!(rig.values(&p, 21), vec![1.0]);
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 40)));
    // A stall misses three frames; one goes, and the grid keeps its phase.
    assert_eq!(rig.values(&p, 105), vec![1.0]);
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 120)));
    // The output's Frame Rate sets the rate; the stored v3.0 rate no longer does.
    let mut slower = p.clone();
    slower.outputs[0].output.as_mut().unwrap().stage.frame_rate = 10.0;
    slower.tracks[0].mixer_device.volume.output.stream_rate = 60.0;
    rig.update(&slower, 106);
    assert_eq!(rig.deadline(&slower), Some(ms(rig.anchor, 200)));
    rig.stages.stop(ms(rig.anchor, 150));
    assert_eq!(rig.deadline(&slower), None);
}

#[test]
fn every_frame_keeps_each_runs_delayed_tail_through_edits_and_restarts() {
    let p = at_50_hz(project(SendMode::EveryFrame, 200.0));
    let mut rig = Rig::new(&p);
    rig.stages.start(rig.anchor);
    rig.stages.stop(ms(rig.anchor, 55));
    let mut edited = p.clone();
    set_volume(&mut edited, 0.4);
    rig.update(&edited, 60);
    // A second run, its own value edited mid-run.
    rig.stages.start(ms(rig.anchor, 70));
    set_volume(&mut edited, 0.7);
    rig.update(&edited, 85);
    rig.stages.stop(ms(rig.anchor, 95));
    let mut sent = vec![];
    for at in (200..=320).step_by(20) {
        sent.push((at, rig.values(&edited, at)));
    }
    assert_eq!(
        sent,
        [
            (200, vec![1.0]),
            (220, vec![1.0]),
            (240, vec![1.0]),
            // 260 is musical 60: between the runs. 0.4 was edited at 60 and goes once as a
            // change; the second run starts 0.4 again at 270.
            (260, vec![0.4]),
            (280, vec![0.4]),
            (300, vec![0.7]),
            (320, vec![]),
        ]
    );
    assert_eq!(rig.deadline(&edited), None);
    assert!(rig.stages.runs.is_empty());
}

#[test]
fn clip_events_go_at_once_and_count_against_their_output() {
    let p = project(SendMode::Continuous, 0.0);
    let mut rig = Rig::new(&p);
    let packet = OscPacket::Message(OscMessage {
        addr: "/volume".into(),
        args: vec![OscType::Float(0.25)],
    });
    let (tx, _rx) = mpsc::sync_channel(100);
    let now = ms(rig.anchor, 0);
    let bytes = rig
        .stages
        .event(
            &mut rig.sender,
            &mut EventSink::new(tx),
            &p.outputs[0],
            &packet,
            now,
            now,
        )
        .unwrap();
    assert_eq!(rig.values(&p, 0), vec![0.25]);
    assert_eq!(
        rig.stages.counts(TrackId(2)),
        Some(Counts {
            messages: 1,
            bytes: bytes as u64,
            packets: 1,
            ..Counts::default()
        })
    );
}

#[test]
fn frame_rate_changes_during_play_keep_the_anchor_and_replace_the_bucket() {
    let mut p = with_stage(many(5, SendMode::EveryFrame), |stage| {
        stage.budget_on = true;
        stage.budget = 50;
    });
    let mut rig = Rig::new(&p);
    rig.stages.start(ms(rig.anchor, 5));
    assert_eq!(rig.values(&p, 5).len(), 5);
    // Start sends exhaust the two-token bucket and reach its debt floor.
    assert!(rig.values(&p, 20).is_empty());
    p.outputs[0].output.as_mut().unwrap().stage.frame_rate = 25.0;
    rig.update(&p, 31);
    assert!(rig.read().is_empty());
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 40)));
    assert!(rig.values(&p, 39).is_empty());
    // The new bucket starts full, with four tokens rather than the old bucket's debt.
    assert_eq!(rig.values(&p, 40).len(), 4);
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 80)));
    assert!(rig.values(&p, 60).is_empty());
    assert_eq!(rig.values(&p, 80).len(), 2);
    p.outputs[0].output.as_mut().unwrap().stage.frame_rate = 50.0;
    rig.update(&p, 91);
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 100)));
    assert_eq!(rig.values(&p, 100).len(), 2);
    rig.stages.stop(ms(rig.anchor, 101));
    assert_eq!(rig.deadline(&p), None);
}

#[test]
fn threshold_settles_on_a_frame_after_a_full_eligible_period_without_an_edit() {
    let mut p = at_50_hz(project(SendMode::Continuous, 20.0));
    p.tracks[0].mixer_device.volume.output.threshold = 0.5;
    let mut rig = Rig::new(&p);
    rig.stages.start(rig.anchor);
    assert_eq!(rig.values(&p, 20), vec![1.0]);
    set_volume(&mut p, 0.9);
    rig.update(&p, 21);
    // Eligible at 41 ms: the 60 ms frame is still short of a full eligible period.
    assert!(rig.values(&p, 40).is_empty());
    assert!(rig.values(&p, 60).is_empty());
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 80)));
    set_volume(&mut p, 0.8);
    rig.update(&p, 61);
    // The newer edit is still delayed at 80 ms, so the earlier value settles first.
    assert_eq!(rig.values(&p, 80), vec![0.9]);
    assert!(rig.values(&p, 100).is_empty());
    assert_eq!(rig.deadline(&p), Some(ms(rig.anchor, 120)));
    assert_eq!(rig.values(&p, 120), vec![0.8]);
    assert_eq!(rig.deadline(&p), None);
    assert_eq!(rig.stages.counts(TrackId(2)).unwrap().messages, 3);
}
