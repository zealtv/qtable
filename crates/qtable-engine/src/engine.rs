//! The single command surface shared by headless and interactive clients.
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::edit::{EditReport, ProjectEdit};
use crate::history::{History, Joined, Step};
use crate::model::{
    BeatTime, EventRef, GlobalQuantisation, Project, SceneId, TICKS_PER_BEAT, Tempo, TrackId,
};
use crate::resolver::UnresolvedOutput;
use crate::scheduler::{self, PublishedState, SchedulerHandle};
use crate::status::{EngineStatus, StatusLog};
use crate::supervise::lock_published;

/// Local application receipt, never a sender wall clock or a hardware timestamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub received_at: Instant,
    pub sequence: Option<u64>,
}

/// Names one thing the user is doing: a drag, a held key, a controller's sweep. Commands that
/// carry the same token as the step on top of the history join that step instead of making
/// another. Minted by `Engine::gesture`,
/// so two clients can't collide; a token the top step doesn't carry starts a step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Gesture(u64);

/// Receipt bound to this engine's current project generation. Runtime only.
#[derive(Clone, Copy, Debug)]
pub struct CommandContext {
    pub receipt: Receipt,
    pub(crate) generation: Instant,
    /// The gesture this command belongs to, if its client marks one.
    pub gesture: Option<Gesture>,
}

impl CommandContext {
    /// The same context, marked as part of `gesture`.
    pub fn with_gesture(mut self, gesture: Option<Gesture>) -> Self {
        self.gesture = gesture;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Submission {
    pub context: CommandContext,
    pub submitted_at: Instant,
    // Read only by the launch-admission report, which the `dev-hooks` feature enables.
    #[cfg_attr(not(feature = "dev-hooks"), allow(dead_code))]
    pub identity: u64,
    pub valid_generation: bool,
}

#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum Command {
    LoadProject(Project),
    Edit(ProjectEdit),
    /// Restore one argument, or all overridden arguments when None. Runtime only.
    ReEnableAutomation {
        field: Option<crate::model::FieldRef>,
    },
    Play,
    Stop,
    SetTempo(f64),
    SetGlobalQuantisation(GlobalQuantisation),
    SetFollowActionsEnabled(bool),
    /// Send the event once with its field defaults, including while stopped.
    FireEvent {
        track: TrackId,
        event: EventRef,
    },
    LaunchClip {
        track: TrackId,
        slot: usize,
    },
    LaunchScene {
        scene: SceneId,
    },
    /// Launch a group row with one shared scheduler clock sample.
    LaunchGroupSlot {
        group: TrackId,
        slot: usize,
    },
    StopTrack {
        track: TrackId,
    },
    StopGroup {
        group: TrackId,
    },
    /// Cancel the scene trigger/follow chain, leaving its launched clips alone.
    CancelSceneLaunch,
    StopAll,
    /// Put the set back as it stood before the latest step; nothing happens with no step to
    /// undo. Playback carries on as after an edit made while playing. The report's
    /// `history_step` names the step undone, and is None when there was none.
    Undo,
    /// Take back the latest undo; nothing happens with no undone step to redo. The report's
    /// `history_step` names the step redone, and is None when there was none.
    Redo,
}

#[derive(Debug)]
pub enum EngineError {
    Project(crate::model::ModelError),
    Scheduler(std::io::Error),
    UnknownTrack(TrackId),
    UnknownScene(SceneId),
    UnknownEvent {
        track: TrackId,
        event: EventRef,
    },
    UnknownGroup(TrackId),
    UnknownSlot {
        track: TrackId,
        track_name: String,
        slot: usize,
    },
    EmptySlot {
        track: TrackId,
        track_name: String,
        slot: usize,
    },
    InvalidTempo(crate::model::ModelError),
    InvalidQuantisation(crate::model::ModelError),
}
impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(error) => write!(f, "invalid project: {error}"),
            Self::Scheduler(error) => write!(f, "scheduler error: {error}"),
            Self::UnknownGroup(group) => write!(f, "unknown group with id {}", group.0),
            Self::UnknownScene(scene) => write!(f, "unknown scene with id {}", scene.0),
            Self::UnknownTrack(track) => write!(f, "unknown track with id {}", track.0),
            Self::UnknownEvent { track, event } => write!(
                f,
                "track {} has no event {:?} on device {}",
                track.0, event.key.0, event.device.0
            ),
            Self::UnknownSlot {
                track,
                track_name,
                slot,
            } => write!(
                f,
                "track {track} ({track_name:?}) has no slot {slot}",
                track = track.0
            ),
            Self::EmptySlot {
                track,
                track_name,
                slot,
            } => write!(
                f,
                "slot {slot} on track {track_name:?} (id {}) is empty",
                track.0
            ),
            Self::InvalidTempo(error) => write!(f, "invalid tempo: {error}"),
            Self::InvalidQuantisation(error) => write!(f, "invalid quantisation: {error}"),
        }
    }
}
impl std::error::Error for EngineError {}

