//! Pure musical state, candidate generation and speculative consumption.
use super::*;

#[derive(Clone, Debug)]
pub enum PlannedAction {
    /// Complete parameter messages at an exact envelope discontinuity, before tied hits.
    Automate {
        output: TrackId,
        fields: Vec<crate::model::FieldRef>,
    },
    Send {
        /// Looked up in the project at dispatch, so the send follows edits to the output.
        output: TrackId,
        /// None for a hit whose event doesn't resolve or whose device is off: it is still
        /// consumed on time, so the voice keeps its place and sends nothing.
        packet: Option<OscPacket>,
    },
    TrackSilent(TrackId),
    /// Retire a cut-off voice after its positively delayed tail, including dummy clips.
    RetireVoice,
    Follow {
        /// Original timer position, retained to validate consumption.
        beat: BeatTime,
        /// Timer position, or the current beat when stale; clamped timers are spent.
        launch_beat: BeatTime,
        signature: crate::model::TimeSignature,
    },
    SceneFollow {
        scene: SceneId,
        /// Original timer position, retained to validate consumption.
        beat: BeatTime,
        /// Timer position, or the current beat when stale; clamped timers are spent.
        launch_beat: BeatTime,
        tempo: Tempo,
        signature: crate::model::TimeSignature,
    },
    /// A global-quantised track stop, independent of track delay.
    StopTrack(TrackId),
}

#[derive(Clone, Debug)]
pub struct DueEvent {
    /// Original deadline; mildly overdue sends run immediately, stale sends are dropped.
    pub at: Instant,
    pub action: PlannedAction,
    pub(super) voice: u64,
    pub(super) cursor: u64,
}

/// A quantised stop and the empty slot that requested it, when there is one.
#[derive(Clone, Copy, Debug)]
pub(super) struct PendingStop {
    pub beat: BeatTime,
    // Scene identity keeps the indication attached to its slot across row edits.
    pub scene: Option<SceneId>,
}

#[derive(Clone)]
pub(super) struct Voice {
    pub(super) id: u64,
    pub(super) track: TrackId,
    pub(super) clip: ClipId,
    pub(super) slot: usize,
    pub(super) launch: BeatTime,
    /// Where elapsed clip time begins; before `launch` for a legato take-over.
    pub(super) origin: BeatTime,
    /// Legacy pass count; live Loop-off uses a content-position continuation.
    pub(super) passes: u64,
    pub(super) continuation: Option<timeline::Continuation>,
    /// Origins inherited by lanes unlinked during playback; others take the voice's `origin`.
    pub(super) envelope_origins: HashMap<crate::model::ArgumentRef, BeatTime>,
    pub(super) envelope_continuations: HashMap<crate::model::ArgumentRef, timeline::Continuation>,
    pub(super) cutoff: Option<BeatTime>,
    /// Musical cutoff retained across discarded clock segments and transport stops.
    pub(super) cutoff_deadline: Option<Instant>,
    pub(super) cursor: u64,
    pub(super) jumps: Arc<jumps::Jumps>,
    pub(super) jump_cursors: HashMap<TrackId, BeatTime>,
    pub(super) output_cursors: HashMap<TrackId, u64>,
    pub(super) order: Vec<usize>,
    pub(super) follow_done: bool,
    /// Follow choice drawn at launch; `None` only for voices armed after the fact.
    pub(super) queued: Option<follow_choice::QueuedFollow>,
    pub(super) silent: bool,
    pub(super) silent_deadline: Option<Instant>,
}

impl Voice {
    /// Unlinked envelopes start at their own Start at each launch/retrigger; on a legato
    /// take-over they take the clip's phase, as if launched with it at its Start (Live 12).
    /// Their natural end holds a value, while the voice's end still cuts them off.
    pub(super) fn envelope_timeline(
        &self,
        clip: &crate::model::Clip,
        envelope: &crate::model::AutomationEnvelope,
    ) -> Timeline {
        match envelope.region {
            None => self.timeline(clip),
            Some(region) => Timeline::new(
                self.launch,
                self.envelope_origin(envelope),
                1,
                self.timeline(clip).end(),
                region.normalised(),
            )
            .continuing(self.envelope_continuations.get(&envelope.target).copied()),
        }
    }

