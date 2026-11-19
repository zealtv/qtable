//! Exact envelope discontinuities share the voice planner and its per-output clocks.
use super::*;
use crate::model::{ArgumentRef, AutomationEnvelope, Axis, FieldRef, Track};
use std::collections::BTreeMap;

#[derive(Clone, Default, PartialEq)]
pub(super) struct Jumps {
    launch: Vec<FieldRef>,
    linked: JumpIndex,
    unlinked: Vec<(ArgumentRef, JumpIndex)>,
}

/// Content discontinuities sharing a timeline (all linked lanes, or one own region).
#[derive(Clone, Default, PartialEq)]
struct JumpIndex {
    points: Vec<(u64, Vec<FieldRef>)>,
    wrap: Vec<FieldRef>,
}

impl Jumps {
    pub(super) fn new(
        project: &Project,
        track: TrackId,
        slot: usize,
        region: Region,
        continuations: &HashMap<ArgumentRef, timeline::Continuation>,
    ) -> Self {
        let Some(clip) = project.clip(track, slot) else {
            return Self::default();
        };
        let Some(track) = project.track(track) else {
            return Self::default();
        };
        let envelopes = clip
            .lanes
            .iter()
            .filter_map(|lane| lane.envelope())
            .filter(|e| !e.points.is_empty() && track.argument_axis(&e.target).is_some());
        let mut jumps = Self {
            launch: envelopes.clone().map(|e| e.target.on(track.id)).collect(),
            linked: JumpIndex::new(
                envelopes.clone().filter(|e| e.region.is_none()),
                track,
                region,
            ),
            unlinked: Vec::new(),
        };
        for envelope in envelopes {
            if let Some(region) = envelope.region {
                let region = continuations
                    .get(&envelope.target)
                    .map_or(region.normalised(), |c| c.previous);
                jumps.unlinked.push((
                    envelope.target.clone(),
                    JumpIndex::new(std::iter::once(envelope), track, region),
                ));
            }
        }
        jumps
    }

    fn next(
        &self,
        voice: &Voice,
        clip: &crate::model::Clip,
        after: Option<BeatTime>,
    ) -> Option<(BeatTime, Vec<FieldRef>)> {
        if self.launch.is_empty() {
            return None;
        }
        let mut next = (after.is_none() || after.is_some_and(|at| at < voice.launch))
            .then(|| (voice.launch, self.launch.clone()))
            .filter(|(_, targets)| !targets.is_empty());
        if let Some(candidate) = self
            .linked
            .next_on(voice.timeline(clip), voice.launch, after)
        {
            merge(&mut next, candidate);
        }
        for (target, jumps) in &self.unlinked {
            let Some(envelope) = clip
                .lanes
                .iter()
                .filter_map(|l| l.envelope())
                .find(|e| &e.target == target)
            else {
                continue;
            };
            if let Some(candidate) =
                jumps.next_on(voice.envelope_timeline(clip, envelope), voice.launch, after)
            {
                merge(&mut next, candidate);
            }
        }
        next.filter(|(beat, _)| {
            voice.cutoff.is_none_or(|end| *beat < end)
                && voice.timeline(clip).end().is_none_or(|end| *beat <= end)
        })
    }
}

