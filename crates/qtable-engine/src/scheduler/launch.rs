//! Clip and scene launches, and immediate or quantised stops.
use super::*;
use crate::model::Clip;

impl Planner {
    /// Apply clip override/global quantisation. A stopped transport starts at
    /// beat zero with an immediate launch, regardless of the selected grid.
    pub fn launch(
        &mut self,
        track: TrackId,
        slot: usize,
        clock: &mut impl TempoClock,
        now: Instant,
    ) -> io::Result<BeatTime> {
        self.launch_from(LaunchOrigin::Manual, track, slot, clock, now)
    }

    pub(super) fn launch_from(
        &mut self,
        origin: LaunchOrigin,
        track: TrackId,
        slot: usize,
        clock: &mut impl TempoClock,
        now: Instant,
    ) -> io::Result<BeatTime> {
        let clip = self
            .project
            .clip(track, slot)
            .ok_or_else(|| invalid("empty or unknown slot/track"))?;
        let stopped = !clock.is_playing();
        let beat = if stopped {
            BeatTime::ZERO
        } else {
            clock
                .beat_at(now)
                .ok_or_else(|| invalid("clock position unavailable"))?
        };
        if !stopped
            && let Some(pending) = self
                .voices
                .iter()
                .find(|v| v.track == track && v.slot == slot && v.launch > beat)
        {
            return Ok(pending.launch);
        }
        let quantisation = origin.grid(clip.quantisation, self.project.global_quantisation);
        let launch = if stopped {
            BeatTime::ZERO
        } else {
            origin
                .boundary(quantisation, beat, clock.time_signature())
                .map_err(|e| invalid(&e.to_string()))?
        };
        // Validate before replacing a trigger or starting the transport.
        // Replace the trigger, undoing its cutoff on the old playing voice.
        self.cancel_triggers(track, beat);
        self.stops.remove(&track);
        debug_assert!(launch >= beat, "launch precedes current position");
        self.launch_at(track, slot, launch)?;
        if stopped {
            clock.play(now);
        }
        Ok(launch)
    }

    /// Clicking an empty slot either requests a track stop or leaves it untouched.
    pub fn trigger_slot(
        &mut self,
        track: TrackId,
        slot: usize,
        clock: &mut impl TempoClock,
        now: Instant,
    ) -> io::Result<bool> {
        let item = self
            .project
            .clip_slot(track, slot)
            .ok_or_else(|| invalid("unknown slot/track"))?;
        if item.clip.is_some() {
            self.launch(track, slot, clock, now)?;
            Ok(false)
        } else if item.has_stop_button {
            self.request_slot_stop(track, slot, clock, now)
        } else {
            Ok(false)
        }
    }

    pub fn request_stop_group(
        &mut self,
        group: TrackId,
        clock: &impl TempoClock,
        now: Instant,
    ) -> io::Result<Vec<TrackId>> {
        let tracks = self
            .project
            .descendant_clip_track_ids(group)
            .ok_or_else(|| invalid("unknown group"))?;
        // Stage the batch so a boundary error cannot stop only part of a group.
        let mut staged = self.clone();
        let mut immediate = Vec::new();
        for track in tracks {
            if staged.request_stop(track, clock, now)? {
                immediate.push(track);
            }
        }
        *self = staged;
        Ok(immediate)
    }

    /// Apply a whole row using one clock sample. Stage changes so an error cannot
    /// leave half a scene launched. Each clip reuses the normal launch path.
    pub fn launch_scene(
        &mut self,
        scene: SceneId,
        clock: &mut InternalClock,
        now: Instant,
    ) -> io::Result<Vec<TrackId>> {
        self.launch_scene_from(LaunchOrigin::Manual, scene, clock, now)
    }

    pub(super) fn launch_scene_from(
        &mut self,
        origin: LaunchOrigin,
        scene: SceneId,
        clock: &mut InternalClock,
        now: Instant,
    ) -> io::Result<Vec<TrackId>> {
        let row = self
            .project
            .scenes
            .iter()
            .position(|s| s.id == scene)
            .ok_or_else(|| invalid("unknown scene"))?;
        let tracks: Vec<_> = self.project.clip_tracks().map(|track| track.id).collect();
        let (mut staged, staged_clock, immediate) =
            self.stage_row(origin, &tracks, row, clock, now)?;
        staged.arm_scene(origin, scene, row, clock, now)?;
        staged.last_triggered_scene = Some(scene);
        // A repeated launch reuses pending voices instead of creating new ids.
        // Track the whole row, including those reused voices; cancellation only
        // removes voices still waiting and leaves later manual launches alone.
        staged.scene_voices = staged
            .voices
            .iter()
            .filter(|v| v.slot == row)
            .map(|v| v.id)
            .collect();
        *self = staged;
        *clock = staged_clock;
        Ok(immediate)
    }