    pub(super) fn envelope_origin(&self, envelope: &crate::model::AutomationEnvelope) -> BeatTime {
        if envelope.region.is_some() {
            self.envelope_origins
                .get(&envelope.target)
                .copied()
                .unwrap_or(self.origin)
        } else {
            self.origin
        }
    }

    pub(super) fn event_limit(&self, clip: &crate::model::Clip) -> u64 {
        self.timeline(clip).event_limit(clip, &self.order)
    }

    pub(super) fn timeline(&self, clip: &crate::model::Clip) -> Timeline {
        Timeline::new(
            self.launch,
            self.origin,
            self.passes,
            self.cutoff,
            Region::of(clip),
        )
        .continuing(self.continuation)
    }
}

/// The next event of each voice, kept between iterations of `plan()` so that
/// only a consumed voice pays for building its packet again.
struct Candidates(Vec<(u64, Option<DueEvent>)>);

impl Candidates {
    fn new(planner: &Planner, clock: &impl TempoClock, now: Instant) -> Self {
        Self(
            planner
                .voices
                .iter()
                .map(|v| (v.id, planner.next(v, clock, now)))
                .collect(),
        )
    }

    fn earliest(&self) -> Option<DueEvent> {
        self.0
            .iter()
            .filter_map(|(_, event)| event.as_ref())
            .min_by_key(|e| (e.at, scene_follow::priority(e), e.voice, e.cursor))
            .cloned()
    }

    /// Bring the cache up to date after `planner` consumed `consumed`.
    fn refresh(
        &mut self,
        planner: &Planner,
        consumed: &DueEvent,
        clock: &impl TempoClock,
        now: Instant,
    ) {
        if !matches!(
            consumed.action,
            PlannedAction::Send { .. }
                | PlannedAction::Automate { .. }
                | PlannedAction::TrackSilent(_)
        ) {
            *self = Self::new(planner, clock, now);
            return;
        }
        let Some(slot) = self.0.iter().position(|(id, _)| *id == consumed.voice) else {
            return;
        };
        match planner.voices.iter().find(|v| v.id == consumed.voice) {
            Some(voice) => self.0[slot].1 = planner.next(voice, clock, now),
            None => {
                self.0.remove(slot);
            }
        }
    }
}

/// Planning has no socket, sleeps or system-clock reads. Consume only events
/// actually dispatched; lookahead is speculative and can be recalculated.
#[derive(Clone)]
pub struct Planner {
    pub(super) project: Arc<Project>,
    pub(super) voices: Vec<Voice>,
    pub(super) resume_clips: Vec<(TrackId, ClipId)>,
    pub(super) rng: u64,
    pub(super) serial: u64,
    pub(super) base_lookahead: Duration,
    pub(super) lookahead: Duration,
    pub(super) jump_deadlines: HashMap<(u64, u64, TrackId), Instant>,
    pub(super) deadlines: HashMap<(u64, u64, TrackId), Instant>,
    pub(super) stops: HashMap<TrackId, PendingStop>,
    pub(super) last_triggered_scene: Option<SceneId>,
    /// Voices the last scene launch created or reused, so Cancel Scene Launch drops
    /// only those still waiting, not clips launched by hand since.
    pub(super) scene_voices: Vec<u64>,
    pub(super) scene_queued: Option<super::follow_choice::QueuedFollow>,
    pub(super) scene_timer: Option<(SceneId, BeatTime)>,
}

impl Planner {
    pub fn new(project: Project, lookahead: Duration) -> io::Result<Self> {
        project
            .validate_stored()
            .map_err(|e| invalid(&e.to_string()))?;
        let base_lookahead = lookahead;
        let lookahead = delay::lookahead(&project, base_lookahead)?;
        Ok(Self {
            rng: project.random_seed,
            project: Arc::new(project),
            voices: Vec::new(),
            resume_clips: Vec::new(),
            serial: 0,
            base_lookahead,
            lookahead,
            deadlines: HashMap::new(),
            jump_deadlines: HashMap::new(),
            stops: HashMap::new(),
            last_triggered_scene: None,
            scene_voices: Vec::new(),
            scene_timer: None,
            scene_queued: None,
        })
    }

    pub fn lookahead(&self) -> Duration {
        self.lookahead
    }

