//! Received commands: held while clip events due soon go first, then applied.
use super::*;
pub(super) mod admission;

/// A command waits for clip events due within this long after it arrives: applying an edit to a
/// large set can take a millisecond, and the clip events must not wait for it.
pub(super) const COMMAND_WAIT: Duration = Duration::from_millis(2);

/// The receiver hung up: the engine has gone.
#[derive(Debug)]
pub(super) struct Disconnected;

/// A received envelope waiting, with its dequeue instant D: for clip events, or for an earlier ordinary
/// frame (hold-commands-behind-earlier-frames, 2026-10-20).
#[derive(Default)]
pub(super) struct Hold(Option<(Envelope, Instant)>);

impl Hold {
    /// The envelope to apply now, with its dequeue instant D. Clip events due soon after D
    /// go first. So does every ordinary frame for a tick before D (`earlier_frame`): a
    /// command can change automation history, so finish already-owed frames first even when
    /// its value history will be effective earlier than dequeue. A fire re-syncs nothing and doesn't wait for frames. `receive` isn't called while
    /// one waits, so later commands stay queued behind it, fires included.
    pub(super) fn next(
        &mut self,
        next_send: Option<Instant>,
        earlier_frame: impl FnOnce(Instant) -> bool,
        receive: impl FnOnce() -> Result<Option<(Envelope, Instant)>, Disconnected>,
    ) -> Result<Option<(Envelope, Instant)>, Disconnected> {
        let command = match self.0.take() {
            Some(held) => Some(held),
            None => receive()?,
        };
        Ok(match command {
            Some((command, arrived))
                if next_send.is_some_and(|at| at <= arrived + COMMAND_WAIT)
                    || !is_fire(&command.command) && earlier_frame(arrived) =>
            {
                self.0 = Some((command, arrived));
                None
            }
            command => command,
        })
    }

    /// Ordinary frames for ticks at or after this instant wait for the held command: it waits
    /// for nothing they send, and once one started its packets would hold the command too.
    pub(super) fn yield_from(&self) -> Option<Instant> {
        self.0
            .as_ref()
            .filter(|(command, _)| !is_fire(&command.command))
            .map(|&(_, arrived)| arrived)
    }
}

fn is_fire(command: &SchedulerCommand) -> bool {
    matches!(command, SchedulerCommand::FireEvent(..))
}

/// The single exhaustive timing/automation classification: manual admission,
/// value enqueue time, epoch invalidation, automation sync and floor advancement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Policy {
    Values,
    Sync,
    Epoch,
    EpochTiming,
    Manual(admission::Manual),
    Timing,
    NonTemporal,
}

pub(super) fn policy(command: &SchedulerCommand) -> Policy {
    match command {
        SchedulerCommand::UpdateParameters(..) => Policy::Values,
        SchedulerCommand::FireEvent(..)
        | SchedulerCommand::Resolved(..)
        | SchedulerCommand::Shutdown => Policy::NonTemporal,
        SchedulerCommand::SetGlobalQuantisation(_) => Policy::EpochTiming,
        SchedulerCommand::SetFollowActionsEnabled(_) => Policy::Timing,
        SchedulerCommand::Launch(track, slot) => {
            Policy::Manual(admission::Manual::Slot(*track, *slot))
        }
        SchedulerCommand::LaunchScene(scene) => Policy::Manual(admission::Manual::Scene(*scene)),
        SchedulerCommand::LaunchGroupSlot(group, slot) => {
            Policy::Manual(admission::Manual::Group(*group, *slot))
        }
        SchedulerCommand::StopTrack(_)
        | SchedulerCommand::StopGroup(_)
        | SchedulerCommand::StopAll => Policy::Manual(admission::Manual::Stop),
        SchedulerCommand::Play
        | SchedulerCommand::Stop
        | SchedulerCommand::SetTempo(_)
        | SchedulerCommand::UpdateProject(_) => Policy::Epoch,
        SchedulerCommand::CancelSceneLaunch | SchedulerCommand::ReEnableAutomation(_) => {
            Policy::Sync
        }
    }
}

/// Last applied history/clock-changing command. Autonomous playback and resolver
/// completion do not move this floor. Equal instants preserve channel FIFO.
#[derive(Default)]
pub(super) struct EffectiveFloor {
    at: Option<Instant>,
    // Latest unavailable interpretation boundary; not a second effective-time floor.
    epoch: Option<Instant>,
}

