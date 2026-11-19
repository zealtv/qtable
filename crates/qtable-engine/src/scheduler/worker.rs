//! Playback thread, command handling and scheduler diagnostics.
use super::*;

#[derive(Clone, Debug)]
pub enum SchedulerCommand {
    Launch(TrackId, usize),
    FireEvent(TrackId, crate::model::EventRef),
    LaunchScene(SceneId),
    CancelSceneLaunch,
    LaunchGroupSlot(TrackId, usize),
    StopTrack(TrackId),
    StopGroup(TrackId),
    StopAll,
    Play,
    Stop,
    SetTempo(Tempo),
    SetGlobalQuantisation(GlobalQuantisation),
    SetFollowActionsEnabled(bool),
    UpdateProject(Project),
    /// Engine-validated snapshot differing only in the named stored values. Structural,
    /// routing, definition and output-setting edits must use UpdateProject instead.
    UpdateParameters(crate::engine::ValueEdit),
    ReEnableAutomation(Option<crate::model::FieldRef>),
    /// From the resolver thread: an output's endpoint is now known.
    Resolved(crate::model::Track, SocketAddr),
    Shutdown,
}

// Boxing `FrameSent`'s profile would allocate on the scheduler thread for every diagnosed frame,
// and change what measurements compare; the events only pass through a bounded channel.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum SchedulerEvent {
    TrackSilent(TrackId),
    /// Bounded by the ordinary event queue; loss is counted by EventsDropped.
    LaunchAdmission {
        received: Instant,
        enqueued: Instant,
        dequeued: Instant,
        effective: Instant,
        submission: Option<u64>,
        sequence: Option<u64>,
        outcome: AdmissionOutcome,
    },
    /// First message too large for a bundle from this output in this session.
    OutputOversize {
        output: TrackId,
        name: String,
    },
    /// Optional timing for one parameter frame (including off-grid transport start).
    FrameSent {
        output: TrackId,
        due: Instant,
        lateness: Duration,
        /// Time spent before this output stage began dispatching the frame.
        entry_lateness: Duration,
        messages: usize,
        profile: FrameProfile,
    },
    /// Transport anchor, also useful for measuring intended deadlines in tests.
    Playing(Instant),
    Error(io::Error),
    /// Events the full queue had to drop since the previous report of this kind.
    EventsDropped {
        count: u64,
    },
    /// Aggregated count of stale OSC packets discarded during catch-up.
    DroppedLateSends {
        count: u64,
    },
    /// Optional send-side timing, captured immediately before calling the sender.
    Sent {
        due: Instant,
        lateness: Duration,
    },
    /// Diagnostics (`dev-hooks` feature): named instants on the scheduler thread (command
    /// handling, automation sync, planning), oldest first. Drained while no send is due within 1 ms.
    #[cfg(feature = "dev-hooks")]
    Marks(Vec<(&'static str, Instant)>),
    /// Measurements only (`dev-hooks` feature; `spawn_with_grid_lead`, `spawn_with_marks`): a
    /// command taken off the channel at `arrived` began to apply at `applied`.
    #[cfg(feature = "dev-hooks")]
    CommandApplied {
        arrived: Instant,
        applied: Instant,
    },
}

/// Phase boundaries of one diagnosed frame, as offsets after its due time. Completion is
/// the frame's `lateness`; dispatch entry is its `entry_lateness`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameProfile {
    /// The exact jump that made this send due: slot lookup's start and end.
    pub jump: Option<(Duration, Duration)>,
    /// The pass that let the frame go (observed; a busy worker can reach it after the cap).
    pub released: Duration,
    /// The last pass that held a grid frame back for a cluster (the cap: under 4 ms).
    pub withheld_last: Option<Duration>,
    pub began: Duration,
    /// Every considered slot sampled and prepared.
    pub prepared: Duration,
    /// Budget selection done and messages built, just before transmission.
    pub built: Duration,
    pub send: crate::sender::SendTiming,
    pub slots: usize,
    /// Slots the frame considered and sampled; an exact jump considers only its own.
    pub considered: usize,
    pub sampled: usize,
    /// Values the frame found overwritten before any frame sent them.
    pub merged: u64,
}

/// Capacity of the scheduler's event queue; events beyond it are dropped.
const EVENT_QUEUE: usize = 4096;
/// With phase marks: a worker pass this long after the last is recorded as a gap.
const GAP: Duration = Duration::from_micros(20);
/// With phase marks: housekeeping between deadlines is recorded when it takes this long.
const HOUSEKEEPING: Duration = Duration::from_micros(10);

