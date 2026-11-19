//! A pure description of one control: what it is, its domain and what it holds now.
use super::*;

/// The domain of a control's value, in control units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Float,
    Int,
    /// 0 or 1.
    Toggle,
    /// The choice's index.
    Enum,
    /// A string argument; no numeric control value.
    Text,
    /// An event's Bang: sends the event once with its field defaults.
    Bang,
    /// A launch, stop or other momentary command with no value.
    Trigger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlAccess {
    ReadWrite,
    /// It can be fired but holds no value to read.
    WriteOnly,
}

/// Everything a mapping list, the Info View or a remote-control type check needs, so none
/// of them re-derives a range or a label.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlDescriptor {
    /// The control's own name: "Volume", "Cutoff", "Solo", "Scene 2".
    pub label: String,
    /// Where it sits: the track (and device) it belongs to, or "Transport".
    pub path: String,
    pub kind: ControlKind,
    /// `[min, max]` in control units. `None` where the model sets no range (Tempo) and for
    /// kinds without a numeric value.
    pub range: Option<[f64; 2]>,
    /// An Enum's choice labels, in index order; empty otherwise.
    pub choices: Vec<String>,
    pub access: ControlAccess,
    /// The value now, in control units; `None` for a WriteOnly or Text control.
    pub value: Option<f64>,
}

/// Label-free facts for gesture resolution. Descriptor paths, labels and Enum choice
/// names belong to `describe_control`; both paths share this domain/value authority.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlValue {
    pub kind: ControlKind,
    pub range: Option<[f64; 2]>,
    pub access: ControlAccess,
    pub value: Option<f64>,
}

impl From<&ControlDescriptor> for ControlValue {
    fn from(descriptor: &ControlDescriptor) -> Self {
        Self {
            kind: descriptor.kind,
            range: descriptor.range,
            access: descriptor.access,
            value: descriptor.value,
        }
    }
}

impl ControlValue {
    fn trigger() -> Self {
        Self {
            kind: ControlKind::Trigger,
            range: None,
            access: ControlAccess::WriteOnly,
            value: None,
        }
    }

    fn toggle(on: bool) -> Self {
        Self {
            kind: ControlKind::Toggle,
            range: Some([0.0, 1.0]),
            access: ControlAccess::ReadWrite,
            value: Some(f64::from(on)),
        }
    }
}

/// The control's domain and current value, without constructing a display descriptor.
/// Missing ids and controls unavailable on a track kind return `None` on both paths.
pub fn describe_value(project: &Project, control: &ControlRef) -> Option<ControlValue> {
    Some(match control {
        ControlRef::Argument(field) => argument_value(&project.field(field)?),
        ControlRef::Event { track, event } => {
            project.track(*track)?.event_def(event)?;
            ControlValue {
                kind: ControlKind::Bang,
                ..ControlValue::trigger()
            }
        }
        ControlRef::EventArgument {
            track,
            event,
            field,
        } => {
            let (_, def) = project.track(*track)?.event_def(event)?;
            event_argument_value(def.fields.iter().find(|f| f.key == *field)?)
        }
        ControlRef::ClipSlot { track, scene } => {
            let row = project.scenes.iter().position(|s| s.id == *scene)?;
            project.track(*track)?.slots.get(row)?;
            ControlValue::trigger()
        }
        ControlRef::Scene(scene) => {
            project.scenes.iter().find(|s| s.id == *scene)?;
            ControlValue::trigger()
        }
        ControlRef::TrackSelect(track) => {
            project.track(*track)?;
            ControlValue::trigger()
        }
        ControlRef::SceneNavigation(SceneNavigation::Select) => ControlValue {
            kind: ControlKind::Int,
            // Always every scene at the time of the input, so there is no range to store.
            range: None,
            access: ControlAccess::ReadWrite,
            // Selection belongs to the host's View, never the saved Project.
            value: None,
        },
        ControlRef::SceneNavigation(_) => ControlValue::trigger(),
        ControlRef::TrackStop(track) => {
            let caps = project.track(*track)?.capabilities();
            (caps.holds_clips || caps.foldable).then_some(())?;
            ControlValue::trigger()
        }
        ControlRef::TrackActivator(track) => {
            let owner = project.track(*track)?;
            owner.capabilities().switched.then_some(())?;
            ControlValue::toggle(owner.mixer.active)
        }
        ControlRef::TrackSolo(track) => {
            let owner = project.track(*track)?;
            owner.capabilities().switched.then_some(())?;
            ControlValue::toggle(owner.mixer.solo)
        }
        ControlRef::Transport(control) => transport_value(project, *control),
    })
}

