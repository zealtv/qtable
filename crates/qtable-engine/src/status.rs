//! What clients can see of the scheduler's event stream: a bounded digest.
use std::collections::VecDeque;

use crate::model::TrackId;
use crate::scheduler::SchedulerEvent;

const RECENT_ERRORS: usize = 8;
const RECENT_SILENCED: usize = 16;

/// Bounded summary of drained scheduler events.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EngineStatus {
    /// Most recent scheduler error, if any occurred since the engine started.
    pub last_error: Option<String>,
    /// Latest output-stage warning, with an occurrence count for client dismissal.
    pub last_warning: Option<String>,
    pub warning_count: u64,
    pub recent_warnings: Vec<String>,
    /// Errors drained so far, including ones that no longer appear in `recent_errors`.
    pub error_count: u64,
    /// Events (errors among them) the scheduler dropped because its queue was full.
    pub dropped_events: u64,
    /// Stale OSC packets reported as dropped during scheduler catch-up.
    pub dropped_late_sends: u64,
    /// The latest few errors, oldest first.
    pub recent_errors: Vec<String>,
    /// Tracks that recently went silent, oldest first.
    pub silenced_tracks: Vec<TrackId>,
    /// Set once the scheduler thread has died; playback no longer advances.
    pub scheduler_failure: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct StatusLog {
    status: EngineStatus,
    errors: VecDeque<String>,
    warnings: VecDeque<String>,
    silenced: VecDeque<TrackId>,
}

impl StatusLog {
    pub(crate) fn record(&mut self, event: SchedulerEvent) {
        match event {
            SchedulerEvent::Error(error) => {
                let message = error.to_string();
                self.status.error_count = self.status.error_count.saturating_add(1);
                self.status.last_error = Some(message.clone());
                push_bounded(&mut self.errors, message, RECENT_ERRORS);
            }
            SchedulerEvent::OutputOversize { name, .. } => {
                self.status.warning_count = self.status.warning_count.saturating_add(1);
                let message = format!(
                    "{name}: OSC message too large for a bundle; sent plain (may fragment)"
                );
                self.status.last_warning = Some(message.clone());
                push_bounded(&mut self.warnings, message, RECENT_ERRORS);
            }
            SchedulerEvent::TrackSilent(track) => {
                push_bounded(&mut self.silenced, track, RECENT_SILENCED);
            }
            SchedulerEvent::EventsDropped { count } => {
                self.status.dropped_events = self.status.dropped_events.saturating_add(count);
            }
            SchedulerEvent::DroppedLateSends { count } => {
                self.status.dropped_late_sends =
                    self.status.dropped_late_sends.saturating_add(count);
            }
            SchedulerEvent::LaunchAdmission { .. }
            | SchedulerEvent::Playing(_)
            | SchedulerEvent::Sent { .. }
            | SchedulerEvent::FrameSent { .. } => {}
            #[cfg(feature = "dev-hooks")]
            SchedulerEvent::CommandApplied { .. } | SchedulerEvent::Marks(_) => {}
        }
    }

    pub(crate) fn status(&self, failure: Option<String>) -> EngineStatus {
        let mut status = self.status.clone();
        status.recent_errors = self.errors.iter().cloned().collect();
        status.recent_warnings = self.warnings.iter().cloned().collect();
        status.silenced_tracks = self.silenced.iter().copied().collect();
        status.scheduler_failure = failure;
        status
    }
}

fn push_bounded<T>(queue: &mut VecDeque<T>, value: T, cap: usize) {
    if queue.len() == cap {
        queue.pop_front();
    }
    queue.push_back(value);
}
