//! Fast-forward voices past sends that are already too late to be worth sending, so a stall costs one cursor
//! jump per output rather than one planned packet per missed event.
use super::*;

impl Planner {
    /// Move each voice's cursor past the sends whose deadline lies more than
    /// `bound` before `now`: the ones the worker would plan and then drop.
    /// Returns how many sends were skipped, for the dropped-sends report.
    ///
    /// Skips nothing for a voice that has pending tempo-change deadlines, no
    /// output, or a clock that can no longer map that instant to a beat.
    pub(super) fn skip_stale_sends(
        &mut self,
        clock: &impl TempoClock,
        now: Instant,
        bound: Duration,
    ) -> u64 {
        let Some(stale_before) = now.checked_sub(bound) else {
            return 0;
        };
        let mut skipped = 0;
        for index in 0..self.voices.len() {
            let voice = &self.voices[index];
            let outputs: Vec<_> = self
                .project
                .sending_outputs(voice.track)
                .map(|o| o.id)
                .collect();
            for output in &outputs {
                let voice = &self.voices[index];
                let Some(target) = self.skippable_cursor(voice, *output, clock, stale_before)
                else {
                    continue;
                };
                let cursor = voice
                    .output_cursors
                    .get(output)
                    .copied()
                    .unwrap_or(voice.cursor);
                if target > cursor {
                    skipped += target - cursor;
                    self.voices[index].output_cursors.insert(*output, target);
                }
            }
            let voice = &mut self.voices[index];
            voice.cursor = outputs
                .iter()
                .map(|id| {
                    voice
                        .output_cursors
                        .get(id)
                        .copied()
                        .unwrap_or(voice.cursor)
                })
                .min()
                .unwrap_or(voice.cursor);
        }
        skipped
    }

    fn skippable_cursor(
        &self,
        voice: &Voice,
        output_id: TrackId,
        clock: &impl TempoClock,
        stale_before: Instant,
    ) -> Option<u64> {
        let track = self.project.track(voice.track)?;
        let clip = track.clip(voice.slot)?;
        let output = self.project.destination(output_id)?;
        if self
            .deadlines
            .keys()
            .any(|(id, _, output)| *id == voice.id && *output == output_id)
        {
            return None;
        }
        // A send is stale when its musical time plus the track delay is before `stale_before`.
        let musical = delay::shift(
            stale_before,
            -self.project.send_delay_ms(track.id, output.id),
        )?;
        let mut limit = clock.beat_at(musical)?;
        // Events at or after a cutoff or stop are never sent, so they are not "dropped".
        for end in voice
            .cutoff
            .into_iter()
            .chain(self.stops.get(&voice.track).map(|stop| stop.beat))
        {
            limit = limit.min(end);
        }
        Some(
            voice
                .timeline(clip)
                .dispatched_events(clip, &voice.order, limit),
        )
    }
}
