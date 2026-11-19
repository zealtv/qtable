//! Saved mappings consume every ordered envelope before reducing parameter values.
use crate::{
    Action, Consumer, Envelope, Gesture, Input, Received, keys,
    midi::decode,
    takeover::{self, HardwareHistory, PickUp, ValueScaling},
    translate::{self, Effect},
};
use qtable_engine::{
    edit::{ProjectEdit, TargetEdit},
    engine::{Command, Snapshot},
    model::*,
};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct MappingResolver {
    keys: keys::Decoder,
    history: HardwareHistory,
    rows: HashMap<MappingId, Row>,
    sources: HashMap<Source, Vec<usize>>,
    /// The scene row the host had selected when this drain began, as it last told us.
    scene: Option<usize>,
}

struct Row {
    definition: RemoteMapping,
    pickup: PickUp,
    scaling: ValueScaling,
    /// The signed part of a unit that a relative encoder has turned a stepped target (Int,
    /// Choice, Scene Select) but not yet moved it by, when its sensitivity is not a whole
    /// number. A row is dropped when its mapping's definition changes and the resolver is
    /// reset on project replacement, which clears it both times.
    carry: f64,
}

impl Row {
    /// The takeover acting on a row is fixed by its definition, so both record what was sent.
    fn sent(&mut self, value: f64) {
        self.pickup.sent(value);
        self.scaling.sent(value);
    }
}

impl MappingResolver {
    /// Hosts call this on project replacement, even when the new set reuses the same ids.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn reconcile(&mut self, project: &Project) {
        self.sources.clear();
        for (index, mapping) in project.mappings.iter().enumerate() {
            self.sources
                .entry(mapping.source.clone())
                .or_default()
                .push(index);
        }
        let definitions: HashMap<_, _> = project.mappings.iter().map(|row| (row.id, row)).collect();
        self.rows.retain(|id, row| {
            definitions
                .get(id)
                .is_some_and(|definition| **definition == row.definition)
        });
        let sources: HashSet<_> = self.sources.keys().collect();
        self.history.retain_sources(&sources);
    }
}

impl Consumer for MappingResolver {
    fn reset(&mut self) {
        MappingResolver::reset(self);
    }

    fn selected_scene(&mut self, row: Option<usize>) {
        self.scene = row;
    }

    fn receive(
        &mut self,
        events: &[Envelope],
        project: &Project,
        snapshot: &Snapshot,
        commands: &mut Vec<Received<Command>>,
    ) {
        let mut actions = Vec::new();
        self.receive_actions(events, project, snapshot, &mut actions);
        commands.extend(actions.into_iter().filter_map(|action| match action.value {
            Action::Command(command) => Some(Received {
                value: command,
                receipt: action.receipt,
                continuous: action.continuous,
            }),
            Action::View(_) => None,
        }));
    }

    fn receive_actions(
        &mut self,
        events: &[Envelope],
        project: &Project,
        snapshot: &Snapshot,
        commands: &mut Vec<Received<Action>>,
    ) {
        self.reconcile(project);
        // Values are deliberately local to one drain. The next drain starts from the
        // accepted engine state, including external edits and rejected remote commands.
        let mut working = HashMap::new();
        let mut segment = Segment::default();
        // Likewise the selected scene: the host applies this drain's moves afterwards, so the
        // resolver follows them itself to know where each encoder click lands.
        let mut scene = self.scene;
        for event in events {
            let (source, midi, key) = match &event.input {
                Input::Midi(bytes) => {
                    let Some(message) = decode::decode(bytes) else {
                        continue;
                    };
                    (Source::Midi(message.source), Some(message), None)
                }
                Input::Key {
                    key,
                    pressed,
                    repeat,
                } => {
                    let source = Source::Key(KeySource {
                        key: KeyName(key.clone()),
                    });
                    if !self.sources.contains_key(&source) {
                        if !pressed {
                            self.keys.decode(key, false, *repeat);
                        }
                        continue;
                    }
                    let Some(gesture) = self.keys.decode(key, *pressed, *repeat) else {
                        continue;
                    };
                    (source, None, Some(gesture))
                }
            };
            let Some(indices) = self.sources.get(&source) else {
                continue;
            };
            let previous = self.history.previous(&event.endpoint, &source);
            for &index in indices {
                let mapping = &project.mappings[index];
                let Some(descriptor) = describe_value(project, &mapping.control) else {
                    continue;
                };
                let current = working
                    .get(&mapping.control)
                    .copied()
                    .or_else(|| current_value(snapshot, &mapping.control, &descriptor));
                let mut gesture =
                    midi.map_or_else(|| key.unwrap(), |message| message.gesture(mapping.mode));
                let row = self.rows.entry(mapping.id).or_insert_with(|| Row {
                    definition: mapping.clone(),
                    pickup: PickUp::default(),
                    scaling: ValueScaling::default(),
                    carry: 0.0,
                });
                if let (Gesture::Absolute(hardware), Some(current)) = (gesture, current) {
                    match takeover::active(mapping, &descriptor) {
                        Takeover::None => {}
                        Takeover::PickUp => {
                            if !row.pickup.allows(mapping, current, previous, hardware) {
                                continue;
                            }
                        }
                        // The scaled travel takes the hardware's place: the ordinary absolute
                        // path then applies Min, Max and Invert.
                        Takeover::ValueScaling => {
                            let Some(travel) =
                                row.scaling.travel(mapping, current, previous, hardware)
                            else {
                                continue;
                            };
                            gesture = Gesture::Absolute(travel);
                        }
                    }
                }
                let Some(effect) = translate::translate(
                    project,
                    mapping,
                    &descriptor,
                    gesture,
                    current,
                    previous,
                    translate::Stepping {
                        carry: &mut row.carry,
                        scene,
                    },
                ) else {
                    continue;
                };
                // A number moved by a CC or pitch bend is an ongoing action, and its moves join
                // one undo step (Live check 2026-11-11). Switches, choices, notes and keys
                // are discrete: no token, so each makes its own step.
                let continuous = (matches!(gesture, Gesture::Absolute(_) | Gesture::Steps(_))
                    && matches!(descriptor.kind, ControlKind::Float | ControlKind::Int)
                    && matches!(source, Source::Midi(_)))
                .then(|| source.clone());
                match effect {
                    Effect::Parameter(field, value) => {
                        working.insert(mapping.control.clone(), value);
                        row.sent(value);
                        segment.set(field, value, continuous);
                    }
                    Effect::EventArgument {
                        device,
                        entry,
                        field,
                        value,
                        argument,
                    } => {
                        // Working value only: the engine keeps the face's value, and this
                        // drain's later moves start from what the edit will store.
                        working.insert(mapping.control.clone(), value);
                        row.sent(value);
                        segment.set_event_field(
                            mapping.control.clone(),
                            ProjectEdit::EditTarget {
                                device,
                                edit: TargetEdit::SetFieldValue {
                                    entry,
                                    field,
                                    value: argument,
                                },
                            },
                            continuous,
                        );
                    }
                    Effect::View(action) => {
                        scene = action.scene_after(scene, project.scenes.len()).or(scene);
                        segment.flush(commands);
                        commands.push(Received::from_event(Action::View(action), event));
                    }
                    Effect::Command(command, value) => {
                        if let Some(value) = value {
                            working.insert(mapping.control.clone(), value);
                            row.sent(value);
                        }
                        // Switch/tempo edits, launches, stops, bangs and re-enable all
                        // observe preceding parameter writes, in mapping-list order.
                        segment.flush(commands);
                        if matches!(command, Command::ReEnableAutomation { .. }) {
                            working
                                .retain(|control, _| !matches!(control, ControlRef::Argument(_)));
                            for row in self.rows.values_mut() {
                                row.pickup = PickUp::default();
                                row.scaling = ValueScaling::default();
                            }
                        }
                        commands.push(
                            Received::from_event(Action::Command(command), event)
                                .continuous(continuous),
                        );
                    }
                }
            }
            // Every matching row must see the SAME preceding physical sample.
            if let Some(message) = midi
                && let Gesture::Absolute(value) = message.gesture(SourceMode::Absolute)
            {
                self.history.record(&event.endpoint, &source, value);
            }
        }
        segment.flush(commands);
    }
}

