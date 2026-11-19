//! Independently ordered OSC sends for every enabled output in a route, while
//! the track is sending (`model::mixer`).
use super::*;

impl Planner {
    pub(super) fn next_packet(&self, voice: &Voice, clock: &impl TempoClock) -> Option<DueEvent> {
        let track = self.project.track(voice.track)?;
        if !self.project.track_sends(track.id) {
            return None;
        }
        let clip = track.clip(voice.slot)?;
        let timeline = voice.timeline(clip);
        // Pick the winner from cheap per-output data, then build one packet.
        // Outputs on the same beat share one (binary-search) clock lookup.
        let mut musical: Option<(BeatTime, Option<Instant>)> = None;
        let packet = self
            .project
            .resolved_route(track.id)
            .iter()
            .enumerate()
            .filter_map(|(position, id)| Some((position, self.project.destination(*id)?)))
            .filter(|(_, output)| output.accepts_sends())
            .filter_map(|(position, output)| {
                let cursor = voice
                    .output_cursors
                    .get(&output.id)
                    .copied()
                    .unwrap_or(voice.cursor);
                let (_, beat) = timeline.event_at(clip, &voice.order, cursor)?;
                if voice.cutoff.is_some_and(|cutoff| beat >= cutoff)
                    || self
                        .stops
                        .get(&voice.track)
                        .is_some_and(|stop| beat >= stop.beat)
                {
                    return None;
                }
                let at = self
                    .deadlines
                    .get(&(voice.id, cursor, output.id))
                    .copied()
                    .or_else(|| {
                        let due = match musical {
                            Some((cached, due)) if cached == beat => due,
                            _ => {
                                let due = clock.instant_at(beat);
                                musical = Some((beat, due));
                                due
                            }
                        };
                        delay::shift(due?, self.project.send_delay_ms(track.id, output.id))
                    })?;
                Some((at, position, output, cursor))
            })
            .min_by_key(|(at, position, ..)| (*at, *position))
            .map(|(at, _, output, cursor)| {
                let (index, _) = timeline
                    .event_at(clip, &voice.order, cursor)
                    .expect("selected event");
                let event = &clip.events[index];
                // The definition gives the piece (Target path + entry path), or None
                // while its device is off; the route adds the rest.
                let packet = self
                    .project
                    .event_path(track.id, &event.definition)
                    .and_then(|piece| {
                        self.project
                            .resolve_event_address(track.id, output.id, &piece)
                    })
                    .map(|address| event_packet(event, &address));
                DueEvent {
                    at,
                    voice: voice.id,
                    cursor,
                    action: PlannedAction::Send {
                        output: output.id,
                        packet,
                    },
                }
            });
        let silence = if !clip.looping && !voice.silent {
            let end = timeline.pass_end()?;
            if voice.cutoff.is_some_and(|cutoff| end >= cutoff)
                || self
                    .stops
                    .get(&voice.track)
                    .is_some_and(|stop| end >= stop.beat)
            {
                None
            } else {
                voice
                    .silent_deadline
                    .or_else(|| clock.instant_at(end))
                    .map(|at| DueEvent {
                        at,
                        action: PlannedAction::TrackSilent(track.id),
                        voice: voice.id,
                        cursor: voice.cursor,
                    })
            }
        } else {
            None
        };
        let cutoff_silence = if !voice.silent && self.tail_offset_ms(voice.track) > 0.0 {
            voice
                .cutoff
                .and_then(|beat| voice.cutoff_deadline.or_else(|| clock.instant_at(beat)))
                .map(|at| DueEvent {
                    at,
                    action: PlannedAction::TrackSilent(voice.track),
                    voice: voice.id,
                    cursor: voice.cursor,
                })
        } else {
            None
        };
        packet
            .into_iter()
            .chain(silence)
            .chain(cutoff_silence)
            .min_by_key(|e| e.at)
    }
}
