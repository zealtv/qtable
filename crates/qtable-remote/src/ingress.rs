//! Bounded ingress and the host's monotonic wake contract. Sequence assignment and
//! insertion share one lock, so concurrent ports cannot publish out of order.
use std::collections::VecDeque;
use std::sync::{
    Condvar, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

pub const CONTINUOUS_CAPACITY: usize = 1024;
pub const DISCRETE_CAPACITY: usize = 256;
pub const DRAIN_LIMIT: usize = 512;
pub const CONTINUOUS_INTERVAL: Duration = Duration::from_nanos(16_666_667);
/// Bound bytes as well as message count (SysEx can otherwise be arbitrarily large).
pub const MIDI_MESSAGE_LIMIT: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Endpoint {
    Midi { id: String, name: String },
    Keyboard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// A complete MIDI message, including its status byte; backend timestamps are not
    /// comparable between connections, so receive time is the shared monotonic clock.
    Midi(Box<[u8]>),
    Key {
        key: String,
        pressed: bool,
        repeat: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Envelope {
    pub endpoint: Endpoint,
    pub received_at: Instant,
    pub sequence: u64,
    pub input: Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueClass {
    Continuous,
    Discrete,
}

impl Input {
    pub fn queue_class(&self) -> QueueClass {
        match self {
            // Pressure joins CC and bend in the continuous queue. Classification is
            // independent of a later mapping's gesture (including bang mappings).
            Self::Midi(bytes)
                if bytes
                    .first()
                    .is_some_and(|b| matches!(b & 0xf0, 0xa0 | 0xb0 | 0xd0 | 0xe0)) =>
            {
                QueueClass::Continuous
            }
            _ => QueueClass::Discrete,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Overflow {
    pub continuous: u64,
    pub discrete: u64,
    pub oversized: u64,
}
impl Overflow {
    pub fn total(self) -> u64 {
        self.continuous + self.discrete + self.oversized
    }
}

#[derive(Debug, Default)]
pub struct Drain {
    pub events: Vec<Envelope>,
    /// Since the previous drain; cumulative counters remain available on Ingress.
    pub dropped: Overflow,
    pub activity: u64,
}

#[derive(Default)]
struct State {
    continuous: VecDeque<Envelope>,
    discrete: VecDeque<Envelope>,
    sequence: u64,
    activity: u64,
    dropped: Overflow,
    reported: Overflow,
    urgent: bool,
    in_flight: bool,
    continuous_due: Option<Instant>,
}

#[derive(Default)]
pub struct Ingress {
    state: Mutex<State>,
    changed: Condvar,
}

impl Ingress {
    /// All producers (including keys) share this ingress instance.
    pub fn push(&self, endpoint: Endpoint, input: Input) {
        let mut state = self.state.lock().unwrap();
        let received_at = Instant::now();
        state.activity += u64::from(matches!(input, Input::Midi(_)));
        state.sequence += 1;
        if matches!(&input, Input::Midi(bytes) if bytes.len() > MIDI_MESSAGE_LIMIT) {
            state.dropped.oversized += 1;
            state.urgent = true;
        } else {
            let event = Envelope {
                endpoint,
                received_at,
                sequence: state.sequence,
                input,
            };
            match event.input.queue_class() {
                QueueClass::Continuous if state.continuous.len() < CONTINUOUS_CAPACITY => {
                    state.continuous.push_back(event);
                    state
                        .continuous_due
                        .get_or_insert(received_at + CONTINUOUS_INTERVAL);
                }
                QueueClass::Discrete if state.discrete.len() < DISCRETE_CAPACITY => {
                    state.discrete.push_back(event)
                }
                QueueClass::Continuous => {
                    state.dropped.continuous += 1;
                    state.urgent = true;
                }
                QueueClass::Discrete => {
                    state.dropped.discrete += 1;
                    state.urgent = true;
                }
            }
        }
        self.changed.notify_one();
    }

    pub fn push_midi(&self, endpoint: &Endpoint, bytes: &[u8]) {
        // Do not allocate an oversized SysEx in the callback.
        if bytes.len() > MIDI_MESSAGE_LIMIT {
            let mut state = self.state.lock().unwrap();
            state.activity += 1;
            state.sequence += 1;
            state.dropped.oversized += 1;
            state.urgent = true;
            self.changed.notify_one();
        } else if !bytes.is_empty() {
            self.push(endpoint.clone(), Input::Midi(bytes.into()));
        }
    }

    /// Port-list changes and errors wake the host without an idle polling subscription.
    pub fn notify_host(&self) {
        self.state.lock().unwrap().urgent = true;
        self.changed.notify_one();
    }

    /// Release an outstanding notification when the host wake subscription stops.
    /// A replacement subscription must be able to wake for input left undrained.
    pub fn reset_wake(&self) {
        self.state.lock().unwrap().in_flight = false;
        self.changed.notify_one();
    }

    /// Used on a host wake thread, never on its UI/engine owner thread. Discrete input
    /// interrupts the continuous timer; only one notification may await a host drain.
    /// A host must set stop and call notify_host when its subscription is dropped.
    pub fn wait_for_wake(&self, stop: &AtomicBool) -> bool {
        let mut state = self.state.lock().unwrap();
        loop {
            if stop.load(Ordering::Acquire) {
                return false;
            }
            let now = Instant::now();
            if !state.in_flight {
                if state.urgent
                    || !state.discrete.is_empty()
                    || state.continuous_due.is_some_and(|due| due <= now)
                {
                    state.in_flight = true;
                    return true;
                }
                if let Some(due) = state.continuous_due {
                    state = self
                        .changed
                        .wait_timeout(state, due.saturating_duration_since(now))
                        .unwrap()
                        .0;
                    continue;
                }
            }
            state = self.changed.wait(state).unwrap();
        }
    }

    /// Project replacement must discard all pending input, including a backlog
    /// larger than one drain. Keep activity, sequence and loss accounting intact.
    pub fn discard_pending(&self) {
        let mut state = self.state.lock().unwrap();
        state.continuous.clear();
        state.discrete.clear();
        state.continuous_due = None;
        state.in_flight = false;
        self.changed.notify_one();
    }

    pub fn drain(&self) -> Drain {
        let mut state = self.state.lock().unwrap();
        let mut events = Vec::with_capacity(DRAIN_LIMIT);
        for _ in 0..DRAIN_LIMIT {
            let queue = match (state.continuous.front(), state.discrete.front()) {
                (Some(c), Some(d)) if c.sequence < d.sequence => &mut state.continuous,
                (Some(_), Some(_)) | (None, Some(_)) => &mut state.discrete,
                (Some(_), None) => &mut state.continuous,
                (None, None) => break,
            };
            events.push(queue.pop_front().unwrap());
        }
        let dropped = Overflow {
            continuous: state.dropped.continuous - state.reported.continuous,
            discrete: state.dropped.discrete - state.reported.discrete,
            oversized: state.dropped.oversized - state.reported.oversized,
        };
        state.reported = state.dropped;
        state.urgent = false;
        state.in_flight = false;
        state.continuous_due =
            (!state.continuous.is_empty()).then(|| Instant::now() + CONTINUOUS_INTERVAL);
        self.changed.notify_one();
        Drain {
            events,
            dropped,
            activity: state.activity,
        }
    }

    pub fn overflow(&self) -> Overflow {
        self.state.lock().unwrap().dropped
    }
}

#[cfg(test)]
mod tests;
