use super::*;
use crate::model::*;
use AdmissionOutcome::*;

fn setup() -> (Planner, InternalClock, Instant) {
    let start = Instant::now();
    let mut clock = InternalClock::default();
    clock.play(start);
    let p = Planner::new(crate::scheduler::tests::project(0.0, true), Duration::ZERO).unwrap();
    (p, clock, start)
}

fn envelope(command: SchedulerCommand, r: Instant, p: Instant) -> Envelope {
    let mut envelope = command.enqueued(p);
    envelope.submission = Some(crate::engine::Submission {
        context: crate::engine::CommandContext {
            receipt: crate::engine::Receipt {
                received_at: r,
                sequence: Some(17),
            },
            generation: r,
            gesture: None,
        },
        submitted_at: p,
        identity: 9,
        valid_generation: true,
    });
    envelope
}

fn decision(
    floor: &EffectiveFloor,
    mut envelope: Envelope,
    d: Instant,
    p: &Planner,
    c: &InternalClock,
) -> Decision {
    floor.admit(&mut envelope, d, p, c);
    envelope.admitted.unwrap()
}

#[test]
fn receipt_is_strict_and_grace_is_inclusive_at_the_nanosecond() {
    let (p, c, start) = setup();
    let b = start + Duration::from_secs(2);
    let ns = Duration::from_nanos(1);
    let floor = EffectiveFloor::default();
    for (r, d, outcome, beat) in [
        (b - ns, b + GRACE, Admitted, 3840),
        (b - ns, b + GRACE + ns, Expired, 7680),
        (b, b + GRACE, Admitted, 7680),
        (b + ns, b + GRACE, Admitted, 7680),
        (b - ns, b - ns, Admitted, 3840),
    ] {
        let result = decision(
            &floor,
            envelope(SchedulerCommand::Launch(TrackId(1), 1), r, r),
            d,
            &p,
            &c,
        );
        assert_eq!(result.outcome, outcome);
        let mut p = p.clone();
        let mut c = c.clone();
        assert_eq!(
            p.launch(TrackId(1), 1, &mut c, result.effective).unwrap(),
            BeatTime(beat)
        );
    }
}

#[test]
fn short_and_fractional_grids_never_recover_an_older_boundary() {
    let (mut p, mut c, start) = setup();
    for bpm in [120.0, 123.456, 120_000.0] {
        c.set_tempo(Tempo::new(bpm).unwrap(), start);
        for grid in [
            QuantisationValue::Triplet(NoteValue::Sixteenth),
            QuantisationValue::Note(NoteValue::ThirtySecond),
        ] {
            Arc::make_mut(&mut p.project).global_quantisation = GlobalQuantisation::Value(grid);
            let ticks = grid.duration(c.time_signature()).unwrap().0;
            let b = c.instant_at(BeatTime(ticks)).unwrap();
            let next = c.instant_at(BeatTime(2 * ticks)).unwrap();
            let r = b - Duration::from_nanos(1);
            let d = next - Duration::from_nanos(1);
            let expected = if d.duration_since(b) <= GRACE {
                Admitted
            } else {
                Expired
            };
            assert_eq!(
                decision(
                    &EffectiveFloor::default(),
                    envelope(SchedulerCommand::StopAll, r, r),
                    d,
                    &p,
                    &c
                )
                .outcome,
                expected
            );
            let expected = if next.duration_since(b) <= GRACE {
                OlderBoundary
            } else {
                Expired
            };
            assert_eq!(
                decision(
                    &EffectiveFloor::default(),
                    envelope(SchedulerCommand::StopAll, r, r),
                    next,
                    &p,
                    &c
                )
                .outcome,
                expected
            );
        }
    }
}

#[test]
fn fifo_floor_preserves_preboundary_batches_but_blocks_postboundary_batches() {
    let (p, c, start) = setup();
    let b = start + Duration::from_secs(2);
    let r = b - Duration::from_millis(1);
    let d = b + Duration::from_millis(1);
    for (posted, outcome) in [
        (b - Duration::from_micros(100), Admitted),
        (b, FloorConflict),
        (b + Duration::from_micros(100), FloorConflict),
    ] {
        let mut floor = EffectiveFloor::default();
        floor.advance(&super::super::timing_tests::values(posted), d);
        let result = decision(
            &floor,
            envelope(SchedulerCommand::Launch(TrackId(1), 1), r, posted.max(r)),
            d,
            &p,
            &c,
        );
        assert_eq!(result.outcome, outcome);
        if outcome == Admitted {
            assert_eq!(result.effective, posted);
        }
        // Interleaved clients: a later received older stamp cannot undo this command.
        let mut launch = envelope(SchedulerCommand::StopAll, r, d);
        floor.admit(&mut launch, d, &p, &c);
        let effective = floor.advance(&launch, d);
        assert_eq!(
            floor.advance(&super::super::timing_tests::values(r), d),
            effective
        );
    }
}

