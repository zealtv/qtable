//! Held commands: clip events due soon after a command arrives go
//! first, and the command's musical decisions are then made at its arrival, not when it is applied.
use super::commands::{Disconnected, Hold, Io, apply};
use super::worker::next_send;
use super::*;
use crate::model::TICKS_PER_BEAT;

mod receive_time;

/// Two tracks of looping one-beat clips with an event on every beat, track 1 playing, on a clock
/// anchored ten seconds ago. The second bar line, `boundary`, has long passed in real time, so a
/// command applied at the instant it is handled, rather than at its arrival, picks a later bar.
struct Rig {
    planner: Planner,
    clock: InternalClock,
    stages: output_stage::OutputStages,
    sender: OscSender,
    tx: EventSink,
    resolver: Resolver,
    reaper: reaper::Reaper,
    oneshots: oneshots::Queue,
    boundary: Instant,
    _events: Receiver<SchedulerEvent>,
}

impl Rig {
    fn new() -> Self {
        let mut project = super::tests::project(0.0, true);
        let mut second = super::tests::copy_track(&project.tracks[0], TrackId(2));
        for slot in &mut second.slots {
            slot.clip.as_mut().unwrap().id.0 += 10;
        }
        project.tracks.push(second);
        let anchor = Instant::now() - Duration::from_secs(10);
        let mut clock = InternalClock::new(project.tempo, project.time_signature);
        clock.play(anchor);
        let mut planner = Planner::new(project, Duration::from_millis(100)).unwrap();
        planner
            .launch_at(TrackId(1), 0, BeatTime(3 * TICKS_PER_BEAT))
            .unwrap();
        let boundary = clock.instant_at(BeatTime(4 * TICKS_PER_BEAT)).unwrap();
        let stages = output_stage::OutputStages::new(
            &planner.project,
            parameter_controls::controls(&planner.project),
            anchor,
        );
        let (queue, events) = mpsc::sync_channel(64);
        let (commands, _) = CommandSender::channel();
        Self {
            planner,
            clock,
            stages,
            sender: OscSender::new().unwrap(),
            tx: EventSink::new(queue),
            resolver: Resolver::spawn(commands).unwrap(),
            reaper: reaper::Reaper::spawn().unwrap(),
            oneshots: oneshots::Queue::default(),
            boundary,
            _events: events,
        }
    }

    /// Consume every event due by `until`, as the worker does when it sends them.
    fn dispatch_through(&mut self, until: Instant) {
        for event in self.planner.plan(&self.clock, until) {
            if event.at > until {
                break;
            }
            self.planner.consume(&event);
        }
    }

    fn apply(&mut self, command: SchedulerCommand, arrived: Instant) {
        let io = Io {
            sender: &mut self.sender,
            tx: &mut self.tx,
            resolver: &self.resolver,
            reaper: &self.reaper,
            oneshots: &mut self.oneshots,
        };
        assert!(apply(
            command,
            arrived,
            &mut self.planner,
            &mut self.clock,
            &mut self.stages,
            io,
        ));
    }

    fn track(&self, event: &DueEvent) -> Option<TrackId> {
        let voice = self.planner.voices.iter().find(|v| v.id == event.voice)?;
        Some(voice.track)
    }

    /// Run a command arriving `before` the boundary through the worker's steps: it is held while
    /// the clip events on the boundary go, then applied. Returns the plan just after the boundary.
    fn held_until_the_boundary(&mut self, command: SchedulerCommand) -> Vec<DueEvent> {
        let arrived = self.boundary - Duration::from_micros(500);
        self.dispatch_through(arrived);
        let mut pending: VecDeque<_> = self.planner.plan(&self.clock, arrived).into();
        assert_eq!(next_send(&pending), Some(self.boundary));
        let mut hold = Hold::default();
        let held = hold.next(
            next_send(&pending),
            |_| false,
            || Ok(Some((command.enqueued(arrived), arrived))),
        );
        assert!(
            matches!(held, Ok(None)),
            "the command waits for the clip events"
        );
        let mut sent = 0;
        while pending
            .front()
            .is_some_and(|event| event.at <= self.boundary)
        {
            let event = pending.pop_front().unwrap();
            assert_eq!(self.track(&event), Some(TrackId(1)));
            self.planner.consume(&event);
            sent += 1;
        }
        assert!(sent > 0, "track 1 has events on the boundary");
        let Ok(Some((command, at))) = hold.next(next_send(&pending), |_| false, || unreachable!())
        else {
            panic!("the command goes once the boundary's events have gone");
        };
        assert_eq!(at, arrived, "a held command keeps its arrival");
        self.apply(command.command, at);
        let replanned = self
            .planner
            .plan(&self.clock, self.boundary + Duration::from_micros(100));
        assert!(
            !replanned
                .iter()
                .any(|event| self.track(event) == Some(TrackId(1))
                    && matches!(event.action, PlannedAction::Send { .. })
                    && event.at <= self.boundary),
            "track 1's events on the boundary went while the command waited; they aren't planned again"
        );
        replanned
    }
}

