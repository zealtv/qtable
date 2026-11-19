//! Every send to a receiver passes through its output stage (output-stage-design, 2026-10-15).
//! Clip events go out at once and are counted. Parameter values land in a slot per
//! `(ParameterRef, output)` and go out on frames, on a wall-clock grid at the output's Frame Rate
//! shared by every output from one anchor. With nothing to send, a stage has no deadline.
//! An optional Budget (messages per second) defers parameter changes and skips repeats but never
//! holds back an event; Refresh resends every value round-robin; Bundle packs messages into
//! datagrams of at most one Ethernet MTU.

pub(super) mod budget;
pub(super) mod bundle;
pub(super) mod cluster;
pub(super) mod frame;
mod send;
pub(super) mod slot;
mod stage;
pub(crate) mod throughput;

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Arc;
#[cfg(feature = "dev-hooks")]
use std::time::Duration;
use std::time::Instant;

use rosc::OscPacket;

use super::events::EventSink;
use super::parameter_controls::Control;
use super::{SchedulerEvent, delay, marks};
use crate::activity::Activity;
use crate::model::{ParameterRef, Project, Track, TrackId};
use crate::sender::OscSender;
use cluster::Chain;
use frame::FrameGrid;
use send::Out;
use slot::{Run, Slot};
use stage::Stage;

#[cfg(test)]
use throughput::Counts;

/// One stage per destination: every output track, and Main while it has a host.
pub(super) struct OutputStages {
    /// The scheduler thread's start, or the transport start less `grid_lead`: every output's
    /// frame grid counts from it.
    anchor: Instant,
    automation: Arc<super::automation::Automation>,
    stages: Vec<Stage>,
    /// Transport runs whose Every Frame sends may still be due, oldest first.
    runs: Vec<Run>,
    membership_changed: bool,
    warned_oversize: HashSet<TrackId>,
    /// Emit `FrameSent` for every frame, for measurements.
    pub diagnostics: bool,
    /// The worker samples actual send timing; deterministic test rigs use their supplied clock.
    #[cfg(test)]
    pub live_timing: bool,
    /// Frames leave a packet at a time through `send_next`, so the worker can send other outputs'
    /// due hits between them; otherwise each frame goes whole as it is built.
    #[cfg(test)]
    pub resumable: bool,
    /// Measurements only: at transport start, re-anchor every output's frame grid so that a tick
    /// falls this long before the start. None keeps the scheduler thread's anchor.
    #[cfg(feature = "dev-hooks")]
    pub grid_lead: Option<Duration>,
}

impl OutputStages {
    /// Stages that know every current value but owe no sends, as a set opens silently.
    pub fn new(project: &Project, controls: Vec<Control>, anchor: Instant) -> Self {
        let mut stages = Self {
            anchor,
            automation: Default::default(),
            stages: Vec::new(),
            runs: Vec::new(),
            membership_changed: true,
            warned_oversize: HashSet::new(),
            diagnostics: false,
            #[cfg(test)]
            live_timing: false,
            #[cfg(test)]
            resumable: false,
            #[cfg(feature = "dev-hooks")]
            grid_lead: None,
        };
        stages.apply(project, controls, anchor, true);
        stages
    }

    /// Playback always samples the live clock and yields between frame packets. Only the
    /// deterministic test rigs can select a synthetic clock or whole-frame dispatch.
    pub fn for_playback(project: &Project, controls: Vec<Control>, anchor: Instant) -> Self {
        let stages = Self::new(project, controls, anchor);
        #[cfg(test)]
        let stages = Self {
            live_timing: true,
            resumable: true,
            ..stages
        };
        stages
    }

    fn live_timing(&self) -> bool {
        #[cfg(test)]
        {
            self.live_timing
        }
        #[cfg(not(test))]
        {
            true
        }
    }

    fn resumable(&self) -> bool {
        #[cfg(test)]
        {
            self.resumable
        }
        #[cfg(not(test))]
        {
            true
        }
    }

    pub fn sync_automation(
        &mut self,
        planner: &super::Planner,
        clock: &crate::clock::InternalClock,
        now: Instant,
    ) -> Arc<super::automation::Automation> {
        self.sync_automation_control(planner, clock, now, &[], None)
    }

