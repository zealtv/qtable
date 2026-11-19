//! One destination's frame planning, parameter selection, Budget and Refresh.
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use rosc::OscMessage;

use super::budget::{self, Candidate, TokenBucket};
use super::cluster::{self, Chain, Release};
use super::frame::FrameGrid;
use super::send::{InFlight, Out, Phases, transmit};
use super::slot::{Run, Slot, Want};
use super::throughput::Counts;
use crate::model::{Destination, OutputStage, ParameterRef, Track, TrackId};
use crate::scheduler::SchedulerEvent;
use crate::scheduler::events::EventSink;
use crate::scheduler::marks;
use crate::sender::OscSender;

/// Round-robin keep-alive: over each interval every slot is offered once, a few per frame.
struct Refresh {
    seconds: f64,
    /// Slots owed a repeat, accrued from elapsed time and capped at one pass.
    owed: f64,
    accrued_to: Instant,
    cursor: usize,
}

pub(super) struct Stage {
    pub(super) output: TrackId,
    pub(super) automation: std::sync::Arc<crate::scheduler::automation::Automation>,
    pub(super) destination: Destination,
    pub(super) grid: FrameGrid,
    /// In the order the project lists its parameters.
    pub(super) slots: Vec<(ParameterRef, Slot)>,
    /// Slot lookup for boundary jumps; rebuilt whenever controls are applied.
    pub(super) slot_index: HashMap<ParameterRef, usize>,
    last_frame: Option<Instant>,
    last_jump: Option<Instant>,
    pub(super) needs_plan: bool,
    /// The next grid frame any slot needs.
    frame_due: Option<Instant>,
    /// The next off-grid start send.
    start_due: Option<Instant>,
    pub(super) counts: Counts,
    pub(super) deferring: bool,
    pub(super) deferred_slots: HashSet<ParameterRef>,
    pub(super) published: Option<(Counts, bool)>,
    /// Messages per second, with its bucket; None while the Budget is Off.
    pub(super) budget: Option<(u32, TokenBucket)>,
    pub(super) bundle: bool,
    refresh: Option<Refresh>,
    /// Clip events due at `held_at`, waiting to leave as one bundle.
    pub(super) held: Vec<OscMessage>,
    pub(super) held_at: Option<Instant>,
    /// Diagnostics: the latest exact jump's slot lookup, until its send reports it.
    pub(super) jump_timing: Option<(Instant, Instant)>,
    /// Built frames whose packets haven't all left yet, oldest first.
    in_flight: VecDeque<InFlight>,
    /// A frame's last packet went since `take_completed` last looked (`turns::Backlogs`).
    pub(super) completed: bool,
    /// The last pass that held the due grid frame back for a cluster, until the frame goes.
    withheld: Option<Instant>,
    /// A pass held the grid frame back because the plan couldn't see far enough ahead.
    pub(super) replan: bool,
}

impl Stage {
    pub(super) fn new(
        output: TrackId,
        destination: Destination,
        anchor: Instant,
        rate: f64,
    ) -> Self {
        Self {
            output,
            automation: Default::default(),
            destination,
            grid: FrameGrid::new(anchor, rate),
            slots: Vec::new(),
            slot_index: HashMap::new(),
            last_frame: None,
            last_jump: None,
            needs_plan: false,
            frame_due: None,
            start_due: None,
            counts: Counts::default(),
            deferring: false,
            deferred_slots: HashSet::new(),
            published: None,
            budget: None,
            bundle: false,
            refresh: None,
            held: Vec::new(),
            held_at: None,
            jump_timing: None,
            in_flight: VecDeque::new(),
            completed: false,
            withheld: None,
            replan: false,
        }
    }

    /// Whether a built frame still has packets to send.
    pub(super) fn sending(&self) -> bool {
        !self.in_flight.is_empty()
    }

    /// The due instant of the oldest frame still sending.
    pub(super) fn sending_due(&self) -> Option<Instant> {
        self.in_flight.front().map(InFlight::due)
    }

    /// Drop frames that can no longer be sent (their output went away).
    pub(super) fn abandon(&mut self) {
        self.in_flight.clear();
    }