#[test]
fn invalid_context_epochs_and_direct_sender_fallback_are_explicit() {
    let (p, c, start) = setup();
    let b = start + Duration::from_secs(2);
    let r = b - Duration::from_millis(1);
    let d = b + Duration::from_millis(1);
    let command = SchedulerCommand::StopAll;
    let mut invalid = envelope(command.clone(), r, r);
    invalid.submission.as_mut().unwrap().valid_generation = false;
    let mut after_submission = envelope(command.clone(), r, r);
    after_submission.submission.as_mut().unwrap().submitted_at = r - Duration::from_nanos(1);
    let mut after_enqueue = envelope(command.clone(), r, r);
    after_enqueue.submission.as_mut().unwrap().submitted_at = d;
    for invalid in [
        invalid,
        after_submission,
        after_enqueue,
        envelope(
            command.clone(),
            d + Duration::from_nanos(1),
            d + Duration::from_nanos(1),
        ),
    ] {
        assert_eq!(
            decision(&EffectiveFloor::default(), invalid, d, &p, &c).outcome,
            InvalidContext
        );
    }
    assert_eq!(
        decision(
            &EffectiveFloor::default(),
            command.clone().enqueued(r),
            d,
            &p,
            &c
        )
        .effective,
        r
    );
    assert_eq!(
        decision(
            &EffectiveFloor::default(),
            envelope(command.clone(), start - Duration::from_secs(1), r),
            d,
            &p,
            &c
        )
        .outcome,
        ClockUnavailable
    );
    for command in [
        SchedulerCommand::Play,
        SchedulerCommand::Stop,
        SchedulerCommand::SetTempo(Tempo::default()),
        SchedulerCommand::SetGlobalQuantisation(GlobalQuantisation::default()),
        SchedulerCommand::UpdateProject((*p.project).clone()),
    ] {
        let mut floor = EffectiveFloor::default();
        floor.advance(&command.enqueued(r), r + Duration::from_nanos(1));
        assert_eq!(
            decision(&floor, envelope(SchedulerCommand::StopAll, r, r), d, &p, &c).outcome,
            EpochChanged
        );
    }
}

#[test]
fn admission_is_frozen_before_own_hold_and_hold_stays_dequeue_based() {
    let (p, c, start) = setup();
    let b = start + Duration::from_secs(2);
    for d in [
        b - Duration::from_micros(100),
        b + Duration::from_micros(100),
    ] {
        let mut floor = EffectiveFloor::default();
        let r = b - Duration::from_millis(1);
        let mut envelope = envelope(SchedulerCommand::Launch(TrackId(1), 1), r, r);
        floor.admit(&mut envelope, d, &p, &c);
        let mut hold = Hold::default();
        assert!(
            hold.next(
                None,
                |at| {
                    assert_eq!(at, d);
                    true
                },
                || Ok(Some((envelope, d)))
            )
            .unwrap()
            .is_none()
        );
        assert_eq!(hold.yield_from(), Some(d));
        assert!(
            hold.next(
                Some(d + COMMAND_WAIT),
                |_| false,
                || panic!("FIFO must not receive again")
            )
            .unwrap()
            .is_none()
        );
        // A can be arbitrarily later; dequeue admission and its effective time survive.
        let (envelope, original_d) = hold
            .next(None, |_| false, || unreachable!())
            .unwrap()
            .unwrap();
        assert_eq!(original_d, d);
        assert_eq!(floor.advance(&envelope, d), r);
        let mut p = p.clone();
        let mut c = c.clone();
        assert_eq!(
            p.launch(TrackId(1), 1, &mut c, envelope.admitted.unwrap().effective)
                .unwrap(),
            BeatTime(3840)
        );
    }
}

#[test]
fn compound_commands_use_one_decision_for_every_grid_and_empty_stop() {
    let (mut p, c, start) = setup();
    let project = Arc::make_mut(&mut p.project);
    let mut second = crate::scheduler::tests::copy_track(&project.tracks[0], TrackId(2));
    for slot in &mut second.slots {
        slot.clip.as_mut().unwrap().id.0 += 10;
    }
    project.tracks.push(second);
    project.insert_group(
        Track::new_group(TrackId(101), "group", 2),
        &[TrackId(1), TrackId(2)],
    );
    let b = start + Duration::from_secs(2);
    let r = b - Duration::from_millis(1);
    let d = b + Duration::from_millis(1);
    for command in [
        SchedulerCommand::LaunchScene(SceneId(2)),
        SchedulerCommand::LaunchGroupSlot(TrackId(101), 1),
    ] {
        for kind in 0..4 {
            let project = Arc::make_mut(&mut p.project);
            let clip = project.clip(TrackId(1), 1).cloned();
            project[TrackId(2)].slots[1].clip = clip;
            let slot = &mut project[TrackId(2)].slots[1];
            match kind {
                0 => {
                    slot.clip.as_mut().unwrap().quantisation =
                        LaunchQuantisation::Value(QuantisationValue::Triplet(NoteValue::Half))
                }
                1 => slot.clip.as_mut().unwrap().quantisation = LaunchQuantisation::None,
                2 => {
                    slot.clip = None;
                    slot.has_stop_button = true;
                }
                _ => {
                    slot.clip = None;
                    slot.has_stop_button = false;
                }
            }
            let result = decision(
                &EffectiveFloor::default(),
                envelope(command.clone(), r, r),
                d,
                &p,
                &c,
            );
            assert_eq!(
                result.outcome,
                if kind == 1 { Unquantised } else { Admitted }
            );
        }
        // A short grid's expired point rejects the entire command, even while
        // the bar member would be eligible or future.
        let project = Arc::make_mut(&mut p.project);
        let clip = project.clip(TrackId(1), 1).unwrap().clone();
        project[TrackId(2)].slots[1].clip = Some(clip);
        project[TrackId(2)].slots[1]
            .clip
            .as_mut()
            .unwrap()
            .quantisation =
            LaunchQuantisation::Value(QuantisationValue::Note(NoteValue::ThirtySecond));
        let old = start + Duration::from_millis(1800);
        assert_eq!(
            decision(
                &EffectiveFloor::default(),
                envelope(command.clone(), old, old),
                d,
                &p,
                &c
            )
            .outcome,
            Expired
        );
    }
}
