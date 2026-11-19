//! complete-clusters-before-frames, gate 1: the worker's main path at synthetic instants, with a
//! real `Planner`, real output stages (frames go whole, as built) and the worker's own `chain`.
//! Hits are recorded, not sent. Costs are fixed, from the 2026-10-20 harness measurements on
//! the maintainer's Mac (Bundle Off): a datagram ~18 µs, a plan ~50–120 µs.
use std::collections::HashMap;
use std::net::UdpSocket;

use super::commands::{Hold, Io, apply};
use super::output_stage::{OutputStages, cluster::CLEARANCE};
use super::*;
use crate::model::*;

pub(super) const US: fn(u64) -> Duration = Duration::from_micros;
pub(super) const MS: fn(u64) -> Duration = Duration::from_millis;
pub(super) const A: TrackId = TrackId(1001);
pub(super) const B: TrackId = TrackId(1002);
/// The cap at 60 Hz: 4 ms, under half a period.
pub(super) const CAP: Duration = Duration::from_millis(4);
/// 60 Hz.
pub(super) const PERIOD: Duration = Duration::from_nanos(16_666_667);
/// One sixteenth at 120 BPM.
pub(super) const SIXTEENTH: Duration = Duration::from_millis(125);

pub(super) const NS: fn(u64) -> Duration = Duration::from_nanos;

/// The grid lead that puts a tick at `tick` after the transport's start.
pub(super) fn lead_for(tick: Duration) -> Duration {
    let period = PERIOD.as_nanos() as u64;
    let tick = tick.as_nanos() as u64 % period;
    NS((period - tick) % period)
}

/// Whether `a` and `b` agree within the grid's rounding (ticks come from float seconds).
pub(super) fn near(a: Instant, b: Instant) -> bool {
    within(a, b, US(1))
}

pub(super) fn within(a: Instant, b: Instant, tolerance: Duration) -> bool {
    a.max(b) - a.min(b) < tolerance
}

const DATAGRAM: Duration = Duration::from_micros(18);
const PLAN: Duration = Duration::from_micros(80);
/// A worker pass with nothing to do.
const PASS: Duration = Duration::from_micros(2);
/// A pass while a frame is held back and nothing else is due: the worker spins.
const SPIN: Duration = Duration::from_micros(10);

/// The measurement harness's clip delays (ms): four tracks on A; arm j adds four on B.
pub(super) const CLIP_DELAYS_MS: [f64; 4] = [0.137, 0.251, 0.373, 0.419];
pub(super) const B_CLIP_DELAYS_MS: [f64; 4] = [0.2, 0.3, 0.4, 0.5];

