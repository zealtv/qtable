use super::*;

pub(super) fn values(at: Instant) -> Envelope {
    SchedulerCommand::UpdateParameters(crate::engine::ValueEdit::for_test(
        super::super::tests::project(0.0, true),
        vec![],
    ))
    .enqueued(at)
}

#[test]
fn backlog_uses_enqueue_instants_and_received_order_even_when_stamps_reverse() {
    let start = Instant::now();
    let mut floor = EffectiveFloor::default();
    let dequeued = start + Duration::from_secs(10);
    for millis in 0..1000 {
        let posted = start + Duration::from_millis(millis);
        assert_eq!(floor.advance(&values(posted), dequeued), posted);
    }
    let newest = start + Duration::from_millis(999);
    // A producer stamped earlier but inserted later; don't sort it ahead of the first.
    assert_eq!(floor.advance(&values(start), dequeued), newest);
    assert_eq!(floor.advance(&values(newest), dequeued), newest);
    // Invalid future input cannot advance history beyond dequeue.
    assert_eq!(
        floor.advance(&values(dequeued + Duration::from_secs(1)), dequeued),
        dequeued
    );
}

#[test]
fn unadmitted_commands_keep_dequeue_time_and_only_temporal_commands_move_floor() {
    let start = Instant::now();
    let dequeued = start + Duration::from_secs(2);
    let project = super::super::tests::project(0.0, true);
    let event = project.tracks[0].slots[0].clip.as_ref().unwrap().events[0]
        .definition
        .clone();
    let cases = [
        (
            SchedulerCommand::Launch(TrackId(1), 0),
            Policy::Manual(admission::Manual::Slot(TrackId(1), 0)),
        ),
        (
            SchedulerCommand::LaunchScene(SceneId(1)),
            Policy::Manual(admission::Manual::Scene(SceneId(1))),
        ),
        (
            SchedulerCommand::LaunchGroupSlot(TrackId(1), 0),
            Policy::Manual(admission::Manual::Group(TrackId(1), 0)),
        ),
        (SchedulerCommand::CancelSceneLaunch, Policy::Sync),
        (
            SchedulerCommand::StopTrack(TrackId(1)),
            Policy::Manual(admission::Manual::Stop),
        ),
        (
            SchedulerCommand::StopGroup(TrackId(1)),
            Policy::Manual(admission::Manual::Stop),
        ),
        (
            SchedulerCommand::StopAll,
            Policy::Manual(admission::Manual::Stop),
        ),
        (SchedulerCommand::Play, Policy::Epoch),
        (SchedulerCommand::Stop, Policy::Epoch),
        (
            SchedulerCommand::SetTempo(Tempo::new(90.0).unwrap()),
            Policy::Epoch,
        ),
        (
            SchedulerCommand::UpdateProject(project.clone()),
            Policy::Epoch,
        ),
        (SchedulerCommand::ReEnableAutomation(None), Policy::Sync),
        (
            SchedulerCommand::SetGlobalQuantisation(GlobalQuantisation::None),
            Policy::EpochTiming,
        ),
        (
            SchedulerCommand::SetFollowActionsEnabled(false),
            Policy::Timing,
        ),
        (
            SchedulerCommand::FireEvent(TrackId(1), event),
            Policy::NonTemporal,
        ),
        (
            SchedulerCommand::Resolved(
                project.outputs[0].clone(),
                "127.0.0.1:9999".parse().unwrap(),
            ),
            Policy::NonTemporal,
        ),
        (SchedulerCommand::Shutdown, Policy::NonTemporal),
    ];
    for (command, expected_policy) in cases {
        assert_eq!(policy(&command), expected_policy, "{command:?}");
        let mut floor = EffectiveFloor::default();
        assert_eq!(floor.advance(&values(start), start), start);
        assert_eq!(floor.advance(&command.enqueued(start), dequeued), dequeued);
        let expected = if expected_policy == Policy::NonTemporal {
            start
        } else {
            dequeued
        };
        assert_eq!(floor.advance(&values(start), dequeued), expected);
    }
}

#[test]
fn hold_protects_dequeue_based_clip_and_frame_deadlines_and_keeps_enqueue_stamp() {
    let start = Instant::now();
    let dequeued = start + Duration::from_secs(1);
    let mut hold = Hold::default();
    assert!(matches!(
        hold.next(
            None,
            |at| {
                assert_eq!(at, dequeued);
                true
            },
            || Ok(Some((values(start), dequeued)))
        ),
        Ok(None)
    ));
    assert_eq!(hold.yield_from(), Some(dequeued));
    assert!(matches!(
        hold.next(Some(dequeued + COMMAND_WAIT), |_| false, || unreachable!()),
        Ok(None)
    ));
    let Ok(Some((envelope, at))) = hold.next(None, |_| false, || unreachable!()) else {
        panic!("released")
    };
    assert_eq!(at, dequeued);
    assert_eq!(envelope.enqueued_at, start);
    assert_eq!(EffectiveFloor::default().advance(&envelope, at), start);
}
