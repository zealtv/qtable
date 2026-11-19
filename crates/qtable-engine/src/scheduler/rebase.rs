//! Keep playing voices coherent when the project is edited under them.
use super::*;
use crate::model::Clip;

/// Event indices in playback order; equal positions keep insertion order.
pub(super) fn event_order(clip: &Clip) -> Vec<usize> {
    let mut order: Vec<_> = (0..clip.events.len()).collect();
    order.sort_by_key(|&i| clip.events[i].position);
    order
}

fn find_clip(project: &Project, track: TrackId, clip: ClipId) -> Option<(usize, &Clip)> {
    let track = project.track(track)?;
    track
        .slots
        .iter()
        .enumerate()
        .find_map(|(i, s)| s.clip.as_ref().filter(|c| c.id == clip).map(|c| (i, c)))
}

fn same_timeline(a: &Clip, b: &Clip) -> bool {
    a.region() == b.region() && a.events == b.events
}

/// A pure Loop-off retains the old stream for delayed recipients.
fn same_envelope_regions(a: &Clip, b: &Clip) -> bool {
    b.lanes.iter().filter_map(|l| l.envelope()).all(|new| {
        a.lanes
            .iter()
            .filter_map(|l| l.envelope())
            .find(|e| e.target == new.target)
            .is_some_and(|old| {
                old.region == new.region
                    || old.region.zip(new.region).is_some_and(|(mut old, new)| {
                        let was_looping = old.looping;
                        old.looping = false;
                        was_looping && old == new
                    })
            })
    })
}

/// What decides when a track's events leave the engine: its delay and whether
/// each output is enabled and the track is sending (Activator, Solo). A change
/// here moves send times without touching the clip.
fn send_path(project: &Project, track: TrackId) -> Vec<(TrackId, u64, bool)> {
    let sends = project.track_sends(track);
    project
        .routed_outputs(track)
        .map(|o| {
            (
                o.id,
                project.send_delay_ms(track, o.id).to_bits(),
                sends && o.mixer.active,
            )
        })
        .collect()
}