pub(super) fn next_send(pending: &VecDeque<DueEvent>) -> Option<Instant> {
    pending
        .iter()
        .find(|event| {
            matches!(
                event.action,
                PlannedAction::Send { .. } | PlannedAction::Automate { .. }
            )
        })
        .map(|event| event.at)
}

/// The remaining clip events at `now` (planned `Send`s and `Automate`s, then queued manual hits),
/// as `cluster::release` judges a due grid frame against them, with the horizon of the plan
/// behind `pending`.
pub(super) fn chain(
    pending: &VecDeque<DueEvent>,
    oneshots: &oneshots::Queue,
    horizon: Instant,
    now: Instant,
    held: &commands::Hold,
) -> output_stage::cluster::Chain {
    let planned = pending
        .iter()
        .filter(|event| {
            matches!(
                event.action,
                PlannedAction::Send { .. } | PlannedAction::Automate { .. }
            )
        })
        .map(|event| event.at);
    output_stage::cluster::Chain::summarise(planned, oneshots.due_times(), horizon, now)
        .yielding(held.yield_from())
}

pub struct SchedulerHandle {
    commands: CommandSender,
    pub events: Receiver<SchedulerEvent>,
    worker: Option<JoinHandle<()>>,
    pub snapshot: Arc<Mutex<PublishedState>>,
    unresolved: crate::resolver::Status,
    activity: Arc<crate::activity::Activity>,
    throughput: Arc<output_stage::throughput::Published>,
}

impl SchedulerHandle {
    /// Outputs the scheduler cannot send to yet, with the reason.
    pub fn unresolved_outputs(&self) -> Vec<crate::resolver::UnresolvedOutput> {
        crate::resolver::list(&self.unresolved)
    }
}

impl SchedulerHandle {
    pub fn output_throughput(&self, now: Instant) -> Vec<(TrackId, super::Throughput)> {
        self.throughput.read(now)
    }

    pub fn activity_levels(&self, now: Instant) -> crate::activity::Levels {
        self.activity.levels(now)
    }

    /// Outputs that sent a packet recently.
    pub fn active_outputs(&self) -> Vec<crate::model::TrackId> {
        self.activity.active(Instant::now())
    }

    /// Tracks that sent a packet recently.
    pub fn active_tracks(&self) -> Vec<crate::model::TrackId> {
        self.activity.active_tracks(Instant::now())
    }

    pub(crate) fn send_with_context(
        &self,
        command: SchedulerCommand,
        context: crate::engine::Submission,
    ) -> io::Result<()> {
        self.commands.send_with_context(command, Some(context))
    }

