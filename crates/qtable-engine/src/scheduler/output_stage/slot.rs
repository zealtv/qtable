//! One parameter's sends to one output: the latest value wins, an unchanged value isn't resent,
//! Threshold holds small changes until they settle, delay makes edits eligible late, and Every
//! Frame resends on every frame of a transport run, delayed tail included.
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::budget::Class;
use crate::model::SendMode;
use crate::scheduler::delay;
use crate::scheduler::parameter_controls::SendValues;

/// Play to Stop, in musical (unshifted) time. A stopped run still owns its delayed tail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Run {
    pub start: Instant,
    pub stop: Option<Instant>,
}

impl Run {
    fn covers(&self, musical: Instant) -> bool {
        self.start <= musical && self.stop.is_none_or(|stop| musical < stop)
    }
}

/// How a parameter sends; changing any of these sends nothing by itself.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Settings {
    pub mode: SendMode,
    /// In sent units, after Mapping; 0 sends any change.
    pub thresholds: Arc<[f64]>,
    /// Track delay, its ancestors and Main, plus output latency.
    pub delay_ms: f64,
}

#[derive(Clone, Debug)]
struct Entry {
    /// Edit instant plus delay: no frame before it may send this value.
    eligible: Instant,
    value: SendValues,
    /// Sent even when equal to the last value sent: transport start resends every value.
    force: bool,
    /// Sent at its eligible instant rather than on the next frame (transport start).
    immediate: bool,
    /// Sent, or found equal to the last value sent.
    done: bool,
}

/// When a slot next needs the stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Want {
    /// At this instant, off the frame grid.
    At(Instant),
    /// On the first frame at or after `from`, and before `until`.
    Frame {
        from: Instant,
        until: Option<Instant>,
    },
}

/// A value a frame could send for a slot.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Pick {
    pub value: SendValues,
    /// A changed value is sent before a repeat of an unchanged one.
    pub class: Class,
    /// Transport start sends are never deferred; they take Budget tokens like events.
    pub forced: bool,
}

/// What one frame found for a slot.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Outcome {
    pub pick: Option<Pick>,
    /// Values overwritten before any frame sent them.
    pub merged: u64,
}

pub(crate) struct Slot {
    pub address: String,
    pub settings: Settings,
    /// Oldest first; one eligible value plus those still inside the delay window. Never empty.
    history: VecDeque<Entry>,
    /// Values compacted between frames, reported by the next prepare.
    merged: u64,
    last_sent: Option<SendValues>,
    last_sent_at: Option<Instant>,
    /// Frames this slot's candidate has waited since it last sent (Budget priority).
    pub accumulator: u64,
    /// The instant whose automation value is the newest entry, until anything else is pushed.
    sampled_at: Option<Instant>,
    /// When this slot reached its receiver: no frame before it is owed automation, however long
    /// ago the stage's last frame went (a reopened track valve).
    pub reached: Instant,
}

impl Slot {
    #[cfg(test)]
    pub(super) fn history_len(&self) -> usize {
        self.history.len()
    }

    /// A slot that knows its value but owes no send: the scheduler's starting state.
    pub fn quiet(
        address: String,
        value: impl Into<SendValues>,
        settings: Settings,
        now: Instant,
    ) -> Self {
        let value = value.into();
        Self {
            address,
            settings,
            history: VecDeque::from([Entry {
                eligible: now,
                value,
                force: false,
                immediate: false,
                done: true,
            }]),
            merged: 0,
            last_sent: None,
            last_sent_at: None,
            accumulator: 0,
            sampled_at: None,
            reached: now,
        }
    }

    /// A slot that newly reaches its receiver and sends its value on the first frame from `now`.
    /// Reaching it (an opened track valve or destination, an enabled parameter or device, a new
    /// route) is a routing change applied with its command, not a musical edit, so delay doesn't
    /// shift it. A fresh slot has sent nothing, so Threshold can't
    /// hold it back.
    pub fn new(
        address: String,
        value: impl Into<SendValues>,
        settings: Settings,
        now: Instant,
    ) -> Self {
        let mut slot = Self::quiet(address, value, settings, now);
        slot.history[0].done = false;
        slot
    }