impl Planner {
    /// Swap model data while voices keep playing. A voice follows its `ClipId`
    /// to wherever the clip now sits on its track, so row moves and inserted
    /// scenes don't stop it. A clip whose events, length or looping changed gets
    /// its event order and cursor rebuilt from the current musical position, so
    /// nothing is skipped, repeated or sent in a burst. A voice whose track
    /// delay, output latency or route changed is rebased the same way, so a
    /// rerouted or delayed voice skips the events that fell behind
    /// instead of sending them as a burst. Returns tracks that
    /// became silent, using the same transition check as event consumption.
    pub fn update_project(
        &mut self,
        project: Project,
        clock: &impl TempoClock,
        now: Instant,
    ) -> io::Result<Vec<TrackId>> {
        project
            .validate_stored()
            .map_err(|e| invalid(&e.to_string()))?;
        let lookahead = delay::lookahead(&project, self.base_lookahead)?;
        let before = self.sounding_tracks();
        let mut rebuilt = Vec::new();
        let old = std::mem::take(&mut self.voices);
        for mut voice in old {
            let Some((slot, clip)) = find_clip(&project, voice.track, voice.clip) else {
                continue;
            };
            let previous = find_clip(&self.project, voice.track, voice.clip).map(|(_, clip)| clip);
            let unchanged = previous.is_some_and(|previous| same_timeline(previous, clip));
            let old_timeline = previous.map(|old| voice.timeline(old));
            let beat = clock.beat_at(now).unwrap_or(voice.launch).max(voice.launch);
            if clip.looping {
                voice.passes = 1;
                voice.continuation = None;
            } else if let Some(old) = previous.filter(|old| old.looping) {
                voice.continuation = Some(timeline::Continuation {
                    at: beat,
                    position: voice.timeline(old).position_at(beat),
                    previous: Region::of(old),
                });
                voice.passes = 1;
            }
            let mut origins = HashMap::new();
            let mut continuations = HashMap::new();
            for envelope in clip.lanes.iter().filter_map(|l| l.envelope()) {
                let Some(region) = envelope.region else {
                    continue;
                };
                let old = previous.and_then(|old| {
                    old.lanes
                        .iter()
                        .filter_map(|l| l.envelope())
                        .find(|e| e.target == envelope.target)
                });
                // Copy the clip's playback phase as well as its markers on live unlink.
                // Keep it lane-local afterwards; relink/removal and a new voice discard it.
                if old.is_some_and(|e| e.region.is_none())
                    && region.normalised() == Region::of(clip)
                {
                    if voice.origin != voice.launch {
                        origins.insert(envelope.target.clone(), voice.origin);
                    }
                    if let Some(c) = voice.continuation {
                        continuations.insert(envelope.target.clone(), c);
                    }
                    continue;
                }
                if let Some(origin) = voice.envelope_origins.get(&envelope.target) {
                    origins.insert(envelope.target.clone(), *origin);
                }
                if region.looping {
                    continue;
                }
                if let Some(old) = old.filter(|e| e.region.is_some_and(|r| r.looping)) {
                    continuations.insert(
                        envelope.target.clone(),
                        timeline::Continuation {
                            at: beat,
                            position: voice
                                .envelope_timeline(previous.unwrap(), old)
                                .position_at(beat),
                            previous: old.region.unwrap().normalised(),
                        },
                    );
                } else if old.is_some_and(|e| e.region.is_some())
                    && let Some(c) = voice.envelope_continuations.get(&envelope.target)
                {
                    continuations.insert(envelope.target.clone(), *c);
                }
            }
            voice.envelope_origins = origins;
            voice.envelope_continuations = continuations;
            let moved = send_path(&self.project, voice.track) != send_path(&project, voice.track);
            voice.slot = slot;
            let jump_region = voice.continuation.map_or(Region::of(clip), |c| c.previous);
            let jumps = Arc::new(jumps::Jumps::new(
                &project,
                voice.track,
                slot,
                jump_region,
                &voice.envelope_continuations,
            ));
            let loop_off = previous.is_some_and(|old| {
                let mut region = old.region();
                region.looping = false;
                old.looping && region == clip.region() && old.lanes == clip.lanes
            });
            if (!loop_off || moved)
                && (jumps != voice.jumps
                    || moved
                    || previous.is_none_or(|old| {
                        old.region() != clip.region() || !same_envelope_regions(old, clip)
                    }))
            {
                for output in project.parameter_outputs(voice.track) {
                    if let Some(position) =
                        delay::shift(now, -project.send_delay_ms(voice.track, output.id))
                            .and_then(|at| clock.beat_at(at))
                            .and_then(|beat| beat.0.checked_sub(1))
                            .map(BeatTime)
                    {
                        let cursor = voice.jump_cursors.entry(output.id).or_insert(position);
                        *cursor = (*cursor).max(position);
                    }
                }
                self.jump_deadlines.retain(|(id, _, _), _| *id != voice.id);
            }
            voice.jumps = jumps;
            // Changed follow settings draw again; layout edits re-resolve the same draw.
            if !voice.follow_done
                && previous.is_none_or(|previous| {
                    previous.follow_actions.actions != clip.follow_actions.actions
                })
            {
                voice.queued = Some(self.draw_follow(&clip.follow_actions.actions));
            }
            if !unchanged || moved {
                self.rebase_voice(&mut voice, clip, &project, clock, now, old_timeline);
                rebuilt.push(voice.id);
            }
            self.voices.push(voice);
        }
        self.deadlines.retain(|(id, _, output), _| {
            if !rebuilt.contains(id) {
                return true;
            }
            let Some(voice) = self.voices.iter().find(|v| v.id == *id) else {
                return false;
            };
            let old_clip = find_clip(&self.project, voice.track, voice.clip).map(|(_, c)| c);
            let new_clip = find_clip(&project, voice.track, voice.clip).map(|(_, c)| c);
            old_clip
                .zip(new_clip)
                .is_some_and(|(a, b)| same_timeline(a, b))
                && output_delay(&self.project, voice.track, *output).is_some()
                && output_delay(&self.project, voice.track, *output)
                    == output_delay(&project, voice.track, *output)
        });
        self.stops
            .retain(|track, _| project.tracks.iter().any(|item| item.id == *track));
        self.clean_deadlines();
        self.lookahead = lookahead;
        // Preserve the scene's launch boundary when its live follow time changes.
        if let Some((scene, beat)) = self.scene_timer
            && let Some(previous) = self.project.scenes.iter().find(|s| s.id == scene)
            && let Some(updated) = project.scenes.iter().find(|s| s.id == scene)
            && previous.follow_actions.time != updated.follow_actions.time
        {
            self.scene_timer = beat
                .0
                .checked_sub(previous.follow_actions.time.0.max(1))
                .and_then(|start| start.checked_add(updated.follow_actions.time.0.max(1)))
                .map(|beat| (scene, BeatTime(beat)));
        }
        if self
            .scene_timer
            .is_some_and(|(scene, _)| !project.scenes.iter().any(|s| s.id == scene))
        {
            self.scene_timer = None;
        }
        self.update_scene_choice(&project);
        self.project = Arc::new(project);
        Ok(self.silenced_since(before))
    }

