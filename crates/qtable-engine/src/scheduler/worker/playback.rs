//! Plan visibility, publication, and the ordered playback pass.
use super::*;

impl Worker {
    pub(super) fn run(mut self) {
        loop {
            marks::pass(GAP);
            let now = Instant::now();
            self.plan(now);
            if self.stages.sending() {
                self.dispatch_turn(now);
                continue;
            }
            self.publish(now);
            match self.wait(now) {
                Ok(Some((command, arrived))) => {
                    if !self.on_command(command, arrived) {
                        break;
                    }
                    continue;
                }
                Ok(None) => {}
                Err(commands::Disconnected) => break,
            }
            let jump_applied = self.dispatch_frames(None);
            if self.stages.sending() {
                // Consume the queued jump so another output's hit can go between its packets.
                if jump_applied && let Some(event) = self.pending.pop_front() {
                    self.consume(&event);
                }
                continue;
            }
            if self.dispatch_oneshot() {
                continue;
            }
            if let Some(event) = self.pending.front() {
                if Instant::now() >= event.at {
                    let index = self
                        .backlogs
                        .index(&self.pending, Instant::now())
                        .unwrap_or(0);
                    if let Some(event) = self.pending.remove(index) {
                        self.dispatch_clip(event, jump_applied);
                    }
                } else {
                    std::hint::spin_loop();
                }
            }
        }
    }
    pub(super) fn plan(&mut self, now: Instant) {
        // A frame that completed since the last pass was its output's turn; the hits
        // due behind it are its backlog.
        for output in self.stages.take_completed() {
            self.backlogs.completed(output, now, &self.pending);
        }
        self.backlogs.close_drained(&self.pending);
        // A grid frame waits for a cluster, but `pending` can outlive its plan's
        // horizon (it refills only once empty), so an empty chain proves nothing
        // beyond it. Plan again now, replacing `pending`: a plan returns every
        // unconsumed event up to its new horizon.
        let visibility = self.stages.take_replan();
        // While a frame is part-way out, don't repeat a plan already known to be empty:
        // nothing is due before its horizon unless a command has come since. An empty
        // plan before every remaining packet cost ~40 us each (cross-output-scheduling,
        // pair-v2).
        if visibility
            || self.pending.is_empty()
                && (!self.stages.sending() || self.quiet_until.is_none_or(|until| now >= until))
        {
            // A held bundle can't gain a member from this plan: every event the last
            // plan found has gone, and so has any at the bundle's instant. Send it now
            // rather than after planning (an empty plan and replan delayed it ~85 us,
            // a full plan ~100 us; bundle-send-timing validation-v1). After a command
            // the replan may find events due at that instant, so the bundle waits for
            // it, as does a bundle a manual hit due at its instant can still join.
            // Events still pending may yet join a held bundle.
            if self.plan_consumed && self.pending.is_empty() {
                self.stages.flush_events(
                    &self.planner.project,
                    &mut self.sender,
                    &mut self.io.tx,
                    self.oneshots.deadline(),
                );
            }
            self.plan_consumed = true;
            self.late_sends
                .skip_stale(&mut self.planner, &self.clock, now);
            let unplanned = marks::len();
            marks::mark(if visibility {
                "plan.visibility"
            } else {
                "plan.begin"
            });
            let (planned, until) = self.planner.plan_until(&self.clock, now);
            self.pending = planned.into();
            self.horizon = until;
            self.quiet_until = self
                .pending
                .is_empty()
                .then(|| now + self.planner.lookahead);
            if self.pending.is_empty() {
                // A slow empty plan is kept under its own label.
                if now.elapsed() >= HOUSEKEEPING {
                    marks::mark("plan.empty");
                } else {
                    marks::truncate(unplanned);
                }
            } else {
                marks::mark("plan.end");
            }
        }
    }
    pub(super) fn publish(&mut self, now: Instant) {
        marks::span("publish.begin", "publish.end", HOUSEKEEPING, || {
            self.stages.publish(
                &self.io.throughput,
                &self.planner.project,
                &mut self.io.tx,
                now,
            )
        });
        let next = self.pending.front();
        let next_send = next_send(&self.pending)
            .into_iter()
            .chain(self.oneshots.deadline())
            .min();
        if next_send.is_none_or(|due| due > now + Duration::from_millis(1)) {
            marks::span("replan.begin", "replan.end", HOUSEKEEPING, || {
                self.stages.replan_dirty()
            });
            #[cfg(feature = "dev-hooks")]
            self.diagnostics.drain_marks(&mut self.io.tx);
        }
        if now.saturating_duration_since(self.automation_published) >= Duration::from_millis(33)
            && next_send.is_none_or(|due| due > now + Duration::from_millis(1))
        {
            let state = match self.io.snapshot.try_lock() {
                Ok(state) => Some(state),
                Err(std::sync::TryLockError::Poisoned(error)) => Some(error.into_inner()),
                Err(std::sync::TryLockError::WouldBlock) => None,
            };
            if let Some(mut state) = state {
                state.automation =
                    marks::span("automation.begin", "automation.end", HOUSEKEEPING, || {
                        self.stages.automation_values(now)
                    });
                self.automation_published = now;
            }
        }
        // Bundled events leave once no event is due at their instant any more.
        self.stages.flush_events(
            &self.planner.project,
            &mut self.sender,
            &mut self.io.tx,
            next_send,
        );
        marks::span("retry.begin", "retry.end", HOUSEKEEPING, || {
            self.publisher.retry(
                &self.io.snapshot,
                &self.planner,
                &self.clock,
                now,
                next.map(|event| event.at),
            )
        });
        if next.is_none_or(|event| {
            event.at > now
                || (matches!(
                    event.action,
                    PlannedAction::Send { .. } | PlannedAction::Automate { .. }
                ) && !self
                    .late_sends
                    .is_stale(now.saturating_duration_since(event.at)))
        }) {
            marks::span("report.begin", "report.end", HOUSEKEEPING, || {
                self.late_sends.report(&mut self.io.tx)
            });
        }
    }
}
