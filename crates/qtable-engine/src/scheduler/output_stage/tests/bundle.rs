use super::*;

#[test]
fn frames_pack_into_one_immediate_bundle_in_priority_order() {
    let mut p = with_stage(many(3, SendMode::Continuous), |stage| stage.bundle = true);
    let mut rig = Rig::new(&p);
    rig.update(&p, 0);
    volumes(&mut p, 0.5);
    rig.update(&p, 1);
    let packets = rig.datagrams(&p, 20);
    let [OscPacket::Bundle(bundle)] = packets.as_slice() else {
        panic!("expected one bundle, got {packets:?}");
    };
    assert_eq!(bundle.timetag, (0, 1).into());
    let values: Vec<_> = bundle
        .content
        .iter()
        .map(|packet| {
            let OscPacket::Message(message) = packet else {
                panic!("nested bundle");
            };
            value(message)
        })
        .collect();
    assert_eq!(values, [0.51, 0.52, 0.53]);
    let counts = rig.stages.counts(TrackId(100)).unwrap();
    assert_eq!((counts.messages, counts.packets), (3, 1));
}

#[test]
fn events_due_at_one_instant_leave_as_one_bundle_and_never_wait_longer() {
    let p = with_stage(many(1, SendMode::Continuous), |stage| stage.bundle = true);
    let mut rig = Rig::new(&p);
    rig.update(&p, 0);
    let (tx, _rx) = mpsc::sync_channel(100);
    let mut tx = EventSink::new(tx);
    let at = ms(rig.anchor, 10);
    send_event(&mut rig, &p, 1.0, 10);
    send_event(&mut rig, &p, 2.0, 10);
    // The next event is due at the same instant: the pair waits for it.
    rig.stages
        .flush_events(&p, &mut rig.sender, &mut tx, Some(at));
    assert!(rig.read().is_empty());
    rig.stages.flush_events(&p, &mut rig.sender, &mut tx, None);
    let packets = rig.read();
    let [OscPacket::Bundle(bundle)] = packets.as_slice() else {
        panic!("expected one bundle, got {packets:?}");
    };
    assert_eq!(bundle.content.len(), 2);
    // A later instant flushes the earlier one as the new event arrives.
    send_event(&mut rig, &p, 3.0, 20);
    send_event(&mut rig, &p, 4.0, 30);
    assert_eq!(rig.read().len(), 1);
    rig.stages.flush_events(&p, &mut rig.sender, &mut tx, None);
    assert_eq!(rig.read().len(), 1);
    let counts = rig.stages.counts(TrackId(100)).unwrap();
    assert_eq!((counts.messages, counts.packets), (4, 3));
}

#[test]
fn held_bundled_events_flush_before_bundle_is_turned_off() {
    held_events_before_update(false);
}

#[test]
fn held_bundled_events_flush_before_the_output_is_removed() {
    held_events_before_update(true);
}

fn held_events_before_update(remove: bool) {
    use crate::scheduler::SchedulerCommand;
    use crate::scheduler::commands::Hold;

    let p = with_stage(many(1, SendMode::Continuous), |stage| stage.bundle = true);
    let mut rig = Rig::new(&p);
    let at = ms(rig.anchor, 10);
    let arrived = at - Duration::from_micros(500);
    let mut edited = p.clone();
    if remove {
        edited.outputs.clear();
        edited.tracks[0].route = Route::empty();
    } else {
        edited.outputs[0].output.as_mut().unwrap().stage.bundle = false;
    }
    let (tx, _rx) = mpsc::sync_channel(100);
    let mut tx = EventSink::new(tx);
    let mut hold = Hold::default();
    send_event(&mut rig, &p, 1.0, 10);
    // As in the worker: keep the bundle while another event at its instant is pending,
    // and hold an UpdateProject that could otherwise discard it or change its packing.
    rig.stages
        .flush_events(&p, &mut rig.sender, &mut tx, Some(at));
    assert!(matches!(
        hold.next(
            Some(at),
            |_| false,
            || Ok(Some((
                SchedulerCommand::UpdateProject(edited).enqueued(arrived),
                arrived
            )))
        ),
        Ok(None)
    ));
    assert!(rig.read().is_empty());
    send_event(&mut rig, &p, 2.0, 10);
    // The next worker pass flushes before releasing and applying the held command.
    rig.stages.flush_events(&p, &mut rig.sender, &mut tx, None);
    let packets = rig.read();
    let [OscPacket::Bundle(bundle)] = packets.as_slice() else {
        panic!("expected one bundle, got {packets:?}");
    };
    assert_eq!(bundle.timetag, (0, 1).into());
    let values: Vec<_> = bundle
        .content
        .iter()
        .map(|packet| {
            let OscPacket::Message(message) = packet else {
                panic!("nested bundle")
            };
            value(message)
        })
        .collect();
    assert_eq!(values, [1.0, 2.0]);
    let counts = rig.stages.counts(TrackId(100)).unwrap();
    assert_eq!((counts.messages, counts.packets), (2, 1));
    let Ok(Some((
        crate::scheduler::Envelope {
            command: SchedulerCommand::UpdateProject(edited),
            ..
        },
        stamp,
    ))) = hold.next(None, |_| false, || unreachable!())
    else {
        panic!("update should be released")
    };
    assert_eq!(stamp, arrived);
    rig.stages.update(&edited, controls(&edited), stamp);
    rig.stages
        .flush_events(&edited, &mut rig.sender, &mut tx, None);
    assert!(rig.read().is_empty());
    if remove {
        assert!(
            rig.stages
                .stages
                .iter()
                .all(|stage| stage.output != TrackId(100))
        );
    } else {
        send_event(&mut rig, &edited, 3.0, 20);
        assert!(
            matches!(rig.read().as_slice(), [OscPacket::Message(message)] if value(message) == 3.0)
        );
    }
}

#[test]
fn resumable_frames_wait_for_send_next_and_keep_their_packing() {
    let sent = |resumable: bool| {
        let mut p = with_stage(many(200, SendMode::Continuous), |stage| stage.bundle = true);
        let mut rig = Rig::new(&p);
        rig.stages.resumable = resumable;
        rig.update(&p, 0);
        // Volumes 0.01-0.99 change from id/10; the rest clamp to 1.0 before and after.
        volumes(&mut p, 0.0);
        rig.update(&p, 1);
        let mut packets = rig.datagrams(&p, 20);
        if resumable {
            assert!(packets.is_empty(), "a resumable frame waits for send_next");
            let (tx, _rx) = mpsc::sync_channel(100);
            let mut tx = EventSink::new(tx);
            while rig.stages.sending() {
                rig.stages
                    .send_next(&p, &mut rig.sender, &rig.activity, &mut tx);
            }
            packets = rig.read();
        }
        (packets, rig.stages.counts(TrackId(100)).unwrap())
    };
    let (whole, whole_counts) = sent(false);
    let (resumed, resumed_counts) = sent(true);
    assert!(
        whole.len() > 1,
        "about 100 changed volumes need more than one bundle"
    );
    assert_eq!(whole, resumed);
    assert_eq!(whole_counts, resumed_counts);
}
