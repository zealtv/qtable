//! Optional measurement setup and reporting, separate from playback decisions.
//!
//! Phase marks, the re-anchored frame grid and the command timestamps exist only with the
//! `dev-hooks` feature, which release builds leave off. Packet diagnostics (`Sent` events) stay.
use super::*;

/// Packet diagnostics plus phase marks for launch-timing measurements.
#[cfg(feature = "dev-hooks")]
pub fn spawn_with_marks(project: Project, lookahead: Duration) -> io::Result<SchedulerHandle> {
    worker::spawn_configured(
        project,
        lookahead,
        Diagnostics {
            packets: true,
            phase_marks: true,
            ..Diagnostics::default()
        },
    )
}

/// Re-anchor every output's frame grid at transport start, with a tick `lead` before it.
/// For phase-controlled measurements only; ordinary playback keeps the scheduler's anchor.
#[cfg(feature = "dev-hooks")]
pub fn spawn_with_grid_lead(
    project: Project,
    lookahead: Duration,
    lead: Duration,
) -> io::Result<SchedulerHandle> {
    worker::spawn_configured(
        project,
        lookahead,
        Diagnostics {
            packets: true,
            grid_lead: Some(lead),
            ..Diagnostics::default()
        },
    )
}

#[derive(Default)]
pub(super) struct Diagnostics {
    packets: bool,
    #[cfg(feature = "dev-hooks")]
    phase_marks: bool,
    #[cfg(feature = "dev-hooks")]
    grid_lead: Option<Duration>,
}

impl Diagnostics {
    pub(super) fn packets(packets: bool) -> Self {
        #[cfg_attr(not(feature = "dev-hooks"), allow(clippy::needless_update))]
        Self {
            packets,
            ..Self::default()
        }
    }

    #[cfg(feature = "dev-hooks")]
    pub(super) fn start(&self) {
        if self.phase_marks {
            marks::enable();
        }
    }

    pub(super) fn configure(&self, stages: &mut output_stage::OutputStages) {
        stages.diagnostics = self.packets;
        #[cfg(feature = "dev-hooks")]
        {
            stages.grid_lead = self.grid_lead;
        }
    }

    #[cfg(feature = "dev-hooks")]
    pub(super) fn drain_marks(&self, tx: &mut EventSink) {
        if self.phase_marks {
            let marks = marks::take();
            if !marks.is_empty() {
                tx.send(SchedulerEvent::Marks(marks));
            }
        }
    }

    #[cfg(feature = "dev-hooks")]
    pub(super) fn launch_admission(
        &self,
        tx: &mut EventSink,
        envelope: &Envelope,
        dequeued: Instant,
    ) {
        if self.phase_marks || self.grid_lead.is_some() {
            commands::admission::report(envelope, dequeued, tx);
        }
    }

    #[cfg(feature = "dev-hooks")]
    pub(super) fn command_applied(&self, tx: &mut EventSink, arrived: Instant, applied: Instant) {
        // Plain packet diagnostics keep their existing event stream.
        if self.phase_marks || self.grid_lead.is_some() {
            tx.send(SchedulerEvent::CommandApplied { arrived, applied });
        }
    }
}
