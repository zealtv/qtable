//! Musical stops retain the already-played stream until its delayed sends finish.
use super::*;

impl Planner {
    pub(super) fn tail_offset_ms(&self, track: TrackId) -> f64 {
        self.project
            .sending_outputs(track)
            .map(|o| self.project.send_delay_ms(track, o.id))
            .fold(0.0, f64::max)
    }

    /// Report musical silence now, retaining only events strictly before the cutoff.
    pub(super) fn stop_at(&mut self, track: TrackId, beat: BeatTime, at: Option<Instant>) {
        self.stops.remove(&track);
        let has_tail = self.tail_offset_ms(track) > 0.0;
        self.voices.retain_mut(|voice| {
            if voice.track != track {
                return true;
            }
            if voice.cutoff.is_some_and(|end| end <= beat) {
                return true;
            }
            if !has_tail || voice.launch >= beat {
                return false;
            }
            voice.cutoff = Some(beat);
            voice.cutoff_deadline = at;
            voice.silent = true;
            voice.follow_done = true;
            true
        });
        self.clean_deadlines();
    }

    /// Freeze all remaining send deadlines before the transport clock loses its mapping.
    pub(super) fn stop_transport(&mut self, clock: &impl TempoClock, now: Instant) {
        let Some(beat) = clock.beat_at(now) else {
            return;
        };
        // Live 12.1, OSC observation 2026-10-12: Global Play restarts armed
        // Session clips at zero. Explicit track stops and ended clips stay silent.
        self.resume_clips = self
            .project
            .clip_tracks()
            .filter_map(|track| {
                self.voices
                    .iter()
                    .filter(|v| {
                        v.track == track.id
                            && !v.silent
                            && v.launch <= beat
                            && v.cutoff.is_none_or(|end| end > beat)
                            && self.project.clip(v.track, v.slot).is_some_and(|c| {
                                c.looping || v.timeline(c).pass_end().is_some_and(|end| end > beat)
                            })
                    })
                    .max_by_key(|v| v.launch)
                    .map(|v| (track.id, v.clip))
            })
            .collect();
        for voice in &mut self.voices {
            if let Some(cutoff) = voice.cutoff.filter(|end| *end <= beat) {
                voice.cutoff_deadline = voice.cutoff_deadline.or_else(|| clock.instant_at(cutoff));
                voice.silent = true;
                voice.follow_done = true;
            }
        }
        self.scene_timer = None;
        self.scene_queued = None;
        let tracks: Vec<_> = self.project.clip_tracks().map(|t| t.id).collect();
        for track in tracks {
            self.stop_at(track, beat, Some(now));
        }
        let mut copy = self.clone();
        let largest = self
            .project
            .clip_tracks()
            .map(|t| self.tail_offset_ms(t.id))
            .fold(0.0, f64::max);
        // Validated combined offsets are finite and bounded.
        copy.lookahead = Duration::from_secs_f64(largest / 1000.0);
        for event in copy.plan(clock, now) {
            if let PlannedAction::Send { output, .. } | PlannedAction::Automate { output, .. } =
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
    }
}