/// A clip track on `output`, delayed `delay_ms`, with a looping clip `length` ticks long and a
/// hit at each of `positions`.
pub(super) fn clip_track(
    id: u64,
    output: TrackId,
    delay_ms: f64,
    length: u64,
    positions: &[u64],
) -> Track {
    let mut track = Track::new(TrackId(id), "clips", 1);
    track.route = Route::single(output);
    track.delay_ms = delay_ms;
    let definition = track.ensure_event(OscAddress::Relative(format!("/clip/t{id}")), &[], || {
        DeviceId(id)
    });
    let mut clip = Clip::new(ClipId(id), "hits", BeatTime(length), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.events = positions
        .iter()
        .map(|&position| Event {
            position: BeatTime(position),
            definition: definition.clone(),
            arguments: vec![],
        })
        .collect();
    track.slots[0].clip = Some(clip);
    track
}

/// Sixteenths over a bar.
pub(super) fn sixteenths() -> Vec<u64> {
    (0..16).map(|step| step * 240).collect()
}

/// A Target track on A whose `length`-tick clip ramps 16 continuous parameters from 0 to 1, so
/// every frame carries 16 changes and the clip's wrap is an exact jump.
pub(super) fn ramp_track(length: u64) -> Track {
    ramp_track_on(A, 500, length)
}

/// `ramp_track` on `output`, its track, device and clip numbered `id`.
pub(super) fn ramp_track_on(output: TrackId, id: u64, length: u64) -> Track {
    let mut track = Track::new(TrackId(id), "envelopes", 1);
    track.route = Route::single(output);
    let device = track.first_target_mut_or_insert(|| DeviceId(id));
    let target = device.target_mut().unwrap();
    let mut clip = Clip::new(
        ClipId(id),
        "automation",
        BeatTime(length),
        LIVE_CLIP_PALETTE[0],
    )
    .unwrap();
    for index in 0..16 {
        let key = format!("p{index}");
        target.parameters.push(ParameterDef {
            key: EntryKey(key.clone()),
            name: String::new(),
            path: OscAddress::Relative(format!("/parameter/{key}")),
            fields: vec![FieldDef::new(
                FieldKey("value".into()),
                FieldType::Float,
                true,
            )],
            send_mode: SendMode::Continuous,
        });
        clip.lanes.push(Lane::Envelope(AutomationEnvelope {
            region: None,
            target: ArgumentRef {
                device: DeviceRef::Chain(DeviceId(id)),
                parameter: ParameterKey(key),
                field: FieldKey("value".into()),
            },
            points: vec![
                Breakpoint {
                    time: BeatTime(0),
                    value: 0.0,
                    curve: 0.0,
                },
                Breakpoint {
                    time: BeatTime(length),
                    value: 1.0,
                    curve: 0.0,
                },
            ],
        }));
    }
    track.slots[0].clip = Some(clip);
    track
}

/// Tracks 11–26 on A, each sending its volume Every Frame while the transport runs.
pub(super) fn every_frame_volumes() -> impl Iterator<Item = Track> {
    (0..16).map(|i| {
        let mut track = Track::new(TrackId(i + 11), "parameter", 1);
        track.route = Route::single(A);
        let output = &mut track.mixer_device.volume.output;
        output.enabled = true;
        output.address = format!("/parameter/v{i}");
        output.send_mode = SendMode::EveryFrame;
        track
    })
}

/// A project of `outputs` at 60 Hz, Bundle Off, holding `tracks`, with one scene.
pub(super) fn project(bpm: f64, outputs: &[TrackId], tracks: Vec<Track>) -> Project {
    Project {
        tempo: Tempo::new(bpm).unwrap(),
        scenes: vec![Scene {
            id: SceneId(1),
            name: "scene".into(),
            follow_actions: SceneFollowActions::default(),
        }],
        outputs: outputs
            .iter()
            .map(|&id| {
                let mut output = Track::new_output(id, "rx", "127.0.0.1", 9);
                output.output.as_mut().unwrap().stage.frame_rate = 60.0;
                output
            })
            .collect(),
        tracks,
        ..Project::default()
    }
}

/// The harness's arms (envelope-jumps-and-override/harness, 2026-10-20) at `bpm`.
#[derive(Clone, Copy, Debug)]
pub(super) enum Arm {
    /// Four clip tracks and 16 Every Frame volumes on A.
    C,
    /// Four clip tracks and a 16-parameter ramp on A.
    G,
    /// G on A, and four more clip tracks on B.
    J,
}

impl Arm {
    pub(super) fn project(self, bpm: f64) -> Project {
        let mut tracks: Vec<_> = CLIP_DELAYS_MS
            .iter()
            .enumerate()
            .map(|(i, &delay)| clip_track(i as u64 + 1, A, delay, 3840, &sixteenths()))
            .collect();
        if let Arm::J = self {
            tracks.extend(
                B_CLIP_DELAYS_MS
                    .iter()
                    .enumerate()
                    .map(|(i, &delay)| clip_track(i as u64 + 5, B, delay, 3840, &sixteenths())),
            );
        }
        match self {
            Arm::C => tracks.extend(every_frame_volumes()),
            Arm::G | Arm::J => tracks.push(ramp_track(3840)),
        }
        let outputs: &[TrackId] = match self {
            Arm::J => &[A, B],
            _ => &[A],
        };
        project(bpm, outputs, tracks)
    }
}

/// A clip event the simulated worker sent.
#[derive(Clone, Copy, Debug)]
pub(super) struct Sent {
    pub(super) output: TrackId,
    pub(super) due: Instant,
    pub(super) at: Instant,
    pub(super) manual: bool,
}

/// A frame the stages built, from its `FrameSent` report.
#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
    pub(super) output: TrackId,
    pub(super) tick: Instant,
    pub(super) start: Instant,
    /// When its last packet went.
    pub(super) end: Instant,
    /// An exact jump or transport start values (a start send), not a grid frame.
    pub(super) start_send: bool,
    pub(super) profile: FrameProfile,
}