impl EffectiveFloor {
    pub(super) fn advance(&mut self, envelope: &Envelope, dequeued_at: Instant) -> Instant {
        let policy = policy(&envelope.command);
        let effective = if policy == Policy::Values {
            // Only our sender supplies production stamps. Do not backdate from invalid
            // future metadata (also useful for synthetic/direct-client tests).
            let enqueued = envelope.enqueued_at.min(dequeued_at);
            self.at.map_or(enqueued, |floor| enqueued.max(floor))
        } else if let Some(decision) = envelope.admitted {
            decision.effective
        } else {
            dequeued_at
        };
        if policy != Policy::NonTemporal {
            self.at = Some(effective);
        }
        if matches!(policy, Policy::Epoch | Policy::EpochTiming) {
            self.epoch = Some(dequeued_at);
        }
        effective
    }
}

/// What a command reaches besides the musical state.
pub(super) struct Io<'a> {
    pub(super) sender: &'a mut OscSender,
    pub(super) tx: &'a mut EventSink,
    pub(super) resolver: &'a Resolver,
    pub(super) reaper: &'a reaper::Reaper,
    pub(super) oneshots: &'a mut oneshots::Queue,
}

/// Apply one command, making its musical decisions at `effective`: a command held while clip events
/// went first retains its dequeue-time admission (or D fallback), and whatever
/// falls due meanwhile goes at once. Returns false for Shutdown.
pub(super) fn apply(
    command: SchedulerCommand,
    effective: Instant,
    planner: &mut Planner,
    clock: &mut InternalClock,
    stages: &mut output_stage::OutputStages,
    io: Io,
) -> bool {
    let Io {
        sender,
        tx,
        resolver,
        reaper,
        oneshots,
    } = io;
    let was_playing = clock.is_playing();
    let sync_automation = matches!(
        policy(&command),
        Policy::Sync | Policy::Epoch | Policy::Manual(_)
    );
    let mut reenable = None;
    match command {
        SchedulerCommand::Shutdown => return false,
        SchedulerCommand::UpdateParameters(edit) => {
            let (project, fields) = edit.into_parts();
            // Engine has already validated this value-only snapshot. Validate the named
            // values at this boundary too, without revalidating/rebasing the whole set.
            let valid = fields.iter().all(|key| {
                planner
                    .project
                    .field(key)
                    .zip(project.field(key))
                    .is_some_and(|(old, new)| old.spec.check(new.value).is_ok())
            });
            if valid {
                debug_assert!(
                    {
                        let mut expected = (*planner.project).clone();
                        fields.iter().all(|key| {
                            expected
                                .set_parameter_values(
                                    &key.parameter,
                                    vec![(key.field.clone(), project.field(key).unwrap().value)],
                                )
                                .is_ok()
                        }) && expected == project
                    },
                    "parameter update changed more than the named stored values"
                );
                reaper.retire(std::mem::replace(&mut planner.project, Arc::new(project)));
                reaper.retire(stages.update_parameters(&planner.project, &fields, effective));
            } else {
                tx.send(SchedulerEvent::Error(invalid("invalid parameter update")));
                reaper.retire(project);
            }
        }
        SchedulerCommand::ReEnableAutomation(field) => reenable = Some(field),
        SchedulerCommand::FireEvent(track, event) => {
            oneshots.fire(&planner.project, track, &event, effective);
        }
        SchedulerCommand::Play => {
            if !clock.is_playing() {
                match planner.play_transport(clock, effective) {
                    Ok(()) => tx.send(SchedulerEvent::Playing(effective)),
                    Err(error) => tx.send(SchedulerEvent::Error(error)),
                }
            }
        }
        SchedulerCommand::Stop => {
            // Live 12 manual, Arrangement "Transport and Playback":
            // a second Stop returns the position to the start.
            if clock.is_playing() {
                planner.stop_transport(clock, effective);
                clock.stop(effective);
            } else {
                clock.rewind();
            }
        }
        SchedulerCommand::StopAll => match planner.request_stop_all(clock, effective) {
            Ok(_) => {}
            Err(e) => {
                tx.send(SchedulerEvent::Error(e));
            }
        },
        SchedulerCommand::StopGroup(group) => {
            match planner.request_stop_group(group, clock, effective) {
                Ok(_) => {}
                Err(e) => {
                    tx.send(SchedulerEvent::Error(e));
                }
            }
        }
        SchedulerCommand::StopTrack(t) => match planner.request_stop(t, clock, effective) {
            Ok(_) => {}
            Err(e) => {
                tx.send(SchedulerEvent::Error(e));
            }
        },
        SchedulerCommand::SetTempo(tempo) => {
            if planner.set_tempo(clock, tempo, effective) {
                // The engine's project carries the new tempo; a later value-only update
                // compares the two projects, so the planner's copy must follow the clock.
                Arc::make_mut(&mut planner.project).tempo = tempo;
            } else {
                tx.send(SchedulerEvent::Error(invalid("tempo change failed")));
            }
        }
        SchedulerCommand::SetFollowActionsEnabled(value) => {
            Arc::make_mut(&mut planner.project).follow_actions_enabled = value;
        }
        SchedulerCommand::SetGlobalQuantisation(value) => {
            if let Err(e) = planner.set_global_quantisation(value) {
                tx.send(SchedulerEvent::Error(e));
            }
        }
        SchedulerCommand::Resolved(output, address) => {
            sender.insert_resolved(&output, address);
        }
        SchedulerCommand::UpdateProject(project) => {
            resolver.request(sender, project.configured_destinations());
            // Keep a reference so the reaper thread frees the old project.
            let previous = Arc::clone(&planner.project);
            let updated = planner.update_project(project, clock, effective);
            if updated.is_ok() {
                oneshots.prune(&planner.project);
                stages.update(
                    &planner.project,
                    parameter_controls::controls(&planner.project),
                    effective,
                );
            }
            reaper.retire(previous);
            match updated {
                Ok(_) => {}
                Err(error) => {
                    tx.send(SchedulerEvent::Error(error));
                }
            }
        }
        SchedulerCommand::LaunchGroupSlot(group, row) => {
            let stopped = !clock.is_playing();
            match planner.launch_group_slot(group, row, clock, effective) {
                Ok(_) => {
                    if stopped && clock.is_playing() {
                        tx.send(SchedulerEvent::Playing(effective));
                    }
                }
                Err(error) => {
                    tx.send(SchedulerEvent::Error(error));
                }
            }
        }
        SchedulerCommand::CancelSceneLaunch => planner.cancel_scene_launch(clock, effective),
        SchedulerCommand::LaunchScene(scene) => {
            let stopped = !clock.is_playing();
            match planner.launch_scene(scene, clock, effective) {
                Ok(_) => {
                    if stopped && clock.is_playing() {
                        tx.send(SchedulerEvent::Playing(effective));
                    }
                }
                Err(error) => {
                    tx.send(SchedulerEvent::Error(error));
                }
            }
        }
        SchedulerCommand::Launch(t, s) => {
            let stopped = !clock.is_playing();
            match planner.trigger_slot(t, s, clock, effective) {
                Ok(_) => {}
                Err(e) => {
                    tx.send(SchedulerEvent::Error(e));
                }
            }
            if stopped && clock.is_playing() {
                tx.send(SchedulerEvent::Playing(effective));
            }
        }
    }
    marks::mark("command.planned");
    // Keep automation history and prepared samples when the command changes no input they read.
    if !sync_automation {
        return true;
    }
    reaper.retire(stages.sync_automation_control(
        planner,
        clock,
        effective,
        &[],
        reenable.as_ref(),
    ));
    if !was_playing && clock.is_playing() {
        stages.start(effective);
        marks::mark("start.end");
    }
    if was_playing && !clock.is_playing() {
        stages.stop(effective);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;

    #[test]
    fn command_sync_preserves_live_values_and_manual_override_semantics() {
        check_parameter_update(false);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "parameter update changed more than the named stored values")]
    fn parameter_update_rejects_a_structural_change_in_debug_builds() {
        check_parameter_update(true);
    }

    fn check_parameter_update(structural_change: bool) {
        let mut project = super::super::tests::project(0.0, true);
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
        let field = target.on(TrackId(1));
        let start = Instant::now();
        let mut planner = Planner::new(project, Duration::ZERO).unwrap();
        let mut clock = InternalClock::default();
        planner.launch(TrackId(1), 0, &mut clock, start).unwrap();
        let mut stages = output_stage::OutputStages::new(
            &planner.project,
            parameter_controls::controls(&planner.project),
            start,
        );
        stages.sync_automation(&planner, &clock, start);
        let mut sender = OscSender::new().unwrap();
        let (queue, _events) = mpsc::sync_channel(64);
        let mut tx = EventSink::new(queue);
        let (commands, _rx) = CommandSender::channel();
        let resolver = Resolver::spawn(commands).unwrap();
        let reaper = reaper::Reaper::spawn().unwrap();
        let mut oneshots = oneshots::Queue::default();
        marks::enable();
        let mut run =
            |command, at, planner: &mut Planner, stages: &mut output_stage::OutputStages| {
                marks::take();
                assert!(apply(
                    command,
                    at,
                    planner,
                    &mut clock,
                    stages,
                    Io {
                        sender: &mut sender,
                        tx: &mut tx,
                        resolver: &resolver,
                        reaper: &reaper,
                        oneshots: &mut oneshots,
                    }
                ));
                marks::take()
                    .iter()
                    .any(|(label, _)| *label == "sync.begin")
            };
        let at = start + Duration::from_millis(125);
        let before = stages.automation_values(at);
        assert!(!before.is_empty());
        for command in [
            SchedulerCommand::Resolved(
                planner.project.outputs[0].clone(),
                "127.0.0.1:9999".parse().unwrap(),
            ),
            SchedulerCommand::SetGlobalQuantisation(GlobalQuantisation::None),
            SchedulerCommand::SetFollowActionsEnabled(false),
        ] {
            assert!(!run(command, at, &mut planner, &mut stages));
            assert_eq!(stages.automation_values(at), before);
        }
        assert_eq!(
            planner.project.global_quantisation,
            GlobalQuantisation::None
        );
        assert!(!planner.project.follow_actions_enabled);
        // Skipping unrelated commands must still let the original clock advance its ramp.
        let later = start + Duration::from_millis(250);
        assert!(stages.automation_values(later)[0].value > before[0].value);
        let mut edited = (*planner.project).clone();
        edited.tracks[0].mixer_device.volume.value = 0.8;
        if structural_change {
            edited.tracks[0].delay_ms += 10.0;
        }
        assert!(!run(
            SchedulerCommand::UpdateParameters(crate::engine::ValueEdit::for_test(
                edited,
                vec![field.clone()]
            )),
            later,
            &mut planner,
            &mut stages
        ));
        let overridden = stages.automation_values(later);
        assert_eq!(overridden[0].state, AutomationState::Overridden);
        assert_eq!(overridden[0].value, 0.8);
        let previous = Arc::clone(&planner.project);
        let mut invalid_project = (*previous).clone();
        invalid_project.tracks[0].mixer_device.volume.value = 0.1;
        let mut missing = field.clone();
        missing.field = FieldKey("missing".into());
        assert!(!run(
            SchedulerCommand::UpdateParameters(crate::engine::ValueEdit::for_test(
                invalid_project,
                vec![field.clone(), missing]
            )),
            later,
            &mut planner,
            &mut stages
        ));
        assert!(
            Arc::ptr_eq(&previous, &planner.project),
            "reject the whole batch"
        );
        assert_eq!(stages.automation_values(later), overridden);
        assert!(
            _events
                .try_iter()
                .any(|event| matches!(event, SchedulerEvent::Error(_)))
        );
        assert!(!run(
            SchedulerCommand::SetFollowActionsEnabled(true),
            later,
            &mut planner,
            &mut stages
        ));
        assert_eq!(stages.automation_values(later), overridden);
        assert!(run(
            SchedulerCommand::ReEnableAutomation(Some(field)),
            later,
            &mut planner,
            &mut stages
        ));
        assert_eq!(
            stages.automation_values(later)[0].state,
            AutomationState::Following
        );
        assert!(run(
            SchedulerCommand::SetTempo(Tempo::new(90.0).unwrap()),
            later,
            &mut planner,
            &mut stages
        ));
        let mut edited = (*planner.project).clone();
        let Lane::Envelope(envelope) =
            &mut edited.tracks[0].slots[0].clip.as_mut().unwrap().lanes[0]
        else {
            panic!("envelope");
        };
        envelope.points[0].value = 0.4;
        assert!(run(
            SchedulerCommand::UpdateProject(edited),
            later,
            &mut planner,
            &mut stages
        ));
        assert!(run(
            SchedulerCommand::Stop,
            later,
            &mut planner,
            &mut stages
        ));
        assert!(run(
            SchedulerCommand::Play,
            later,
            &mut planner,
            &mut stages
        ));
    }
}

#[cfg(test)]
mod timing_tests;

#[cfg(test)]
mod queued_tests;
