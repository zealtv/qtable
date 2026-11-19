//! Off-thread output resolution, so DNS never runs on the UI or scheduler threads.
//!
//! The scheduler hands the resolver the outputs it cannot already send to. The
//! resolver looks each one up on its own thread and reports back with
//! `scheduler::SchedulerCommand::Resolved`. Failures stay in a shared status table and are
//! retried with backoff; playback skips sends to unresolved outputs. Resolved
//! hostnames are looked up again on a slow timer, so a receiver that moves to a new
//! address is followed. Only the newest wanted list is acted on.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::mpsc::{self, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use crate::model::{Track, TrackId};
use crate::scheduler::{CommandSender, SchedulerCommand};
use crate::sender::OscSender;

/// Minimum gap between "unresolved" errors for one output.
const REPORT_EVERY: Duration = Duration::from_secs(5);

/// How the resolver paces its lookups.
#[derive(Clone, Copy, Debug)]
struct Timing {
    /// Wait after a host's first failed lookup; doubles with each further failure.
    retry: Duration,
    /// Longest wait between lookups of a failing host.
    retry_max: Duration,
    /// Wait between re-lookups of a host that resolved, to follow address changes.
    refresh: Duration,
}

impl Timing {
    /// Wait before retry number `failures` (1-based): doubles up to `retry_max`.
    fn backoff(&self, failures: u32) -> Duration {
        let doublings = failures.saturating_sub(1).min(16);
        self.retry
            .saturating_mul(1 << doublings)
            .min(self.retry_max)
    }
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            retry: Duration::from_secs(2),
            retry_max: Duration::from_secs(30),
            refresh: Duration::from_secs(30),
        }
    }
}

/// An output that has no usable address yet, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnresolvedOutput {
    pub id: TrackId,
    pub reason: String,
}

pub(crate) type Status = Arc<Mutex<HashMap<TrackId, String>>>;

/// Lock the status table, ignoring poison: it holds plain strings that the next
/// lookup overwrites.
fn lock(status: &Status) -> MutexGuard<'_, HashMap<TrackId, String>> {
    status.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One hostname the resolver keeps resolved, and when to look it up next.
struct Entry {
    output: Track,
    due: Instant,
    failures: u32,
    /// The last address found; `None` until the first lookup succeeds.
    address: Option<SocketAddr>,
}

/// The resolver thread's bookkeeping: which hosts to look up and when.
/// It does no I/O itself, so tests drive it with a fake clock and lookup.
struct Book {
    entries: HashMap<TrackId, Entry>,
    status: Status,
    timing: Timing,
}

impl Book {
    fn new(status: Status, timing: Timing) -> Self {
        Self {
            entries: HashMap::new(),
            status,
            timing,
        }
    }

    /// Adopt the latest wanted list. Hosts already tracked keep their schedule
    /// and last address; new or re-pointed ones are due at once.
    fn sync(&mut self, wanted: Vec<Track>, now: Instant) {
        let mut table = lock(&self.status);
        let mut kept = HashMap::new();
        for output in wanted {
            let id = output.id;
            let entry = match self.entries.remove(&id) {
                Some(old) if endpoint(&old.output) == endpoint(&output) => Entry { output, ..old },
                _ => {
                    table.insert(id, "resolving".into());
                    Entry {
                        output,
                        due: now,
                        failures: 0,
                        address: None,
                    }
                }
            };
            kept.insert(id, entry);
        }
        self.entries = kept;
        table.retain(|id, _| self.entries.contains_key(id));
    }

    /// How long until the earliest lookup; `None` when nothing is tracked.
    fn wait(&self, now: Instant) -> Option<Duration> {
        let due = self.entries.values().map(|entry| entry.due).min()?;
        Some(due.saturating_duration_since(now))
    }

    /// Look up the host that has waited longest, if any is due, and schedule its
    /// next attempt. Returns the output and address only when the address
    /// is new or changed, which is when the scheduler needs to hear of it.
    fn lookup_next(
        &mut self,
        now: Instant,
        lookup: impl FnOnce(&Track) -> io::Result<SocketAddr>,
    ) -> Option<(Track, SocketAddr)> {
        let entry = self
            .entries
            .values_mut()
            .filter(|entry| entry.due <= now)
            .min_by_key(|entry| (entry.due, entry.output.id.0))?;
        let id = entry.output.id;
        match lookup(&entry.output) {
            Ok(address) => {
                entry.failures = 0;
                entry.due = now + self.timing.refresh;
                lock(&self.status).remove(&id);
                let changed = entry.address.replace(address) != Some(address);
                changed.then(|| (entry.output.clone(), address))
            }
            Err(error) => {
                entry.failures += 1;
                entry.due = now + self.timing.backoff(entry.failures);
                // A failed refresh keeps sending to the last good address.
                if entry.address.is_none() {
                    lock(&self.status).insert(id, error.to_string());
                }
                None
            }
        }
    }
}

type Lookup = Box<dyn Fn(&Track) -> io::Result<SocketAddr> + Send>;

pub(crate) struct Resolver {
    wanted: Sender<Vec<Track>>,
    status: Status,
}

