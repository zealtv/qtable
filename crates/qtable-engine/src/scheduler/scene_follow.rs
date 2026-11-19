//! Live source status as of 2026-10-11; IDs refer to the Live checklist.
use super::follow_choice::{FollowOutcome, QueuedFollow, scale};
use super::*;
// S03: Live 12 check 2 (2026-10-11) observed clip Next wrapping;
// scene Next remains inferred, as does scene Previous. See live-observations.
const WRAP: bool = true;
const ANY_INCLUDES_SOURCE: bool = true; // S05: inferred from Live 12 Any definition applied to scene groups.
const SINGLETON_RESTARTS: bool = true; // S09: inferred from Live 12 singleton Other, applied to scenes.
// S10: Live 12 check 3 (2026-10-11) supports this tested case;
// earliest boundary across mixed clip quantisations remains guessed.
const TIMER_AT_FIRST_CLIP_BOUNDARY: bool = true;
const SCENE_AFTER_CLIP: bool = true; // S11: documented, Live 12 manual: scenes take precedence.
// Live 12 check 3 (2026-10-11, live-observations): observed scene
// follow launches after one bar, confirming bypass of global quantisation.
pub(super) const SCENE_FOLLOW_BYPASS_GLOBAL: bool = true;
const STOP_ALL_TRACKS: bool = true; // S13: guessed; scene Stop scope needs a Live observation.

pub(super) fn priority(event: &DueEvent) -> u8 {
    match event.action {
        PlannedAction::Automate { .. } => 1,
        PlannedAction::Send { .. } | PlannedAction::TrackSilent(_) => 2,
        PlannedAction::SceneFollow { .. } if SCENE_AFTER_CLIP => 1,
        _ => 0,
    }
}
impl Planner {
    /// Cancel Scene Launch (2026-11-04):
    /// drop the scene's follow chain and the clip launches it still holds at their quantize
    /// points. Each affected track carries on as before: a playing clip keeps playing and
    /// a stopped track stays stopped. Clips already started are not stopped, and stops
    /// the scene requested for its empty stop-button slots are left to run.
    pub(super) fn cancel_scene_launch(&mut self, clock: &impl TempoClock, now: Instant) {
        self.last_triggered_scene = None;
        self.scene_timer = None;
        self.scene_queued = None;
        let waiting = std::mem::take(&mut self.scene_voices);
        let Some(beat) = clock.is_playing().then(|| clock.beat_at(now)).flatten() else {
            return;
        };
        let mut tracks: Vec<_> = self
            .voices
            .iter()
            .filter(|v| v.launch > beat && waiting.contains(&v.id))
            .map(|v| v.track)
            .collect();
        tracks.sort_by_key(|t| t.0);
        tracks.dedup();
        for track in tracks {
            // Removes the waiting trigger and undoes its cutoff on the playing clip.
            self.cancel_triggers(track, beat);
        }
        self.clean_deadlines();
    }

    // Sound On Sound, Live 11: Session View (captured 2026-10-10):
    // no clips or stop/record buttons separates scene follow-action ranges.
    fn scene_separator(&self, row: usize) -> bool {
        self.project
            .clip_tracks()
            .all(|t| t.slots[row].clip.is_none() && !t.slots[row].has_stop_button)
    }

    fn scene_has_clips(&self, row: usize) -> bool {
        self.project
            .clip_tracks()
            .any(|t| t.slots[row].clip.is_some())
    }

    pub(super) fn arm_scene(
        &mut self,
        origin: LaunchOrigin,
        scene: SceneId,
        row: usize,
        clock: &InternalClock,
        now: Instant,
    ) -> io::Result<()> {
        let beat = clock
            .beat_at(now)
            .ok_or_else(|| invalid("scene clock unavailable"))?;
        let boundary = if TIMER_AT_FIRST_CLIP_BOUNDARY {
            self.voices
                .iter()
                .filter(|v| v.slot == row && v.launch >= beat)
                .map(|v| v.launch)
                .min()
        } else {
            None
        }
        .unwrap_or(if clock.is_playing() {
            origin
                .boundary(
                    self.project.global_quantisation,
                    beat,
                    clock.time_signature(),
                )
                .map_err(|e| invalid(&e.to_string()))?
        } else {
            BeatTime::ZERO
        });
        // Sound On Sound (2026-10-10 capture): jumping to a scene with no clips
        // terminates follow actions, while normal scene launch/stop behaviour still applies.
        let follow = self.project.scenes[row].follow_actions.clone();
        self.scene_timer = if follow.actions.enabled && self.scene_has_clips(row) {
            boundary
                .0
                .checked_add(follow.time.0.max(1))
                .map(|b| (scene, BeatTime(b)))
        } else {
            None
        };
        // Inferred from clip behaviour, Live check pending (2026-10-11):
        // choose at scene launch and retain the draw until the timer fires.
        self.scene_queued = self.scene_timer.map(|_| self.draw_follow(&follow.actions));
        Ok(())
    }

