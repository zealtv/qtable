//! GUI host for the headless remote hub. The wake subscription is independent of redraws.
mod ports_panel;
mod settings;
mod wake;

use qtable_remote::{Drain, Envelope, Ingress, MappingResolver, MidiPorts, PortSnapshot, Router};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) use ports_panel::{indicator, tab};
pub(crate) use wake::subscription;

const ACTIVITY_DECAY: Duration = Duration::from_millis(150);
const DEBUG_LOG_LIMIT: usize = 128;

#[derive(Debug, Clone)]
pub(crate) enum Action {
    Wake,
    SetRemote(String, bool),
    Error(String),
}

pub(crate) struct State {
    pub ingress: Arc<Ingress>,
    /// Physical key (iced's unmodified key) to the typed character it pressed.
    pub held_keys: HashMap<String, String>,
    pub router: Router,
    pub ports: PortSnapshot,
    pub runtime: Option<MidiPorts>,
    settings: settings::Settings,
    settings_path: Option<PathBuf>,
    pub activity: u64,
    pub last_activity: Option<Instant>,
    pub debug_log: VecDeque<Envelope>,
    debug_print: bool,
}

impl Default for State {
    fn default() -> Self {
        let mut router = Router::default();
        router.subscribe(MappingResolver::default());
        Self {
            ingress: Arc::default(),
            held_keys: HashMap::new(),
            router,
            ports: PortSnapshot::default(),
            runtime: None,
            settings: settings::Settings::default(),
            settings_path: None,
            activity: 0,
            last_activity: None,
            debug_log: VecDeque::new(),
            debug_print: false,
        }
    }
}

impl State {
    /// Only the actual app boot starts hardware I/O; App::new stays deterministic for tests.
    pub fn start(&mut self) -> Result<(), String> {
        let location = settings::location()?;
        let settings = settings::load(&location);
        if let Ok(settings) = &settings {
            self.settings = settings.clone();
            self.settings_path = Some(location.current);
        }
        self.debug_print = std::env::var_os("QTABLE_MIDI_DEBUG").is_some();
        self.runtime = Some(
            MidiPorts::start(self.ingress.clone(), self.settings.midi.remote.clone())
                .map_err(|error| format!("Could not start MIDI input: {error}"))?,
        );
        settings.map(|_| ())
    }

    pub fn set_remote(&mut self, name: String, enabled: bool) -> Result<(), String> {
        self.settings.midi.remote.insert(name.clone(), enabled);
        if let Some(runtime) = &self.runtime {
            runtime.set_remote(name, enabled);
        }
        let path = self
            .settings_path
            .as_deref()
            .ok_or("MIDI settings could not be loaded; changes apply for this session only")?;
        settings::save(path, &self.settings)
    }

    pub fn refresh_ports(&mut self) -> Option<String> {
        let ports = self.runtime.as_ref()?.snapshot();
        let error = if ports != self.ports {
            ports
                .error
                .clone()
                .or_else(|| ports.ports.iter().find_map(|port| port.error.clone()))
        } else {
            None
        };
        self.ports = ports;
        error
    }

    pub fn observe(&mut self, drain: Drain) {
        if drain.activity != self.activity {
            self.last_activity = Some(Instant::now());
            self.activity = drain.activity;
        }
        for event in drain.events {
            if self.debug_print {
                eprintln!(
                    "MIDI input #{} {:?} {:?} {:?}",
                    event.sequence, event.received_at, event.endpoint, event.input
                );
            }
            if self.debug_log.len() == DEBUG_LOG_LIMIT {
                self.debug_log.pop_front();
            }
            self.debug_log.push_back(event);
        }
    }

    pub fn activity_level(&self) -> f32 {
        self.last_activity.map_or(0.0, |last| {
            1.0 - (last.elapsed().as_secs_f32() / ACTIVITY_DECAY.as_secs_f32()).min(1.0)
        })
    }
}