    /// The newest value, eligible or not.
    pub fn value(&self) -> SendValues {
        self.history
            .back()
            .expect("slot history is never empty")
            .value
            .clone()
    }

    /// A value edit at `edited`. An edit to the same value adds nothing.
    pub fn edit(&mut self, value: impl Into<SendValues>, edited: Instant) {
        let value = value.into();
        if !value.same(&self.value()) {
            self.push(value, edited, false);
        }
    }

    /// A scheduler sample already belongs to this output frame; do not delay it twice.
    pub fn sample(&mut self, value: SendValues, at: Instant) {
        self.sampled_at = Some(at);
        if value.same(&self.value()) {
            return;
        }
        self.history.push_back(Entry {
            eligible: at,
            value,
            force: false,
            immediate: false,
            done: false,
        });
        self.compact(at);
    }

    /// Exact jump samples are already delayed, and share transport-start deduplication.
    pub fn jump(&mut self, value: SendValues, at: Instant) {
        if self.last_sent_at == Some(at)
            && self
                .last_sent
                .as_ref()
                .is_some_and(|last| last.same(&value))
        {
            return;
        }
        self.history.push_back(Entry {
            eligible: at,
            value,
            force: true,
            immediate: true,
            done: false,
        });
        self.compact(at);
        self.sampled_at = Some(at);
    }

    /// Whether sampling automation at `at` again would change nothing.
    pub fn sampled(&self, at: Instant) -> bool {
        self.sampled_at == Some(at)
    }

    /// The automation changed: the next sample counts, whatever its instant.
    pub fn forget_sample(&mut self) {
        self.sampled_at = None;
    }

    pub fn sent_since(&self, at: Instant) -> bool {
        self.last_sent_at.is_some_and(|sent| sent >= at)
    }

    pub fn sent_at(&mut self, value: SendValues, at: Instant) {
        self.sent(value);
        self.last_sent_at = Some(at);
    }

    /// The receiver or address changed: the current value goes again.
    pub fn resend(&mut self, now: Instant) {
        self.last_sent = None;
        if self.history.back().is_some_and(|entry| entry.done) {
            self.push(self.value(), now, false);
        }
    }

    /// Transport start resends the current value as soon as it is eligible, ahead of the frame
    /// grid, so it still wins its tie with a clip event launched at the same instant.
    pub fn start(&mut self, now: Instant) {
        let sampled = self.sampled_at;
        self.push(self.value(), now, true);
        // The same value goes again, so it is still the automation value sampled at that instant.
        self.sampled_at = sampled;
    }

    fn push(&mut self, value: SendValues, edited: Instant, start: bool) {
        self.sampled_at = None;
        self.history.push_back(Entry {
            eligible: delay::shift(edited, self.settings.delay_ms).unwrap_or(edited),
            value,
            force: start,
            immediate: start,
            done: false,
        });
        self.compact(edited);
    }

    /// Keep the newest value eligible by `at` and every value still waiting for its delay.
    fn compact(&mut self, at: Instant) -> bool {
        let Some(newest) = self.history.iter().rposition(|entry| entry.eligible <= at) else {
            return false;
        };
        let (mut force, mut immediate) = (false, false);
        for entry in self.history.drain(..newest) {
            if !entry.done {
                self.merged += 1;
                force |= entry.force;
                immediate |= entry.immediate;
            }
        }
        self.history[0].force |= force;
        self.history[0].immediate |= immediate;
        true
    }

