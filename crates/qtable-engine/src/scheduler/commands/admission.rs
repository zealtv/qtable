//! Receive-time decisions are made once, at first dequeue, before the D-based hold.
use super::*;

/// Musical admission grace, independent of clip protection and stale-send bounds.
const GRACE: Duration = Duration::from_millis(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionOutcome {
    Admitted,
    Stopped,
    Unquantised,
    InvalidContext,
    EpochChanged,
    ClockUnavailable,
    Expired,
    OlderBoundary,
    FloorConflict,
    NoAction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::scheduler) enum Manual {
    Slot(TrackId, usize),
    Scene(SceneId),
    Group(TrackId, usize),
    Stop,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::scheduler) struct Decision {
    pub(super) effective: Instant,
    // Read only by the launch-admission report, which the `dev-hooks` feature enables.
    #[cfg_attr(not(feature = "dev-hooks"), allow(dead_code))]
    pub(in crate::scheduler) outcome: AdmissionOutcome,
}

impl EffectiveFloor {
    pub(in crate::scheduler) fn admit(
        &self,
        envelope: &mut Envelope,
        dequeued: Instant,
        planner: &Planner,
        clock: &InternalClock,
    ) {
        let Policy::Manual(action) = policy(&envelope.command) else {
            return;
        };
        debug_assert!(envelope.admitted.is_none());
        let decision = self.decide(envelope, dequeued, planner, clock, action);
        envelope.admitted = Some(match decision {
            Ok(effective) => Decision {
                effective,
                outcome: AdmissionOutcome::Admitted,
            },
            Err(outcome) => Decision {
                effective: dequeued,
                outcome,
            },
        });
    }

    fn decide(
        &self,
        envelope: &Envelope,
        d: Instant,
        planner: &Planner,
        clock: &InternalClock,
        action: Manual,
    ) -> Result<Instant, AdmissionOutcome> {
        use AdmissionOutcome::*;
        if !clock.is_playing() {
            return Err(Stopped);
        }
        let p = envelope.enqueued_at;
        let r = if let Some(submission) = envelope.submission {
            let r = submission.context.receipt.received_at;
            if !submission.valid_generation
                || r > submission.submitted_at
                || submission.submitted_at > p
            {
                return Err(InvalidContext);
            }
            r
        } else {
            p
        };
        if p > d {
            return Err(InvalidContext);
        }
        if self.epoch.is_some_and(|epoch| r < epoch) {
            return Err(EpochChanged);
        }
        let e = self.at.map_or(r, |floor| r.max(floor));
        let receipt_beat = clock.beat_at(r).ok_or(ClockUnavailable)?;
        let effective_beat = clock.beat_at(e).ok_or(ClockUnavailable)?;
        let dequeue_beat = clock.beat_at(d).ok_or(ClockUnavailable)?;
        let signature = clock.time_signature();
        let mut checked = false;
        let mut check = |grid: GlobalQuantisation| -> Result<(), AdmissionOutcome> {
            if grid == GlobalQuantisation::None {
                return Err(Unquantised);
            }
            checked = true;
            let b = grid
                .next_boundary(receipt_beat, signature)
                .map_err(|_| ClockUnavailable)?;
            let at = clock.instant_at(b).ok_or(ClockUnavailable)?;
            if at <= d {
                if d.duration_since(at) > GRACE {
                    return Err(Expired);
                }
                // Strict next-boundary at B is the following point. If that point
                // has also arrived, B is no longer the most recent boundary.
                let following = grid
                    .next_boundary(b, signature)
                    .map_err(|_| ClockUnavailable)?;
                if following <= dequeue_beat {
                    return Err(OlderBoundary);
                }
            }
            if grid
                .next_boundary(effective_beat, signature)
                .map_err(|_| ClockUnavailable)?
                != b
            {
                return Err(FloorConflict);
            }
            Ok(())
        };
        let project = &planner.project;
        let mut slot = |track, row| -> Result<(), AdmissionOutcome> {
            let slot = project.clip_slot(track, row).ok_or(NoAction)?;
            if let Some(clip) = &slot.clip {
                check(LaunchOrigin::Manual.grid(clip.quantisation, project.global_quantisation))
            } else if slot.has_stop_button {
                check(project.global_quantisation)
            } else {
                Ok(())
            }
        };
        match action {
            Manual::Slot(track, row) => slot(track, row)?,
            Manual::Group(group, row) => {
                for track in project.descendant_clip_track_ids(group).ok_or(NoAction)? {
                    slot(track, row)?;
                }
            }
            Manual::Scene(scene) => {
                let row = project
                    .scenes
                    .iter()
                    .position(|s| s.id == scene)
                    .ok_or(NoAction)?;
                for track in project.clip_tracks() {
                    slot(track.id, row)?;
                }
            }
            Manual::Stop => check(project.global_quantisation)?,
        }
        if !checked {
            return Err(NoAction);
        }
        Ok(e)
    }
}

/// All outcomes share the existing bounded queue and dropped-event accounting.
#[cfg(feature = "dev-hooks")]
pub(in crate::scheduler) fn report(envelope: &Envelope, dequeued: Instant, tx: &mut EventSink) {
    if let Some(decision) = envelope.admitted {
        tx.send(SchedulerEvent::LaunchAdmission {
            received: envelope
                .submission
                .map_or(envelope.enqueued_at, |s| s.context.receipt.received_at),
            enqueued: envelope.enqueued_at,
            dequeued,
            effective: decision.effective,
            submission: envelope.submission.map(|s| s.identity),
            sequence: envelope.submission.and_then(|s| s.context.receipt.sequence),
            outcome: decision.outcome,
        });
    }
}

#[cfg(test)]
mod tests;
