//! complete-clusters-before-frames (proposal revision 3, 2026-10-20): an ordinary frame waits for
//! a nearby cluster of clip events to finish, up to a cap, instead of starting part-way through
//! it. The worker summarises the remaining clip events as a `Chain`; `release` decides.
use std::time::{Duration, Instant};

/// Clearance: a frame goes only while no clip event is due within this long (provisional).
pub(crate) const CLEARANCE: Duration = Duration::from_millis(1);
/// The most an ordinary frame is held back after its tick (provisional); never more than half a
/// frame period, so holding back alone never skips a tick.
const MAX_DEFERRAL: Duration = Duration::from_millis(4);

/// The clip events not yet sent at a pass: the first due by `now + CLEARANCE`, overdue ones
/// included, and each next one due within `CLEARANCE` of the one before. Clip events are planned
/// `Send`s and `Automate`s and queued manual hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Chain {
    /// The chain's last due instant; None when nothing is due by `now + CLEARANCE`.
    pub(crate) end: Option<Instant>,
    /// The horizon E of the plan behind `pending`: every unconsumed clip event due by it is known.
    pub(crate) horizon: Instant,
    pub(crate) now: Instant,
    /// A command waiting in `Hold` (not a fire), from its arrival: an ordinary frame for a tick
    /// at or after it yields until the command applies (hold-commands-behind-earlier-frames).
    pub(crate) yield_from: Option<Instant>,
}

impl Chain {
    /// Summarise two due-ordered lists of clip event instants (planned events, manual hits).
    pub(crate) fn summarise(
        planned: impl Iterator<Item = Instant>,
        manual: impl Iterator<Item = Instant>,
        horizon: Instant,
        now: Instant,
    ) -> Self {
        let (mut planned, mut manual) = (planned.peekable(), manual.peekable());
        let mut next = || match (planned.peek(), manual.peek()) {
            (Some(a), Some(b)) if b < a => manual.next(),
            (Some(_), _) => planned.next(),
            (None, _) => manual.next(),
        };
        let mut end: Option<Instant> = None;
        // The first event links to now; each later one to the event before it.
        while let Some(at) = next() {
            if at > end.unwrap_or(now) + CLEARANCE {
                break;
            }
            end = Some(at);
        }
        Self {
            end,
            horizon,
            now,
            yield_from: None,
        }
    }

    /// The same chain, with ordinary frames for ticks from `from` yielding to a waiting command.
    pub(crate) fn yielding(self, from: Option<Instant>) -> Self {
        Self {
            yield_from: from,
            ..self
        }
    }

    /// Whether the plan sees at least `CLEARANCE` ahead of now. E can lie in the past: the worker
    /// replans only once `pending` empties, and lifecycle events can keep it from emptying.
    pub(crate) fn visible(&self) -> bool {
        self.horizon >= self.now + CLEARANCE
    }

    /// Whether the chain could continue beyond what the plan knows. `release` doesn't need it: an
    /// open chain is waited for only up to the cap, as a closed one is.
    #[cfg(test)]
    pub(crate) fn open(&self) -> bool {
        !self.visible() || self.end.is_some_and(|end| end + CLEARANCE > self.horizon)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Release {
    Go,
    Wait,
    /// Wait, and replan: an empty chain says nothing about the time beyond the horizon.
    Replan,
}

/// Whether an ordinary frame due at `tick` goes at the chain's pass, in the proposal's order.
pub(crate) fn release(tick: Instant, period: Duration, chain: &Chain) -> Release {
    let cap = tick + MAX_DEFERRAL.min(period / 2);
    match chain.end {
        _ if chain.now >= cap => Release::Go,
        None if chain.visible() => Release::Go,
        None => Release::Replan,
        Some(end) if end <= cap => Release::Wait,
        Some(_) => Release::Go,
    }
}

#[cfg(test)]
mod tests;