    fn rebase_voice(
        &self,
        voice: &mut Voice,
        clip: &Clip,
        project: &Project,
        clock: &impl TempoClock,
        now: Instant,
        old_timeline: Option<Timeline>,
    ) {
        let previous = find_clip(&self.project, voice.track, voice.clip).map(|(_, c)| c);
        let keep_sent = previous.is_some_and(|old| same_timeline(old, clip));
        let old_order = std::mem::replace(&mut voice.order, event_order(clip));
        let mut cursors = HashMap::new();
        for output in project.sending_outputs(voice.track) {
            if keep_sent
                && output_delay(&self.project, voice.track, output.id)
                    == output_delay(project, voice.track, output.id)
            {
                cursors.insert(
                    output.id,
                    voice
                        .output_cursors
                        .get(&output.id)
                        .copied()
                        .unwrap_or(voice.cursor),
                );
                continue;
            }
            let position = delay::shift(now, -project.send_delay_ms(voice.track, output.id))
                .and_then(|at| clock.beat_at(at))
                .unwrap_or(BeatTime::ZERO);
            // Never send again what went: an edit waits for clip events due soon after it
            // arrives, then rebases at its arrival, before events this voice has since sent.
            let sent = voice
                .output_cursors
                .get(&output.id)
                .copied()
                .unwrap_or(voice.cursor);
            let position = previous
                .and_then(|old| last_sent(old, &old_order, old_timeline?, sent))
                .map_or(position, |beat| {
                    position.max(BeatTime(beat.0.saturating_add(1)))
                });
            let mut cursor = voice
                .timeline(clip)
                .dispatched_events(clip, &voice.order, position);
            if keep_sent {
                cursor = cursor.max(voice.output_cursors.get(&output.id).copied().unwrap_or(0));
            }
            cursors.insert(output.id, cursor);
        }
        voice.cursor = cursors.values().copied().min().unwrap_or_else(|| {
            let beat = clock.beat_at(now).unwrap_or(BeatTime::ZERO);
            voice
                .timeline(clip)
                .dispatched_events(clip, &voice.order, beat)
        });
        voice.output_cursors = cursors;
        // A longer or looping clip is no longer over; a shorter one may be.
        let now_beat = clock.beat_at(now).unwrap_or(BeatTime::ZERO);
        let ended = !clip.looping
            && voice
                .timeline(clip)
                .pass_end()
                .is_some_and(|end| now_beat >= end);
        if !ended && voice.cutoff.is_none_or(|end| now_beat < end) {
            voice.silent = false;
            voice.silent_deadline = None;
        }
    }
}

/// Where the last of `sent` events fell on the voice's old timeline, if it sent any.
fn last_sent(old: &Clip, order: &[usize], timeline: Timeline, sent: u64) -> Option<BeatTime> {
    timeline
        .event_at(old, order, sent.checked_sub(1)?)
        .map(|(_, beat)| beat)
}

fn output_delay(project: &Project, track: TrackId, output: TrackId) -> Option<u64> {
    project.track(track)?;
    let output = project.sending_outputs(track).find(|o| o.id == output)?;
    Some(project.send_delay_ms(track, output.id).to_bits())
}
