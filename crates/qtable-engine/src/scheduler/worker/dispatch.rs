//! Due hits, exact jumps, lifecycle events and packet turns.
use super::*;

/// Where a clip hit goes and what it reports.
struct Hit<'a> {
    sender: &'a mut OscSender,
    tx: &'a mut EventSink,
    activity: &'a crate::activity::Activity,
    report_gate: &'a mut ReportGate,
}

/// Send one due clip hit. An output deleted (or Main unset) since planning has nothing to send to,
/// and a hit on an inactive device sends nothing; the caller still consumes the event. The output
/// stage reports it `Sent` (with diagnostics) when its datagram goes.
fn send_clip_hit(
    event: &DueEvent,
    output: TrackId,
    packet: Option<&OscPacket>,
    planner: &Planner,
    stages: &mut output_stage::OutputStages,
    hit: Hit,
) {
    let Some(packet) = packet else {
        return;
    };
    let Some(output) = planner
        .project
        .destination(output)
        .filter(|o| o.destination().is_some())
    else {
        return;
    };
    send_hit(
        event.at,
        || {
            planner
                .voices
                .iter()
                .find(|v| v.id == event.voice)
                .map(|v| v.track)
        },
        output,
        packet,
        stages,
        hit,
    );
}

/// The shared send-and-report path for planned clip hits and queued manual hits. Eligibility
/// belongs to the caller; the output stage emits `Sent` only when the datagram actually goes.
fn send_hit(
    at: Instant,
    track: impl FnOnce() -> Option<TrackId>,
    output: &crate::model::Track,
    packet: &OscPacket,
    stages: &mut output_stage::OutputStages,
    hit: Hit,
) {
    marks::mark("hit.begin");
    let sent = stages.event(hit.sender, hit.tx, output, packet, at, Instant::now());
    marks::mark("hit.end");
    if let Err(e) = sent {
        // Unresolved outputs report at most once per interval.
        if hit.sender.is_resolved(output) {
            hit.tx.send(SchedulerEvent::Error(e));
        } else if hit.report_gate.allow(output.id, Instant::now()) {
            hit.tx.send(SchedulerEvent::Error(unresolved_error(output)));
        }
    } else {
        hit.activity.record(output.id, track(), Instant::now());
    }
}