#[derive(Clone, Debug)]
pub struct TriggeredSlot {
    pub slot: usize,
    pub launch_beat: BeatTime,
}
#[derive(Clone, Debug)]
pub struct TrackSnapshot {
    pub id: TrackId,
    pub playing_slot: Option<usize>,
    pub triggered: Option<TriggeredSlot>,
    pub play_position_beats: Option<f64>,
    /// How far through the current pass the playing clip has played, 0.0 to 1.0. The first
    /// pass runs from Start to the loop end, so an intro counts as a pass of its own; the
    /// Session grid draws this and works out nothing itself.
    pub pass_progress: Option<f64>,
    pub next_follow_beat: Option<BeatTime>,
    /// Slot the playing clip's follow action will launch next, chosen at launch.
    pub queued_follow_slot: Option<usize>,
    /// True when the resolved route contains no outputs.
    pub empty_route: bool,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub playing: bool,
    pub tempo_bpm: f64,
    pub time_signature: crate::model::TimeSignature,
    pub current_beat: BeatTime,
    pub global_quantisation: GlobalQuantisation,
    /// Track stop boundaries in musical time; empty after dispatch.
    pub pending_stops: Vec<(TrackId, BeatTime)>,
    /// Empty slots whose stop buttons are pending, resolved against current scene order.
    pub pending_stop_slots: Vec<(TrackId, usize)>,
    /// Most recently accepted scene trigger; independent of clip playback.
    pub last_triggered_scene: Option<SceneId>,
    /// Output of the armed scene follow action, chosen at scene launch.
    pub queued_follow_scene: Option<SceneId>,
    pub tracks: Vec<TrackSnapshot>,
    /// Content positions for playing clips and the last position of stopped clips.
    pub clip_positions: Vec<scheduler::ClipPosition>,
    /// The same for each unlinked envelope, on its own region.
    pub lane_positions: Vec<scheduler::LanePosition>,
    /// Runtime effective values; never written into the saved project.
    pub automation: Vec<scheduler::AutomationValue>,
    /// Outputs with no usable address yet; their sends are skipped.
    pub unresolved_outputs: Vec<UnresolvedOutput>,
    /// Outputs that sent a packet in the last 150 ms (for activity meters).
    pub active_outputs: Vec<TrackId>,
    /// Tracks that sent a packet in the last 150 ms (for track activity meters).
    pub active_tracks: Vec<TrackId>,
    /// Send brightness in 0..=1, fading to zero over 330 ms.
    pub output_activity: Vec<(TrackId, f32)>,
    pub track_activity: Vec<(TrackId, f32)>,
    /// Per-destination counts in the preceding second, including Main when configured.
    pub output_throughput: Vec<(TrackId, scheduler::Throughput)>,
    /// Digest of scheduler events drained by `Engine::poll_events`.
    pub status: EngineStatus,
}