    /// Send one packet of the oldest frame still sending, reporting the frame once it has all gone.
    pub(super) fn send_next(
        &mut self,
        sender: &mut OscSender,
        output: &Track,
        activity: &crate::activity::Activity,
        tx: &mut EventSink,
        diagnostics: bool,
        live_timing: bool,
    ) {
        if let Some(frame) = self.in_flight.front_mut()
            && frame.send_next(
                sender,
                output,
                activity,
                tx,
                &mut self.counts,
                diagnostics,
                live_timing,
            )
        {
            self.in_flight.pop_front();
            self.completed = true;
        }
    }

    pub(super) fn due(&self) -> Option<Instant> {
        match (self.frame_due, self.start_due) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    pub(super) fn jump_due(&mut self, at: Instant) {
        self.start_due = Some(self.start_due.map_or(at, |due| due.min(at)));
    }

    /// Cache the deadlines; only edits, transport changes and frames move them.
    pub(super) fn plan(&mut self, runs: &[Run]) {
        let period = self.grid.period();
        let after = self.last_frame.map(|frame| frame + Duration::from_nanos(1));
        let automation_after = self
            .last_frame
            .map(|at| at + Duration::from_nanos(1))
            .into_iter()
            // The clock rounds nanoseconds at exact beat boundaries. Move past that
            // tolerance after an off-grid jump so it is not woken again on a frame.
            .chain(self.last_jump.map(|at| at + Duration::from_nanos(10)))
            .max();
        let (mut frame_due, mut start_due) = (None::<Instant>, None::<Instant>);
        for (target, slot) in &self.slots {
            if let Some(from) = automation_after
                .or(self.automation.anchor())
                .map(|from| from.max(slot.reached))
                && let Some(at) = self
                    .automation
                    .next_frame(target, from, slot.settings.delay_ms)
                && let Some(tick) = self.grid.tick_at_or_after(at)
            {
                frame_due = Some(frame_due.map_or(tick, |due| due.min(tick)));
            }
            slot.wants(period, runs, |want| match want {
                Want::At(at) => start_due = Some(start_due.map_or(at, |due| due.min(at))),
                Want::Frame { from, until } => {
                    let from = after.map_or(from, |after| from.max(after));
                    if let Some(tick) = self.grid.tick_at_or_after(from)
                        && until.is_none_or(|until| tick < until)
                    {
                        frame_due = Some(frame_due.map_or(tick, |due| due.min(tick)));
                    }
                }
            });
        }
        if let Some(refresh) = &self.refresh
            && !self.slots.is_empty()
        {
            let wait = (1.0 - refresh.owed).max(0.0) * refresh.seconds / self.slots.len() as f64;
            let due = refresh.accrued_to + Duration::from_secs_f64(wait);
            let from = after.map_or(due, |after| due.max(after));
            if let Some(tick) = self.grid.tick_at_or_after(from) {
                frame_due = Some(frame_due.map_or(tick, |due| due.min(tick)));
            }
        }
        self.frame_due = frame_due;
        self.start_due = start_due;
        self.needs_plan = false;
    }

    /// Take the budget and refresh settings from the output's stage; unchanged ones keep their
    /// state, and a changed Budget or Frame Rate starts a full bucket.
    pub(super) fn configure(&mut self, settings: &OutputStage, now: Instant) {
        let rate = self.grid.rate();
        self.bundle = settings.bundle;
        self.budget = match (settings.budget(), self.budget.take()) {
            (Some(budget), Some((held, bucket)))
                if budget == held && rate == bucket.frame_rate() =>
            {
                Some((held, bucket))
            }
            (Some(budget), _) => Some((budget, TokenBucket::new(budget, rate, now))),
            (None, _) => None,
        };
        self.refresh = match (settings.refresh(), self.refresh.take()) {
            (Some(seconds), Some(refresh)) if refresh.seconds == seconds => Some(refresh),
            (Some(seconds), _) => Some(Refresh {
                seconds,
                owed: 0.0,
                accrued_to: now,
                cursor: 0,
            }),
            (None, _) => None,
        };
    }

    /// The slots a refresh step offers at `at`, as a start and a count, wrapping round the slots.
    fn refresh_step(&mut self, at: Instant) -> Option<(usize, usize)> {
        let len = self.slots.len();
        let refresh = self.refresh.as_mut()?;
        let elapsed = at.saturating_duration_since(refresh.accrued_to);
        refresh.accrued_to = refresh.accrued_to.max(at);
        if len == 0 {
            return None;
        }
        refresh.owed =
            (refresh.owed + len as f64 * elapsed.as_secs_f64() / refresh.seconds).min(len as f64);
        // The small allowance keeps a step that rounding left a hair short from waiting a frame.
        let count = ((refresh.owed + 1e-9).floor() as usize).min(len);
        if count == 0 {
            return None;
        }
        refresh.owed = (refresh.owed - count as f64).max(0.0);
        let start = refresh.cursor % len;
        refresh.cursor = (start + count) % len;
        Some((start, count))
    }

    /// Send the held events as bundles under the MTU rule. With `diagnostics`, each event reports
    /// `Sent` once, its lateness taken just before its own datagram's send call.
    pub(super) fn flush(
        &mut self,
        sender: &mut OscSender,
        output: &Track,
        tx: &mut EventSink,
        diagnostics: bool,
    ) {
        let messages = std::mem::take(&mut self.held);
        let held_at = self.held_at.take();
        let mut departures = Vec::new();
        marks::mark("flush.begin");
        transmit(
            sender,
            output,
            tx,
            &mut self.counts,
            messages,
            true,
            None,
            |_, before| {
                if diagnostics {
                    departures.push(before);
                }
            },
        );
        marks::mark("flush.end");
        if let Some(due) = held_at {
            for before in departures {
                tx.send(SchedulerEvent::Sent {
                    due,
                    lateness: before.saturating_duration_since(due),
                });
            }
        }
    }

    /// Whether this stage owes an ordinary frame for a tick before `before`. A dirty stage (after
    /// a frame under live timing) shows no deadline until planned, so it plans first when its grid
    /// has a tick after its last frame and before `before`.
    pub(super) fn earlier_frame(&mut self, runs: &[Run], before: Instant) -> bool {
        let unplanned = self.needs_plan
            && before
                .checked_sub(Duration::from_nanos(1))
                .and_then(|at| self.grid.latest_tick(at))
                .is_some_and(|tick| self.last_frame.is_none_or(|last| tick > last));
        if unplanned {
            self.plan(runs);
        }
        self.frame_due.is_some_and(|due| due < before)
    }

    /// The grid frame, if one is due and `cluster::release` lets it go before the remaining clip
    /// events in `chain`, then any start sends due off the grid; false if nothing was due yet.
    pub(super) fn dispatch(&mut self, runs: &[Run], out: &mut Out, chain: &Chain) -> bool {
        let period = self.grid.period();
        let frame_due = self.frame_due.filter(|&due| due <= out.now);
        // A frame for a tick at or after a waiting command's arrival goes after it, from the
        // state it leaves.
        let frame_due = frame_due.filter(|&due| chain.yield_from.is_none_or(|from| due < from));
        let frame_due = frame_due.filter(|&due| match cluster::release(due, period, chain) {
            Release::Go => true,
            held => {
                self.withheld = Some(out.now);
                self.replan |= held == Release::Replan;
                false
            }
        });
        // Start sends still win ties with clip events, so a volume precedes its launch.
        if frame_due.is_none() && !self.start_due.is_some_and(|due| due <= out.now) {
            return false;
        }
        let immediate = self.start_due.filter(|due| *due <= out.now);
        let jump = self.jump_timing.take();
        if let Some(at) = immediate
            && self.slots.iter().any(|(_, slot)| slot.starts_by(at))
        {
            out.due = at;
            out.jump = jump;
            self.frame(at, runs, out, None, |slot| slot.starts_by(at));
            self.last_jump = Some(at);
        }
        // A late frame goes once, as the latest tick, without a burst of the missed ones.
        if let Some(due) = frame_due
            && let Some(tick) = self.grid.latest_tick(out.now)
        {
            if out.now.saturating_duration_since(due) > self.grid.period() {
                self.counts.frames_late += 1;
            }
            out.due = due;
            out.jump = None;
            // Only a pass since this tick held this frame back.
            out.withheld = self.withheld.take().filter(|&at| at >= due);
            self.last_frame = Some(tick);
            let refresh = self.refresh_step(tick);
            // A late immediate send already served this dispatch; do not burst a
            // redundant grid repeat behind it. Other slots still receive the frame.
            self.frame(tick, runs, out, refresh, |slot| {
                immediate.is_none_or(|at| !slot.sent_since(at))
            });
        }
        if out.live_timing {
            self.frame_due = None;
            // Keep future off-grid starts visible while the full frame replan waits
            // for adjacent clip packets. They must still win their own ties.
            self.start_due = self
                .slots
                .iter()
                .filter_map(|(_, slot)| slot.next_start())
                .min();
            self.needs_plan = true;
        } else {
            self.plan(runs);
        }
        true
    }

    /// One frame: transport start values go first and take tokens like events; changes then
    /// repeats follow in priority order while the Budget lasts.
    fn frame(
        &mut self,
        at: Instant,
        runs: &[Run],
        out: &mut Out,
        refresh: Option<(usize, usize)>,
        only: impl Fn(&Slot) -> bool,
    ) {
        let began = out.diagnostics.then(Instant::now);
        let (mut considered, mut sampled, mut merged) = (0, 0, 0);
        let period = self.grid.period();
        let len = self.slots.len();
        let mut forced = Vec::new();
        let mut candidates = Vec::new();
        let mut values = Vec::new();
        for (index, (target, slot)) in self.slots.iter_mut().enumerate() {
            if !only(slot) {
                continue;
            }
            considered += 1;
            // An exact jump sampled this slot at `at` just before its send.
            if !slot.sampled(at)
                && let Some(value) = self.automation.sample(target, at, slot.settings.delay_ms)
            {
                sampled += 1;
                slot.sample(value, at);
            }
            let refreshed =
                refresh.is_some_and(|(start, count)| (index + len - start) % len < count);
            let outcome = slot.prepare(at, period, runs, refreshed);
            self.counts.merged += outcome.merged;
            merged += outcome.merged;
            match outcome.pick {
                Some(pick) if pick.forced => forced.push((index, pick.value)),
                Some(pick) => {
                    candidates.push(Candidate {
                        key: index,
                        class: pick.class,
                        accumulator: slot.accumulator,
                    });
                    values.push((index, pick.value));
                }
                None => {
                    self.deferred_slots.remove(target);
                }
            }
        }
        let prepared = began.map(|_| Instant::now());
        if let Some((_, bucket)) = &mut self.budget {
            bucket.refill(at);
            for _ in &forced {
                bucket.event(at);
            }
        }
        let selection = budget::select(&mut candidates, self.budget.as_mut().map(|(_, b)| b));
        for (candidate, (index, _)) in candidates.iter().zip(&values) {
            self.slots[*index].1.accumulator = candidate.accumulator;
        }
        for &index in &selection.deferred {
            self.deferred_slots
                .insert(self.slots[values[index].0].0.clone());
        }
        for &index in &selection.skipped {
            self.deferred_slots.remove(&self.slots[values[index].0].0);
        }
        self.counts.deferred += selection.deferred.len() as u64;
        self.counts.skipped += selection.skipped.len() as u64;
        let sending: Vec<_> = forced
            .into_iter()
            .chain(selection.chosen.iter().map(|&i| values[i].clone()))
            .collect();
        let mut tracks = Vec::with_capacity(sending.len());
        let mut messages = Vec::with_capacity(sending.len());
        for (index, value) in sending {
            let (target, slot) = &mut self.slots[index];
            slot.sent_at(value.clone(), at);
            self.deferred_slots.remove(target);
            tracks.push(target.track);
            messages.push(OscMessage {
                addr: slot.address.clone(),
                args: value.arguments(),
            });
        }
        self.deferring = !self.deferred_slots.is_empty();
        let withheld = out.withheld.take();
        let phases = began.zip(prepared).map(|(began, prepared)| Phases {
            released: out.now,
            withheld,
            began,
            prepared,
            built: Instant::now(),
            slots: len,
            considered,
            sampled,
            merged,
        });
        if let Some(frame) = InFlight::new(
            out,
            &mut self.counts,
            messages,
            self.bundle,
            &tracks,
            phases,
        ) {
            self.in_flight.push_back(frame);
        }
        if !out.resumable {
            while self.sending() {
                self.send_next(
                    out.sender,
                    out.output,
                    out.activity,
                    out.tx,
                    out.diagnostics,
                    out.live_timing,
                );
            }
        }
    }
}