    pub fn send(&self, command: SchedulerCommand) -> io::Result<()> {
        self.commands.send(command)
    }
}
impl Drop for SchedulerHandle {
    fn drop(&mut self) {
        let _ = self.commands.send(SchedulerCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Create the playback thread and request asynchronous endpoint resolution.
/// Coarse waits use recv_timeout (commands interrupt them); the last 6 ms
/// before a clip send and the last 1 ms before an output frame spin while
/// polling commands. Clip events due soon go before a frame or a command
/// that would delay them. Planned candidates are cached until dispatch
/// or a command invalidates them. macOS uses USER_INTERACTIVE thread QoS.
/// This is best-effort timing, not a realtime OS.
pub fn spawn(project: Project, lookahead: Duration) -> io::Result<SchedulerHandle> {
    spawn_with_diagnostics(project, lookahead, false)
}

/// Enable per-packet send timing for measurements; ordinary playback emits no diagnostics.
pub fn spawn_with_diagnostics(
    project: Project,
    lookahead: Duration,
    diagnostics: bool,
) -> io::Result<SchedulerHandle> {
    spawn_configured(
        project,
        lookahead,
        measurements::Diagnostics::packets(diagnostics),
    )
}

mod control;
mod dispatch;
mod playback;

/// State owned exclusively by the playback thread. Clients communicate through WorkerIo.
struct Worker {
    planner: Planner,
    sender: OscSender,
    clock: InternalClock,
    io: WorkerIo,
    #[cfg(feature = "dev-hooks")]
    diagnostics: measurements::Diagnostics,
    pending: VecDeque<DueEvent>,
    backlogs: turns::Backlogs,
    held: commands::Hold,
    effective_floor: commands::EffectiveFloor,
    oneshots: oneshots::Queue,
    report_gate: ReportGate,
    late_sends: late::LateSends,
    publisher: publish::Publisher,
    stages: output_stage::OutputStages,
    automation_published: Instant,
    /// Horizon of the latest empty plan, until a command invalidates it.
    quiet_until: Option<Instant>,
    /// Every event since the last command has been dispatched when pending empties.
    plan_consumed: bool,
    /// E: all unconsumed clip events due by this horizon are in pending.
    horizon: Instant,
}

struct WorkerIo {
    rx: Receiver<Envelope>,
    resolver: Resolver,
    reaper: reaper::Reaper,
    tx: EventSink,
    snapshot: Arc<Mutex<PublishedState>>,
    activity: Arc<crate::activity::Activity>,
    throughput: Arc<output_stage::throughput::Published>,
}

impl Worker {
    fn new(
        planner: Planner,
        sender: OscSender,
        clock: InternalClock,
        io: WorkerIo,
        diagnostics: measurements::Diagnostics,
    ) -> Self {
        #[cfg(feature = "dev-hooks")]
        diagnostics.start();
        let mut stages = output_stage::OutputStages::for_playback(
            &planner.project,
            parameter_controls::controls(&planner.project),
            Instant::now(),
        );
        diagnostics.configure(&mut stages);
        io.reaper
            .retire(stages.sync_automation(&planner, &clock, Instant::now()));
        let automation_published = Instant::now();
        let late_sends = late::LateSends::new(&planner.project);
        Self {
            planner,
            sender,
            clock,
            io,
            #[cfg(feature = "dev-hooks")]
            diagnostics,
            stages,
            late_sends,
            automation_published,
            pending: VecDeque::new(),
            backlogs: turns::Backlogs::default(),
            held: commands::Hold::default(),
            effective_floor: commands::EffectiveFloor::default(),
            oneshots: oneshots::Queue::default(),
            report_gate: ReportGate::default(),
            publisher: publish::Publisher::default(),
            quiet_until: None,
            plan_consumed: true,
            horizon: Instant::now(),
        }
    }
}

pub(super) fn spawn_configured(
    project: Project,
    lookahead: Duration,
    diagnostics: measurements::Diagnostics,
) -> io::Result<SchedulerHandle> {
    let planner = Planner::new(project, lookahead.max(Duration::from_millis(12)))?;
    let mut sender = OscSender::new()?;
    let (commands, rx) = CommandSender::channel();
    let resolver = Resolver::spawn(commands.clone())?;
    let reaper = reaper::Reaper::spawn()?;
    let unresolved = resolver.status();
    let activity = Arc::new(crate::activity::Activity::default());
    let worker_activity = Arc::clone(&activity);
    let throughput = Arc::new(output_stage::throughput::Published::default());
    let worker_throughput = Arc::clone(&throughput);
    resolver.request(&mut sender, planner.project.configured_destinations());
    let clock = InternalClock::new(planner.project.tempo, planner.project.time_signature);
    let (queue, events) = mpsc::sync_channel(EVENT_QUEUE);
    let tx = events::EventSink::new(queue);
    let snapshot = Arc::new(Mutex::new(PublishedState {
        playing: false,
        tempo: planner.project.tempo,
        signature: planner.project.time_signature,
        global_quantisation: planner.project.global_quantisation,
        pending_stops: Vec::new(),
        pending_stop_slots: Vec::new(),
        last_triggered_scene: None,
        queued_follow_scene: None,
        beat: BeatTime::ZERO,
        anchor: Instant::now(),
        tracks: Vec::new(),
        clip_positions: Vec::new(),
        lane_positions: Vec::new(),
        automation: Vec::new(),
        failure: None,
    }));
    let worker_snapshot = Arc::clone(&snapshot);
    let worker = thread::Builder::new()
        .name("qtable-scheduler".into())
        .spawn(move || {
            let mut failure_events = tx.share();
            let failure_state = Arc::clone(&worker_snapshot);
            let outcome = catch_unwind(AssertUnwindSafe(move || {
                #[cfg(target_os = "macos")]
                realtime::configure();
                Worker::new(
                    planner,
                    sender,
                    clock,
                    WorkerIo {
                        rx,
                        resolver,
                        reaper,
                        tx,
                        snapshot: worker_snapshot,
                        activity: worker_activity,
                        throughput: worker_throughput,
                    },
                    diagnostics,
                )
                .run();
            }));
            if let Err(payload) = outcome {
                report_panic(payload.as_ref(), &mut failure_events, &failure_state);
            }
        })?;
    Ok(SchedulerHandle {
        commands,
        unresolved,
        activity,
        throughput,
        events,
        worker: Some(worker),
        snapshot,
    })
}