#[test]
fn the_command_hold_includes_exactly_two_milliseconds_after_arrival() {
    let arrived = Instant::now();
    let boundary = arrived + commands::COMMAND_WAIT;
    let mut hold = Hold::default();
    assert!(matches!(
        hold.next(
            Some(boundary),
            |_| false,
            || Ok(Some((SchedulerCommand::Play.enqueued(arrived), arrived)))
        ),
        Ok(None)
    ));
    let Ok(Some((
        Envelope {
            command: SchedulerCommand::Play,
            ..
        },
        at,
    ))) = hold.next(
        Some(boundary + Duration::from_nanos(1)),
        |_| false,
        || unreachable!("a held command leaves later commands queued"),
    )
    else {
        panic!("a send beyond the hold window does not delay the command");
    };
    assert_eq!(at, arrived);
}

#[test]
fn a_command_waits_for_a_send_behind_track_silent() {
    let arrived = Instant::now();
    let send_at = arrived + Duration::from_millis(1);
    let later = arrived + Duration::from_millis(3);
    let mut pending = VecDeque::from([
        DueEvent {
            at: arrived,
            action: PlannedAction::TrackSilent(TrackId(1)),
            voice: 1,
            cursor: 0,
        },
        DueEvent {
            at: send_at,
            action: PlannedAction::Send {
                output: TrackId(2),
                packet: None,
            },
            voice: 2,
            cursor: 0,
        },
        DueEvent {
            at: later,
            action: PlannedAction::Send {
                output: TrackId(2),
                packet: None,
            },
            voice: 2,
            cursor: 1,
        },
    ]);
    assert_eq!(next_send(&pending), Some(send_at));
    let mut hold = Hold::default();
    assert!(matches!(
        hold.next(
            next_send(&pending),
            |_| false,
            || Ok(Some((SchedulerCommand::Play.enqueued(arrived), arrived)))
        ),
        Ok(None)
    ));
    pending.pop_front();
    assert!(matches!(
        hold.next(next_send(&pending), |_| false, || unreachable!()),
        Ok(None)
    ));
    pending.pop_front();
    let Ok(Some((
        Envelope {
            command: SchedulerCommand::Play,
            ..
        },
        at,
    ))) = hold.next(next_send(&pending), |_| false, || unreachable!())
    else {
        panic!("only sends within the command's hold window go first");
    };
    assert_eq!(at, arrived);
    pending.pop_front();
    assert_eq!(next_send(&pending), None);
}

#[test]
fn a_launch_held_across_a_boundary_with_clip_events_still_starts_on_it() {
    let mut rig = Rig::new();
    let replanned = rig.held_until_the_boundary(SchedulerCommand::Launch(TrackId(2), 0));
    let voice = rig
        .planner
        .voices
        .iter()
        .find(|v| v.track == TrackId(2))
        .unwrap();
    assert_eq!(voice.launch, BeatTime(4 * TICKS_PER_BEAT));
    let first = replanned
        .iter()
        .find(|event| rig.track(event) == Some(TrackId(2)))
        .unwrap();
    assert!(matches!(first.action, PlannedAction::Send { .. }));
    assert_eq!(
        first.at, rig.boundary,
        "track 2's first event is due on the boundary, so it goes at once"
    );
}

