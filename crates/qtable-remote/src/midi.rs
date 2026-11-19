//! MIDI adapter and hot-plug discovery. Remote preferences are by name (per machine),
//! while runtime endpoints retain backend IDs to distinguish identical controller names.
pub mod decode;
#[cfg(target_os = "macos")]
mod notifications;
use crate::{Endpoint, Ingress};
use midir::{Ignore, MidiInput, MidiInputConnection, MidiInputPort};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

const DISCOVERY_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    pub id: String,
    pub name: String,
    pub remote: bool,
    pub connected: bool,
    pub error: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PortSnapshot {
    pub ports: Vec<Port>,
    pub error: Option<String>,
}

#[derive(Default)]
struct Preferences {
    remote: BTreeMap<String, bool>,
    revision: u64,
    stop: bool,
}

/// Owned by the host. Discovery and connection lifetimes stay off its engine thread.
pub struct MidiPorts {
    preferences: Arc<(Mutex<Preferences>, Condvar)>,
    snapshot: Arc<Mutex<PortSnapshot>>,
    worker: Option<JoinHandle<()>>,
}

impl MidiPorts {
    pub fn start(ingress: Arc<Ingress>, remote: BTreeMap<String, bool>) -> std::io::Result<Self> {
        let preferences = Arc::new((
            Mutex::new(Preferences {
                remote,
                ..Default::default()
            }),
            Condvar::new(),
        ));
        let snapshot = Arc::new(Mutex::new(PortSnapshot::default()));
        let worker_preferences = preferences.clone();
        let worker_snapshot = snapshot.clone();
        let worker = thread::Builder::new()
            .name("qtable-midi-ports".into())
            .spawn(move || {
                run(ingress, worker_preferences, worker_snapshot);
            })?;
        Ok(Self {
            preferences,
            snapshot,
            worker: Some(worker),
        })
    }

    pub fn set_remote(&self, name: String, enabled: bool) {
        let mut preferences = self.preferences.0.lock().unwrap();
        preferences.remote.insert(name, enabled);
        preferences.revision += 1;
        self.preferences.1.notify_one();
    }

    pub fn snapshot(&self) -> PortSnapshot {
        self.snapshot.lock().unwrap().clone()
    }
}

impl Drop for MidiPorts {
    fn drop(&mut self) {
        self.preferences.0.lock().unwrap().stop = true;
        self.preferences.1.notify_one();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn publish(snapshot: &Mutex<PortSnapshot>, next: PortSnapshot, ingress: &Ingress) {
    let mut current = snapshot.lock().unwrap();
    if *current != next {
        *current = next;
        ingress.notify_host();
    }
}

fn run(
    ingress: Arc<Ingress>,
    preferences: Arc<(Mutex<Preferences>, Condvar)>,
    snapshot: Arc<Mutex<PortSnapshot>>,
) {
    let mut connections: HashMap<String, MidiInputConnection<()>> = HashMap::new();
    let mut input = MidiInput::new("qtable input discovery");
    loop {
        let (remote, revision) = {
            let state = preferences.0.lock().unwrap();
            if state.stop {
                break;
            }
            (state.remote.clone(), state.revision)
        };
        // CoreMIDI delivers external endpoint changes on the run loop where the
        // process first created a client: this worker, not the GUI's run loop.
        #[cfg(target_os = "macos")]
        notifications::pump();
        let next = match &input {
            Ok(input) => refresh(input, &remote, &mut connections, &ingress),
            Err(error) => {
                connections.clear();
                PortSnapshot {
                    error: Some(format!("MIDI input unavailable: {error}")),
                    ..Default::default()
                }
            }
        };
        publish(&snapshot, next, &ingress);
        let state = preferences.0.lock().unwrap();
        let _ = preferences
            .1
            .wait_timeout_while(state, DISCOVERY_INTERVAL, |state| {
                !state.stop && state.revision == revision
            })
            .unwrap();
        // Keep one discovery client alive. CoreMIDI clients are process-lived in
        // midir's backend; recreating one each second would accumulate clients.
        if input.is_err() {
            input = MidiInput::new("qtable input discovery");
        }
    }
}

/// A scan's backend handle and stable identity; reconciliation needs no MIDI I/O.
struct Available<P> {
    handle: P,
    id: String,
    name: String,
}

fn refresh(
    input: &MidiInput,
    remote: &BTreeMap<String, bool>,
    connections: &mut HashMap<String, MidiInputConnection<()>>,
    ingress: &Arc<Ingress>,
) -> PortSnapshot {
    let available = input
        .ports()
        .into_iter()
        .filter_map(|port| {
            input.port_name(&port).ok().map(|name| Available {
                id: port.id(),
                name,
                handle: port,
            })
        })
        .collect();
    reconcile(available, remote, connections, |port| {
        connect(port, ingress)
    })
}

fn connect(
    port: &Available<MidiInputPort>,
    ingress: &Arc<Ingress>,
) -> Result<MidiInputConnection<()>, String> {
    let mut input = MidiInput::new("qtable remote input").map_err(|e| e.to_string())?;
    input.ignore(Ignore::None);
    let endpoint = Endpoint::Midi {
        id: port.id.clone(),
        name: port.name.clone(),
    };
    let ingress = ingress.clone();
    input
        .connect(
            &port.handle,
            "qtable remote",
            move |_, bytes, _| ingress.push_midi(&endpoint, bytes),
            (),
        )
        .map_err(|e| e.to_string())
}

fn reconcile<P, C>(
    available: Vec<Available<P>>,
    remote: &BTreeMap<String, bool>,
    connections: &mut HashMap<String, C>,
    mut connect: impl FnMut(&Available<P>) -> Result<C, String>,
) -> PortSnapshot {
    connections.retain(|id, _| {
        available
            .iter()
            .any(|port| port.id == *id && remote.get(&port.name) == Some(&true))
    });
    let mut ports = Vec::new();
    for port in available {
        let enabled = remote.get(&port.name).copied().unwrap_or(false);
        let mut error = None;
        if enabled && !connections.contains_key(&port.id) {
            match connect(&port) {
                Ok(connection) => {
                    connections.insert(port.id.clone(), connection);
                }
                Err(cause) => {
                    error = Some(format!("MIDI input {}: {cause}", port.name));
                }
            }
        }
        ports.push(Port {
            connected: connections.contains_key(&port.id),
            id: port.id,
            name: port.name,
            remote: enabled,
            error,
        });
    }
    ports.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
    PortSnapshot { ports, error: None }
}

#[cfg(test)]
mod tests;
