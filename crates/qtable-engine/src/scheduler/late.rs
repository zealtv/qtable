//! Bound send catch-up while retaining musical state transitions.
use std::time::{Duration, Instant};

use super::{DueEvent, EventSink, PlannedAction, Planner, SchedulerEvent};
use crate::clock::TempoClock;
use crate::model::Project;

// From 2026-10-11: allow ordinary launch/dispatch lateness,
// plus the full negative offset (track delay + output latency) of an immediate unquantised launch.
pub(super) const LATE_SEND_MARGIN: Duration = Duration::from_millis(50);

pub(super) struct LateSends {
    bound: Duration,
    dropped: u64,
}

impl LateSends {
    pub(super) fn new(project: &Project) -> Self {
        let mut result = Self {
            bound: LATE_SEND_MARGIN,
            dropped: 0,
        };
        result.update_bound(project);
        result
    }

    pub(super) fn update_bound(&mut self, project: &Project) {
        // Planner validates representable offsets before accepting the project.
        let delay = super::delay::max_early(project).unwrap_or(Duration::MAX);
        self.bound = delay.saturating_add(LATE_SEND_MARGIN);
    }

    pub(super) fn is_stale(&self, lateness: Duration) -> bool {
        lateness > self.bound
    }

    /// Count and skip the sends already past the bound, instead of planning each one to drop it.
    pub(super) fn skip_stale(
        &mut self,
        planner: &mut Planner,
        clock: &impl TempoClock,
        now: Instant,
    ) {
        let skipped = planner.skip_stale_sends(clock, now, self.bound)
            + planner.skip_stale_jumps(clock, now, self.bound);
        self.dropped = self.dropped.saturating_add(skipped);
    }

    /// Called only for Send actions, immediately before touching the socket.
    pub(super) fn discard(
        &mut self,
        planner: &mut Planner,
        event: &DueEvent,
        lateness: Duration,
    ) -> bool {
        let has_packet = match &event.action {
            PlannedAction::Send { packet, .. } => packet.is_some(),
            PlannedAction::Automate { .. } => true,
            _ => return false,
        };
        if !self.is_stale(lateness) {
            return false;
        }
        planner.consume(event);
        if has_packet {
            self.dropped = self.dropped.saturating_add(1);
        }
        true
    }