/// A posted command: posting, arrival (taken by the worker) and application.
#[derive(Clone, Copy, Debug)]
pub(super) struct Applied {
    pub(super) fire: bool,
    pub(super) posted: Instant,
    pub(super) arrived: Instant,
    pub(super) applied: Instant,
    pub(super) effective: Instant,
    pub(super) admission: Option<commands::admission::AdmissionOutcome>,
}

pub(super) struct Sim {
    pub(super) planner: Planner,
    pub(super) clock: InternalClock,
    pub(super) stages: OutputStages,
    sender: OscSender,
    /// One receiver per output, so each output's frames pair with their own datagrams.
    receivers: Vec<(TrackId, UdpSocket)>,
    /// Messages received and not yet paired with a frame, per output.
    received: HashMap<TrackId, VecDeque<(String, f32)>>,
    activity: crate::activity::Activity,
    tx: EventSink,
    events: Receiver<SchedulerEvent>,
    resolver: Resolver,
    reaper: reaper::Reaper,
    pub(super) oneshots: oneshots::Queue,
    /// Commands posted and not yet taken, with their postings; the worker takes one only once
    /// `hold` is empty and stamps its arrival then, as `worker.rs` does.
    inbox: VecDeque<Envelope>,
    hold: Hold,
    effective_floor: commands::EffectiveFloor,
    /// The posting of the command in `hold`.
    held_posted: Option<Instant>,
    /// When each command was applied.
    pub(super) applied: Vec<Instant>,
    /// Every command applied after being posted (`send`), in order.
    pub(super) log: Vec<Applied>,
    /// Frames leave a packet per pass through the worker's turn path (`worker.rs`), with due
    /// clip events on other outputs between packets; otherwise each goes whole as built.
    pub(super) resumable: bool,
    /// What one frame packet costs on the turn path (a Bundle On frame is one datagram, which can
    /// take far longer than a hit's).
    pub(super) frame_packet: Duration,
    /// The stages run with live timing, as in the worker: after a frame a stage is dirty until
    /// replanned, which the worker does only when no clip event is due within 1 ms.
    pub(super) live_timing: bool,
    pub(super) pending: VecDeque<DueEvent>,
    /// Hits completed frames released (`turns::Backlogs`), as the worker keeps them.
    pub(super) backlogs: turns::Backlogs,
    /// Take turns through backlogs, as the worker does; off reproduces the order before
    /// backlog-turns-implementation, for comparison.
    pub(super) backlog_turns: bool,
    /// E, the horizon of the plan behind `pending`.
    pub(super) horizon: Instant,
    pub(super) now: Instant,
    /// The transport's start.
    pub(super) start: Instant,
    /// Check at every pass that every unconsumed clip event due by E is in `pending`.
    pub(super) check_visibility: bool,
    pub(super) sent: Vec<Sent>,
    pub(super) frames: Vec<Frame>,
    /// Decode every frame's datagrams into `values` (off: they're drained unread).
    pub(super) record_values: bool,
    /// With `record_values`, each frame's messages as (address, first float argument), parallel
    /// to `frames`.
    pub(super) values: Vec<Vec<(String, f32)>>,
    /// Passes that held back a due grid frame: (its tick, the pass).
    pub(super) withheld: Vec<(Instant, Instant)>,
    /// Every plan: when, and whether a visibility replan.
    pub(super) plans: Vec<(Instant, bool)>,
}