    pub(super) fn update_scene_choice(&mut self, project: &Project) {
        if let Some((scene, _)) = self.scene_timer {
            let previous = self.project.scenes.iter().find(|s| s.id == scene);
            let updated = project.scenes.iter().find(|s| s.id == scene);
            if let Some(updated) = updated
                && previous.is_none_or(|previous| {
                    previous.follow_actions.actions != updated.follow_actions.actions
                })
            {
                self.scene_queued = Some(self.draw_follow(&updated.follow_actions.actions));
            }
        } else {
            self.scene_queued = None;
        }
    }

    pub(super) fn queued_follow_scene(&self) -> Option<SceneId> {
        let (scene, _) = self.scene_timer?;
        let row = self.project.scenes.iter().position(|s| s.id == scene)?;
        if !self.project.follow_actions_enabled
            || !self.project.scenes[row].follow_actions.actions.enabled
        {
            return None;
        }
        match self.scene_follow_outcome(row, self.scene_queued?) {
            FollowOutcome::Launch(row) => Some(self.project.scenes[row].id),
            _ => None,
        }
    }

    pub(super) fn scene_event(&self, clock: &impl TempoClock, now: Instant) -> Option<DueEvent> {
        let (scene, beat) = self.scene_timer?;
        let model = self.project.scenes.iter().find(|s| s.id == scene)?;
        if !clock.is_playing()
            || !self.project.follow_actions_enabled
            || !model.follow_actions.actions.enabled
        {
            return None;
        }
        let (launch_beat, at) = follow::follow_position(clock, beat, now)?;
        Some(DueEvent {
            at,
            action: PlannedAction::SceneFollow {
                scene,
                beat,
                launch_beat,
                tempo: clock.tempo(),
                signature: clock.time_signature(),
            },
            voice: 0,
            cursor: 0,
        })
    }
    pub(super) fn apply_scene_follow(
        &mut self,
        scene: SceneId,
        beat: BeatTime,
        launch_beat: BeatTime,
        tempo: Tempo,
        signature: crate::model::TimeSignature,
        at: Instant,
    ) -> io::Result<()> {
        if self.scene_timer != Some((scene, beat)) || !self.project.follow_actions_enabled {
            return Ok(());
        }
        let Some(row) = self.project.scenes.iter().position(|s| s.id == scene) else {
            self.scene_timer = None;
            self.scene_queued = None;
            return Ok(());
        };
        let actions = self.project.scenes[row].follow_actions.actions.clone();
        if !actions.enabled {
            return Ok(());
        }
        self.scene_timer = None;
        let queued = self.scene_queued.take();
        // Live 12 check 5 (2026-10-11): missed clip timers are spent; infer the
        // same for scene timers and edits, retaining the scheduling jitter margin.
        if launch_beat > beat {
            return Ok(());
        }
        let Some(queued) = queued else {
            return Ok(());
        };
        let destination = match self.scene_follow_outcome(row, queued) {
            FollowOutcome::Nothing => return Ok(()),
            FollowOutcome::Stop => {
                if STOP_ALL_TRACKS {
                    self.stop_all();
                }
                return Ok(());
            }
            FollowOutcome::Launch(row) => row,
        };
        debug_assert!(launch_beat >= beat, "scene follow launch precedes timer");
        let mut clock = InternalClock::at_position(tempo, signature, launch_beat, at);
        let id = self.project.scenes[destination].id;
        self.launch_scene_from(LaunchOrigin::SceneFollow, id, &mut clock, at)?;
        Ok(())
    }

    fn scene_follow_outcome(&self, row: usize, queued: QueuedFollow) -> FollowOutcome {
        let action = queued.action;
        let first = (0..row)
            .rev()
            .find(|&i| self.scene_separator(i))
            .map_or(0, |i| i + 1);
        let last = (row + 1..self.project.scenes.len())
            .find(|&i| self.scene_separator(i))
            .map_or(self.project.scenes.len() - 1, |i| i - 1);
        let count = last - first + 1;
        let destination = match action {
            FollowAction::NoAction => return FollowOutcome::Nothing,
            FollowAction::Stop => return FollowOutcome::Stop,
            FollowAction::PlayAgain => row,
            FollowAction::Previous => {
                if row == first {
                    if WRAP { last } else { row }
                } else {
                    row - 1
                }
            }
            FollowAction::Next => {
                if row == last {
                    if WRAP { first } else { row }
                } else {
                    row + 1
                }
            }
            FollowAction::First => first,
            FollowAction::Last => last,
            FollowAction::Jump(id) => {
                let Some(index) = self.project.scenes.iter().position(|s| s.id == id) else {
                    return FollowOutcome::Nothing;
                };
                index
            }
            FollowAction::Any | FollowAction::Other => {
                let candidates: Vec<_> = (first..=last)
                    .filter(|&i| i != row || (action == FollowAction::Any && ANY_INCLUDES_SOURCE))
                    .collect();
                if candidates.is_empty() {
                    if count == 1 && SINGLETON_RESTARTS {
                        row
                    } else {
                        return FollowOutcome::Nothing;
                    }
                } else {
                    candidates[scale(queued.pick, candidates.len())]
                }
            }
        };
        FollowOutcome::Launch(destination)
    }
}