    pub fn sync_automation_control(
        &mut self,
        planner: &super::Planner,
        clock: &crate::clock::InternalClock,
        now: Instant,
        manual: &[crate::model::FieldRef],
        reenable: Option<&Option<crate::model::FieldRef>>,
    ) -> Arc<super::automation::Automation> {
        marks::mark("sync.begin");
        let previous = Arc::clone(&self.automation);
        let automation = Arc::make_mut(&mut self.automation);
        automation.sync(planner, clock, now);
        marks::mark("sync.automation");
        automation.control(now, manual, reenable);
        for stage in &mut self.stages {
            stage.automation = Arc::clone(&self.automation);
            for (target, slot) in &mut stage.slots {
                slot.forget_sample();
                if let Some(value) = self.automation.sample(target, now, slot.settings.delay_ms) {
                    slot.sample(value, now);
                }
            }
            stage.plan(&self.runs);
        }
        marks::mark("sync.end");
        previous
    }

    /// Whether a consumed event at `at` changed what drives automation from `at` on.
    pub fn automation_changes(
        &self,
        planner: &super::Planner,
        clock: &crate::clock::InternalClock,
        at: Instant,
    ) -> bool {
        self.automation.changes(planner, clock, at)
    }

    pub fn automation_values(&self, now: Instant) -> Vec<super::AutomationValue> {
        self.automation.values(now)
    }

    /// Accept an edited project: a changed value is stamped now; a new or re-addressed slot,
    /// or one whose receiver moved, sends its value again.
    pub fn update(&mut self, project: &Project, controls: Vec<Control>, now: Instant) {
        self.apply(project, controls, now, false);
    }

    /// Value-only edit: membership, routing and all unrelated prepared samples stay intact.
    pub fn update_parameters(
        &mut self,
        project: &Arc<Project>,
        fields: &[crate::model::FieldRef],
        now: Instant,
    ) -> Arc<super::automation::Automation> {
        marks::mark("parameters.begin");
        let previous = Arc::clone(&self.automation);
        Arc::make_mut(&mut self.automation).update_parameters(project, now, fields);
        let targets: HashSet<_> = fields.iter().map(|field| &field.parameter).collect();
        for stage in &mut self.stages {
            stage.automation = Arc::clone(&self.automation);
            for target in &targets {
                let Some(&index) = stage.slot_index.get(*target) else {
                    // Muted/soloed-away or unrouted fields still get their runtime override.
                    continue;
                };
                let (_, slot) = &mut stage.slots[index];
                if let Some(value) = project.parameter(target).and_then(|parameter| {
                    super::parameter_controls::send_values(target, &parameter)
                }) {
                    slot.edit(value, now);
                }
                slot.forget_sample();
                if let Some(value) = self.automation.sample(target, now, slot.settings.delay_ms) {
                    slot.sample(value, now);
                }
            }
            stage.plan(&self.runs);
        }
        marks::mark("parameters.end");
        previous
    }