impl Sim {
    /// `project` with every output's frame grid placed so that a tick falls `lead` before the
    /// transport's start (and so before every even sixteenth at 120 BPM). Stopped, at 50 ms before
    /// the start.
    pub(super) fn new(mut project: Project, lookahead: Duration, lead: Duration) -> Self {
        let mut receivers = Vec::new();
        for output in &mut project.outputs {
            let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
            receiver.set_nonblocking(true).unwrap();
            output.destination_mut().unwrap().port = receiver.local_addr().unwrap().port();
            receivers.push((output.id, receiver));
        }
        let start = Instant::now() + Duration::from_secs(1);
        let planner = Planner::new(project, lookahead).unwrap();
        let clock = InternalClock::new(planner.project.tempo, planner.project.time_signature);
        let mut stages = OutputStages::new(
            &planner.project,
            parameter_controls::controls(&planner.project),
            start - lead,
        );
        stages.diagnostics = true;
        let (queue, events) = mpsc::sync_channel(1 << 16);
        let (commands, _) = CommandSender::channel();
        let resolver = Resolver::spawn(commands).unwrap();
        let mut sender = OscSender::new().unwrap();
        resolver.request(&mut sender, planner.project.configured_destinations());
        let now = start - MS(50);
        stages.sync_automation(&planner, &clock, now);
        Self {
            planner,
            clock,
            stages,
            sender,
            receivers,
            received: HashMap::new(),
            activity: Default::default(),
            tx: EventSink::new(queue),
            events,
            resolver,
            reaper: reaper::Reaper::spawn().unwrap(),
            oneshots: Default::default(),
            inbox: VecDeque::new(),
            hold: Hold::default(),
            effective_floor: commands::EffectiveFloor::default(),
            held_posted: None,
            applied: Vec::new(),
            log: Vec::new(),
            resumable: false,
            frame_packet: DATAGRAM,
            live_timing: false,
            pending: VecDeque::new(),
            backlogs: Default::default(),
            backlog_turns: true,
            horizon: now,
            now,
            start,
            check_visibility: false,
            sent: Vec::new(),
            frames: Vec::new(),
            record_values: false,
            values: Vec::new(),
            withheld: Vec::new(),
            plans: Vec::new(),
        }
    }

    /// Run until `start`, launch scene 1 there (the transport starts with it), and run on.
    pub(super) fn launched(mut self) -> Self {
        let start = self.start;
        self.run_until(start);
        self.now = start;
        self.command(SchedulerCommand::LaunchScene(SceneId(1)));
        self
    }

    /// A command posted now. The worker takes it once nothing is held, stamping its arrival, and
    /// holds it while a clip event is due within `COMMAND_WAIT` of its arrival or an earlier
    /// ordinary frame is owed. Values use max(posting, command floor); manual commands
    /// use first-dequeue admission, and other commands use dequeue time.
    pub(super) fn send(&mut self, command: SchedulerCommand) {
        self.inbox.push_back(command.enqueued(self.now));
    }

    /// Synthetic engine submission with an earlier client receipt, through the
    /// same envelope/dequeue/hold/application path as ordinary posted commands.
    pub(super) fn send_received(&mut self, command: SchedulerCommand, received_at: Instant) {
        self.send(command);
        self.inbox.back_mut().unwrap().submission = Some(crate::engine::Submission {
            context: crate::engine::CommandContext {
                receipt: crate::engine::Receipt {
                    received_at,
                    sequence: None,
                },
                generation: self.start,
                gesture: None,
            },
            submitted_at: self.now,
            identity: 1,
            valid_generation: true,
        });
    }

    /// The worker's command path, applied at once: `pending` clears, and the next pass plans.
    pub(super) fn command(&mut self, command: SchedulerCommand) {
        let mut envelope = command.enqueued(self.now);
        self.effective_floor
            .admit(&mut envelope, self.now, &self.planner, &self.clock);
        self.apply(envelope, self.now);
    }

    fn apply(&mut self, envelope: Envelope, arrived: Instant) {
        let effective = self.effective_floor.advance(&envelope, arrived);
        let admission = envelope.admitted.map(|decision| decision.outcome);
        let command = envelope.command;
        if commands::policy(&command) != commands::Policy::Values {
            self.pending.clear();
            self.backlogs.clear();
        }
        self.applied.push(self.now);
        if let Some(posted) = self.held_posted.take() {
            self.log.push(Applied {
                fire: matches!(command, SchedulerCommand::FireEvent(..)),
                posted,
                arrived,
                applied: self.now,
                effective,
                admission,
            });
        }
        let io = Io {
            sender: &mut self.sender,
            tx: &mut self.tx,
            resolver: &self.resolver,
            reaper: &self.reaper,
            oneshots: &mut self.oneshots,
        };
        assert!(apply(
            command,
            effective,
            &mut self.planner,
            &mut self.clock,
            &mut self.stages,
            io,
        ));
        self.now += PASS;
    }

    pub(super) fn run_until(&mut self, until: Instant) {
        while self.now < until {
            self.pass();
        }
    }