/// An engine-validated snapshot containing only the named stored-value changes.
///
/// The payload is opaque to scheduler clients; edits must enter through `Engine::execute`.
/// ```compile_fail
/// use qtable_engine::{engine::ValueEdit, model::Project};
/// let edit = ValueEdit { project: Project::default(), fields: vec![] };
/// ```
#[derive(Clone, Debug)]
pub struct ValueEdit {
    project: Project,
    fields: Vec<crate::model::FieldRef>,
}

impl ValueEdit {
    pub(crate) fn into_parts(self) -> (Project, Vec<crate::model::FieldRef>) {
        (self.project, self.fields)
    }

    #[cfg(test)]
    pub(crate) fn for_test(project: Project, fields: Vec<crate::model::FieldRef>) -> Self {
        Self { project, fields }
    }
}

pub struct Engine {
    generation: Instant,
    submission_identity: u64,
    project: Project,
    scheduler: SchedulerHandle,
    lookahead: Duration,
    diagnostics: bool,
    #[cfg(feature = "dev-hooks")]
    measurement_grid_lead: Option<Duration>,
    published: Arc<Mutex<PublishedState>>,
    log: Mutex<StatusLog>,
    project_revision: u64,
    history: History,
    /// Names the current content; see `Engine::content_id`.
    content_id: u64,
    last_content_id: u64,
    last_gesture: AtomicU64,
}
impl Engine {
    /// A new token for one gesture. Attach it to every command of that gesture with
    /// `CommandContext::with_gesture`, and the content changes they make count as one step.
    pub fn gesture(&self) -> Gesture {
        Gesture(self.last_gesture.fetch_add(1, Ordering::Relaxed) + 1)
    }
    /// Say that no open step takes more commands: a gesture ended. The next command of a token
    /// that had a step starts a new one. Does nothing when no step is open.
    pub fn close_gestures(&mut self) {
        self.history.close_open_run();
    }
    fn apply_parameter_edit(
        &mut self,
        project: Project,
        fields: Vec<crate::model::FieldRef>,
        step: Option<&'static str>,
        gesture: Option<Gesture>,
    ) -> Result<(), EngineError> {
        // `edit::apply` has checked the candidate: structural edits validate
        // the whole project; SetParameter(s) atomically check only named values.
        // Resolution happens on the scheduler's resolver thread; unresolved
        // outputs never block or reject an edit.
        self.scheduler
            .send(if fields.is_empty() {
                scheduler::SchedulerCommand::UpdateProject(project.clone())
            } else {
                scheduler::SchedulerCommand::UpdateParameters(ValueEdit {
                    project: project.clone(),
                    fields,
                })
            })
            .map_err(EngineError::Scheduler)?;
        self.commit(project, step, gesture);
        Ok(())
    }
    /// Make `after` the owned project. A change to the content takes a fresh identity, and a
    /// `step` label also records the set as it was before; an `after` equal to the current
    /// project changes nothing and makes no step; neither does a `step` of `None` (view state,
    /// mappings), which leaves the open step and the redo stack alone. With a `gesture` that the top step
    /// carries, the change joins that step instead; if that leaves the set as the step found it,
    /// the step disappears and the content identity it began with returns.
    fn commit(&mut self, after: Project, step: Option<&'static str>, gesture: Option<Gesture>) {
        let before = std::mem::replace(&mut self.project, after);
        self.bump_revision();
        if before == self.project {
            return;
        }
        if let Some(label) = step {
            // A step-making change removes a mapping only by removing its control (a mapping
            // edit makes no step), so a shorter list is the cheap test for the rows to keep.
            let dropped = if self.project.mappings.len() < before.mappings.len() {
                crate::edit::deleted_mappings(&before, &self.project)
            } else {
                Vec::new()
            };
            match gesture.map(|gesture| self.history.join(gesture, &self.project)) {
                Some(Joined::Step) => {
                    self.history.extend_dropped(dropped);
                    self.content_id = self.fresh_content_id();
                    return;
                }
                Some(Joined::Vanished {
                    before_id,
                    mappings_differ,
                }) => {
                    // The file differs from the one the step began with when mappings
                    // changed meanwhile; those stay outside the history.
                    self.content_id = if mappings_differ {
                        self.fresh_content_id()
                    } else {
                        before_id
                    };
                    return;
                }
                Some(Joined::No) | None => {}
            }
            self.history.record(Step {
                label,
                before: Arc::new(before),
                before_id: self.content_id,
                gesture,
                dropped,
            });
        }
        self.content_id = self.fresh_content_id();
    }
    fn fresh_content_id(&mut self) -> u64 {
        self.last_content_id += 1;
        self.last_content_id
    }
    fn bump_revision(&mut self) {
        self.project_revision = self.project_revision.wrapping_add(1);
    }
    /// A number naming the current content of the set. Every change takes a fresh one, except
    /// that an undo or redo restores the number the snapshot had, so a client can tell "the
    /// set matches the file" by comparing it with the number at the last save. Unlike
    /// `project_revision`, it does not only count up. A fold or an expand makes a fresh one
    /// too, since the saved file would differ.
    pub fn content_id(&self) -> u64 {
        self.content_id
    }
    /// Whether `Command::Undo` would do anything.
    pub fn can_undo(&self) -> bool {
        self.history.peek_undo().is_some()
    }
    /// Whether `Command::Redo` would do anything.
    pub fn can_redo(&self) -> bool {
        self.history.peek_redo().is_some()
    }
    /// The name of the step an undo would take back, like "Rename Clip".
    pub fn undo_label(&self) -> Option<&'static str> {
        self.history.peek_undo().map(|step| step.label)
    }
    /// The name of the step a redo would put back.
    pub fn redo_label(&self) -> Option<&'static str> {
        self.history.peek_redo().map(|step| step.label)
    }
    /// Counts successful project mutations (starting at 0), including loads.
    /// Clients key caches and dirty tracking on it instead of bumping their own.
    pub fn project_revision(&self) -> u64 {
        self.project_revision
    }
    /// The project currently owned by this engine.
    pub fn project(&self) -> &Project {
        &self.project
    }

    pub fn new(project: Project) -> Result<Self, EngineError> {
        Self::with_diagnostics(project, false)
    }

    /// Enable send-side timing events for loopback measurements.
    pub fn with_diagnostics(project: Project, diagnostics: bool) -> Result<Self, EngineError> {
        Self::configured(
            project,
            diagnostics,
            #[cfg(feature = "dev-hooks")]
            None,
        )
    }

    /// Start a scheduler thread; a measurement's grid lead picks its phase-controlled variant.
    fn spawn_scheduler(
        project: Project,
        lookahead: Duration,
        diagnostics: bool,
        #[cfg(feature = "dev-hooks")] grid_lead: Option<Duration>,
    ) -> std::io::Result<scheduler::SchedulerHandle> {
        #[cfg(feature = "dev-hooks")]
        if let Some(lead) = grid_lead {
            return scheduler::spawn_with_grid_lead(project, lookahead, lead);
        }
        scheduler::spawn_with_diagnostics(project, lookahead, diagnostics)
    }

    /// Measurement host: use the scheduler's existing phase-controlled frame grid and
    /// command-application timestamps. Ordinary clients use `new`/`with_diagnostics`.
    /// Only with the `dev-hooks` feature, which release builds leave off.
    #[cfg(feature = "dev-hooks")]
    pub fn with_grid_lead(project: Project, lead: Duration) -> Result<Self, EngineError> {
        Self::configured(project, true, Some(lead))
    }

    fn configured(
        mut project: Project,
        diagnostics: bool,
        #[cfg(feature = "dev-hooks")] grid_lead: Option<Duration>,
    ) -> Result<Self, EngineError> {
        project.normalise_main();
        project.validate_stored().map_err(EngineError::Project)?;
        let lookahead = Duration::from_millis(12);
        let scheduler = Self::spawn_scheduler(
            project.clone(),
            lookahead,
            diagnostics,
            #[cfg(feature = "dev-hooks")]
            grid_lead,
        )
        .map_err(EngineError::Scheduler)?;
        let published = Arc::clone(&scheduler.snapshot);
        Ok(Self {
            generation: Instant::now(),
            submission_identity: 0,
            project,
            scheduler,
            lookahead,
            diagnostics,
            #[cfg(feature = "dev-hooks")]
            measurement_grid_lead: grid_lead,
            published,
            log: Mutex::new(StatusLog::default()),
            project_revision: 0,
            history: History::default(),
            content_id: 0,
            last_content_id: 0,
            last_gesture: AtomicU64::new(0),
        })
    }

    /// Scheduler notifications, including optional send-side timing.
    pub fn scheduler_events(&self) -> &std::sync::mpsc::Receiver<scheduler::SchedulerEvent> {
        &self.scheduler.events
    }

    /// Drain queued scheduler events into the status digest. Clients call this
    /// each frame or prompt; events nobody drains are dropped once the queue fills.
    /// Tests that read `scheduler_events()` directly should not call it.
    pub fn poll_events(&self) {
        let mut log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        while let Ok(event) = self.scheduler.events.try_recv() {
            log.record(event);
        }
    }

    pub fn track_id_by_name(&self, name: &str) -> Option<TrackId> {
        self.project
            .clip_tracks()
            .find(|track| track.name == name)
            .map(|track| track.id)
    }

    /// Commands enqueue scheduler changes; owned project data updates immediately.
    /// Playback snapshots catch up asynchronously, and scheduler failures surface
    /// in status. Loading starts the replacement scheduler before dropping the old
    /// one, so their threads can briefly overlap (neither owns an exclusive socket).
    /// Run one command. An edit's report says what it removed or converted beyond what it
    /// named; every other command reports nothing.
    pub fn execute(&mut self, command: Command) -> Result<EditReport, EngineError> {
        let context = self.command_context(Receipt {
            received_at: Instant::now(),
            sequence: None,
        });
        self.execute_with_context(command, context)
    }

    pub fn command_context(&self, receipt: Receipt) -> CommandContext {
        CommandContext {
            receipt,
            generation: self.generation,
            gesture: None,
        }
    }

    /// Preserve local receipt through client selection, parsing and validation work.
    pub fn execute_with_context(
        &mut self,
        command: Command,
        context: CommandContext,
    ) -> Result<EditReport, EngineError> {
        let submitted_at = Instant::now();
        self.submission_identity = self.submission_identity.wrapping_add(1);
        let submission = Submission {
            context,
            submitted_at,
            identity: self.submission_identity,
            valid_generation: context.generation == self.generation
                && context.receipt.received_at >= self.generation,
        };
        match command {
            Command::ReEnableAutomation { field } => {
                self.scheduler
                    .send(scheduler::SchedulerCommand::ReEnableAutomation(field))
                    .map_err(EngineError::Scheduler)?;
            }
            Command::Edit(edit) => {
                let fields = match &edit {
                    ProjectEdit::SetParameter { target, values } => values
                        .iter()
                        .map(|(field, _)| crate::model::FieldRef {
                            parameter: target.clone(),
                            field: field.clone(),
                        })
                        .collect(),
                    ProjectEdit::SetParameters(parameters) => parameters
                        .iter()
                        .flat_map(|(target, values)| {
                            values.iter().map(|(field, _)| crate::model::FieldRef {
                                parameter: target.clone(),
                                field: field.clone(),
                            })
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                let step = edit.makes_step().then(|| edit.label());
                let mut project = self.project.clone();
                let report =
                    crate::edit::apply(&mut project, edit).map_err(EngineError::Project)?;
                self.apply_parameter_edit(project, fields, step, context.gesture)?;
                return Ok(report);
            }
            Command::LoadProject(mut project) => {
                project.normalise_main();
                project.validate_stored().map_err(EngineError::Project)?;
                let scheduler = Self::spawn_scheduler(
                    project.clone(),
                    self.lookahead,
                    self.diagnostics,
                    #[cfg(feature = "dev-hooks")]
                    self.measurement_grid_lead,
                )
                .map_err(EngineError::Scheduler)?;
                self.published = Arc::clone(&scheduler.snapshot);
                self.scheduler = scheduler;
                self.generation = Instant::now();
                *self.log.lock().unwrap_or_else(|e| e.into_inner()) = StatusLog::default();
                self.project = project;
                self.history.clear();
                self.content_id = self.fresh_content_id();
                self.bump_revision();
            }
            Command::Undo => return self.step_history(restore::Direction::Undo),
            Command::Redo => return self.step_history(restore::Direction::Redo),
            Command::SetTempo(bpm) => {
                let tempo = Tempo::new(bpm).map_err(EngineError::InvalidTempo)?;
                self.scheduler
                    .send(scheduler::SchedulerCommand::SetTempo(tempo))
                    .map_err(EngineError::Scheduler)?;
                // Keep the owned project current so a save writes the new tempo.
                let mut project = self.project.clone();
                project.tempo = tempo;
                self.commit(project, Some("Set Tempo"), context.gesture);
            }
            Command::SetFollowActionsEnabled(value) => {
                self.scheduler
                    .send(scheduler::SchedulerCommand::SetFollowActionsEnabled(value))
                    .map_err(EngineError::Scheduler)?;
                let mut project = self.project.clone();
                project.follow_actions_enabled = value;
                self.commit(project, Some("Set Follow Actions"), context.gesture);
            }
            Command::SetGlobalQuantisation(value) => {
                value.validate().map_err(EngineError::InvalidQuantisation)?;
                self.scheduler
                    .send(scheduler::SchedulerCommand::SetGlobalQuantisation(value))
                    .map_err(EngineError::Scheduler)?;
                let mut project = self.project.clone();
                project.global_quantisation = value;
                self.commit(project, Some("Set Global Quantisation"), context.gesture);
            }
            Command::FireEvent { track, event } => {
                if self
                    .project
                    .track(track)
                    .and_then(|t| t.event_def(&event))
                    .is_none()
                {
                    return Err(EngineError::UnknownEvent { track, event });
                }
                self.scheduler
                    .send(scheduler::SchedulerCommand::FireEvent(track, event))
                    .map_err(EngineError::Scheduler)?;
            }
            Command::LaunchClip { track, slot } => {
                self.validate_slot_launch(track, slot)?;
                self.scheduler
                    .send_with_context(scheduler::SchedulerCommand::Launch(track, slot), submission)
                    .map_err(EngineError::Scheduler)?;
            }
            Command::LaunchGroupSlot { group, slot } => {
                let members = self
                    .project
                    .descendant_clip_track_ids(group)
                    .ok_or(EngineError::UnknownGroup(group))?;
                // Validate the complete batch before enqueuing any launch/stop.
                for track in members {
                    self.validate_slot_launch(track, slot)?;
                }
                self.scheduler
                    .send_with_context(
                        scheduler::SchedulerCommand::LaunchGroupSlot(group, slot),
                        submission,
                    )
                    .map_err(EngineError::Scheduler)?;
            }
            Command::LaunchScene { scene } => {
                if !self.project.scenes.iter().any(|s| s.id == scene) {
                    return Err(EngineError::UnknownScene(scene));
                }
                self.scheduler
                    .send_with_context(scheduler::SchedulerCommand::LaunchScene(scene), submission)
                    .map_err(EngineError::Scheduler)?;
            }
            Command::CancelSceneLaunch => self
                .scheduler
                .send(scheduler::SchedulerCommand::CancelSceneLaunch)
                .map_err(EngineError::Scheduler)?,
            Command::StopGroup { group } => {
                if self.project.descendant_clip_track_ids(group).is_none() {
                    return Err(EngineError::UnknownGroup(group));
                }
                self.scheduler
                    .send_with_context(scheduler::SchedulerCommand::StopGroup(group), submission)
                    .map_err(EngineError::Scheduler)?;
            }
            Command::StopTrack { track } => {
                if self.project.track(track).is_none() {
                    return Err(EngineError::UnknownTrack(track));
                }
                self.scheduler
                    .send_with_context(scheduler::SchedulerCommand::StopTrack(track), submission)
                    .map_err(EngineError::Scheduler)?;
            }
            Command::Play => self
                .scheduler
                .send(scheduler::SchedulerCommand::Play)
                .map_err(EngineError::Scheduler)?,
            Command::Stop => self
                .scheduler
                .send(scheduler::SchedulerCommand::Stop)
                .map_err(EngineError::Scheduler)?,
            Command::StopAll => self
                .scheduler
                .send_with_context(scheduler::SchedulerCommand::StopAll, submission)
                .map_err(EngineError::Scheduler)?,
        }
        Ok(EditReport::default())
    }

    fn validate_slot_launch(&self, track: TrackId, slot: usize) -> Result<(), EngineError> {
        let data = self
            .project
            .clip_tracks()
            .find(|t| t.id == track)
            .ok_or(EngineError::UnknownTrack(track))?;
        data.slots
            .get(slot)
            .ok_or_else(|| EngineError::UnknownSlot {
                track,
                track_name: data.name.clone(),
                slot,
            })?;
        Ok(())
    }

    pub fn snapshot(&self) -> Snapshot {
        let state = lock_published(&self.published).clone();
        let now = Instant::now();
        let ticks = if state.playing {
            let elapsed = now.saturating_duration_since(state.anchor).as_secs_f64();
            let advance = (elapsed * state.tempo.bpm() / 60.0 * TICKS_PER_BEAT as f64).floor();
            state
                .beat
                .ticks()
                .saturating_add(advance.max(0.0).min(u64::MAX as f64) as u64)
        } else {
            state.beat.ticks()
        };
        let current = BeatTime(ticks);
        let activity = self.scheduler.activity_levels(now);
        let mut clip_positions = state.clip_positions.clone();
        let mut lane_positions = state.lane_positions.clone();
        for voice in state.tracks.iter().filter(|v| v.launch <= current) {
            let at = state
                .pending_stops
                .iter()
                .filter(|(id, _)| *id == voice.track)
                .fold(current, |at, (_, stop)| at.min(*stop));
            voice.remember_at(at, &mut clip_positions, &mut lane_positions);
        }
        let tracks = self
            .project
            .clip_tracks()
            .map(|track| {
                let voices: Vec<_> = state
                    .tracks
                    .iter()
                    .filter(|v| v.track == track.id)
                    .collect();
                let playing = voices
                    .iter()
                    .filter(|v| {
                        v.launch <= current
                            && v.cutoff.is_none_or(|end| end > current)
                            && !state
                                .pending_stops
                                .iter()
                                .any(|(id, beat)| *id == track.id && *beat <= current)
                    })
                    .max_by_key(|v| v.launch);
                let triggered = voices
                    .iter()
                    .filter(|v| v.launch > current)
                    .min_by_key(|v| v.launch)
                    .map(|v| TriggeredSlot {
                        slot: v.slot,
                        launch_beat: v.launch,
                    });
                let position =
                    playing.map(|v| v.position_at(current).ticks() as f64 / TICKS_PER_BEAT as f64);
                let pass_progress = playing.map(|v| v.pass_progress_at(current));
                TrackSnapshot {
                    empty_route: self.project.resolved_route(track.id).is_empty(),
                    id: track.id,
                    playing_slot: playing.map(|v| v.slot),
                    triggered,
                    play_position_beats: position,
                    pass_progress,
                    next_follow_beat: playing.and_then(|v| v.next_follow_beat),
                    queued_follow_slot: playing.and_then(|v| v.queued_follow_slot),
                }
            })
            .collect();
        Snapshot {
            playing: state.playing,
            tempo_bpm: state.tempo.bpm(),
            time_signature: state.signature,
            current_beat: current,
            global_quantisation: state.global_quantisation,
            pending_stops: state.pending_stops,
            pending_stop_slots: state.pending_stop_slots,
            last_triggered_scene: state.last_triggered_scene,
            queued_follow_scene: state.queued_follow_scene,
            tracks,
            clip_positions,
            lane_positions,
            automation: state.automation,
            unresolved_outputs: self.scheduler.unresolved_outputs(),
            active_outputs: self.scheduler.active_outputs(),
            active_tracks: self.scheduler.active_tracks(),
            output_activity: activity.outputs,
            track_activity: activity.tracks,
            output_throughput: self.scheduler.output_throughput(now),
            status: self
                .log
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .status(state.failure),
        }
    }
}

mod restore;

#[cfg(test)]
mod override_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measurement_host_exposes_application_timestamps_after_project_reload() {
        use crate::scheduler::SchedulerEvent;
        let mut engine =
            Engine::with_grid_lead(Project::default(), Duration::from_micros(50)).unwrap();
        engine
            .execute(Command::LoadProject(Project::default()))
            .unwrap();
        let before = Instant::now();
        engine.execute(Command::SetTempo(121.0)).unwrap();
        let until = before + Duration::from_secs(2);
        loop {
            match engine
                .scheduler_events()
                .recv_timeout(until.saturating_duration_since(Instant::now()))
                .unwrap()
            {
                SchedulerEvent::CommandApplied { arrived, applied } => {
                    assert!(before <= arrived && arrived <= applied);
                    break;
                }
                SchedulerEvent::Error(error) => panic!("{error}"),
                _ => {}
            }
        }
    }
    #[test]
    fn parameter_commands_mark_the_narrow_path_without_a_full_sync() {
        parameter_command_marks(false);
    }
    #[test]
    fn parameter_batches_use_one_narrow_command_without_a_full_sync() {
        parameter_command_marks(true);
    }
    fn parameter_command_marks(batch: bool) {
        use crate::edit::ProjectEdit;
        use crate::model::*;
        use crate::scheduler::SchedulerEvent;
        use qtable_test_support::TestReceiver;
        let receiver = TestReceiver::bind().unwrap();
        let project = Project {
            tracks: vec![
                Track::new(TrackId(1), "track", 0),
                Track::new(TrackId(2), "other", 0),
            ],
            outputs: vec![Track::new_output(
                TrackId(201),
                "test",
                "127.0.0.1",
                receiver.addr().port(),
            )],
            ..Project::default()
        };
        let field = ArgumentRef::volume().on(TrackId(1));
        let mut engine = Engine::new(project).unwrap();
        engine.scheduler =
            scheduler::spawn_with_marks(engine.project.clone(), Duration::from_millis(5)).unwrap();
        engine.published = Arc::clone(&engine.scheduler.snapshot);
        let edit = if batch {
            ProjectEdit::SetParameters(vec![
                (field.parameter, vec![(field.field, 0.4)]),
                (
                    ParameterRef::volume(TrackId(2)),
                    vec![(FieldKey("value".into()), 0.6)],
                ),
            ])
        } else {
            ProjectEdit::SetParameter {
                target: field.parameter,
                values: vec![(field.field, 0.4)],
            }
        };
        engine.execute(Command::Edit(edit)).unwrap();
        let until = Instant::now() + Duration::from_secs(1);
        let mut labels = Vec::new();
        while !labels.contains(&"command.end") {
            match engine
                .scheduler_events()
                .recv_timeout(until.saturating_duration_since(Instant::now()))
                .unwrap()
            {
                SchedulerEvent::Marks(batch) => {
                    labels.extend(batch.into_iter().map(|(label, _)| label))
                }
                SchedulerEvent::Error(error) => panic!("{error}"),
                _ => {}
            }
        }
        let start = labels
            .iter()
            .position(|label| *label == "command.begin")
            .unwrap();
        let labels = &labels[start..];
        let begin = labels
            .iter()
            .position(|label| *label == "parameters.begin")
            .unwrap();
        let end = labels
            .iter()
            .position(|label| *label == "parameters.end")
            .unwrap();
        assert!(begin < end);
        assert_eq!(
            labels
                .iter()
                .filter(|label| **label == "parameters.begin")
                .count(),
            1
        );
        assert_eq!(
            labels
                .iter()
                .filter(|label| **label == "command.begin")
                .count(),
            1
        );
        assert!(!labels.contains(&"sync.begin"), "{labels:?}");
    }
}
