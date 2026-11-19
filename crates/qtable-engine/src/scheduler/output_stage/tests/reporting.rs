use super::*;

#[test]
fn frame_diagnostics_are_per_frame_and_stalls_count_the_original_due_time() {
    let p = at_50_hz(project(SendMode::EveryFrame, 0.0));
    let mut rig = Rig::new(&p);
    rig.stages.start(ms(rig.anchor, 5));
    let (tx, rx) = mpsc::sync_channel(100);
    let mut sink = EventSink::new(tx);
    rig.stages.diagnostics = true;
    for at in [5, 21, 105] {
        rig.stages.dispatch(
            &p,
            &mut rig.sender,
            &rig.activity,
            &mut sink,
            &clear(ms(rig.anchor, at), None),
        );
    }
    let events: Vec<_> = rx.try_iter().collect();
    assert_eq!(events.len(), 3, "{events:?}");
    for (event, (due_ms, late_ms)) in events.iter().zip([(5, 0), (20, 1), (40, 65)]) {
        let SchedulerEvent::FrameSent {
            output,
            due,
            lateness,
            entry_lateness,
            messages,
            profile,
        } = event
        else {
            panic!("{event:?}")
        };
        assert_eq!(*output, TrackId(2));
        assert_eq!(*due, ms(rig.anchor, due_ms));
        assert_eq!(*lateness, Duration::from_millis(late_ms));
        assert_eq!(*entry_lateness, Duration::from_millis(late_ms));
        assert_eq!(*messages, 1);
        // The rig's clock is supplied, so only the counts are deterministic.
        assert_eq!(profile.send.packets, 1);
        assert_eq!((profile.slots, profile.considered), (1, 1));
        assert_eq!(profile.jump, None);
    }
    assert_eq!(rig.stages.counts(TrackId(2)).unwrap().frames_late, 1);
    // Diagnostics off still sends and counts, but emits no timing events.
    rig.stages.diagnostics = false;
    rig.stages.dispatch(
        &p,
        &mut rig.sender,
        &rig.activity,
        &mut sink,
        &clear(ms(rig.anchor, 120), None),
    );
    assert!(rx.try_recv().is_err());
}

#[test]
fn oversize_events_count_and_warn_once_per_output_across_project_updates() {
    let mut p = project(SendMode::Continuous, 0.0);
    p.outputs[0].output.as_mut().unwrap().stage.bundle = true;
    let mut rig = Rig::new(&p);
    let shared = throughput::Published::default();
    let (tx, rx) = mpsc::sync_channel(100);
    let mut sink = EventSink::new(tx);
    let packet = OscPacket::Message(OscMessage {
        addr: "/large".into(),
        args: vec![OscType::String("x".repeat(1500))],
    });
    for at in [1, 2] {
        rig.stages
            .event(
                &mut rig.sender,
                &mut sink,
                &p.outputs[0],
                &packet,
                ms(rig.anchor, at),
                ms(rig.anchor, at),
            )
            .unwrap();
        rig.stages
            .flush_events(&p, &mut rig.sender, &mut sink, None);
        rig.stages
            .publish(&shared, &p, &mut sink, ms(rig.anchor, at));
        rig.update(&p, at);
    }
    let events: Vec<_> = rx.try_iter().collect();
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(
        matches!(&events[0], SchedulerEvent::OutputOversize { output: TrackId(2), name } if name == "Out")
    );
    let value = shared
        .read(ms(rig.anchor, 2))
        .into_iter()
        .find(|(id, _)| *id == TrackId(2))
        .unwrap()
        .1;
    assert_eq!(value.messages, 2);
    assert_eq!(value.packets, 2);
    assert_eq!(value.oversize, 2);
    assert_eq!(
        value.bytes,
        2 * rosc::encoder::encode(&packet).unwrap().len() as u64
    );
    let mut status = crate::status::StatusLog::default();
    for event in events {
        status.record(event);
    }
    let status = status.status(None);
    assert_eq!(status.warning_count, 1);
    assert_eq!(status.error_count, 0);
    assert!(status.last_warning.unwrap().contains("Out"));
}

#[test]
fn deferring_tracks_waiting_changes_and_clears_when_their_slots_are_removed() {
    let mut p = many(4, SendMode::Continuous);
    let stage = &mut p.outputs[0].output.as_mut().unwrap().stage;
    stage.budget_on = true;
    stage.budget = 10;
    let mut rig = Rig::new(&p);
    for track in &mut p.tracks {
        track.mixer_device.volume.value = 0.7;
    }
    rig.update(&p, 1);
    assert_eq!(rig.values(&p, 20).len(), 1);
    let shared = throughput::Published::default();
    let (tx, _rx) = mpsc::sync_channel(100);
    let mut sink = EventSink::new(tx);
    rig.stages
        .publish(&shared, &p, &mut sink, ms(rig.anchor, 20));
    let value = shared
        .read(ms(rig.anchor, 20))
        .into_iter()
        .find(|(id, _)| *id == TrackId(100))
        .unwrap()
        .1;
    assert_eq!(value.deferred, 3);
    assert!(value.deferring);
    for track in &mut p.tracks {
        track.mixer_device.volume.output.enabled = false;
    }
    rig.update(&p, 21);
    rig.stages
        .publish(&shared, &p, &mut sink, ms(rig.anchor, 21));
    let value = shared
        .read(ms(rig.anchor, 21))
        .into_iter()
        .find(|(id, _)| *id == TrackId(100))
        .unwrap()
        .1;
    assert_eq!(value.deferred, 3);
    assert!(!value.deferring);
}

#[test]
fn reverting_a_deferred_change_to_the_sent_value_clears_deferring() {
    let mut p = with_stage(many(4, SendMode::Continuous), |stage| {
        stage.budget_on = true;
        stage.budget = 10;
    });
    let mut rig = Rig::new(&p);
    rig.stages.start(rig.anchor);
    assert_eq!(rig.values(&p, 0).len(), 4);
    let original: Vec<_> = p
        .tracks
        .iter()
        .map(|track| track.mixer_device.volume.value)
        .collect();
    volumes(&mut p, 0.5);
    rig.update(&p, 1);
    assert!(rig.values(&p, 20).is_empty());
    assert!(
        rig.stages
            .stages
            .iter()
            .find(|stage| stage.output == TrackId(100))
            .unwrap()
            .deferring
    );
    for (track, value) in p.tracks.iter_mut().zip(original) {
        track.mixer_device.volume.value = value;
    }
    rig.update(&p, 21);
    assert!(rig.values(&p, 40).is_empty());
    assert!(
        !rig.stages
            .stages
            .iter()
            .find(|stage| stage.output == TrackId(100))
            .unwrap()
            .deferring
    );
}