    /// Flush once catch-up reaches a fresh send, a future deadline or a command.
    /// Keep accumulating across overdue follow/stop/silence transitions.
    pub(super) fn report(&mut self, events: &mut EventSink) {
        if self.dropped != 0 {
            let count = std::mem::take(&mut self.dropped);
            events.send(SchedulerEvent::DroppedLateSends { count });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::{InternalClock, TempoClock};
    use crate::model::{BeatTime, TrackId};
    use std::sync::mpsc;

    fn setup(delay: f64, looping: bool) -> (Instant, InternalClock, Planner, LateSends) {
        let project = super::super::tests::project(delay, looping);
        let late = LateSends::new(&project);
        let mut planner = Planner::new(project, Duration::from_millis(12)).unwrap();
        let start = Instant::now();
        let mut clock = InternalClock::default();
        clock.play(start);
        planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
        (start, clock, planner, late)
    }

    fn dropped_at_immediate_launch(delay: f64, latency: f64) -> u64 {
        let mut project = super::super::tests::project(delay, true);
        project.outputs[0].delay_ms = latency;
        let mut late = LateSends::new(&project);
        let mut planner = Planner::new(project, Duration::from_millis(12)).unwrap();
        let start = Instant::now();
        let mut clock = InternalClock::default();
        clock.play(start);
        planner.launch_at(TrackId(1), 0, BeatTime::ZERO).unwrap();
        late.skip_stale(&mut planner, &clock, start);
        let (tx, rx) = mpsc::sync_channel(4);
        late.report(&mut EventSink::new(tx));
        match rx.try_recv() {
            Ok(SchedulerEvent::DroppedLateSends { count }) => count,
            _ => 0,
        }
    }

    // From 2026-10-11: the same -500 ms offset must keep its early sends
    // whether it comes from track delay or from output latency.
    #[test]
    fn negative_output_latency_widens_the_bound_like_track_delay() {
        assert_eq!(dropped_at_immediate_launch(-500.0, 0.0), 0);
        assert_eq!(dropped_at_immediate_launch(0.0, -500.0), 0);
        assert_eq!(dropped_at_immediate_launch(-300.0, -200.0), 0);
    }

    #[test]
    fn stalled_dispatch_drops_cached_and_replanned_sends_then_resumes() {
        let (start, clock, mut planner, mut late) = setup(0.0, true);
        // Cache a candidate before the scheduler stalls; dispatch checks its age
        // as well as the backlog subsequently planned at the new wall time.
        let cached = planner.plan(&clock, start).remove(0);
        let now = start + Duration::from_millis(750);
        assert!(late.discard(&mut planner, &cached, now.duration_since(cached.at)));
        let backlog = planner.plan(&clock, now);
        assert_eq!(backlog.len(), 5);
        let mut fresh = 0;
        for event in backlog {
            if !late.discard(
                &mut planner,
                &event,
                now.saturating_duration_since(event.at),
            ) {
                fresh += 1;
                planner.consume(&event);
            }
        }
        assert_eq!(fresh, 2); // The two events exactly at 750 ms still send.
        let (tx, rx) = mpsc::sync_channel(8);
        late.report(&mut EventSink::new(tx.clone()));
        assert!(matches!(
            rx.try_recv(),
            Ok(SchedulerEvent::DroppedLateSends { count: 4 })
        ));
        late.report(&mut EventSink::new(tx.clone()));
        assert!(rx.try_recv().is_err());
        assert!(planner.plan(&clock, now).is_empty());
        let resumed = planner.plan(&clock, start + Duration::from_secs(1));
        assert_eq!(resumed.len(), 1);
        assert_eq!(resumed[0].at, start + Duration::from_secs(1));
    }

    #[test]
    fn stale_hits_on_an_inactive_target_advance_without_a_drop_notice() {
        let (start, clock, mut planner, mut late) = setup(0.0, true);
        let mut project = (*planner.project).clone();
        project.tracks[0].devices[0].active = false;
        planner.update_project(project, &clock, start).unwrap();
        let cached = planner.plan(&clock, start).remove(0);
        assert!(matches!(
            cached.action,
            PlannedAction::Send { packet: None, .. }
        ));
        let now = start + Duration::from_millis(100);
        assert!(late.discard(&mut planner, &cached, now.duration_since(cached.at)));
        assert!(planner.plan(&clock, now).is_empty());
        let (tx, rx) = mpsc::sync_channel(4);
        late.report(&mut EventSink::new(tx));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn bound_includes_negative_delay_and_strict_margin_but_not_positive_or_lookahead() {
        let (start, clock, mut planner, mut late) = setup(-200.0, true);
        let event = planner.plan(&clock, start).remove(0);
        assert_eq!(event.at, start - Duration::from_millis(200));
        assert!(!late.discard(&mut planner, &event, Duration::from_millis(200)));
        assert!(!late.discard(&mut planner, &event, Duration::from_millis(250)));
        assert!(late.discard(&mut planner, &event, Duration::from_nanos(250_000_001)));
        let mut project = super::super::tests::project(1000.0, true);
        late.update_bound(&project);
        assert!(!late.is_stale(Duration::from_millis(50)));
        assert!(late.is_stale(Duration::from_nanos(50_000_001)));
        project.tracks[0].delay_ms = -500.0;
        late.update_bound(&project);
        assert!(!late.is_stale(Duration::from_millis(550)));
        assert!(late.is_stale(Duration::from_millis(551)));
    }

    #[test]
    fn overdue_state_events_are_retained_and_do_not_split_drop_report() {
        let (start, clock, mut planner, mut late) = setup(0.0, false);
        let now = start + Duration::from_secs(1);
        let events = planner.plan(&clock, now);
        assert_eq!(events.len(), 4);
        for event in events {
            let discarded = late.discard(&mut planner, &event, now.duration_since(event.at));
            assert_eq!(
                discarded,
                matches!(event.action, PlannedAction::Send { .. })
            );
            if !discarded {
                assert!(matches!(
                    event.action,
                    PlannedAction::TrackSilent(TrackId(1))
                ));
                planner.consume(&event);
            }
        }
        assert!(planner.voices.is_empty());
        let (tx, rx) = mpsc::sync_channel(8);
        late.report(&mut EventSink::new(tx.clone()));
        assert!(matches!(
            rx.try_recv(),
            Ok(SchedulerEvent::DroppedLateSends { count: 3 })
        ));

        for action in [
            PlannedAction::RetireVoice,
            PlannedAction::StopTrack(TrackId(1)),
            PlannedAction::Follow {
                beat: BeatTime::ZERO,
                launch_beat: BeatTime::ZERO,
                signature: clock.time_signature(),
            },
            PlannedAction::SceneFollow {
                scene: crate::model::SceneId(1),
                beat: BeatTime::ZERO,
                launch_beat: BeatTime::ZERO,
                tempo: clock.tempo(),
                signature: clock.time_signature(),
            },
        ] {
            let event = DueEvent {
                at: start,
                action,
                voice: 0,
                cursor: 0,
            };
            assert!(!late.discard(&mut planner, &event, Duration::from_secs(1)));
        }
        late.report(&mut EventSink::new(tx.clone()));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn engine_status_accumulates_drop_reports() {
        let mut status = crate::status::StatusLog::default();
        status.record(SchedulerEvent::DroppedLateSends { count: 4 });
        status.record(SchedulerEvent::DroppedLateSends { count: 3 });
        assert_eq!(status.status(None).dropped_late_sends, 7);
        assert_eq!(status.status(None).error_count, 0);
    }
}