/// The descriptor of `control` in `project`, or `None` when the ids no longer resolve
/// (the track, scene, device or entry was deleted) or the track kind lacks the control.
pub fn describe_control(project: &Project, control: &ControlRef) -> Option<ControlDescriptor> {
    let facts = describe_value(project, control)?;
    let mut choices = Vec::new();
    let (path, label) = match control {
        ControlRef::Argument(field) => {
            let owner = project.track(field.parameter.track)?;
            let view = project.field(field)?;
            let path = match field.parameter.device {
                DeviceRef::Mixer => format!("{} / Mixer", owner.name),
                DeviceRef::Chain(id) => format!("{} / {}", owner.name, owner.device(id)?.name),
            };
            if facts.kind == ControlKind::Enum {
                choices = view.options.iter().map(|o| o.label.clone()).collect();
            }
            // A parameter with several arguments names each one after both, as an event's
            // argument reads ("Position x"); a one-argument parameter keeps its own name.
            let parameter = match field.parameter.device {
                DeviceRef::Chain(id) => owner
                    .device(id)
                    .and_then(Device::target)
                    .and_then(|target| target.parameter(&field.parameter.key)),
                DeviceRef::Mixer => None,
            };
            let label = match parameter {
                Some(parameter) if parameter.fields.len() > 1 => {
                    format!("{} {}", parameter.display_name(), view.spec.name)
                }
                _ => view.spec.name.to_string(),
            };
            (path, label)
        }
        ControlRef::Event { track, event } => {
            let owner = project.track(*track)?;
            let (device, def) = owner.event_def(event)?;
            (
                format!("{} / {}", owner.name, device.name),
                def.display_name().into(),
            )
        }
        ControlRef::EventArgument {
            track,
            event,
            field,
        } => {
            let owner = project.track(*track)?;
            let (device, def) = owner.event_def(event)?;
            let position = def.fields.iter().position(|f| f.key == *field)?;
            let argument = &def.fields[position];
            if facts.kind == ControlKind::Enum {
                choices = argument.options.iter().map(|o| o.label.clone()).collect();
            }
            (
                format!("{} / {}", owner.name, device.name),
                format!("{} {}", def.display_name(), argument.display_name(position)),
            )
        }
        ControlRef::ClipSlot { track, scene } => {
            let owner = project.track(*track)?;
            let row = project.scenes.iter().position(|s| s.id == *scene)?;
            (owner.name.clone(), format!("Slot {}", row + 1))
        }
        ControlRef::Scene(scene) => {
            let row = project.scenes.iter().position(|s| s.id == *scene)?;
            let name = &project.scenes[row].name;
            let label = if name.is_empty() {
                format!("Scene {}", row + 1)
            } else {
                name.clone()
            };
            ("Scenes".into(), label)
        }
        ControlRef::TrackSelect(track) => {
            (project.track(*track)?.name.clone(), "Select Track".into())
        }
        ControlRef::SceneNavigation(control) => ("Scenes".into(), control.label().into()),
        ControlRef::TrackStop(track) => (project.track(*track)?.name.clone(), "Stop".into()),
        ControlRef::TrackActivator(track) => {
            (project.track(*track)?.name.clone(), "Activator".into())
        }
        ControlRef::TrackSolo(track) => (project.track(*track)?.name.clone(), "Solo".into()),
        ControlRef::Transport(control) => {
            let label = match control {
                TransportControl::Play => "Play",
                TransportControl::Stop => "Stop",
                TransportControl::StopAll => "Stop All Clips",
                TransportControl::ReEnableAutomation => "Re-Enable Automation",
                TransportControl::Tempo => "Tempo",
                TransportControl::Quantisation => {
                    choices = GlobalQuantisation::CHOICES
                        .iter()
                        .map(|q| q.label())
                        .collect();
                    "Quantisation"
                }
                TransportControl::FollowActions => "Follow Actions",
            };
            ("Transport".into(), label.into())
        }
    };
    Some(ControlDescriptor {
        label,
        path,
        choices,
        kind: facts.kind,
        range: facts.range,
        access: facts.access,
        value: facts.value,
    })
}

fn transport_value(project: &Project, control: TransportControl) -> ControlValue {
    match control {
        TransportControl::Play
        | TransportControl::Stop
        | TransportControl::StopAll
        | TransportControl::ReEnableAutomation => ControlValue::trigger(),
        TransportControl::Tempo => ControlValue {
            kind: ControlKind::Float,
            access: ControlAccess::ReadWrite,
            value: Some(project.tempo.bpm()),
            ..ControlValue::trigger()
        },
        TransportControl::Quantisation => ControlValue {
            kind: ControlKind::Enum,
            range: Some([0.0, (GlobalQuantisation::CHOICES.len() - 1) as f64]),
            access: ControlAccess::ReadWrite,
            value: GlobalQuantisation::CHOICES
                .iter()
                .position(|q| *q == project.global_quantisation)
                .map(|index| index as f64),
        },
        TransportControl::FollowActions => ControlValue::toggle(project.follow_actions_enabled),
    }
}

fn argument_value(view: &FieldView<'_>) -> ControlValue {
    let kind = match (view.control, view.value_type) {
        (Control::Toggle, _) | (_, FieldType::Bool) => ControlKind::Toggle,
        (Control::Enum, _) => ControlKind::Enum,
        (Control::Text, _) | (_, FieldType::String) => ControlKind::Text,
        (_, FieldType::Int) => ControlKind::Int,
        (_, FieldType::Float) => ControlKind::Float,
    };
    let numeric = kind != ControlKind::Text;
    ControlValue {
        kind,
        range: numeric.then_some([view.spec.min, view.spec.max]),
        access: ControlAccess::ReadWrite,
        value: numeric.then_some(view.value),
    }
}

/// An event argument's domain, from its definition: a continuous Float or Int has the range the
/// argument sets (`None` when it sets none), a Toggle 0–1 and an Enum its choices' indexes. A
/// Text argument holds no numeric value.
fn event_argument_value(field: &FieldDef) -> ControlValue {
    let (kind, range) = match (field.control, field.value_type) {
        (Control::Toggle, _) | (_, FieldType::Bool) => (ControlKind::Toggle, Some([0.0, 1.0])),
        (Control::Enum, _) => (
            ControlKind::Enum,
            Some([0.0, field.options.len().saturating_sub(1) as f64]),
        ),
        (Control::Text, _) | (_, FieldType::String) => (ControlKind::Text, None),
        (_, FieldType::Int) => (ControlKind::Int, field.range),
        (_, FieldType::Float) => (ControlKind::Float, field.range),
    };
    ControlValue {
        kind,
        range,
        access: ControlAccess::ReadWrite,
        // A field the face has not set yet holds its default, as a Bang would send it.
        value: (kind != ControlKind::Text)
            .then(|| field.value.unwrap_or_else(|| field.default_value())),
    }
}