    fn apply(&mut self, project: &Project, controls: Vec<Control>, now: Instant, quiet: bool) {
        let mut slots: HashMap<(TrackId, ParameterRef), Slot> = HashMap::new();
        let mut moved = Vec::new();
        let mut stages = Vec::new();
        for output in project.configured_destinations() {
            let Some(destination) = output.destination() else {
                continue;
            };
            let settings = output.output_stage().cloned().unwrap_or_default();
            let rate = settings.frame_rate;
            let stage = match self.stages.iter().position(|s| s.output == output.id) {
                Some(index) => {
                    let mut stage = self.stages.swap_remove(index);
                    if stage.destination != *destination {
                        stage.destination = destination.clone();
                        moved.push(output.id);
                    }
                    if stage.grid.rate() != rate {
                        stage.grid = FrameGrid::new(self.anchor, rate);
                    }
                    slots.extend(
                        stage
                            .slots
                            .drain(..)
                            .map(|(target, slot)| ((output.id, target), slot)),
                    );
                    stage
                }
                None => Stage::new(output.id, destination.clone(), self.anchor, rate),
            };
            let mut stage = stage;
            stage.configure(&settings, now);
            stages.push(stage);
        }
        self.stages = stages;
        self.membership_changed = true;
        for control in controls {
            let Some(stage) = self.stages.iter_mut().find(|s| s.output == control.output) else {
                continue;
            };
            let slot = match slots.remove(&(control.output, control.target.clone())) {
                Some(mut slot) => {
                    slot.settings = control.settings;
                    if slot.address != control.address {
                        slot.address = control.address;
                        slot.edit(control.send, now);
                        slot.resend(now);
                    } else {
                        slot.edit(control.send, now);
                    }
                    slot
                }
                None if quiet => Slot::quiet(control.address, control.send, control.settings, now),
                None => Slot::new(control.address, control.send, control.settings, now),
            };
            stage.slots.push((control.target, slot));
        }
        for stage in &mut self.stages {
            stage.slot_index = stage
                .slots
                .iter()
                .enumerate()
                .map(|(index, (target, _))| (target.clone(), index))
                .collect();
            if moved.contains(&stage.output) {
                for (_, slot) in &mut stage.slots {
                    slot.resend(now);
                }
            }
            stage
                .deferred_slots
                .retain(|target| stage.slots.iter().any(|(held, _)| held == target));
            stage.deferring = !stage.deferred_slots.is_empty();
            stage.plan(&self.runs);
        }
    }

    /// Transport start: every value goes again, and Every Frame runs until Stop.
    pub fn start(&mut self, now: Instant) {
        self.stop(now);
        #[cfg(feature = "dev-hooks")]
        if let Some(anchor) = self.grid_lead.and_then(|lead| now.checked_sub(lead)) {
            self.anchor = anchor;
            for stage in &mut self.stages {
                stage.grid = FrameGrid::new(anchor, stage.grid.rate());
            }
        }
        self.runs.push(Run {
            start: now,
            stop: None,
        });
        for stage in &mut self.stages {
            for (_, slot) in &mut stage.slots {
                slot.start(now);
            }
            stage.plan(&self.runs);
        }
    }

    /// Every Frame stops at the first frame whose shifted musical time passes `now`, so a
    /// positive delay still delivers the tail. Stop sends no final message.
    pub fn stop(&mut self, now: Instant) {
        if let Some(run) = self.runs.last_mut().filter(|run| run.stop.is_none()) {
            run.stop = Some(now);
            for stage in &mut self.stages {
                stage.plan(&self.runs);
            }
        }
    }

    pub fn jump(&mut self, output: TrackId, fields: &[crate::model::FieldRef], at: Instant) {
        let began = self.diagnostics.then(Instant::now);
        let Some(stage) = self.stages.iter_mut().find(|stage| stage.output == output) else {
            return;
        };
        let mut jumped = HashSet::new();
        for field in fields {
            let Some(&index) = stage.slot_index.get(&field.parameter) else {
                continue;
            };
            if jumped.contains(&index) {
                continue;
            }
            let (target, slot) = &mut stage.slots[index];
            if !self.automation.follows(field, at, slot.settings.delay_ms) {
                continue;
            }
            // A launch's own sync already sampled every slot at this instant.
            let value = if slot.sampled(at) {
                Some(slot.value())
            } else {
                self.automation.sample(target, at, slot.settings.delay_ms)
            };
            if let Some(value) = value {
                slot.jump(value, at);
                stage.jump_due(at);
                jumped.insert(index);
            }
        }
        stage.jump_timing = began.map(|began| (began, Instant::now()));
    }