    pub(super) fn plan(&mut self, visibility: bool) {
        let (planned, until) = self.planner.plan_until(&self.clock, self.now);
        self.pending = planned.into();
        self.horizon = until;
        self.plans.push((self.now, visibility));
        self.now += PLAN;
    }

    /// One pass of the worker's main path, as of `worker.rs` after this change.
    pub(super) fn pass(&mut self) {
        self.stages.resumable = self.resumable;
        self.stages.live_timing = self.live_timing;
        for output in self.stages.take_completed() {
            if self.backlog_turns {
                self.backlogs.completed(output, self.now, &self.pending);
            }
        }
        self.backlogs.close_drained(&self.pending);
        let visibility = self.stages.take_replan();
        if visibility || self.pending.is_empty() {
            self.plan(visibility);
        }
        if self.check_visibility {
            self.assert_visible();
        }
        if self.resumable && self.stages.sending() {
            self.turn();
            return;
        }
        let next_send = worker::next_send(&self.pending)
            .into_iter()
            .chain(self.oneshots.deadline())
            .min();
        if self.live_timing && next_send.is_none_or(|due| due > self.now + MS(1)) {
            self.stages.replan_dirty();
        }
        let now = self.now;
        let (inbox, posted) = (&mut self.inbox, &mut self.held_posted);
        let receive = || {
            Ok(inbox.pop_front().map(|mut envelope| {
                *posted = Some(envelope.enqueued_at);
                self.effective_floor
                    .admit(&mut envelope, now, &self.planner, &self.clock);
                (envelope, now)
            }))
        };
        let (stages, project, sender) = (&mut self.stages, &self.planner.project, &self.sender);
        let earlier_frame = |arrived| stages.earlier_frame(project, sender, arrived);
        if let Ok(Some((command, arrived))) = self.hold.next(next_send, earlier_frame, receive) {
            self.apply(command, arrived);
            return;
        }
        let mut jump_applied = false;
        if let Some(event) = self.pending.front().filter(|e| e.at <= self.now)
            && let PlannedAction::Automate { output, fields } = &event.action
        {
            self.stages.jump(*output, fields, event.at);
            jump_applied = true;
        }
        self.dispatch();
        if self.resumable && self.stages.sending() {
            if jump_applied && let Some(event) = self.pending.pop_front() {
                self.planner.consume_with_error(&event);
            }
            return;
        }
        if let Some(shot) = self
            .oneshots
            .pop_due(self.now, self.pending.front().map(|e| e.at))
        {
            self.sent.push(Sent {
                output: shot.output,
                due: shot.at,
                at: self.now,
                manual: true,
            });
            self.now += DATAGRAM;
            return;
        }
        if self.pending.front().is_some_and(|e| e.at <= self.now) {
            let index = self.backlogs.index(&self.pending, self.now).unwrap_or(0);
            let event = self.pending.remove(index).unwrap();
            match &event.action {
                PlannedAction::Send { packet, output } => {
                    self.backlogs.served(*output);
                    if packet.is_some() {
                        self.sent.push(Sent {
                            output: output_of(&event.action),
                            due: event.at,
                            at: self.now,
                            manual: false,
                        });
                        self.now += DATAGRAM;
                    }
                }
                PlannedAction::Automate { output, fields } => {
                    self.sent.push(Sent {
                        output: output_of(&event.action),
                        due: event.at,
                        at: self.now,
                        manual: false,
                    });
                    if !jump_applied {
                        self.stages.jump(*output, fields, event.at);
                        self.dispatch();
                    }
                }
                _ => {}
            }
            let lifecycle = !matches!(
                event.action,
                PlannedAction::Send { .. } | PlannedAction::Automate { .. }
            );
            self.planner.consume_with_error(&event);
            if lifecycle
                && self
                    .stages
                    .automation_changes(&self.planner, &self.clock, event.at)
            {
                self.stages
                    .sync_automation(&self.planner, &self.clock, event.at);
            }
            return;
        }
        // Wait for the next deadline, at most 1 ms; spin while a due frame is held back.
        let deadline = self.stages.deadline(&self.planner.project, &self.sender);
        let held = deadline.is_some_and(|due| due <= self.now);
        let next = [
            self.pending.front().map(|e| e.at),
            self.oneshots.deadline(),
            deadline.filter(|_| !held),
        ]
        .into_iter()
        .flatten()
        .min();
        let most = if held { SPIN } else { MS(1) };
        let step = next.map_or(most, |at| at.saturating_duration_since(self.now).min(most));
        self.now += step.max(PASS);
    }

