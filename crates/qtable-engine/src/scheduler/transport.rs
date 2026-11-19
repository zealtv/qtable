//! Restart the clips armed at global Stop, matching Live 12.1's global Play.
use super::*;

impl Planner {
    pub(super) fn play_transport(
        &mut self,
        clock: &mut InternalClock,
        now: Instant,
    ) -> io::Result<()> {
        let mut staged = self.clone();
        staged.resume_clips.clear();
        for &(track, clip) in &self.resume_clips {
            let slot = self.project.track(track).and_then(|t| {
                t.slots
                    .iter()
                    .position(|s| s.clip.as_ref().is_some_and(|c| c.id == clip))
            });
            if let Some(slot) = slot {
                staged.launch_at(track, slot, BeatTime::ZERO)?;
            }
        }
        *self = staged;
        clock.play(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn global_play_restarts_the_clip_and_explicit_track_stop_disarms_it() {
        let now = Instant::now();
        let project = super::super::tests::project(0.0, true);
        let mut clock = InternalClock::new(project.tempo, project.time_signature);
        let mut planner = Planner::new(project, Duration::ZERO).unwrap();
        planner.launch(TrackId(1), 0, &mut clock, now).unwrap();
        let stop = now + Duration::from_millis(125);
        planner.stop_transport(&clock, stop);
        clock.stop(stop);
        assert_eq!(planner.resume_clips.len(), 1);
        clock.rewind();
        planner
            .play_transport(&mut clock, stop + Duration::from_secs(1))
            .unwrap();
        assert_eq!(planner.voices.len(), 1);
        assert_eq!(planner.voices[0].launch, BeatTime::ZERO);
        assert!(!planner.voices[0].silent);
        let stop = stop + Duration::from_secs(2);
        planner.stop_transport(&clock, stop);
        clock.stop(stop);
        planner.request_stop(TrackId(1), &clock, stop).unwrap();
        planner
            .play_transport(&mut clock, stop + Duration::from_secs(1))
            .unwrap();
        assert!(planner.voices.is_empty());
    }
}