    /// Send a clip event at once, counting it against its output's Budget. With Bundle on, events
    /// due at one instant leave together as a bundle; `flush_events` sends them as soon as the
    /// next event is due later. With diagnostics, every event reports `Sent` once, its lateness
    /// taken just before the send call of the datagram that carries it, whether that's now or
    /// at the flush; an event whose send fails reports only the error.
    pub fn event(
        &mut self,
        sender: &mut OscSender,
        tx: &mut EventSink,
        output: &Track,
        packet: &OscPacket,
        at: Instant,
        now: Instant,
    ) -> io::Result<usize> {
        let diagnostics = self.diagnostics;
        let Some(stage) = self.stages.iter_mut().find(|s| s.output == output.id) else {
            return send_now(sender, tx, output, packet, at, diagnostics);
        };
        if let Some((_, bucket)) = &mut stage.budget {
            bucket.event(now);
        }
        if let (true, OscPacket::Message(message)) = (stage.bundle, packet) {
            if !sender.is_resolved(output) {
                return send_now(sender, tx, output, packet, at, diagnostics);
            }
            if stage.held_at.is_some_and(|held| held != at) {
                stage.flush(sender, output, tx, diagnostics);
            }
            stage.held_at = Some(at);
            stage.held.push(message.clone());
            return Ok(0);
        }
        let bytes = send_now(sender, tx, output, packet, at, diagnostics)?;
        if bytes > 0 {
            stage.counts.messages += 1;
            stage.counts.packets += 1;
            stage.counts.bytes += bytes as u64;
        }
        Ok(bytes)
    }

    /// Send held events, except those due at `keep`: the next event may join their bundle.
    pub fn flush_events(
        &mut self,
        project: &Project,
        sender: &mut OscSender,
        tx: &mut EventSink,
        keep: Option<Instant>,
    ) {
        let diagnostics = self.diagnostics;
        for stage in &mut self.stages {
            if stage.held.is_empty() || stage.held_at == keep {
                continue;
            }
            match project.destination(stage.output) {
                Some(output) => stage.flush(sender, output, tx, diagnostics),
                None => stage.held.clear(),
            }
        }
    }

    /// The nearest frame or start send among outputs that can send now.
    pub fn deadline(&self, project: &Project, sender: &OscSender) -> Option<Instant> {
        self.stages
            .iter()
            .filter_map(|stage| Some((stage.output, stage.due()?)))
            .filter(|(output, _)| {
                project
                    .destination(*output)
                    .is_some_and(|o| sender.is_resolved(o))
            })
            .map(|(_, due)| due)
            .min()
    }

    /// Send the frames and start sends due by `now`; a grid frame waits for the remaining clip
    /// events in `chain` (taken at `now`) while `cluster::release` holds it back.
    pub fn dispatch(
        &mut self,
        project: &Project,
        sender: &mut OscSender,
        activity: &Activity,
        tx: &mut EventSink,
        chain: &Chain,
    ) {
        let now = chain.now;
        let mut dispatched = false;
        let live_timing = self.live_timing();
        let resumable = self.resumable();
        for stage in &mut self.stages {
            if !stage.due().is_some_and(|due| due <= now) {
                continue;
            }
            let Some(output) = project
                .destination(stage.output)
                .filter(|o| o.destination().is_some() && sender.is_resolved(o))
            else {
                continue;
            };
            let mut out = Out {
                output,
                sender,
                activity,
                tx,
                diagnostics: self.diagnostics,
                live_timing,
                now,
                due: stage.due().unwrap(),
                jump: None,
                withheld: None,
                resumable,
            };
            dispatched |= stage.dispatch(&self.runs, &mut out, chain);
        }
        if dispatched {
            self.forget_runs(now);
        }
    }

    /// Whether an ordinary frame for a tick before `before` is part-way out, or owed on an output
    /// it can reach, so a command arriving then waits for it (`commands::Hold`). Frames to
    /// unresolved destinations don't go, so they don't hold commands.
    pub fn earlier_frame(
        &mut self,
        project: &Project,
        sender: &OscSender,
        before: Instant,
    ) -> bool {
        let mut owed = self.sending();
        for stage in &mut self.stages {
            let reachable = project
                .destination(stage.output)
                .is_some_and(|o| o.destination().is_some() && sender.is_resolved(o));
            owed |= reachable && stage.earlier_frame(&self.runs, before);
        }
        owed
    }

    /// Whether a grid frame was held back since the last call because the plan behind `pending`
    /// couldn't see `cluster::CLEARANCE` ahead; the worker then plans again at once.
    pub fn take_replan(&mut self) -> bool {
        self.stages
            .iter_mut()
            .fold(false, |any, stage| std::mem::take(&mut stage.replan) | any)
    }

    /// Whether any output holds Bundle On events back. The worker flushes them before taking a
    /// command, or holds the command while an event due at their instant remains, so a command
    /// (a closing track valve included) never applies under a held bundle.
    pub fn holds_events(&self) -> bool {
        self.stages.iter().any(|stage| !stage.held.is_empty())
    }