    /// The worker's turn path while a frame is part-way out: a due clip event on an output not
    /// sending goes between its packets; otherwise the next packet goes.
    fn turn(&mut self) {
        let stages = &self.stages;
        let index = turns::turn_index(&self.pending, self.now, |output| stages.sending_to(output));
        match index.and_then(|index| self.pending.remove(index)) {
            Some(event) => {
                match &event.action {
                    PlannedAction::Send { packet, output } => {
                        self.backlogs.served(*output);
                        if packet.is_some() {
                            self.sent.push(Sent {
                                output: output_of(&event.action),
                                due: event.at,
                                at: self.now,
                                manual: false,
                            });
                            self.now += DATAGRAM;
                        }
                    }
                    PlannedAction::Automate { output, fields } => {
                        self.sent.push(Sent {
                            output: output_of(&event.action),
                            due: event.at,
                            at: self.now,
                            manual: false,
                        });
                        self.stages.jump(*output, fields, event.at);
                        self.dispatch();
                    }
                    _ => {}
                }
                self.planner.consume_with_error(&event);
            }
            None => {
                self.stages.send_next(
                    &self.planner.project,
                    &mut self.sender,
                    &self.activity,
                    &mut self.tx,
                );
                self.now += self.frame_packet;
                self.collect();
            }
        }
        self.now += PASS;
    }

    /// The stages' dispatch with the worker's chain, then each frame's cost.
    pub(super) fn dispatch(&mut self) {
        let now = self.now;
        let chain = worker::chain(&self.pending, &self.oneshots, self.horizon, now, &self.hold);
        self.stages.dispatch(
            &self.planner.project,
            &mut self.sender,
            &self.activity,
            &mut self.tx,
            &chain,
        );
        if let Some(due) = self
            .stages
            .deadline(&self.planner.project, &self.sender)
            .filter(|&due| due <= now)
        {
            self.withheld.push((due, now));
        }
        self.collect();
    }

    /// Frames finished since the last call, with their datagrams. A whole frame's packets cost
    /// their datagrams here; a resumable one's cost one each as they went.
    fn collect(&mut self) {
        let mut buffer = [0; 65536];
        for (output, receiver) in &self.receivers {
            while let Ok(n) = receiver.recv(&mut buffer) {
                if self.record_values {
                    let (_, packet) = rosc::decoder::decode_udp(&buffer[..n]).unwrap();
                    messages_of(packet, self.received.entry(*output).or_default());
                }
            }
        }
        let events: Vec<_> = self.events.try_iter().collect();
        for event in events {
            if let SchedulerEvent::FrameSent {
                output,
                due,
                entry_lateness,
                messages,
                profile,
                ..
            } = event
            {
                if self.record_values {
                    let receiver = &self
                        .receivers
                        .iter()
                        .find(|(id, _)| *id == output)
                        .unwrap()
                        .1;
                    let received = self.received.entry(output).or_default();
                    // Loopback delivery can trail the send under load: wait up to 1 s.
                    let waited = std::time::Instant::now();
                    while received.len() < messages && waited.elapsed() < MS(1000) {
                        if let Ok(n) = receiver.recv(&mut buffer) {
                            let (_, packet) = rosc::decoder::decode_udp(&buffer[..n]).unwrap();
                            messages_of(packet, received);
                        }
                    }
                    assert!(
                        received.len() >= messages,
                        "{} of a frame's {messages} messages arrived (due {:?})",
                        received.len(),
                        due.saturating_duration_since(self.start)
                    );
                    self.values.push(received.drain(..messages).collect());
                }
                if !self.resumable {
                    self.now += DATAGRAM * messages as u32;
                }
                self.frames.push(Frame {
                    output,
                    tick: due,
                    start: due + entry_lateness,
                    end: self.now,
                    start_send: profile.jump.is_some() || due == self.start,
                    profile,
                });
            }
        }
    }

