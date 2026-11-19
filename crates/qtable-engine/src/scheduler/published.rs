//! Snapshot data shared with engine clients.
use super::*;
use crate::model::ArgumentRef;

#[derive(Clone, Debug)]
pub struct PublishedTrack {
    pub track: TrackId,
    pub slot: usize,
    pub clip: ClipId,
    pub passes: u64,
    pub launch: BeatTime,
    /// Where elapsed clip time begins; earlier than `launch` after a legato take-over.
    pub origin: BeatTime,
    pub length: BeatTime,
    pub region: crate::model::region::Region,
    pub(super) continuation: Option<timeline::Continuation>,
    pub looping: bool,
    pub cutoff: Option<BeatTime>,
    pub next_follow_beat: Option<BeatTime>,
    /// Slot the armed follow action will launch, while its timer is pending.
    pub queued_follow_slot: Option<usize>,
    /// Each unlinked envelope's own timeline; linked lanes follow the clip's.
    pub(super) lanes: Vec<(ArgumentRef, Timeline)>,
}

/// Last content position of a clip, retained when it stops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipPosition {
    pub track: TrackId,
    pub clip: ClipId,
    pub position: BeatTime,
}

/// Last content position of an unlinked envelope on its own region, retained when it stops.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanePosition {
    pub track: TrackId,
    pub clip: ClipId,
    pub target: ArgumentRef,
    pub position: BeatTime,
}

impl PublishedTrack {
    pub fn position_at(&self, beat: BeatTime) -> BeatTime {
        Timeline::new(
            self.launch,
            self.origin,
            self.passes,
            self.cutoff,
            self.region,
        )
        .continuing(self.continuation)
        .position_at(beat)
    }

    /// How far through the current pass the clip has played at `beat`, 0.0 to 1.0.
    pub fn pass_progress_at(&self, beat: BeatTime) -> f64 {
        Timeline::new(
            self.launch,
            self.origin,
            self.passes,
            self.cutoff,
            self.region,
        )
        .continuing(self.continuation)
        .pass_progress(beat)
    }

    /// The content positions of the voice's unlinked envelopes at `beat`.
    pub fn lane_positions_at(&self, beat: BeatTime) -> impl Iterator<Item = LanePosition> + '_ {
        self.lanes
            .iter()
            .map(move |(target, timeline)| LanePosition {
                track: self.track,
                clip: self.clip,
                target: target.clone(),
                position: timeline.position_at(beat),
            })
    }

    /// Clip and unlinked lane positions at `at`, recorded over earlier ones.
    pub(crate) fn remember_at(
        &self,
        at: BeatTime,
        clips: &mut Vec<ClipPosition>,
        lanes: &mut Vec<LanePosition>,
    ) {
        remember(
            clips,
            ClipPosition {
                track: self.track,
                clip: self.clip,
                position: self.position_at(at),
            },
        );
        for value in self.lane_positions_at(at) {
            if let Some(old) = lanes.iter_mut().find(|p| {
                p.track == value.track && p.clip == value.clip && p.target == value.target
            }) {
                *old = value;
            } else {
                lanes.push(value);
            }
        }
    }
}

fn remember(positions: &mut Vec<ClipPosition>, value: ClipPosition) {
    if let Some(old) = positions
        .iter_mut()
        .find(|p| p.track == value.track && p.clip == value.clip)
    {
        *old = value;
    } else {
        positions.push(value);
    }
}

#[derive(Clone, Debug)]
pub struct PublishedState {
    pub playing: bool,
    pub tempo: Tempo,
    pub signature: crate::model::TimeSignature,
    pub global_quantisation: GlobalQuantisation,
    pub pending_stops: Vec<(TrackId, BeatTime)>,
    /// Empty slots whose stop buttons are pending, resolved against current scene order.
    pub pending_stop_slots: Vec<(TrackId, usize)>,
    pub last_triggered_scene: Option<SceneId>,
    /// Destination of the armed scene follow action, chosen at scene launch.
    pub queued_follow_scene: Option<SceneId>,
    pub beat: BeatTime,
    pub anchor: Instant,
    pub tracks: Vec<PublishedTrack>,
    pub clip_positions: Vec<ClipPosition>,
    pub lane_positions: Vec<LanePosition>,
    pub automation: Vec<super::AutomationValue>,
    /// Set when the scheduler thread died from a panic.
    pub failure: Option<String>,
}