    /// Whether any frame still has packets to send.
    pub fn sending(&self) -> bool {
        self.stages.iter().any(Stage::sending)
    }

    /// The outputs whose frame's last packet went since the last call; the caller stamps the
    /// completion with its own clock (`turns::Backlogs`).
    pub fn take_completed(&mut self) -> Vec<TrackId> {
        self.stages
            .iter_mut()
            .filter_map(|stage| std::mem::take(&mut stage.completed).then_some(stage.output))
            .collect()
    }

    /// Whether `output` still has frame packets to send; its hits must wait for them.
    pub fn sending_to(&self, output: TrackId) -> bool {
        self.stages
            .iter()
            .any(|stage| stage.output == output && stage.sending())
    }

    /// Send the next packet of whichever sending frame is due earliest.
    pub fn send_next(
        &mut self,
        project: &Project,
        sender: &mut OscSender,
        activity: &Activity,
        tx: &mut EventSink,
    ) {
        let live_timing = self.live_timing();
        let Some(stage) = self
            .stages
            .iter_mut()
            .filter(|stage| stage.sending())
            .min_by_key(|stage| stage.sending_due())
        else {
            return;
        };
        match project
            .destination(stage.output)
            .filter(|o| o.destination().is_some() && sender.is_resolved(o))
        {
            Some(output) => {
                stage.send_next(sender, output, activity, tx, self.diagnostics, live_timing)
            }
            None => stage.abandon(),
        }
    }

    /// Rebuild expensive frame deadlines after the packets already due at this instant leave.
    pub fn replan_dirty(&mut self) {
        for stage in &mut self.stages {
            if stage.needs_plan {
                stage.plan(&self.runs);
            }
        }
    }

    /// Drop stopped runs whose tails have passed for every slot.
    fn forget_runs(&mut self, now: Instant) {
        if self.runs.iter().all(|run| run.stop.is_none()) {
            return;
        }
        let latest = self
            .stages
            .iter()
            .flat_map(|stage| &stage.slots)
            .map(|(_, slot)| slot.settings.delay_ms)
            .fold(0.0, f64::max);
        self.runs.retain(|run| {
            run.stop
                .is_none_or(|stop| delay::shift(stop, latest).is_none_or(|end| end > now))
        });
    }

    /// Publish counters without ever waiting for a snapshot reader.
    pub fn publish(
        &mut self,
        shared: &throughput::Published,
        project: &Project,
        tx: &mut EventSink,
        now: Instant,
    ) {
        if !self.membership_changed
            && self
                .stages
                .iter()
                .all(|stage| stage.published == Some((stage.counts, stage.deferring)))
        {
            return;
        }
        for stage in &self.stages {
            if stage.counts.oversize > 0 && self.warned_oversize.insert(stage.output) {
                let name = project
                    .track(stage.output)
                    .map_or("?", |track| track.name.as_str());
                tx.send(SchedulerEvent::OutputOversize {
                    output: stage.output,
                    name: name.to_owned(),
                });
            }
        }
        if shared.record(
            self.stages
                .iter()
                .map(|stage| (stage.output, stage.counts, stage.deferring)),
            now,
        ) {
            self.membership_changed = false;
            for stage in &mut self.stages {
                stage.published = Some((stage.counts, stage.deferring));
            }
        }
    }

    #[cfg(test)]
    fn counts(&self, output: TrackId) -> Option<Counts> {
        self.stages
            .iter()
            .find(|stage| stage.output == output)
            .map(|stage| stage.counts)
    }
}

/// Send a clip event's packet now. With `diagnostics`, report it `Sent` once the sender accepts
/// it, its lateness taken just before the send call.
fn send_now(
    sender: &mut OscSender,
    tx: &mut EventSink,
    output: &Track,
    packet: &OscPacket,
    due: Instant,
    diagnostics: bool,
) -> io::Result<usize> {
    let before = Instant::now();
    let bytes = sender.send(output, packet)?;
    if diagnostics {
        tx.send(SchedulerEvent::Sent {
            due,
            lateness: before.saturating_duration_since(due),
        });
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests;