impl Resolver {
    /// Start the lookup thread. It exits once the `Resolver` is dropped or the
    /// scheduler's command channel closes.
    pub(crate) fn spawn(commands: CommandSender) -> io::Result<Self> {
        Self::spawn_with(commands, Timing::default(), Box::new(lookup))
    }

    fn spawn_with(commands: CommandSender, timing: Timing, lookup: Lookup) -> io::Result<Self> {
        let (wanted, requests) = mpsc::channel::<Vec<Track>>();
        let status: Status = Arc::default();
        let mut book = Book::new(Arc::clone(&status), timing);
        thread::Builder::new()
            .name("qtable-resolver".into())
            .spawn(move || {
                loop {
                    let incoming = match book.wait(Instant::now()) {
                        None => requests.recv().map_err(|_| RecvTimeoutError::Disconnected),
                        Some(wait) if wait.is_zero() => {
                            requests.try_recv().map_err(|error| match error {
                                TryRecvError::Empty => RecvTimeoutError::Timeout,
                                TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
                            })
                        }
                        Some(wait) => requests.recv_timeout(wait),
                    };
                    match incoming {
                        Ok(mut latest) => {
                            // Only the newest list matters; skip any older ones queued behind it.
                            while let Ok(newer) = requests.try_recv() {
                                latest = newer;
                            }
                            book.sync(latest, Instant::now());
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                    // One lookup per pass, so a new request waits for at most one.
                    if let Some((output, address)) = book.lookup_next(Instant::now(), &lookup)
                        && commands
                            .send(SchedulerCommand::Resolved(output, address))
                            .is_err()
                    {
                        return;
                    }
                }
            })?;
        Ok(Self { wanted, status })
    }

    /// Replace the set of outputs to keep resolved; an empty list cancels all lookups.
    pub(crate) fn want(&self, outputs: Vec<Track>) {
        let _ = self.wanted.send(outputs);
    }

    /// Resolve IP literals now (no DNS) and hand every hostname to the resolver
    /// thread, which looks it up and then refreshes it on a slow timer.
    pub(crate) fn request<'a>(
        &self,
        sender: &mut OscSender,
        outputs: impl IntoIterator<Item = &'a Track>,
    ) {
        let hostnames = outputs
            .into_iter()
            .filter(|d| {
                let literal = endpoint(d).is_some_and(|(host, _)| host.parse::<IpAddr>().is_ok());
                !(literal && sender.resolve_literal(d))
            })
            .cloned()
            .collect();
        self.want(hostnames);
    }

    pub(crate) fn status(&self) -> Status {
        Arc::clone(&self.status)
    }
}

/// Blocking lookup, IPv4 only like the sender socket. Call from the resolver thread.
fn lookup(output: &Track) -> io::Result<SocketAddr> {
    let (host, port) = endpoint(output)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "track is not an output"))?;
    (host, port)
        .to_socket_addrs()?
        .find(SocketAddr::is_ipv4)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "host resolved to no IPv4 address",
            )
        })
}

/// Allows one report per output every `REPORT_EVERY`.
#[derive(Default)]
pub(crate) struct ReportGate(HashMap<TrackId, Instant>);

impl ReportGate {
    pub(crate) fn allow(&mut self, id: TrackId, now: Instant) -> bool {
        match self.0.get(&id) {
            Some(last) if now.saturating_duration_since(*last) < REPORT_EVERY => false,
            _ => {
                self.0.insert(id, now);
                true
            }
        }
    }
}

pub(crate) fn unresolved_error(output: &Track) -> io::Error {
    let (host, port) = endpoint(output).unwrap_or_default();
    io::Error::new(
        io::ErrorKind::AddrNotAvailable,
        format!(
            "output {:?} ({host}:{port}) is unresolved; skipping its sends",
            output.name
        ),
    )
}

/// Host and port of an output track; None for any other kind.
fn endpoint(output: &Track) -> Option<(&str, u16)> {
    output.destination().map(|d| (d.host.as_str(), d.port))
}

