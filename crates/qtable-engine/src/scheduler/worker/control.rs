//! Command reception/holding and application on the playback thread.
use super::*;

impl Worker {
    pub(super) fn wait(
        &mut self,
        now: Instant,
    ) -> Result<Option<(Envelope, Instant)>, commands::Disconnected> {
        let next = self.pending.front();
        let next_send = next_send(&self.pending)
            .into_iter()
            .chain(self.oneshots.deadline())
            .min();
        // Clip deadlines spin for their last 6 ms, frames for their last 1 ms.
        let wait = next.as_ref().map_or(Duration::from_millis(7), |e| {
            e.at.saturating_duration_since(now)
        });
        let wait = self
            .oneshots
            .deadline()
            .map_or(wait, |at| wait.min(at.saturating_duration_since(now)));
        let frame_wait = self
            .stages
            .deadline(&self.planner.project, &self.sender)
            .map(|at| at.saturating_duration_since(now));
        let receive = || {
            let receive = || {
                if let Some(coarse) = wait::coarse(wait, frame_wait) {
                    match self.io.rx.recv_timeout(coarse) {
                        Ok(c) => Ok(Some((c, Instant::now()))),
                        Err(mpsc::RecvTimeoutError::Disconnected) => Err(commands::Disconnected),
                        Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
                    }
                } else {
                    match self.io.rx.try_recv() {
                        Ok(c) => Ok(Some((c, Instant::now()))),
                        Err(mpsc::TryRecvError::Disconnected) => Err(commands::Disconnected),
                        Err(mpsc::TryRecvError::Empty) => Ok(None),
                    }
                }
            };
            let received = receive()?;
            Ok(received.map(|(mut envelope, d)| {
                self.effective_floor
                    .admit(&mut envelope, d, &self.planner, &self.clock);
                (envelope, d)
            }))
        };
        let earlier_frame = |arrived| {
            self.stages
                .earlier_frame(&self.planner.project, &self.sender, arrived)
        };
        self.held.next(next_send, earlier_frame, receive)
    }
    pub(super) fn on_command(&mut self, envelope: Envelope, arrived: Instant) -> bool {
        marks::mark_at("command.arrived", arrived);
        marks::mark("command.begin");
        self.late_sends.report(&mut self.io.tx);
        let effective = self.effective_floor.advance(&envelope, arrived);
        #[cfg(feature = "dev-hooks")]
        self.diagnostics
            .launch_admission(&mut self.io.tx, &envelope, arrived);
        let command = envelope.command;
        let values_only = commands::policy(&command) == commands::Policy::Values;
        if !values_only {
            self.pending.clear();
            self.backlogs.clear();
            self.quiet_until = None;
            self.plan_consumed = false;
        }
        debug_assert!(
            !self.stages.holds_events(),
            "a command applied under a held bundle"
        );
        let sounding = (!values_only).then(|| self.planner.sounding_tracks());
        #[cfg(feature = "dev-hooks")]
        let applied = Instant::now();
        let io = commands::Io {
            sender: &mut self.sender,
            tx: &mut self.io.tx,
            resolver: &self.io.resolver,
            reaper: &self.io.reaper,
            oneshots: &mut self.oneshots,
        };
        if !commands::apply(
            command,
            effective,
            &mut self.planner,
            &mut self.clock,
            &mut self.stages,
            io,
        ) {
            return false;
        }
        #[cfg(feature = "dev-hooks")]
        self.diagnostics
            .command_applied(&mut self.io.tx, arrived, applied);
        self.publisher.mark_commanded();
        if let Some(sounding) = sounding {
            silence::report(self.planner.silenced_since(sounding), &mut self.io.tx);
            self.late_sends.update_bound(&self.planner.project);
        }
        marks::mark("command.end");
        true
    }
}