impl JumpIndex {
    fn new<'a>(
        envelopes: impl Iterator<Item = &'a AutomationEnvelope>,
        track: &Track,
        region: Region,
    ) -> Self {
        let mut points: BTreeMap<u64, Vec<FieldRef>> = BTreeMap::new();
        let mut wrap = Vec::new();
        for envelope in envelopes {
            let Some(axis) = track.argument_axis(&envelope.target) else {
                continue;
            };
            let target = envelope.target.on(track.id);
            for pair in envelope.points.windows(2) {
                if pair[0].value != pair[1].value
                    && (axis == Axis::Discrete || pair[0].time == pair[1].time)
                {
                    let targets = points.entry(pair[1].time.0).or_default();
                    if !targets.contains(&target) {
                        targets.push(target.clone());
                    }
                }
            }
            if region.looping {
                // Exclude a discrete/pair jump at the outgoing endpoint.
                let before = if axis == Axis::Discrete {
                    envelope
                        .points
                        .iter()
                        .rev()
                        .find(|p| p.time < region.loop_end)
                        .unwrap_or(&envelope.points[0])
                        .value
                } else {
                    envelope
                        .points
                        .iter()
                        .find(|p| p.time == region.loop_end)
                        .map_or_else(
                            || envelope.value_at(region.loop_end.0 as f64, axis).unwrap(),
                            |p| p.value,
                        )
                };
                if envelope.value_at(region.loop_start.0 as f64, axis) != Some(before) {
                    wrap.push(target);
                }
            }
        }
        Self {
            points: points.into_iter().collect(),
            wrap,
        }
    }

    fn next_on(
        &self,
        timeline: Timeline,
        launch: BeatTime,
        after: Option<BeatTime>,
    ) -> Option<(BeatTime, Vec<FieldRef>)> {
        let from = after
            .map_or(launch.0, |beat| beat.0.saturating_add(1))
            .max(launch.0);
        let mut next: Option<(BeatTime, Vec<FieldRef>)> = None;
        for (position, targets) in &self.points {
            let beat = match timeline.jump_at_or_after(BeatTime(*position), BeatTime(from)) {
                Some(beat) => beat,
                None => continue,
            };
            if beat.0 < from {
                continue;
            }
            match &mut next {
                Some((earliest, _)) if *earliest < beat => {}
                Some((earliest, selected)) if *earliest == beat => {
                    for target in targets {
                        if !selected.contains(target) {
                            selected.push(target.clone());
                        }
                    }
                }
                _ => next = Some((beat, targets.clone())),
            }
        }
        if !self.wrap.is_empty()
            && let Some(beat) = timeline.next_wrap_after(BeatTime(from.saturating_sub(1)))
        {
            match &mut next {
                Some((earliest, _)) if *earliest < beat => {}
                Some((earliest, selected)) if *earliest == beat => {
                    for target in &self.wrap {
                        if !selected.contains(target) {
                            selected.push(target.clone());
                        }
                    }
                }
                _ => next = Some((beat, self.wrap.clone())),
            }
        }
        next.filter(|(beat, _)| timeline.end().is_none_or(|end| *beat <= end))
    }

    fn missed(&self, timeline: Timeline, from: BeatTime, until: BeatTime) -> u64 {
        self.points
            .iter()
            .map(|(position, _)| timeline.jump_occurrences(BeatTime(*position), from, until))
            .sum::<u64>()
            + if self.wrap.is_empty() {
                0
            } else {
                timeline.wraps(from, until)
            }
    }
}

fn merge(next: &mut Option<(BeatTime, Vec<FieldRef>)>, (beat, targets): (BeatTime, Vec<FieldRef>)) {
    match next {
        Some((earliest, _)) if *earliest < beat => {}
        Some((earliest, selected)) if *earliest == beat => {
            for target in targets {
                if !selected.contains(&target) {
                    selected.push(target);
                }
            }
        }
        _ => *next = Some((beat, targets)),
    }
}

impl Planner {
    pub(super) fn next_jump(&self, voice: &Voice, clock: &impl TempoClock) -> Option<DueEvent> {
        let clip = self.project.clip(voice.track, voice.slot)?;
        self.project
            .parameter_outputs(voice.track)
            .into_iter()
            .filter_map(|output| {
                let (beat, fields) =
                    voice
                        .jumps
                        .next(voice, clip, voice.jump_cursors.get(&output.id).copied())?;
                if self
                    .stops
                    .get(&voice.track)
                    .is_some_and(|stop| beat >= stop.beat)
                {
                    return None;
                }
                let at = self
                    .jump_deadlines
                    .get(&(voice.id, beat.0, output.id))
                    .copied()
                    .or_else(|| {
                        delay::shift(
                            clock.instant_at(beat)?,
                            self.project.send_delay_ms(voice.track, output.id),
                        )
                    })?;
                Some(DueEvent {
                    at,
                    voice: voice.id,
                    cursor: beat.0,
                    action: PlannedAction::Automate {
                        output: output.id,
                        fields,
                    },
                })
            })
            .min_by_key(|event| event.at)
    }