pub(crate) fn list(status: &Status) -> Vec<UnresolvedOutput> {
    let mut list: Vec<_> = lock(status)
        .iter()
        .map(|(id, reason)| UnresolvedOutput {
            id: *id,
            reason: reason.clone(),
        })
        .collect();
    list.sort_by_key(|item| item.id.0);
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIMING: Timing = Timing {
        retry: Duration::from_secs(2),
        retry_max: Duration::from_secs(30),
        refresh: Duration::from_secs(30),
    };

    fn host(id: u64, name: &str) -> Track {
        Track::new_output(TrackId(id), format!("d{id}"), name, 9000)
    }

    fn addr(last: u8) -> SocketAddr {
        SocketAddr::from(([10, 0, 0, last], 9000))
    }

    fn fail(_: &Track) -> io::Result<SocketAddr> {
        Err(io::Error::other("no such host"))
    }

    fn book() -> Book {
        Book::new(Arc::default(), TIMING)
    }

    #[test]
    fn gate_limits_each_output_separately() {
        let mut gate = ReportGate::default();
        let t0 = Instant::now();
        assert!(gate.allow(TrackId(1), t0));
        assert!(!gate.allow(TrackId(1), t0 + Duration::from_secs(1)));
        assert!(gate.allow(TrackId(2), t0 + Duration::from_secs(1)));
        assert!(gate.allow(TrackId(1), t0 + REPORT_EVERY));
    }

    #[test]
    fn failing_host_backs_off_to_the_cap_without_delaying_others() {
        let mut book = book();
        let t0 = Instant::now();
        book.sync(vec![host(1, "a.local"), host(2, "b.local")], t0);
        let mut now = t0;
        let mut waits = Vec::new();
        for _ in 0..6 {
            book.lookup_next(now, fail);
            let wait = book.entries[&TrackId(1)].due - now;
            waits.push(wait.as_secs());
            now = book.entries[&TrackId(1)].due;
            // Host 2 resolves once; its schedule must not follow host 1's backoff.
            book.entries.get_mut(&TrackId(2)).unwrap().due = now + Duration::from_secs(99);
        }
        assert_eq!(waits, [2, 4, 8, 16, 30, 30]);
    }

    #[test]
    fn success_resets_backoff_and_clears_status() {
        let mut book = book();
        let t0 = Instant::now();
        book.sync(vec![host(1, "a.local")], t0);
        book.lookup_next(t0, fail);
        assert_eq!(list(&book.status)[0].reason, "no such host");
        let t1 = t0 + Duration::from_secs(2);
        let found = book.lookup_next(t1, |_| Ok(addr(1)));
        assert_eq!(found.map(|(_, a)| a), Some(addr(1)));
        assert!(list(&book.status).is_empty());
        assert_eq!(book.entries[&TrackId(1)].failures, 0);
        assert_eq!(book.wait(t1), Some(TIMING.refresh));
    }

    #[test]
    fn refresh_reports_only_a_changed_address() {
        let mut book = book();
        let t0 = Instant::now();
        book.sync(vec![host(1, "a.local")], t0);
        assert!(book.lookup_next(t0, |_| Ok(addr(1))).is_some());
        let t1 = t0 + TIMING.refresh;
        assert!(book.lookup_next(t1, |_| Ok(addr(1))).is_none());
        let t2 = t1 + TIMING.refresh;
        let moved = book.lookup_next(t2, |_| Ok(addr(2)));
        assert_eq!(moved.map(|(_, a)| a), Some(addr(2)));
    }

    #[test]
    fn failed_refresh_keeps_the_host_resolved() {
        let mut book = book();
        let t0 = Instant::now();
        book.sync(vec![host(1, "a.local")], t0);
        book.lookup_next(t0, |_| Ok(addr(1)));
        book.lookup_next(t0 + TIMING.refresh, fail);
        assert!(list(&book.status).is_empty());
        assert_eq!(book.wait(t0 + TIMING.refresh), Some(TIMING.retry));
        // Back at the same address afterwards: nothing new to report.
        assert!(
            book.lookup_next(t0 + TIMING.refresh + TIMING.retry, |_| Ok(addr(1)))
                .is_none()
        );
    }

    #[test]
    fn sync_keeps_schedules_for_unchanged_hosts_and_resets_edited_ones() {
        let mut book = book();
        let t0 = Instant::now();
        book.sync(vec![host(1, "a.local"), host(2, "b.local")], t0);
        book.lookup_next(t0, |_| Ok(addr(1)));
        book.lookup_next(t0, fail);
        let t1 = t0 + Duration::from_secs(1);
        // Host 1 unchanged, host 2 re-pointed, host 3 added, nothing else wanted.
        book.sync(
            vec![host(1, "a.local"), host(2, "c.local"), host(3, "d.local")],
            t1,
        );
        assert_eq!(book.entries[&TrackId(1)].due, t0 + TIMING.refresh);
        assert_eq!(book.entries[&TrackId(2)].due, t1);
        assert_eq!(book.entries[&TrackId(3)].due, t1);
        assert_eq!(list(&book.status).len(), 2);
        book.sync(vec![host(3, "d.local")], t1);
        assert_eq!(book.entries.len(), 1);
        assert_eq!(list(&book.status).len(), 1);
    }

    #[test]
    fn requests_queued_behind_a_lookup_collapse_to_the_newest() {
        let (commands, resolved) = CommandSender::channel();
        let looked_up = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&looked_up);
        let resolver = Resolver::spawn_with(
            commands,
            TIMING,
            Box::new(move |output| {
                log.lock()
                    .unwrap()
                    .push(output.destination().unwrap().host.clone());
                thread::sleep(Duration::from_millis(100));
                Ok(addr(1))
            }),
        )
        .unwrap();
        resolver.want(vec![host(1, "first")]);
        thread::sleep(Duration::from_millis(30));
        for name in ["second", "third", "fourth", "last"] {
            resolver.want(vec![host(1, name)]);
        }
        let deadline = Duration::from_secs(5);
        let mut seen = 0;
        while seen < 2 {
            resolved.recv_timeout(deadline).expect("resolved in time");
            seen += 1;
        }
        let log = looked_up.lock().unwrap();
        assert_eq!(*log, ["first", "last"]);
    }
}
