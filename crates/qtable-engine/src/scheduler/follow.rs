//! Live source status as of 2026-10-11; checklist IDs refer to the Live test plan.
use super::follow_choice::FollowOutcome;
use super::*;
const BYPASS_GLOBAL: bool = true; // Q01–Q07: documented, Live 12 manual bypasses global, not clip quantisation.
const FOLLOW_AFTER_END: bool = true; // T03, T05: inferred from Live 12 duration-from-start rule, nonloop end unverified.

pub(super) fn follow_position(
    clock: &impl TempoClock,
    beat: BeatTime,
    now: Instant,
) -> Option<(BeatTime, Instant)> {
    let current = clock.beat_at(now)?;
    // Preserve on-grid launches through ordinary scheduling jitter. Earlier
    // tempo segments may be unmappable; estimate their age at the current tempo.
    let stale = clock.instant_at(beat).map_or_else(
        || {
            current.0.saturating_sub(beat.0) as f64
                > late::LATE_SEND_MARGIN.as_secs_f64()
                    * (clock.tempo().bpm() / 60.0)
                    * crate::model::TICKS_PER_BEAT as f64
        },
        |at| now.saturating_duration_since(at) > late::LATE_SEND_MARGIN,
    );
    // Live 12 check 5 (2026-10-11, live-observations): a timer missed
    // while disabled is spent until relaunch. Apply this to edits by inference.
    // Clamp stale timers to now so consumption can spend them without launching.
    if beat < current && stale {
        Some((current, now))
    } else {
        Some((beat, due_at(clock, beat, now)?))
    }
}

/// Where a launch came from. Follow timers fire exactly on musical positions, so
/// their launches round to the boundary at or after the timer, not strictly after.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LaunchOrigin {
    Manual,
    ClipFollow,
    SceneFollow,
}

impl LaunchOrigin {
    /// The grid a clip launches on; follow launches may bypass global quantisation.
    pub(super) fn grid(
        self,
        clip: LaunchQuantisation,
        global: GlobalQuantisation,
    ) -> GlobalQuantisation {
        let bypass = match self {
            Self::Manual => false,
            Self::ClipFollow => BYPASS_GLOBAL,
            Self::SceneFollow => SCENE_FOLLOW_BYPASS_GLOBAL,
        };
        match clip {
            LaunchQuantisation::Global if bypass => GlobalQuantisation::None,
            LaunchQuantisation::Global => global,
            LaunchQuantisation::None => GlobalQuantisation::None,
            LaunchQuantisation::Value(value) => GlobalQuantisation::Value(value),
        }
    }

    pub(super) fn boundary(
        self,
        grid: GlobalQuantisation,
        at: BeatTime,
        signature: crate::model::TimeSignature,
    ) -> Result<BeatTime, crate::model::ModelError> {
        match self {
            Self::Manual => grid.next_boundary(at, signature),
            _ => grid.boundary_at_or_after(at, signature),
        }
    }
}

impl Planner {
    pub(super) fn follow_beat(&self, voice: &Voice) -> Option<BeatTime> {
        let clip = self.project.clip(voice.track, voice.slot)?;
        let follow = &clip.follow_actions;
        if voice.follow_done || !follow.actions.enabled || !self.project.follow_actions_enabled {
            return None;
        }
        // Live 12 manual (2026-10-10 capture): Linked = length × Multiplier;
        // Unlinked = Follow Action Time from the actual start of play.
        // A Live 12 screenshot (2026-10-12): with Loop off, Linked
        // shows an unclickable "Clip End" in place of the multiplier, so it fires at clip end.
        let duration = if follow.linked {
            voice.timeline(clip).linked_duration(
                u64::from(follow.multiplier.numerator()),
                u64::from(follow.multiplier.denominator()),
            )?
        } else {
            follow.time.0
        };
        if !FOLLOW_AFTER_END && !clip.looping && duration > clip.length().0 {
            return None;
        }
        let beat = BeatTime(voice.launch.0.checked_add(duration.max(1))?);
        if voice.cutoff.is_some_and(|end| beat >= end)
            || self
                .stops
                .get(&voice.track)
                .is_some_and(|stop| beat >= stop.beat)
        {
            return None;
        }
        Some(beat)
    }

    // SplitMix64: reproducible from the project's seed.
    pub(super) fn next_u64(&mut self) -> u64 {
        self.rng = self.rng.wrapping_add(0x9e3779b97f4a7c15);
        let mut value = self.rng;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
        value ^ (value >> 31)
    }

    // Rejection sampling keeps the result unbiased for all weights.
    pub(super) fn random_below(&mut self, bound: u64) -> u64 {
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let value = self.next_u64();
            if value >= threshold {
                return value % bound;
            }
        }
    }

    pub(super) fn apply_follow(
        &mut self,
        id: u64,
        beat: BeatTime,
        launch_beat: BeatTime,
        signature: crate::model::TimeSignature,
    ) -> io::Result<()> {
        let Some(voice) = self.voices.iter().find(|v| v.id == id).cloned() else {
            return Ok(());
        };
        if self.follow_beat(&voice) != Some(beat) {
            return Ok(());
        }
        if let Some(live) = self.voices.iter_mut().find(|v| v.id == id) {
            live.follow_done = true;
        }
        // Live 12.1, 2026-10-11, observed over AbletonOSC:
        // a pending track stop suppresses clip follow actions. Spend the timer
        // without choosing an action; keep playing until the stop boundary.
        if self.stops.contains_key(&voice.track) {
            return Ok(());
        }
        // A clamped timer is spent; only relaunching arms another follow action.
        if launch_beat > beat {
            return Ok(());
        }
        let queued = match voice.queued {
            Some(queued) => queued,
            None => {
                let Some(clip) = self.project.clip(voice.track, voice.slot) else {
                    return Ok(());
                };
                let actions = clip.follow_actions.actions.clone();
                self.draw_follow(&actions)
            }
        };
        let destination = match self.follow_outcome(voice.track, voice.slot, queued) {
            FollowOutcome::Nothing => return Ok(()),
            FollowOutcome::Stop => {
                self.stop_now(voice.track);
                return Ok(());
            }
            FollowOutcome::Launch(destination) => destination,
        };
        let Some(clip) = self.project.clip(voice.track, destination) else {
            return Ok(());
        };
        let origin = LaunchOrigin::ClipFollow;
        let grid = origin.grid(clip.quantisation, self.project.global_quantisation);
        let launch = origin
            .boundary(grid, launch_beat, signature)
            .map_err(|error| invalid(&error.to_string()))?;
        debug_assert!(
            launch >= launch_beat,
            "follow launch precedes current position"
        );
        // Follow launches replace any queued manual trigger on this track.
        self.voices
            .retain(|v| v.track != voice.track || v.launch <= launch_beat);
        self.stops.remove(&voice.track);
        self.launch_at(voice.track, destination, launch)?;
        Ok(())
    }
}