    pub(super) fn next(
        &self,
        voice: &Voice,
        clock: &impl TempoClock,
        now: Instant,
    ) -> Option<DueEvent> {
        let packet = self
            .next_packet(voice, clock)
            .into_iter()
            .chain(self.next_jump(voice, clock))
            .min_by_key(|event| (event.at, scene_follow::priority(event)));
        let retirement = voice
            .cutoff
            .and_then(|beat| voice.cutoff_deadline.or_else(|| due_at(clock, beat, now)))
            .and_then(|at| delay::shift(at, self.tail_offset_ms(voice.track)))
            .map(|at| DueEvent {
                at,
                action: PlannedAction::RetireVoice,
                voice: voice.id,
                cursor: voice.cursor,
            });
        let follow = self.follow_beat(voice).and_then(|beat| {
            let (launch_beat, at) = follow::follow_position(clock, beat, now)?;
            Some(DueEvent {
                at,
                action: PlannedAction::Follow {
                    beat,
                    launch_beat,
                    signature: clock.time_signature(),
                },
                voice: voice.id,
                cursor: voice.cursor,
            })
        });
        let packet = packet
            .into_iter()
            .chain(follow)
            .min_by_key(|e| (e.at, !matches!(e.action, PlannedAction::Follow { .. })));
        match (packet, retirement) {
            (Some(packet), Some(retirement)) => Some(if retirement.at <= packet.at {
                retirement
            } else {
                packet
            }),
            (packet, retirement) => packet.or(retirement),
        }
    }

    /// Ordered candidates through now + lookahead, including overdue sends. Once the first
    /// candidate is found, the horizon extends (once, never again) to that event + lookahead,
    /// so a cluster of closely spaced hits is planned whole, ahead of its first deadline, rather
    /// than again between its deadlines when the worker's queue empties. Planning there cost
    /// ~100 us between hits ~50 us apart (arm-j-miss-diagnosis).
    pub fn plan(&self, clock: &impl TempoClock, now: Instant) -> Vec<DueEvent> {
        self.plan_until(clock, now).0
    }

    /// As `plan`, with the horizon it planned to: every unconsumed event due by it is in the plan.
    pub fn plan_until(&self, clock: &impl TempoClock, now: Instant) -> (Vec<DueEvent>, Instant) {
        let Some(mut end) = now.checked_add(self.lookahead) else {
            return (Vec::new(), now);
        };
        let mut copy = self.clone();
        let mut result = Vec::new();
        // One candidate per voice. Consuming a send or silence changes only its
        // own voice, so only that candidate is rebuilt; other actions can launch,
        // retire or cut off several voices, so they rebuild all of them.
        let mut candidates = Candidates::new(&copy, clock, now);
        loop {
            let voice_event = candidates.earliest();
            let stop_event = copy
                .stops
                .iter()
                .filter_map(|(&track, stop)| {
                    let at = due_at(clock, stop.beat, now)?;
                    Some(DueEvent {
                        at,
                        action: PlannedAction::StopTrack(track),
                        voice: 0,
                        cursor: track.0,
                    })
                })
                .min_by_key(|e| (e.at, e.cursor));
            let scene_event = copy.scene_event(clock, now);
            let Some(event) = voice_event
                .into_iter()
                .chain(stop_event)
                .chain(scene_event)
                .min_by_key(|e| (e.at, scene_follow::priority(e), e.voice, e.cursor))
            else {
                break;
            };
            if event.at > end {
                break;
            }
            if result.is_empty()
                && let Some(anchored) = event.at.checked_add(self.lookahead)
            {
                end = end.max(anchored);
            }
            // Simulate failures on the copy; only real dispatch reports them to clients.
            let _ = copy.consume_action(&event);
            candidates.refresh(&copy, &event, clock, now);
            result.push(event);
        }
        (result, end)
    }