    pub(super) fn jumps_finished(&self, voice: &Voice) -> bool {
        let Some(clip) = self.project.clip(voice.track, voice.slot) else {
            return true;
        };
        self.project
            .parameter_outputs(voice.track)
            .into_iter()
            .all(|output| {
                voice
                    .jumps
                    .next(voice, clip, voice.jump_cursors.get(&output.id).copied())
                    .is_none()
            })
    }
}

impl Planner {
    /// Arithmetic catch-up: a stalled worker never enumerates every missed loop jump.
    pub(super) fn skip_stale_jumps(
        &mut self,
        clock: &impl TempoClock,
        now: Instant,
        bound: Duration,
    ) -> u64 {
        let Some(before) = now.checked_sub(bound) else {
            return 0;
        };
        let mut skipped = 0;
        for voice in &mut self.voices {
            if voice.jumps.launch.is_empty() {
                continue;
            }
            let Some(clip) = self.project.clip(voice.track, voice.slot) else {
                continue;
            };
            let timeline = voice.timeline(clip);
            for output in self.project.parameter_outputs(voice.track) {
                if self
                    .jump_deadlines
                    .keys()
                    .any(|(id, _, out)| *id == voice.id && *out == output.id)
                {
                    continue;
                }
                let Some(mut limit) =
                    delay::shift(before, -self.project.send_delay_ms(voice.track, output.id))
                        .and_then(|at| clock.beat_at(at))
                        .map(|beat| beat.0)
                else {
                    continue;
                };
                for end in voice
                    .cutoff
                    .into_iter()
                    .chain(self.stops.get(&voice.track).map(|stop| stop.beat))
                {
                    limit = limit.min(end.0);
                }
                if !clip.looping
                    && let Some(end) = timeline.pass_end()
                {
                    limit = limit.min(end.0.saturating_add(1));
                }
                let from = voice
                    .jump_cursors
                    .get(&output.id)
                    .map_or(voice.launch.0, |beat| beat.0.saturating_add(1))
                    .max(voice.launch.0);
                if limit <= from {
                    continue;
                }
                if voice.launch.0 >= from && voice.launch.0 < limit {
                    skipped += 1;
                }
                // Launch already coalesces all lanes' point jumps at that instant.
                let from = BeatTime(from.max(voice.launch.0.saturating_add(1)));
                skipped += voice.jumps.linked.missed(timeline, from, BeatTime(limit));
                for (target, jumps) in &voice.jumps.unlinked {
                    let Some(envelope) = clip
                        .lanes
                        .iter()
                        .filter_map(|l| l.envelope())
                        .find(|e| &e.target == target)
                    else {
                        continue;
                    };
                    let timeline = voice.envelope_timeline(clip, envelope);
                    let until = timeline
                        .end()
                        .map_or(limit, |end| limit.min(end.0.saturating_add(1)));
                    skipped += jumps.missed(timeline, from, BeatTime(until));
                }
                voice.jump_cursors.insert(output.id, BeatTime(limit - 1));
            }
        }
        skipped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::scheduler::cluster_sim::{MS, Sim, US, within};

    fn project(soloed_away: bool) -> Project {
        let mut project = super::super::tests::project(0.0, true);
        if soloed_away {
            let mut other = Track::new(TrackId(2), "Solo", project.scenes.len());
            other.mixer.solo = true;
            project.tracks.push(other);
        } else {
            project.tracks[0].mixer.active = false;
        }
        let track = &mut project.tracks[0];
        let target = track.devices[0].target_mut().unwrap();
        target.parameters.push(ParameterDef {
            key: EntryKey("gate".into()),
            name: String::new(),
            path: OscAddress::Relative("/gate".into()),
            fields: vec![FieldDef::new(
                FieldKey("value".into()),
                FieldType::Bool,
                true,
            )],
            send_mode: SendMode::OnRelease,
        });
        track.slots[0]
            .clip
            .as_mut()
            .unwrap()
            .lanes
            .push(Lane::Envelope(AutomationEnvelope {
                region: None,
                target: ArgumentRef {
                    device: DeviceRef::Chain(DeviceId(1)),
                    parameter: ParameterKey("gate".into()),
                    field: FieldKey("value".into()),
                },
                points: vec![
                    Breakpoint {
                        time: BeatTime::ZERO,
                        value: 0.0,
                        curve: 0.0,
                    },
                    Breakpoint {
                        time: BeatTime(240),
                        value: 1.0,
                        curve: 0.0,
                    },
                ],
            }));
        project.outputs[0].output.as_mut().unwrap().stage.frame_rate = 30.0;
        project
    }

    #[test]
    fn wrap_uses_the_interpolated_endpoint_when_points_extend_past_the_markers() {
        let mut p = project(false);
        let clip = p.tracks[0].slots[0].clip.as_mut().unwrap();
        clip.lanes = vec![Lane::Envelope(AutomationEnvelope {
            target: ArgumentRef::volume(),
            region: None,
            points: [(0, 0.0), (480, 1.0), (1920, 0.0)]
                .into_iter()
                .map(|(t, value)| Breakpoint {
                    time: BeatTime(t),
                    value,
                    curve: 0.0,
                })
                .collect(),
        })];
        p.validate().unwrap();
        let jumps = Jumps::new(
            &p,
            TrackId(1),
            0,
            super::Region::of(p.clip(TrackId(1), 0).unwrap()),
            &HashMap::new(),
        );
        assert!(!jumps.linked.wrap.is_empty());
        assert!(
            jumps
                .linked
                .points
                .iter()
                .all(|(position, _)| *position < 960)
        );
    }

    #[test]
    fn toggle_launch_steps_and_wraps_send_at_exact_instants() {
        let mut project = project(false);
        project.tracks[0].mixer.active = true;
        let mut sim = Sim::new(project, MS(20), Duration::ZERO).launched();
        sim.run_until(sim.start + MS(650));
        let jumps: Vec<_> = sim
            .frames
            .iter()
            .filter(|frame| frame.profile.jump.is_some())
            .collect();
        assert_eq!(jumps.len(), 4, "{jumps:?}");
        // 125 and 625 ms lie between the 30 Hz frame ticks.
        for (frame, ms) in jumps.into_iter().zip([0, 125, 500, 625]) {
            assert_eq!(frame.tick, sim.start + MS(ms));
            assert!(within(frame.start, frame.tick, US(100)), "{frame:?}");
            assert_eq!(frame.output, TrackId(201));
        }
    }

    #[test]
    fn muted_or_soloed_away_sources_send_no_jumps_while_their_automation_keeps_moving() {
        for soloed_away in [false, true] {
            let project = project(soloed_away);
            assert!(project.sending_outputs(TrackId(1)).next().is_none());
            let mut sim = Sim::new(project, MS(20), Duration::ZERO).launched();
            let gate = |sim: &Sim, ms: u64| {
                sim.stages
                    .automation_values(sim.start + MS(ms))
                    .into_iter()
                    .find(|value| value.field.parameter.key.0 == "gate")
                    .map(|value| value.value)
            };
            let mut seen = Vec::new();
            for ms in [100, 200, 550, 650] {
                sim.run_until(sim.start + MS(ms));
                seen.push(gate(&sim, ms));
            }
            assert_eq!(seen, [Some(0.0), Some(1.0), Some(0.0), Some(1.0)]);
            assert!(sim.frames.is_empty(), "{:?}", sim.frames);
        }
    }

    #[test]
    fn jump_outputs_match_slot_routing_and_keep_destination_gates() {
        let mut project = project(false);
        project.tracks[0].mixer.active = true;
        let outputs = |project: &Project| {
            project
                .parameter_outputs(TrackId(1))
                .iter()
                .map(|o| o.id)
                .collect::<Vec<_>>()
        };
        let slots = |project: &Project| {
            parameter_controls::controls(project)
                .into_iter()
                .map(|control| control.output)
                .collect::<Vec<_>>()
        };
        assert_eq!(outputs(&project), vec![TrackId(201)]);
        assert_eq!(outputs(&project), slots(&project));
        // A disabled device has no slot, but its voice's cursors keep advancing.
        project.tracks[0].devices[0].active = false;
        assert_eq!(outputs(&project), vec![TrackId(201)]);
        assert!(slots(&project).is_empty());
        project.tracks[0].devices[0].active = true;
        // So does a muted source: its valve closes slots, not the jump timeline.
        project.tracks[0].mixer.active = false;
        assert_eq!(outputs(&project), vec![TrackId(201)]);
        assert!(slots(&project).is_empty());
        project.tracks[0].mixer.active = true;
        assert_eq!(outputs(&project), slots(&project));
        project.outputs[0].mixer.active = false;
        assert!(outputs(&project).is_empty());
        assert_eq!(outputs(&project), slots(&project));
    }

    #[test]
    fn stale_jump_cursors_and_rebase_advance_on_muted_and_soloed_away_tracks() {
        for soloed_away in [false, true] {
            let start = Instant::now();
            let mut clock = InternalClock::default();
            clock.play(start);
            let mut planner = Planner::new(project(soloed_away), MS(20)).unwrap();
            planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
            // Launch, step, wrap, step are all stale; the next wrap is at 1 s.
            assert_eq!(planner.skip_stale_jumps(&clock, start + MS(800), MS(50)), 4);
            let jump = planner.next_jump(&planner.voices[0], &clock).unwrap();
            assert_eq!(jump.at, start + MS(1000));

            let mut edited = (*planner.project).clone();
            edited.tracks[0].delay_ms = -100.0;
            planner
                .update_project(edited, &clock, start + MS(950))
                .unwrap();
            // The delay edit moves that wrap into the past: rebase skips it.
            let jump = planner.next_jump(&planner.voices[0], &clock).unwrap();
            assert_eq!(jump.at, start + MS(1025));
        }
    }

    #[test]
    fn disabled_devices_keep_jump_cursors_current_before_reenable() {
        let start = Instant::now();
        let mut clock = InternalClock::default();
        clock.play(start);
        let mut project = project(false);
        project.tracks[0].devices[0].active = false;
        let mut planner = Planner::new(project, MS(20)).unwrap();
        planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
        assert!(parameter_controls::controls(&planner.project).is_empty());
        assert_eq!(planner.skip_stale_jumps(&clock, start + MS(800), MS(50)), 4);
        let mut edited = (*planner.project).clone();
        edited.tracks[0].devices[0].active = true;
        planner
            .update_project(edited, &clock, start + MS(800))
            .unwrap();
        let jump = planner.next_jump(&planner.voices[0], &clock).unwrap();
        assert_eq!(jump.at, start + MS(1000));
    }

    #[test]
    fn pending_delayed_jumps_keep_a_closed_source_voice_unfinished() {
        for soloed_away in [false, true] {
            let start = Instant::now();
            let mut clock = InternalClock::default();
            clock.play(start);
            let mut project = project(soloed_away);
            project.tracks[0].delay_ms = 1000.0;
            project.tracks[0].slots[0].clip.as_mut().unwrap().looping = false;
            let mut planner = Planner::new(project, Duration::from_secs(2)).unwrap();
            planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
            let events = planner.plan(&clock, start);
            let jumps: Vec<_> = events
                .iter()
                .filter(|e| matches!(e.action, PlannedAction::Automate { .. }))
                .collect();
            assert_eq!(jumps.len(), 2, "{events:?}");
            assert_eq!(jumps[0].at, start + MS(1000));
            assert_eq!(jumps[1].at, start + MS(1125));
            assert!(!planner.jumps_finished(&planner.voices[0]));
            planner.consume(jumps[0]);
            assert!(!planner.jumps_finished(&planner.voices[0]));
            planner.consume(jumps[1]);
            assert!(planner.jumps_finished(&planner.voices[0]));
        }
    }
}