impl PublishedState {
    pub(super) fn publish(&mut self, planner: &Planner, clock: &InternalClock, now: Instant) {
        let beat = clock.beat_at(now).unwrap_or(self.beat);
        // Capture the outgoing voices before replacing them, including exact queued
        // stop/cutoff boundaries even if publication happens slightly afterwards.
        for voice in self.tracks.iter().filter(|v| v.launch <= beat) {
            let at = self
                .pending_stops
                .iter()
                .filter(|(id, _)| *id == voice.track)
                .fold(beat, |at, (_, stop)| at.min(*stop));
            voice.remember_at(at, &mut self.clip_positions, &mut self.lane_positions);
        }
        if !clock.is_playing() && beat == BeatTime::ZERO && self.beat > BeatTime::ZERO {
            // Live 12.1's second Stop also rewinds the armed Session clips.
            for position in &mut self.clip_positions {
                if planner
                    .resume_clips
                    .contains(&(position.track, position.clip))
                {
                    position.position = BeatTime::ZERO;
                }
            }
            for position in &mut self.lane_positions {
                if planner
                    .resume_clips
                    .contains(&(position.track, position.clip))
                {
                    position.position = BeatTime::ZERO;
                }
            }
        }
        let exists = |track: TrackId, clip: ClipId| {
            planner.project.track(track).is_some_and(|t| {
                t.slots
                    .iter()
                    .any(|s| s.clip.as_ref().is_some_and(|c| c.id == clip))
            })
        };
        self.clip_positions.retain(|p| exists(p.track, p.clip));
        self.lane_positions.retain(|p| exists(p.track, p.clip));
        self.playing = clock.is_playing();
        self.tempo = clock.tempo();
        self.signature = clock.time_signature();
        self.global_quantisation = planner.project.global_quantisation;
        self.last_triggered_scene = planner.last_triggered_scene;
        self.queued_follow_scene = clock
            .is_playing()
            .then(|| planner.queued_follow_scene())
            .flatten();
        self.pending_stops = planner
            .stops
            .iter()
            .map(|(&track, stop)| (track, stop.beat))
            .collect();
        self.pending_stop_slots = planner
            .stops
            .iter()
            .filter_map(|(&track, stop)| {
                let scene = stop.scene?;
                let slot = planner.project.scenes.iter().position(|s| s.id == scene)?;
                let model = planner.project.clip_slot(track, slot)?;
                (model.clip.is_none() && model.has_stop_button).then_some((track, slot))
            })
            .collect();
        self.pending_stop_slots.sort_by_key(|(track, _)| track.0);
        self.pending_stops.sort_by_key(|(track, _)| track.0);
        self.beat = clock.beat_at(now).unwrap_or(self.beat);
        self.anchor = now;
        self.tracks = planner
            .voices
            .iter()
            .filter(|voice| !voice.silent)
            .filter_map(|voice| {
                let clip = planner.project.clip(voice.track, voice.slot)?;
                Some(PublishedTrack {
                    track: voice.track,
                    slot: voice.slot,
                    clip: voice.clip,
                    passes: voice.passes,
                    launch: voice.launch,
                    origin: voice.origin,
                    length: clip.length(),
                    region: Region::of(clip),
                    continuation: voice.continuation,
                    looping: clip.looping,
                    cutoff: voice.cutoff,
                    next_follow_beat: planner.follow_beat(voice),
                    queued_follow_slot: (!planner.stops.contains_key(&voice.track))
                        .then(|| planner.follow_beat(voice))
                        .flatten()
                        .and_then(|_| planner.queued_follow_slot(voice)),
                    lanes: clip
                        .lanes
                        .iter()
                        .filter_map(|lane| lane.envelope())
                        .filter(|envelope| envelope.region.is_some())
                        .map(|envelope| {
                            (
                                envelope.target.clone(),
                                voice.envelope_timeline(clip, envelope),
                            )
                        })
                        .collect(),
                })
            })
            .collect();
    }
}

#[cfg(test)]
mod position_tests {
    use super::*;
    #[test]
    fn phase_wraps_and_stays_at_the_exact_stop_even_after_voice_retirement() {
        let now = Instant::now();
        let project = super::super::tests::project(0.0, true);
        let mut clock = InternalClock::new(project.tempo, project.time_signature);
        let mut planner = Planner::new(project, Duration::ZERO).unwrap();
        planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
        let mut state = super::super::follow_queue_tests::published(&planner, &clock);
        let length = state.tracks[0].length.0;
        assert_eq!(
            state.tracks[0].position_at(BeatTime(length + 240)),
            BeatTime(240)
        );
        let stop = now + Duration::from_millis(125);
        let expected = clock.beat_at(stop).unwrap();
        planner.stop_transport(&clock, stop);
        clock.stop(stop);
        state.publish(&planner, &clock, stop);
        assert_eq!(state.clip_positions[0].position, expected);
        planner.stop_all();
        clock.rewind();
        state.publish(&planner, &clock, stop + Duration::from_secs(1));
        assert_eq!(state.clip_positions[0].position, expected);
    }

    #[test]
    fn nonlooping_end_is_visible_and_loop_off_keeps_the_current_pass_position() {
        let voice = PublishedTrack {
            track: TrackId(1),
            slot: 0,
            clip: ClipId(1),
            passes: 2,
            launch: BeatTime::ZERO,
            origin: BeatTime::ZERO,
            length: BeatTime(960),
            region: Region {
                start: BeatTime::ZERO,
                end: BeatTime(960),
                loop_start: BeatTime::ZERO,
                loop_end: BeatTime(960),
                looping: false,
            },
            continuation: None,
            looping: false,
            cutoff: None,
            next_follow_beat: None,
            queued_follow_slot: None,
            lanes: vec![],
        };
        assert_eq!(voice.position_at(BeatTime(1200)), BeatTime(240));
        assert_eq!(voice.position_at(BeatTime(1920)), BeatTime(960));
        assert_eq!(voice.position_at(BeatTime(5000)), BeatTime(960));
    }
}