    /// Stage a row against one original clock anchor, including stopped launches.
    /// Empty stop buttons affect only their own member; other empty slots are untouched.
    pub(super) fn stage_row(
        &self,
        origin: LaunchOrigin,
        tracks: &[TrackId],
        row: usize,
        clock: &InternalClock,
        now: Instant,
    ) -> io::Result<(Self, InternalClock, Vec<TrackId>)> {
        let mut staged = self.clone();
        let mut staged_clock = clock.clone();
        let mut immediate = Vec::new();
        for &track in tracks {
            let slot = self
                .project
                .clip_slot(track, row)
                .ok_or_else(|| invalid("unknown slot/track"))?;
            if slot.clip.is_some() {
                // Every stopped clip uses beat zero, even after the first starts playback.
                if !clock.is_playing() {
                    staged_clock = clock.clone();
                }
                staged.launch_from(origin, track, row, &mut staged_clock, now)?;
            } else if slot.has_stop_button && staged.request_slot_stop(track, row, clock, now)? {
                immediate.push(track);
            }
        }
        Ok((staged, staged_clock, immediate))
    }

    /// Launch only this group's row, atomically, without arming scene follow actions.
    pub fn launch_group_slot(
        &mut self,
        group: TrackId,
        row: usize,
        clock: &mut InternalClock,
        now: Instant,
    ) -> io::Result<Vec<TrackId>> {
        let tracks = self
            .project
            .descendant_clip_track_ids(group)
            .ok_or_else(|| invalid("unknown group"))?;
        let (staged, staged_clock, immediate) =
            self.stage_row(LaunchOrigin::Manual, &tracks, row, clock, now)?;
        *self = staged;
        *clock = staged_clock;
        Ok(immediate)
    }

    pub fn set_global_quantisation(&mut self, value: GlobalQuantisation) -> io::Result<()> {
        value.validate().map_err(|e| invalid(&e.to_string()))?;
        Arc::make_mut(&mut self.project).global_quantisation = value;
        Ok(())
    }

    /// Cancel a trigger immediately; a playing track stops on the global grid.
    /// Returns true only when the track becomes silent immediately.
    ///
    /// A track whose only voice is a queued launch counts as sounding, so
    /// cancelling it reports silence although nothing played (review 2 #7,
    /// pinned by the stop tests). A future "stop messages only when the track
    /// goes silent" option must tell the two apart, and must apply the track
    /// delay itself: silence is reported at the musical end, before delayed sends.
    pub fn request_stop(
        &mut self,
        track: TrackId,
        clock: &impl TempoClock,
        now: Instant,
    ) -> io::Result<bool> {
        self.request_stop_from(track, None, clock, now)
    }

    fn request_slot_stop(
        &mut self,
        track: TrackId,
        slot: usize,
        clock: &impl TempoClock,
        now: Instant,
    ) -> io::Result<bool> {
        let scene = self.project.scenes.get(slot).map(|scene| scene.id);
        self.request_stop_from(track, scene, clock, now)
    }

    fn request_stop_from(
        &mut self,
        track: TrackId,
        scene: Option<SceneId>,
        clock: &impl TempoClock,
        now: Instant,
    ) -> io::Result<bool> {
        self.resume_clips.retain(|(id, _)| *id != track);
        let beat = clock
            .beat_at(now)
            .ok_or_else(|| invalid("clock position unavailable"))?;
        let had_voice = self.sounding_tracks().contains(&track);
        self.cancel_triggers(track, beat);
        let playing = self.voices.iter().any(|v| v.track == track);
        if !playing
            || !clock.is_playing()
            || self.project.global_quantisation == GlobalQuantisation::None
        {
            self.stop_at(track, beat, Some(now));
            return Ok(had_voice);
        }
        // Repeated stop requests retain their original pending boundary.
        let boundary = self
            .project
            .global_quantisation
            .next_boundary(beat, clock.time_signature())
            .map_err(|e| invalid(&e.to_string()))?;
        self.stops
            .entry(track)
            .and_modify(|stop| stop.scene = scene)
            .or_insert(PendingStop {
                beat: boundary,
                scene,
            });
        self.clean_deadlines();
        Ok(false)
    }

    pub fn request_stop_all(
        &mut self,
        clock: &impl TempoClock,
        now: Instant,
    ) -> io::Result<Vec<TrackId>> {
        self.resume_clips.clear();
        self.scene_timer = None;
        self.scene_queued = None;
        let mut tracks: Vec<_> = self.voices.iter().map(|v| v.track).collect();
        tracks.sort_by_key(|t| t.0);
        tracks.dedup();
        let mut immediate = Vec::new();
        for track in tracks {
            if self.request_stop(track, clock, now)? {
                immediate.push(track);
            }
        }
        Ok(immediate)
    }