#[test]
fn a_quantised_stop_held_across_a_boundary_with_clip_events_still_stops_on_it() {
    let mut rig = Rig::new();
    let replanned = rig.held_until_the_boundary(SchedulerCommand::StopTrack(TrackId(1)));
    assert_eq!(
        rig.planner.stops.get(&TrackId(1)).map(|stop| stop.beat),
        Some(BeatTime(4 * TICKS_PER_BEAT))
    );
    assert!(replanned.iter().any(|event| matches!(
        event.action,
        PlannedAction::StopTrack(TrackId(1))
    ) && event.at == rig.boundary));
    assert!(
        !replanned
            .iter()
            .any(|event| rig.track(event) == Some(TrackId(1))
                && matches!(event.action, PlannedAction::Send { .. })),
        "track 1 sends nothing after the boundary it stops on"
    );
}

#[test]
fn commands_behind_a_held_one_stay_queued_in_order() {
    let boundary = Instant::now();
    let arrived = boundary - Duration::from_micros(500);
    let (commands, rx) = mpsc::channel();
    commands
        .send(SchedulerCommand::Launch(TrackId(2), 0))
        .unwrap();
    commands
        .send(SchedulerCommand::StopTrack(TrackId(2)))
        .unwrap();
    let receive = |at: Instant| {
        let rx = &rx;
        move || match rx.try_recv() {
            Ok(command) => Ok(Some((command.enqueued(at), at))),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(Disconnected),
        }
    };
    let mut hold = Hold::default();
    assert!(matches!(
        hold.next(Some(boundary), |_| false, receive(arrived)),
        Ok(None)
    ));
    // Still waiting: the stop isn't received, so it can't overtake the launch.
    assert!(matches!(
        hold.next(Some(boundary), |_| false, || unreachable!()),
        Ok(None)
    ));
    let later = boundary + Duration::from_millis(250);
    let Ok(Some((
        Envelope {
            command: SchedulerCommand::Launch(TrackId(2), 0),
            ..
        },
        at,
    ))) = hold.next(Some(later), |_| false, || unreachable!())
    else {
        panic!("the held launch goes first");
    };
    assert_eq!(at, arrived);
    let stop_arrived = boundary + Duration::from_micros(100);
    let Ok(Some((
        Envelope {
            command: SchedulerCommand::StopTrack(TrackId(2)),
            ..
        },
        at,
    ))) = hold.next(Some(later), |_| false, receive(stop_arrived))
    else {
        panic!("then the stop queued behind it");
    };
    assert_eq!(at, stop_arrived);
    assert!(matches!(
        hold.next(None, |_| false, receive(later)),
        Ok(None)
    ));
}

#[test]
fn an_edit_held_across_a_boundary_does_not_send_the_boundary_again() {
    let mut rig = Rig::new();
    let mut project = (*rig.planner.project).clone();
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    clip.events[0].position = BeatTime(120);
    // The rebased clip's events at its new position are still to come; the boundary's went.
    let replanned = rig.held_until_the_boundary(SchedulerCommand::UpdateProject(project));
    let next = replanned
        .iter()
        .find(|event| rig.track(event) == Some(TrackId(1)))
        .unwrap();
    assert_eq!(
        Some(next.at),
        rig.clock.instant_at(BeatTime(4 * TICKS_PER_BEAT + 120))
    );
}

#[test]
fn admitted_launch_recovers_boundary_without_replaying_the_old_hit() {
    let mut rig = Rig::new();
    let posted = rig.boundary - Duration::from_micros(500);
    let dequeued = rig.boundary + Duration::from_micros(100);
    rig.dispatch_through(dequeued); // Old clip's boundary hit has already gone.
    let mut envelope = SchedulerCommand::Launch(TrackId(2), 0).enqueued(posted);
    let mut floor = commands::EffectiveFloor::default();
    floor.admit(&mut envelope, dequeued, &rig.planner, &rig.clock);
    let effective = floor.advance(&envelope, dequeued);
    assert_eq!(effective, posted);
    rig.apply(envelope.command, effective);
    let voice = rig
        .planner
        .voices
        .iter()
        .find(|v| v.track == TrackId(2))
        .unwrap();
    assert_eq!(voice.launch, BeatTime(4 * TICKS_PER_BEAT));
    let planned = rig.planner.plan(&rig.clock, dequeued);
    assert!(
        !planned
            .iter()
            .any(|event| rig.track(event) == Some(TrackId(1)) && event.at <= rig.boundary)
    );
    assert!(
        planned
            .iter()
            .any(|event| rig.track(event) == Some(TrackId(2)) && event.at == rig.boundary)
    );
}