impl Worker {
    pub(super) fn dispatch_turn(&mut self, now: Instant) {
        // While a frame is part-way out, due hits on other outputs and exact jumps take turns
        // between its packets, respecting the voice/output/lifecycle barriers in turns::turn_index.
        let event = turns::turn_index(&self.pending, now, |output| self.stages.sending_to(output))
            .and_then(|index| self.pending.remove(index));
        match event {
            Some(event) => {
                marks::mark("turn.event");
                self.dispatch_clip(event, false);
            }
            None => {
                marks::mark("turn.packet");
                self.stages.send_next(
                    &self.planner.project,
                    &mut self.sender,
                    &self.io.activity,
                    &mut self.io.tx,
                );
            }
        }
        self.stages.flush_events(
            &self.planner.project,
            &mut self.sender,
            &mut self.io.tx,
            next_send(&self.pending),
        );
    }
    pub(super) fn dispatch_frames(&mut self, event: Option<&DueEvent>) -> bool {
        // A removed event has already passed the stale-send gate. Otherwise inspect the front
        // in place, without cloning its fields or removing it from the cluster's remaining chain.
        let event = event.or_else(|| {
            self.pending.front().filter(|event| {
                event.at <= Instant::now()
                    && matches!(event.action, PlannedAction::Automate { .. })
                    && !self
                        .late_sends
                        .is_stale(Instant::now().saturating_duration_since(event.at))
            })
        });
        let mut jump_applied = false;
        if let Some(DueEvent {
            at,
            action: PlannedAction::Automate { output, fields },
            ..
        }) = event
        {
            self.stages.jump(*output, fields, *at);
            jump_applied = true;
        }
        // Start sends and exact jumps win ties; ordinary grid frames wait for the bounded cluster.
        marks::span("dispatch.begin", "dispatch.end", HOUSEKEEPING, || {
            self.stages.dispatch(
                &self.planner.project,
                &mut self.sender,
                &self.io.activity,
                &mut self.io.tx,
                &chain(
                    &self.pending,
                    &self.oneshots,
                    self.horizon,
                    Instant::now(),
                    &self.held,
                ),
            )
        });
        jump_applied
    }
    pub(super) fn dispatch_oneshot(&mut self) -> bool {
        if let Some(shot) = self
            .oneshots
            .pop_due(Instant::now(), self.pending.front().map(|e| e.at))
        {
            if self.planner.project.track_sends(shot.track)
                && self
                    .planner
                    .project
                    .event_path(shot.track, &shot.event)
                    .is_some()
                && let Some(output) = self
                    .planner
                    .project
                    .destination(shot.output)
                    .filter(|o| o.accepts_sends())
            {
                send_hit(
                    shot.at,
                    || Some(shot.track),
                    output,
                    &shot.packet,
                    &mut self.stages,
                    Hit {
                        sender: &mut self.sender,
                        tx: &mut self.io.tx,
                        activity: &self.io.activity,
                        report_gate: &mut self.report_gate,
                    },
                );
            }
            return true;
        }
        false
    }
    pub(super) fn dispatch_clip(&mut self, event: DueEvent, jump_applied: bool) {
        match &event.action {
            PlannedAction::Automate { .. } => {
                let lateness = Instant::now().saturating_duration_since(event.at);
                if self.late_sends.discard(&mut self.planner, &event, lateness) {
                    return;
                }
                // The deadline may have crossed after the earlier check,
                // while dispatching an output frame. Never consume an exact
                // jump without first applying and sending it.
                if !jump_applied {
                    self.dispatch_frames(Some(&event));
                }
            }
            PlannedAction::Send { output, packet } => {
                let lateness = Instant::now().saturating_duration_since(event.at);
                if self.late_sends.discard(&mut self.planner, &event, lateness) {
                    return;
                }
                self.backlogs.served(*output);
                send_clip_hit(
                    &event,
                    *output,
                    packet.as_ref(),
                    &self.planner,
                    &mut self.stages,
                    Hit {
                        sender: &mut self.sender,
                        tx: &mut self.io.tx,
                        activity: &self.io.activity,
                        report_gate: &mut self.report_gate,
                    },
                );
            }
            PlannedAction::TrackSilent(_)
            | PlannedAction::StopTrack(_)
            | PlannedAction::RetireVoice
            | PlannedAction::Follow { .. }
            | PlannedAction::SceneFollow { .. } => {}
        }
        self.consume(&event);
        if !matches!(
            event.action,
            PlannedAction::Send { .. } | PlannedAction::Automate { .. }
        ) {
            marks::mark(match event.action {
                PlannedAction::TrackSilent(_) => "consumed.silent",
                PlannedAction::StopTrack(_) => "consumed.stop",
                PlannedAction::RetireVoice => "consumed.retire",
                PlannedAction::Follow { .. } => "consumed.follow",
                PlannedAction::SceneFollow { .. } => "consumed.scene_follow",
                PlannedAction::Send { .. } | PlannedAction::Automate { .. } => "consumed.send",
            });
            // Most lifecycle events (a track going silent, a cut-off voice
            // retiring) change no driver still live; re-syncing for them cost
            // ~2 ms each with 256 driven fields (launch-timing-investigation).
            if self
                .stages
                .automation_changes(&self.planner, &self.clock, event.at)
            {
                self.io.reaper.retire(self.stages.sync_automation(
                    &self.planner,
                    &self.clock,
                    event.at,
                ));
            }
            self.publisher.mark_dirty();
        }
    }
    pub(super) fn consume(&mut self, event: &DueEvent) {
        let (silent, error) = self.planner.consume_with_error(event);
        silence::report(silent, &mut self.io.tx);
        if let Some(error) = error {
            self.io.tx.send(SchedulerEvent::Error(error));
        }
    }
}