    pub(super) fn clean_deadlines(&mut self) {
        self.jump_deadlines
            .retain(|(id, _, _), _| self.voices.iter().any(|v| v.id == *id));
        self.deadlines
            .retain(|(id, _, _), _| self.voices.iter().any(|v| v.id == *id));
    }

    /// Explicit launch seam for deterministic tests and later quantisation.
    pub fn launch_at(&mut self, track: TrackId, slot: usize, launch: BeatTime) -> io::Result<()> {
        self.project
            .track(track)
            .ok_or_else(|| invalid("unknown track"))?;
        let clip = self
            .project
            .clip(track, slot)
            .ok_or_else(|| invalid("empty or unknown slot"))?;
        let clip_id = clip.id;
        let region = Region::of(clip);
        let follow_done = !clip.follow_actions.actions.enabled;
        let actions = clip.follow_actions.actions.clone();
        let order = rebase::event_order(clip);
        let (origin, cursor) = self.start_position(track, clip, &order, launch);
        // Live chooses the follow destination when the clip launches.
        let queued = (!follow_done).then(|| self.draw_follow(&actions));
        self.voices
            .retain(|v| v.track != track || v.launch < launch || (v.silent && v.cutoff.is_some()));
        for voice in &mut self.voices {
            if voice.track == track && !(voice.silent && voice.cutoff.is_some()) {
                voice.cutoff = Some(voice.cutoff.map_or(launch, |old| old.min(launch)));
            }
        }
        self.clean_deadlines();
        self.serial = self
            .serial
            .checked_add(1)
            .ok_or_else(|| invalid("voice overflow"))?;
        self.voices.push(Voice {
            id: self.serial,
            track,
            clip: clip_id,
            slot,
            launch,
            origin,
            passes: 1,
            continuation: None,
            envelope_origins: HashMap::new(),
            envelope_continuations: HashMap::new(),
            cutoff: None,
            cutoff_deadline: None,
            cursor,
            output_cursors: HashMap::new(),
            jumps: Arc::new(jumps::Jumps::new(
                &self.project,
                track,
                slot,
                region,
                &HashMap::new(),
            )),
            jump_cursors: HashMap::new(),
            order,
            follow_done,
            queued,
            silent: false,
            silent_deadline: None,
        });
        Ok(())
    }

    /// Content origin and first event cursor for a voice launching at `launch`.
    /// A legato clip takes over the unwrapped time since the playing clip's
    /// origin, mapped through its own timeline (Live 12.1 via AbletonOSC, 2026-10-12);
    /// a non-looping clip past its end starts finished. Otherwise it starts at Start.
    fn start_position(
        &self,
        track: TrackId,
        clip: &Clip,
        order: &[usize],
        launch: BeatTime,
    ) -> (BeatTime, u64) {
        let playing = clip
            .legato
            .then(|| {
                self.voices
                    .iter()
                    .filter(|v| v.track == track && !v.silent && v.launch < launch)
                    .filter(|v| v.cutoff.is_none_or(|end| end > launch))
                    .filter(|v| {
                        let source = self.project.clip(v.track, v.slot);
                        source.is_some_and(|c| {
                            c.looping || v.timeline(c).pass_end().is_some_and(|end| launch < end)
                        })
                    })
                    .max_by_key(|v| v.launch)
            })
            .flatten();
        let Some(source) = playing else {
            return (launch, 0);
        };
        let timeline = Timeline::new(launch, launch, 1, None, Region::of(clip));
        let origin = timeline.legato_origin(source.origin);
        let timeline = Timeline::new(launch, origin, 1, None, Region::of(clip));
        (origin, timeline.dispatched_events(clip, order, launch))
    }

    pub fn stop_now(&mut self, track: TrackId) -> bool {
        self.resume_clips.retain(|(id, _)| *id != track);
        self.stops.remove(&track);
        let active = self.sounding_tracks().contains(&track);
        self.voices.retain(|v| v.track != track);
        self.deadlines
            .retain(|(id, _, _), _| self.voices.iter().any(|v| v.id == *id));
        active
    }
    pub fn stop_all(&mut self) -> Vec<TrackId> {
        self.resume_clips.clear();
        let tracks = self.sounding_tracks();
        self.scene_timer = None;
        self.scene_queued = None;
        self.voices.clear();
        self.stops.clear();
        self.deadlines.clear();
        tracks
    }
}
