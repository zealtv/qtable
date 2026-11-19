//! Runtime-only envelope values. Each lifecycle change retains its old clock and drivers
//! until every positive output delay has passed it. Frames never edit the project.
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::parameter_controls::{SendTemplate, SendValues};
use super::{Planner, Timeline, delay, marks};
use crate::clock::{InternalClock, TempoClock};
use crate::model::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomationState {
    Following,
    Held,
    Overridden,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomationValue {
    pub field: FieldRef,
    pub value: f64,
    pub state: AutomationState,
}

#[derive(Clone)]
struct Driver {
    voice: u64,
    project: Arc<Project>,
    track: usize,
    slot: usize,
    lane: usize,
    launch: BeatTime,
    origin: BeatTime,
    continuation: Option<super::timeline::Continuation>,
    end: Option<BeatTime>,
    axis: Axis,
    /// Non-flat point pairs, indexed once per state rebuild, shared by history clones.
    changes: Arc<[usize]>,
}

impl Driver {
    fn clip(&self) -> &Clip {
        self.project.tracks[self.track]
            .clip(self.slot)
            .expect("indexed clip")
    }
    fn envelope(&self) -> &AutomationEnvelope {
        self.clip().lanes[self.lane]
            .envelope()
            .expect("indexed envelope")
    }
    fn region(&self) -> crate::model::region::Region {
        self.envelope()
            .region
            .unwrap_or_else(|| self.clip().region())
            .normalised()
    }
    fn timeline(&self) -> Timeline {
        Timeline::held(self.launch, self.origin, self.end, self.region())
            .continuing(self.continuation)
    }
    fn position(&self, ticks: f64) -> f64 {
        self.timeline().sample_position(ticks)
    }
    fn value(&self, ticks: f64) -> f64 {
        self.envelope()
            .value_at(self.position(ticks), self.axis)
            .expect("nonempty envelope")
    }
    fn outgoing_value(&self, ticks: f64) -> f64 {
        self.envelope()
            .value_at(self.timeline().outgoing_position(ticks), self.axis)
            .expect("nonempty envelope")
    }
    fn active(&self, ticks: f64) -> bool {
        ticks >= self.launch.0 as f64 && self.end.is_none_or(|end| ticks < end.0 as f64)
    }
}

#[derive(Clone)]
struct Override {
    value: f64,
    state: AutomationState,
    voices: Vec<u64>,
}
#[derive(Clone, Default)]
struct Field {
    overridden: Option<Override>,
    held: Option<f64>,
    drivers: Vec<Driver>,
}
impl Field {
    fn value(&self, ticks: f64) -> Option<(f64, AutomationState)> {
        let driver = self
            .drivers
            .iter()
            .rev()
            .find(|d| d.launch.0 as f64 <= ticks);
        if let Some(overridden) = &self.overridden
            && driver.is_none_or(|d| overridden.voices.contains(&d.voice))
        {
            return Some((overridden.value, overridden.state));
        }
        match self
            .drivers
            .iter()
            .rev()
            .find(|d| d.launch.0 as f64 <= ticks)
        {
            Some(d) => Some((
                d.value(ticks),
                if d.active(ticks) {
                    AutomationState::Following
                } else {
                    AutomationState::Held
                },
            )),
            None => self.held.map(|v| (v, AutomationState::Held)),
        }
    }
}

/// A parameter with driven fields, indexed when its state is built so frames needn't search.
#[derive(Clone)]
struct Driven {
    fields: Vec<FieldRef>,
    /// Complete base values and conversion metadata for this state's definition.
    template: Arc<SendTemplate>,
    base: Arc<[f64]>,
    positions: Arc<[Option<FieldRef>]>,
}

#[derive(Clone)]
struct State {
    project: Arc<Project>,
    at: Instant,
    clock: InternalClock,
    fields: HashMap<FieldRef, Arc<Field>>,
    parameters: HashMap<ParameterRef, Arc<Driven>>,
}
impl State {
    fn held_value(&self, field: &FieldRef, at: Instant) -> Option<f64> {
        let ticks = self.clock.fractional_ticks_at(at)?;
        let field = self.fields.get(field)?;
        if let Some((value, state)) = field.value(ticks)
            && field.overridden.is_some()
            && state != AutomationState::Following
        {
            return Some(value);
        }
        field
            .drivers
            .iter()
            .rev()
            .find(|driver| driver.launch.0 as f64 <= ticks)
            .map(|driver| driver.outgoing_value(ticks))
            .or(field.held)
    }
    fn value(&self, field: &FieldRef, at: Instant) -> Option<(f64, AutomationState)> {
        self.fields
            .get(field)?
            .value(self.clock.fractional_ticks_at(at)?)
    }
}

/// One envelope lane of a voice, as a sync turns it into a driver.
#[derive(Clone, Debug, PartialEq)]
struct Source {
    voice: u64,
    track: TrackId,
    slot: usize,
    lane: usize,
    launch: BeatTime,
    origin: BeatTime,
    continuation: Option<super::timeline::Continuation>,
    end: Option<BeatTime>,
}

impl Source {
    /// Whether this driver can still change a value at `ticks`; once its end has passed it only
    /// holds its end value, which a rebuilt state would carry as the same held value.
    fn live(&self, ticks: f64) -> bool {
        self.end.is_none_or(|end| end.0 as f64 > ticks)
    }
}

/// Every envelope lane the planner's voices drive, in voice order.
fn sources(planner: &Planner) -> Vec<Source> {
    let project = &planner.project;
    let mut sources = Vec::new();
    for voice in &planner.voices {
        let Some(clip) = project.clip(voice.track, voice.slot) else {
            continue;
        };
        for (lane, envelope) in clip.lanes.iter().enumerate() {
            if let Some(envelope) = envelope.envelope().filter(|e| !e.points.is_empty()) {
                let timeline = voice.envelope_timeline(clip, envelope);
                let end = timeline
                    .end()
                    .into_iter()
                    .chain(planner.stops.get(&voice.track).map(|stop| stop.beat))
                    .min();
                sources.push(Source {
                    voice: voice.id,
                    track: voice.track,
                    slot: voice.slot,
                    lane,
                    launch: voice.launch,
                    origin: voice.envelope_origin(envelope),
                    continuation: if envelope.region.is_some() {
                        voice.envelope_continuations.get(&envelope.target).copied()
                    } else {
                        voice.continuation
                    },
                    end,
                });
            }
        }
    }
    sources
}

#[derive(Clone, Default)]
pub(super) struct Automation {
    history: Vec<Arc<State>>,
    project: Option<Arc<Project>>,
    /// What the latest sync built its drivers from.
    sources: Vec<Source>,
    /// Recomputed on structural syncs; value edits cannot change output delays.
    history_delay: Duration,
}

impl Automation {
    pub fn anchor(&self) -> Option<Instant> {
        self.history.last().map(|s| s.at)
    }

    /// Whether a sync at `at` would change any value from `at` on. Only a command changes the
    /// clock or the project, and a command always syncs; a consumed event can only add, end or
    /// remove voices. A voice whose end has passed holds its end value either way.
    pub fn changes(&self, planner: &Planner, clock: &InternalClock, at: Instant) -> bool {
        let (Some(project), Some(ticks)) = (&self.project, clock.fractional_ticks_at(at)) else {
            return true;
        };
        if !Arc::ptr_eq(project, &planner.project) {
            return true;
        }
        let live = |sources: &[Source]| -> Vec<Source> {
            sources.iter().filter(|s| s.live(ticks)).cloned().collect()
        };
        live(&self.sources) != live(&sources(planner))
    }
    /// Rebuild at a command or voice transition, never on a frame or spin iteration.
    pub fn sync(&mut self, planner: &Planner, clock: &InternalClock, at: Instant) {
        let project = &planner.project;
        // With the same project (most syncs: launches and lifecycle changes), nothing was edited,
        // no identity was removed and every parameter keeps its place.
        let unchanged = self
            .project
            .as_ref()
            .is_some_and(|previous| Arc::ptr_eq(previous, project));
        let mut fields = HashMap::new();
        if let Some(old) = self.history.last() {
            for key in old.fields.keys() {
                let Some(current) = project.field(key) else {
                    continue;
                };
                let edited = !unchanged
                    && self
                        .project
                        .as_ref()
                        .and_then(|p| p.field(key))
                        .is_some_and(|previous| previous.value != current.value);
                let held = if edited && old.fields[key].overridden.is_none() {
                    None
                } else {
                    old.held_value(key, at).map(|v| {
                        current
                            .spec
                            .check(v.clamp(current.spec.min, current.spec.max))
                            .unwrap_or(current.value)
                    })
                };
                if held.is_some() {
                    fields.insert(
                        key.clone(),
                        Arc::new(Field {
                            held,
                            overridden: old
                                .value(key, at)
                                .filter(|(_, state)| *state != AutomationState::Following)
                                .and_then(|_| old.fields.get(key)?.overridden.clone())
                                .map(|mut overridden| {
                                    overridden.value = held.unwrap_or(current.value);
                                    overridden
                                }),
                            drivers: vec![],
                        }),
                    );
                }
            }
        }
        marks::mark("sync.carried");
        let sources = sources(planner);
        for source in &sources {
            let Some(envelope) = project
                .clip(source.track, source.slot)
                .and_then(|clip| clip.lanes[source.lane].envelope())
            else {
                continue;
            };
            let Some(axis) = project
                .track(source.track)
                .and_then(|t| t.argument_axis(&envelope.target))
            else {
                continue;
            };
            Arc::make_mut(fields.entry(envelope.target.on(source.track)).or_default())
                .drivers
                .push(Driver {
                    voice: source.voice,
                    project: Arc::clone(project),
                    track: project
                        .tracks
                        .iter()
                        .position(|t| t.id == source.track)
                        .expect("source track"),
                    slot: source.slot,
                    lane: source.lane,
                    launch: source.launch,
                    origin: source.origin,
                    continuation: source.continuation,
                    end: source.end,
                    axis,
                    changes: wake::changing_segments(&envelope.points),
                });
        }
        self.sources = sources;
        for field in fields.values_mut() {
            Arc::make_mut(field).drivers.sort_by_key(|d| d.launch);
        }
        marks::mark("sync.drivers");
        // A changed definition must also invalidate past histories for removed identities.
        for state in self.history.iter_mut().filter(|_| !unchanged) {
            if state.fields.keys().any(|key| project.field(key).is_none()) {
                Arc::make_mut(state)
                    .fields
                    .retain(|key, _| project.field(key).is_some());
            }
        }
        marks::mark("sync.pruned");
        self.project = Some(Arc::clone(project));
        // Commands retain their arrival time while due packets go first. A command can
        // therefore supersede a state published a little later during that drain.
        let keep = self.history.partition_point(|state| state.at < at);
        self.history.truncate(keep);
        let mut parameters: HashMap<ParameterRef, Arc<Driven>> = HashMap::new();
        for key in fields.keys() {
            Arc::make_mut(parameters.entry(key.parameter.clone()).or_insert_with(|| {
                let parameter = project
                    .parameter(&key.parameter)
                    .expect("indexed parameter");
                Arc::new(Driven {
                    fields: Vec::new(),
                    template: Arc::new(SendTemplate::new(&key.parameter, &parameter)),
                    base: parameter.fields.iter().map(|f| f.value).collect(),
                    positions: parameter
                        .fields
                        .iter()
                        .map(|f| {
                            let key = FieldRef {
                                parameter: key.parameter.clone(),
                                field: f.key.clone().into_owned(),
                            };
                            fields.contains_key(&key).then_some(key)
                        })
                        .collect(),
                })
            }))
            .fields
            .push(key.clone());
        }
        self.history.push(Arc::new(State {
            project: Arc::clone(project),
            at,
            clock: clock.clone(),
            fields,
            parameters,
        }));
        let largest = project
            .tracks
            .iter()
            .flat_map(|t| {
                project
                    .routed_outputs(t.id)
                    .map(move |o| project.send_delay_ms(t.id, o.id))
            })
            .fold(0.0, f64::max);
        self.history_delay = Duration::from_secs_f64(largest / 1000.0);
        self.prune(at);
    }

    fn prune(&mut self, at: Instant) {
        let oldest = at.checked_sub(self.history_delay).unwrap_or(at);
        let keep = self
            .history
            .partition_point(|s| s.at <= oldest)
            .saturating_sub(1);
        self.history.drain(..keep);
    }

    /// A value-only command retains every prepared driver, clock and conversion template.
    /// Histories share unchanged fields/parameters; only the named bases and overrides detach.
    pub fn update_parameters(&mut self, project: &Arc<Project>, at: Instant, manual: &[FieldRef]) {
        self.project = Some(Arc::clone(project));
        let Some(previous) = self.history.last() else {
            return;
        };
        let mut state = (**previous).clone();
        state.at = at;
        state.project = Arc::clone(project);
        for key in manual {
            if let Some(indexed) = state.parameters.get_mut(&key.parameter)
                && let Some(parameter) = project.parameter(&key.parameter)
                && let Some(position) = parameter.fields.iter().position(|f| *f.key == key.field)
            {
                Arc::make_mut(&mut Arc::make_mut(indexed).base)[position] =
                    parameter.fields[position].value;
            }
        }
        let keep = self.history.partition_point(|s| s.at < at);
        self.history.truncate(keep);
        self.history.push(Arc::new(state));
        self.control(at, manual, None);
        self.prune(at);
    }

    /// Explicit identities preserve manual resets whose stored base did not change.
    pub fn control(
        &mut self,
        at: Instant,
        manual: &[FieldRef],
        reenable: Option<&Option<FieldRef>>,
    ) {
        let Some(state) = self.history.last_mut() else {
            return;
        };
        let state = Arc::make_mut(state);
        let Some(ticks) = state.clock.fractional_ticks_at(at) else {
            return;
        };
        for key in manual {
            let Some(field) = state.fields.get_mut(key) else {
                continue;
            };
            let field = Arc::make_mut(field);
            let Some(base) = state.project.field(key) else {
                continue;
            };
            match field.value(ticks) {
                Some((_, AutomationState::Following | AutomationState::Overridden)) => {
                    field.overridden = Some(Override {
                        value: base.value,
                        state: AutomationState::Overridden,
                        voices: field
                            .drivers
                            .iter()
                            .filter(|d| d.launch.0 as f64 <= ticks)
                            .map(|d| d.voice)
                            .collect(),
                    });
                }
                Some((_, AutomationState::Held)) => {
                    field.held = Some(base.value);
                    field.overridden = Some(Override {
                        value: base.value,
                        state: AutomationState::Held,
                        voices: field
                            .drivers
                            .iter()
                            .filter(|d| d.launch.0 as f64 <= ticks)
                            .map(|d| d.voice)
                            .collect(),
                    });
                }
                _ => {}
            }
        }
        if let Some(only) = reenable {
            for (key, field) in &mut state.fields {
                if only.as_ref().is_none_or(|only| only == key)
                    && field
                        .overridden
                        .as_ref()
                        .is_some_and(|o| o.state == AutomationState::Overridden)
                {
                    Arc::make_mut(field).overridden = None;
                }
            }
        }
    }

    fn state(&self, at: Instant) -> Option<&State> {
        let index = self
            .history
            .partition_point(|s| s.at <= at)
            .checked_sub(1)?;
        Some(&self.history[index])
    }
    pub fn values(&self, at: Instant) -> Vec<AutomationValue> {
        let Some(state) = self.state(at) else {
            return vec![];
        };
        state
            .fields
            .keys()
            .filter_map(|field| {
                let (value, status) = state.value(field, at)?;
                Some(AutomationValue {
                    field: field.clone(),
                    value,
                    state: status,
                })
            })
            .collect()
    }

    pub fn follows(&self, field: &FieldRef, at: Instant, delay_ms: f64) -> bool {
        let Some(musical) = delay::shift(at, -delay_ms) else {
            return false;
        };
        let Some(state) = self.state(musical) else {
            return false;
        };
        state
            .value(field, musical)
            .is_some_and(|(_, status)| status != AutomationState::Overridden)
    }

    /// One complete message, through the same conversion boundary as manual parameter sends.
    pub fn sample(&self, target: &ParameterRef, at: Instant, delay_ms: f64) -> Option<SendValues> {
        let musical = delay::shift(at, -delay_ms)?;
        let state = self.state(musical)?;
        let indexed = state.parameters.get(target)?;
        let mut values = [0.0; FieldDef::MAX_ARGUMENTS];
        let mut driven = false;
        for (i, base) in indexed.base.iter().enumerate() {
            values[i] = *base;
            if let Some((value, _)) = indexed.positions[i]
                .as_ref()
                .and_then(|key| state.value(key, musical))
            {
                values[i] = indexed.template.checked(i, value)?;
                driven = true;
            }
        }
        if !driven {
            return None;
        }
        indexed
            .template
            .sample(values[..indexed.base.len()].iter().copied())
    }
}

#[cfg(test)]
mod tests;
mod wake;