    /// Whether this value is a change too small to send yet. It settles, and sends, once it has
    /// been eligible for a full frame with no newer edit.
    fn holds(&self, entry: &Entry, at: Instant, period: Duration) -> bool {
        self.settings.mode != SendMode::EveryFrame
            && !entry.force
            && self
                .last_sent
                .as_ref()
                .is_some_and(|last| entry.value.holds(last, &self.settings.thresholds))
            && entry.eligible + period > at
    }

    /// Whether frame time `at` falls within a run, once shifted back by this slot's delay.
    fn streams(&self, at: Instant, runs: &[Run]) -> bool {
        self.settings.mode == SendMode::EveryFrame
            && delay::shift(at, -self.settings.delay_ms)
                .is_some_and(|musical| runs.iter().any(|run| run.covers(musical)))
    }

    /// The frame at `at`: take the newest eligible value and decide whether it could go. A
    /// `refresh` step offers the value again even when nothing else would send it. Nothing
    /// counts as sent until `sent` says so, so a value the Budget defers stays dirty.
    pub fn prepare(
        &mut self,
        at: Instant,
        period: Duration,
        runs: &[Run],
        refresh: bool,
    ) -> Outcome {
        let mut outcome = Outcome::default();
        if !self.compact(at) {
            return outcome;
        }
        outcome.merged = std::mem::take(&mut self.merged);
        // An immediate start/jump and a grid tick at the same instant share one send,
        // including Every Frame and Refresh repeats.
        if self.last_sent_at == Some(at)
            && self
                .last_sent
                .as_ref()
                .is_some_and(|last| last.same(&self.history[0].value))
        {
            self.history[0].done = true;
            return outcome;
        }
        let streams = self.streams(at, runs);
        let held = self.holds(&self.history[0], at, period);
        let last_sent = self.last_sent.as_ref();
        let entry = &mut self.history[0];
        let changed = entry.force || last_sent.is_none_or(|last| !last.same(&entry.value));
        if !changed {
            entry.done = true;
        }
        let class = if !entry.done && changed && !held {
            Class::Change
        } else if streams || refresh {
            Class::Repeat
        } else {
            return outcome;
        };
        outcome.pick = Some(Pick {
            value: entry.value.clone(),
            class,
            forced: class == Class::Change && entry.force,
        });
        outcome
    }

    /// The picked value went out.
    pub fn sent(&mut self, value: impl Into<SendValues>) {
        self.last_sent = Some(value.into());
        self.history[0].done = true;
    }

    /// Whether an off-grid start send is due by `now`.
    pub fn starts_by(&self, now: Instant) -> bool {
        self.history
            .iter()
            .any(|entry| entry.immediate && !entry.done && entry.eligible <= now)
    }

    pub fn next_start(&self) -> Option<Instant> {
        self.history
            .iter()
            .filter(|entry| entry.immediate && !entry.done)
            .map(|entry| entry.eligible)
            .min()
    }

    /// Report each reason this slot needs a frame or an off-grid send.
    pub fn wants(&self, period: Duration, runs: &[Run], mut want: impl FnMut(Want)) {
        for entry in self.history.iter().filter(|entry| !entry.done) {
            if entry.immediate {
                want(Want::At(entry.eligible));
            } else if self.holds(entry, entry.eligible, period) {
                want(Want::Frame {
                    from: entry.eligible + period,
                    until: None,
                });
            } else {
                want(Want::Frame {
                    from: entry.eligible,
                    until: None,
                });
            }
        }
        if self.settings.mode != SendMode::EveryFrame {
            return;
        }
        let Some(first) = self.history.iter().map(|entry| entry.eligible).min() else {
            return;
        };
        for run in runs {
            let Some(start) = delay::shift(run.start, self.settings.delay_ms) else {
                continue;
            };
            let until = match run.stop {
                Some(stop) => match delay::shift(stop, self.settings.delay_ms) {
                    Some(until) => Some(until),
                    None => continue,
                },
                None => None,
            };
            want(Want::Frame {
                from: start.max(first),
                until,
            });
        }
    }
}

#[cfg(test)]
mod tests;