fn current_value(
    snapshot: &Snapshot,
    control: &ControlRef,
    descriptor: &ControlValue,
) -> Option<f64> {
    if let ControlRef::Argument(field) = control
        && let Some(value) = snapshot
            .automation
            .iter()
            .find(|value| &value.field == field)
    {
        return Some(value.value);
    }
    descriptor.value
}

#[derive(Default)]
struct Segment {
    values: Vec<(FieldRef, f64, Option<Source>)>,
    positions: HashMap<FieldRef, usize>,
    /// Event argument edits, one per mapped control: a later value replaces the earlier.
    event_fields: Vec<(ControlRef, ProjectEdit, Option<Source>)>,
}

impl Segment {
    /// A later value of the same argument replaces the earlier one, and with it the source
    /// that moved it.
    fn set(&mut self, field: FieldRef, value: f64, continuous: Option<Source>) {
        if let Some(index) = self.positions.get(&field) {
            self.values[*index].1 = value;
            self.values[*index].2 = continuous;
        } else {
            self.positions.insert(field.clone(), self.values.len());
            self.values.push((field, value, continuous));
        }
    }

    /// As `set`, for an event argument: the last edit per control survives, in the place of
    /// the first.
    fn set_event_field(&mut self, control: ControlRef, edit: ProjectEdit, source: Option<Source>) {
        match self.event_fields.iter_mut().find(|(c, ..)| *c == control) {
            Some(slot) => *slot = (control, edit, source),
            None => self.event_fields.push((control, edit, source)),
        }
    }

    fn flush(&mut self, commands: &mut Vec<Received<Action>>) {
        for (_, edit, source) in self.event_fields.drain(..) {
            commands
                .push(Received::unstamped(Action::Command(Command::Edit(edit))).continuous(source));
        }
        if self.values.is_empty() {
            return;
        }
        // One batch per controller source, in the order each first appeared: a source's moves
        // share an undo gesture, and another source's must not ride on it. Several arguments
        // fanned out from one source stay one batch.
        type Batch = (Option<Source>, Vec<(ParameterRef, Vec<(FieldKey, f64)>)>);
        let mut batches: Vec<Batch> = Vec::new();
        for (field, value, continuous) in self.values.drain(..) {
            let batch = match batches.iter().position(|(source, _)| *source == continuous) {
                Some(index) => index,
                None => {
                    batches.push((continuous, Vec::new()));
                    batches.len() - 1
                }
            };
            let parameters = &mut batches[batch].1;
            match parameters
                .iter()
                .position(|(parameter, _)| *parameter == field.parameter)
            {
                Some(index) => parameters[index].1.push((field.field, value)),
                None => parameters.push((field.parameter, vec![(field.field, value)])),
            }
        }
        self.positions.clear();
        for (source, parameters) in batches {
            commands.push(
                Received::unstamped(Action::Command(Command::Edit(ProjectEdit::SetParameters(
                    parameters,
                ))))
                .continuous(source),
            );
        }
    }
}

#[cfg(test)]
mod tests;