    /// The values the grid frame on `output` at `tick` (after the start) sent, by address.
    pub(super) fn sent_values(&self, output: TrackId, tick: Duration) -> Option<&[(String, f32)]> {
        self.frames
            .iter()
            .zip(&self.values)
            .find(|(f, _)| {
                f.output == output && !f.start_send && (f.tick - self.start).abs_diff(tick) < US(1)
            })
            .map(|(_, values)| values.as_slice())
    }

    /// Every unconsumed clip event due by E is in `pending`, as a fresh plan finds them.
    fn assert_visible(&self) {
        let clip = |e: &&DueEvent| {
            e.at <= self.horizon
                && matches!(
                    e.action,
                    PlannedAction::Send { .. } | PlannedAction::Automate { .. }
                )
        };
        let key = |e: &DueEvent| (e.at, e.voice, e.cursor);
        let (fresh, _) = self.planner.plan_until(&self.clock, self.now);
        let known: Vec<_> = self.pending.iter().filter(clip).map(key).collect();
        let truth: Vec<_> = fresh.iter().filter(clip).map(key).collect();
        assert_eq!(known, truth, "pending at {:?}", self.now - self.start);
    }

    /// Clip events sent, grouped into clusters: in due order, each next one due within
    /// `CLEARANCE` of the one before.
    pub(super) fn clusters(&self) -> Vec<Vec<Sent>> {
        let mut sent = self.sent.clone();
        sent.sort_by_key(|s| s.due);
        let mut clusters: Vec<Vec<Sent>> = Vec::new();
        for s in sent {
            match clusters.last_mut() {
                Some(cluster) if s.due <= cluster.last().unwrap().due + CLEARANCE => {
                    cluster.push(s)
                }
                _ => clusters.push(vec![s]),
            }
        }
        clusters
    }

    /// Grid frames that started between two clip events of one cluster, both sent at or after the
    /// frame's tick (the parts of a cluster sent before the tick don't count), as offsets from
    /// the transport's start.
    pub(super) fn splits(&self) -> Vec<String> {
        let clusters = self.clusters();
        let mut splits = Vec::new();
        for frame in self.frames.iter().filter(|f| !f.start_send) {
            for cluster in &clusters {
                let remaining = cluster.iter().filter(|s| s.at >= frame.tick);
                let before = remaining.clone().any(|s| s.at < frame.start);
                let after = remaining.clone().any(|s| s.at >= frame.start);
                if before && after {
                    splits.push(format!(
                        "frame due {:?} began {:?}, cluster sent {:?}",
                        frame.tick - self.start,
                        frame.start - self.start,
                        cluster
                            .iter()
                            .map(|s| s.at.saturating_duration_since(self.start))
                            .collect::<Vec<_>>()
                    ));
                }
            }
        }
        splits
    }

    /// Passes at or after a held frame's cap that still held it back.
    pub(super) fn overheld(&self) -> Vec<(Duration, Duration)> {
        self.withheld
            .iter()
            .filter(|(tick, at)| *at >= *tick + CAP)
            .map(|(tick, at)| (*tick - self.start, *at - *tick))
            .collect()
    }

    /// Grid frames by tick, after the start.
    pub(super) fn grid_frames(&self, output: TrackId) -> Vec<Frame> {
        self.frames
            .iter()
            .filter(|f| f.output == output && !f.start_send && f.tick > self.start)
            .copied()
            .collect()
    }
}

/// Each message in `packet`, in order, with its first float argument (NaN if none).
fn messages_of(packet: rosc::OscPacket, into: &mut VecDeque<(String, f32)>) {
    match packet {
        rosc::OscPacket::Message(message) => {
            let value = match message.args.first() {
                Some(rosc::OscType::Float(v)) => *v,
                Some(rosc::OscType::Double(v)) => *v as f32,
                _ => f32::NAN,
            };
            into.push_back((message.addr, value));
        }
        rosc::OscPacket::Bundle(bundle) => {
            for packet in bundle.content {
                messages_of(packet, into);
            }
        }
    }
}

/// The output a `Send` or `Automate` goes to.
fn output_of(action: &PlannedAction) -> TrackId {
    match action {
        PlannedAction::Send { output, .. } | PlannedAction::Automate { output, .. } => *output,
        _ => unreachable!("only sends and jumps are recorded"),
    }
}
