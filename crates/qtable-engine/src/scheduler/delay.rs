//! Recompute speculative planning from the caller's base and current track delays.
use super::{Duration, Project, invalid, io};

/// Shift a musical deadline into send time, or use the negated delay to undo it.
/// Projects validate finite, bounded delays before entering the scheduler.
pub(super) fn shift(at: std::time::Instant, delay_ms: f64) -> Option<std::time::Instant> {
    let amount = Duration::try_from_secs_f64(delay_ms.abs() / 1000.0).ok()?;
    if delay_ms < 0.0 {
        at.checked_sub(amount)
    } else {
        at.checked_add(amount)
    }
}

/// How far ahead of its musical time any track can send: the most negative combined
/// track delay + output latency over enabled routed outputs (or the bare track delay).
pub(super) fn max_early_ms(project: &Project) -> f64 {
    project
        .clip_tracks()
        .map(|track| {
            let delay = project.effective_delay_ms(track.id);
            let earliest = project
                .routed_outputs(track.id)
                .filter(|o| o.mixer.active)
                .map(|o| project.send_delay_ms(track.id, o.id))
                .fold(delay, f64::min);
            (-earliest).max(0.0)
        })
        .fold(0.0, f64::max)
}

/// `max_early_ms` as a duration; projects validate finite, bounded offsets first.
pub(super) fn max_early(project: &Project) -> io::Result<Duration> {
    Duration::try_from_secs_f64(max_early_ms(project) / 1000.0)
        .map_err(|_| invalid("unrepresentable track delay"))
}

pub(super) fn lookahead(project: &Project, base: Duration) -> io::Result<Duration> {
    Ok(base.max(max_early(project)?))
}

#[cfg(test)]
mod tests {
    use super::super::Planner;
    use super::*;
    use crate::clock::{InternalClock, TempoClock};
    use crate::model::{BeatTime, TrackId};
    use std::time::Instant;

    #[test]
    fn updates_grow_and_shrink_back_to_original_base() {
        let mut project = super::super::tests::project(-1000.0, true);
        let base = Duration::from_millis(12);
        let mut planner = Planner::new(project.clone(), base).unwrap();
        let mut clock = InternalClock::default();
        let now = Instant::now();
        clock.play(now);
        planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
        assert_eq!(planner.lookahead(), Duration::from_secs(1));
        for (delay_ms, expected_ms) in [
            (-200.0, 200),
            (-5.0, 12),
            (1000.0, 12),
            (-500.0, 500),
            (0.0, 12),
        ] {
            project.tracks[0].delay_ms = delay_ms;
            planner
                .update_project(project.clone(), &clock, now)
                .unwrap();
            assert_eq!(planner.lookahead(), Duration::from_millis(expected_ms));
        }
        // Removing the delayed track also restores the base.
        project.tracks[0].delay_ms = -1000.0;
        planner
            .update_project(project.clone(), &clock, now)
            .unwrap();
        project.tracks.clear();
        assert_eq!(
            planner.update_project(project, &clock, now).unwrap(),
            [TrackId(1)]
        );
        assert_eq!(planner.lookahead(), base);
    }

    #[test]
    fn group_and_main_delay_add_to_the_track_delay() {
        use crate::model::Track;
        let mut project = super::super::tests::project(-100.0, true);
        {
            let mut group = Track::new_group(crate::model::TrackId(101), "g", project.scenes.len());
            group.delay_ms = -200.0;
            group.colour = crate::model::LIVE_CLIP_PALETTE[0];
            project.insert_group(group, &[TrackId(1)]);
        }
        assert_eq!(max_early_ms(&project), 300.0);
        project.main.delay_ms = -50.0;
        assert_eq!(max_early_ms(&project), 350.0);
        // A positive group delay cancels the member's negative one.
        project[crate::model::TrackId(101)].delay_ms = 100.0;
        project.main.delay_ms = 0.0;
        assert_eq!(max_early_ms(&project), 0.0);
        let planner = Planner::new(project, Duration::ZERO).unwrap();
        assert_eq!(planner.tail_offset_ms(TrackId(1)), 0.0);
    }

    #[test]
    fn largest_current_negative_delay_wins_and_large_caller_base_is_preserved() {
        let mut project = super::super::tests::project(-1000.0, true);
        let mut second = crate::scheduler::tests::copy_track(&project.tracks[0], TrackId(2));
        second.delay_ms = -400.0;
        for slot in &mut second.slots {
            slot.clip.as_mut().unwrap().id.0 += 10;
        }
        project.tracks.push(second);
        let clock = InternalClock::default();
        let now = Instant::now();
        let mut planner = Planner::new(project.clone(), Duration::ZERO).unwrap();
        project.tracks[0].delay_ms = -50.0;
        planner
            .update_project(project.clone(), &clock, now)
            .unwrap();
        assert_eq!(planner.lookahead(), Duration::from_millis(400));
        project.tracks[1].delay_ms = 400.0;
        planner
            .update_project(project.clone(), &clock, now)
            .unwrap();
        assert_eq!(planner.lookahead(), Duration::from_millis(50));
        project.tracks[0].delay_ms = 0.0;
        planner
            .update_project(project.clone(), &clock, now)
            .unwrap();
        assert_eq!(planner.lookahead(), Duration::ZERO);
        let base = Duration::from_secs(2);
        let mut planner = Planner::new(project.clone(), base).unwrap();
        project.tracks[0].delay_ms = -1000.0;
        planner.update_project(project, &clock, now).unwrap();
        assert_eq!(planner.lookahead(), base);
    }

    #[test]
    fn invalid_update_does_not_change_lookahead_or_playing_voice() {
        let mut project = super::super::tests::project(-100.0, true);
        let mut planner = Planner::new(project.clone(), Duration::from_millis(12)).unwrap();
        planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
        let clock = InternalClock::default();
        let now = Instant::now();
        project.tracks[0].delay_ms = -10000.0;
        assert!(Planner::new(project.clone(), Duration::ZERO).is_err());
        assert!(planner.update_project(project, &clock, now).is_err());
        assert_eq!(planner.lookahead(), Duration::from_millis(100));
        assert_eq!(planner.project.tracks[0].delay_ms, -100.0);
        assert_eq!(planner.voices.len(), 1);
    }
}