    /// Retain deadlines from elapsed tempo segments: InternalClock intentionally
    /// discards their mapping, but positively delayed packets may still be pending.
    pub fn set_tempo(&mut self, clock: &mut impl TempoClock, tempo: Tempo, now: Instant) -> bool {
        let mut copy = self.clone();
        let positive = self
            .project
            .clip_tracks()
            .flat_map(|t| {
                let project = &self.project;
                project
                    .routed_outputs(t.id)
                    .map(move |o| project.send_delay_ms(t.id, o.id).max(0.0))
            })
            .fold(0.0, f64::max);
        copy.lookahead = copy
            .lookahead
            .max(Duration::try_from_secs_f64(positive / 1000.0).unwrap_or(Duration::ZERO));
        let events = copy.plan(clock, now);
        for event in events {
            let Some(voice) = self.voices.iter().find(|v| v.id == event.voice) else {
                continue;
            };
            let delay = if let PlannedAction::Send { output, .. }
            | PlannedAction::Automate { output, .. } = event.action
            {
                self.project.send_delay_ms(voice.track, output)
            } else {
                0.0
            };
            let musical = delay::shift(event.at, -delay);
            if musical.is_some_and(|at| at <= now)
                && let PlannedAction::Send { output, .. } | PlannedAction::Automate { output, .. } =
                    event.action
            {
                let deadlines = if matches!(event.action, PlannedAction::Automate { .. }) {
                    &mut self.jump_deadlines
                } else {
                    &mut self.deadlines
                };
                deadlines.insert((event.voice, event.cursor, output), event.at);
            }
        }
        for voice in &mut self.voices {
            if let Some(at) = voice.cutoff.and_then(|beat| clock.instant_at(beat))
                && at <= now
            {
                voice.cutoff_deadline = Some(at);
            }
            let Some(clip) = self.project.clip(voice.track, voice.slot) else {
                continue;
            };
            if !clip.looping
                && !voice.silent
                && let Some(end) = voice
                    .timeline(clip)
                    .pass_end()
                    .and_then(|beat| clock.instant_at(beat))
                && end <= now
            {
                voice.silent_deadline = Some(end);
            }
        }

        clock.set_tempo(tempo, now)
    }

    pub(super) fn consume_action(&mut self, event: &DueEvent) -> io::Result<()> {
        let result = match event.action {
            PlannedAction::SceneFollow {
                scene,
                beat,
                launch_beat,
                tempo,
                signature,
            } => self.apply_scene_follow(scene, beat, launch_beat, tempo, signature, event.at),
            PlannedAction::Follow {
                beat,
                launch_beat,
                signature,
            } => self.apply_follow(event.voice, beat, launch_beat, signature),
            PlannedAction::StopTrack(track) => {
                if let Some(beat) = self.stops.get(&track).map(|stop| stop.beat) {
                    self.stop_at(track, beat, Some(event.at));
                }
                return Ok(());
            }
            PlannedAction::RetireVoice => {
                self.voices.retain(|v| v.id != event.voice);
                Ok(())
            }
            _ => {
                if let Some(v) = self.voices.iter_mut().find(|v| v.id == event.voice) {
                    match &event.action {
                        PlannedAction::TrackSilent(_) => {
                            v.silent = true;
                            if self.project.sending_outputs(v.track).next().is_none() {
                                v.cursor = self
                                    .project
                                    .clip(v.track, v.slot)
                                    .map_or(0, |clip| v.event_limit(clip));
                            }
                        }
                        PlannedAction::Automate { output, .. } => {
                            self.jump_deadlines
                                .remove(&(event.voice, event.cursor, *output));
                            v.jump_cursors.insert(*output, BeatTime(event.cursor));
                        }
                        PlannedAction::Send { output, .. } => {
                            let cursor = v.output_cursors.entry(*output).or_insert(v.cursor);
                            if *cursor == event.cursor {
                                self.deadlines.remove(&(event.voice, event.cursor, *output));
                                *cursor += 1;
                                v.cursor = self
                                    .project
                                    .sending_outputs(v.track)
                                    .map(|o| {
                                        v.output_cursors.get(&o.id).copied().unwrap_or(v.cursor)
                                    })
                                    .min()
                                    .unwrap_or(v.cursor);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
        };
        // Spending a stale follow can release an ended voice retained for its timer.
        let finished: Vec<_> = self
            .voices
            .iter()
            .filter(|v| {
                v.cutoff.is_none()
                    && v.silent
                    && self
                        .project
                        .clip(v.track, v.slot)
                        .is_none_or(|clip| v.cursor >= v.event_limit(clip))
                    && v.follow_done
                    && self.jumps_finished(v)
            })
            .map(|v| v.id)
            .collect();
        self.voices.retain(|v| !finished.contains(&v.id));
        self.clean_deadlines();
        result
    }
}
