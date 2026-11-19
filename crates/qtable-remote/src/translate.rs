//! The one control-to-command translation. Domain/range facts come from the model.
use crate::{Gesture, ViewAction};
use qtable_engine::{edit::ProjectEdit, engine::Command, model::*};

// One transient result on the owner stack; boxing would allocate for every bang.
#[allow(clippy::large_enum_variant)]
pub enum Effect {
    Parameter(FieldRef, f64),
    /// An event argument's new value: the face edit, with no OSC. `value` is the number the
    /// controller moved it to, after snapping, and `argument` the OSC value that number stores.
    EventArgument {
        device: DeviceId,
        entry: EntryKey,
        field: FieldKey,
        value: f64,
        argument: OscArgument,
    },
    View(ViewAction),
    Command(Command, Option<f64>),
}

/// What a relative move of a stepped target needs beyond the control: the mapping's `carry` (its
/// resolver's remainder, which only such a move reads or changes) and the host's selected scene row.
pub struct Stepping<'a> {
    pub carry: &'a mut f64,
    pub scene: Option<usize>,
}

pub fn translate(
    project: &Project,
    mapping: &RemoteMapping,
    descriptor: &ControlValue,
    gesture: Gesture,
    current: Option<f64>,
    previous: Option<f64>,
    stepping: Stepping,
) -> Option<Effect> {
    let effect = effect(
        project, mapping, descriptor, gesture, current, previous, stepping,
    )?;
    // Gate sets a level rather than flipping one, so a repeated note-on or a stray note-off
    // finds the control already there and sends nothing: no empty undo step.
    if is_gated(mapping, descriptor, gesture)
        && let Some(current) = current
        && effect.value() == Some(current)
    {
        return None;
    }
    Some(effect)
}

impl Effect {
    /// The value this effect sets, for the effects that set one.
    fn value(&self) -> Option<f64> {
        match self {
            Self::Parameter(_, value) | Self::EventArgument { value, .. } => Some(*value),
            Self::Command(_, value) => *value,
            Self::View(_) => None,
        }
    }
}

/// Whether Gate governs this message: a Gate mapping on a note or key, where the target holds a
/// value. A trigger holds none, so Gate there behaves as Latch (the inspector does not offer it).
fn is_gated(mapping: &RemoteMapping, descriptor: &ControlValue, gesture: Gesture) -> bool {
    mapping.mode == SourceMode::Gate
        && matches!(gesture, Gesture::Press | Gesture::Release)
        && !matches!(
            descriptor.kind,
            ControlKind::Text | ControlKind::Bang | ControlKind::Trigger
        )
        && mapping.control != ControlRef::SceneNavigation(SceneNavigation::Select)
}

fn effect(
    project: &Project,
    mapping: &RemoteMapping,
    descriptor: &ControlValue,
    gesture: Gesture,
    current: Option<f64>,
    previous: Option<f64>,
    Stepping { carry, scene }: Stepping,
) -> Option<Effect> {
    // Scene selection lives in Song.View: express relative moves rather than inventing
    // a current selection in the Project or scheduler snapshot. The host applies these
    // in order, so Up/Down then Launch in one input drain sees the new highlight.
    // Scene Select spans every scene at the time of the input (2026-11-04), so its
    // stored Min and Max play no part: adding or deleting scenes needs no mapping edit.
    if mapping.control == ControlRef::SceneNavigation(SceneNavigation::Select) {
        let count = project.scenes.len();
        let action = match gesture {
            // A note or key cycles through the scenes and wraps.
            Gesture::Press => ViewAction::NextScene,
            Gesture::Steps(0) | Gesture::Release => return None,
            // An encoder steps and stops at the ends. Its sensitivity scales the steps, and
            // the part of a scene that does not yet make one waits in `carry`. The host says
            // which scene is selected (`scene`), so a spin past either end owes nothing, as
            // it does for an Int or a Choice.
            Gesture::Steps(steps) => {
                let whole = whole_steps(steps, mapping.sensitivity, carry);
                if let Some(row) = scene.filter(|&row| row < count) {
                    let last = count - 1;
                    let target = i64::try_from(row).unwrap_or(i64::MAX) + i64::from(whole);
                    let cut_short = whole != 0 && !(0..=last as i64).contains(&target);
                    let stuck =
                        whole == 0 && ((row == 0 && *carry < 0.0) || (row == last && *carry > 0.0));
                    if cut_short || stuck {
                        *carry = 0.0;
                    }
                }
                match whole {
                    0 => return None,
                    whole => ViewAction::MoveScene(whole),
                }
            }
            // An absolute CC spreads its whole range over the scenes in equal bands, the
            // top endpoint still selecting the last, as an Enum's choices do.
            Gesture::Absolute(fraction) if count > 0 => ViewAction::SelectScene(
                ((fraction.clamp(0.0, 1.0) * count as f64).floor() as usize).min(count - 1),
            ),
            Gesture::Absolute(_) => return None,
        };
        return Some(Effect::View(action));
    }
    let value = match descriptor.kind {
        ControlKind::Text => return None,
        ControlKind::Bang | ControlKind::Trigger => {
            let fire = match gesture {
                Gesture::Press => true,
                Gesture::Absolute(value) => {
                    let needs_crossing = descriptor.kind == ControlKind::Bang
                        || matches!(
                            mapping.control,
                            ControlRef::ClipSlot { .. }
                                | ControlRef::TrackSelect(_)
                                | ControlRef::SceneNavigation(_)
                        );
                    value >= 0.5 && (!needs_crossing || previous.is_none_or(|v| v < 0.5))
                }
                Gesture::Steps(steps) => steps > 0,
                Gesture::Release => false,
            };
            if !fire {
                return None;
            }
            None
        }
        _ => Some(numeric(mapping, descriptor, gesture, current?, carry)?),
    };
    Some(match &mapping.control {
        ControlRef::Argument(field) => {
            // Match the engine's snapping now so the next relative move starts at the
            // value it will actually accept, including multiple moves in one drain.
            let value = project.field(field)?.spec.check(value?).ok()?;
            Effect::Parameter(field.clone(), value)
        }
        ControlRef::Event { track, event } => Effect::Command(
            Command::FireEvent {
                track: *track,
                event: event.clone(),
            },
            None,
        ),
        // A mapped argument sets the value the event's face holds and sends nothing: OSC
        // goes out only when the event's Bang fires.
        ControlRef::EventArgument {
            track,
            event,
            field,
        } => {
            let (_, definition) = project.track(*track)?.event_def(event)?;
            let definition = definition.fields.iter().find(|f| f.key == *field)?;
            // Match the face's snapping now, so the next relative move starts at the value
            // the edit will store, including several moves in one drain. A Float argument is
            // stored as an f32, so take that rounding too: Pick-Up, Value Scaling and Gate
            // compare against what the engine reports next, not the f64 sent.
            let spec = definition.spec();
            let mut value = spec.check(value?).ok()?;
            if definition.control != Control::Enum && definition.value_type == FieldType::Float {
                // A range end with no exact f32 (Max 0.1) rounds to a number just outside the
                // range, which the engine refuses; step to the nearest f32 inside instead.
                let mut stored = value as f32;
                if f64::from(stored) > spec.max {
                    stored = stored.next_down();
                } else if f64::from(stored) < spec.min {
                    stored = stored.next_up();
                }
                value = spec.check(f64::from(stored)).ok()?;
            }
            Effect::EventArgument {
                device: event.device,
                entry: event.key.clone(),
                field: field.clone(),
                value,
                argument: definition.argument_at(value),
            }
        }
        ControlRef::ClipSlot { track, scene } => {
            let slot = project.scenes.iter().position(|row| row.id == *scene)?;
            let command = if project.track(*track)?.is_group() {
                Command::LaunchGroupSlot {
                    group: *track,
                    slot,
                }
            } else {
                Command::LaunchClip {
                    track: *track,
                    slot,
                }
            };
            Effect::Command(command, None)
        }
        ControlRef::Scene(scene) => Effect::Command(Command::LaunchScene { scene: *scene }, None),
        ControlRef::TrackSelect(track) => Effect::View(ViewAction::SelectTrack(*track)),
        ControlRef::SceneNavigation(control) => match control {
            SceneNavigation::Launch => Effect::View(ViewAction::LaunchScene),
            SceneNavigation::CancelLaunch => Effect::Command(Command::CancelSceneLaunch, None),
            SceneNavigation::Up => Effect::View(ViewAction::MoveScene(-1)),
            SceneNavigation::Down => Effect::View(ViewAction::MoveScene(1)),
            SceneNavigation::Select => unreachable!("selector handled before value resolution"),
        },
        ControlRef::TrackStop(track) => Effect::Command(
            if project.track(*track)?.is_group() {
                Command::StopGroup { group: *track }
            } else {
                Command::StopTrack { track: *track }
            },
            None,
        ),
        ControlRef::TrackActivator(track) => Effect::Command(
            Command::Edit(ProjectEdit::SetActive {
                strips: vec![*track],
                active: value? != 0.0,
            }),
            value,
        ),
        ControlRef::TrackSolo(track) => Effect::Command(
            Command::Edit(ProjectEdit::SetSolo {
                strips: vec![*track],
                solo: value? != 0.0,
                exclusive: false,
            }),
            value,
        ),
        ControlRef::Transport(transport) => {
            let command = match transport {
                TransportControl::Play => Command::Play,
                TransportControl::Stop => Command::Stop,
                TransportControl::StopAll => Command::StopAll,
                TransportControl::ReEnableAutomation => Command::ReEnableAutomation { field: None },
                TransportControl::Tempo => Command::SetTempo(value?),
                TransportControl::Quantisation => Command::SetGlobalQuantisation(
                    *GlobalQuantisation::CHOICES.get(value? as usize)?,
                ),
                TransportControl::FollowActions => Command::SetFollowActionsEnabled(value? != 0.0),
            };
            Effect::Command(command, value)
        }
    })
}

/// The units a relative message moves a stepped target (Int, Choice, Scene Select): its decoded
/// steps times the mapping's sensitivity, plus what earlier messages left over. Whole units are
/// returned and the signed fraction stays in `carry`, so at 25 % four detents make one unit.
/// At 100 % the steps come back as they went in and `carry` stays 0.
fn whole_steps(steps: i32, sensitivity: f64, carry: &mut f64) -> i32 {
    let scaled = f64::from(steps) * sensitivity + *carry;
    let whole = scaled.trunc();
    *carry = scaled - whole;
    whole as i32
}

/// `carry` is the resolver's remainder for this mapping; only a relative move of a stepped
/// target reads or changes it.
fn numeric(
    mapping: &RemoteMapping,
    descriptor: &ControlValue,
    gesture: Gesture,
    current: f64,
    carry: &mut f64,
) -> Option<f64> {
    let min = mapping.min;
    let max = mapping.max;
    let gated = is_gated(mapping, descriptor, gesture);
    let down = gesture == Gesture::Press;
    let value = match (descriptor.kind, gesture) {
        // Gate (design 4.1): down goes to Max or On, up to Min or Off. Invert is Min above Max;
        // a switch reads only the order of the two ends, so Invert swaps On and Off.
        (ControlKind::Toggle, _) if gated => f64::from(down != (min > max)),
        (ControlKind::Enum, _) if gated => {
            // The choice at the end: inside a fractional range, the nearest whole choice.
            let (low, high) = (min.min(max).ceil(), min.max(max).floor());
            let end = if down { max } else { min };
            if low > high {
                end.round()
            } else {
                end.clamp(low, high)
            }
        }
        (_, _) if gated => {
            if down {
                max
            } else {
                min
            }
        }
        (_, Gesture::Release | Gesture::Steps(0)) => return None,
        (ControlKind::Toggle, Gesture::Press) => f64::from(current == 0.0),
        (ControlKind::Toggle, Gesture::Steps(steps)) => f64::from(steps > 0),
        (ControlKind::Toggle, Gesture::Absolute(value)) => {
            if matches!(mapping.control, ControlRef::TrackActivator(_)) {
                // Switch windows use normalized hardware units (the descriptor's 0..1).
                f64::from((min.min(max)..=min.max(max)).contains(&value))
            } else {
                f64::from(scale(value, min, max)? >= 0.5)
            }
        }
        (ControlKind::Enum, Gesture::Press) => {
            if current >= min.max(max) {
                min.min(max)
            } else {
                current + 1.0
            }
        }
        (ControlKind::Enum, Gesture::Absolute(fraction)) => {
            // Equal-width choice bins, with the top endpoint still selecting the last.
            let low = min.min(max).ceil();
            let high = min.max(max).floor();
            let fraction = if min > max { 1.0 - fraction } else { fraction };
            if low > high {
                scale(fraction, min, max)?.round()
            } else {
                scale(fraction, low, high + 1.0)?.floor().clamp(low, high)
            }
        }
        (_, Gesture::Press) => {
            if current == max {
                min
            } else {
                max
            }
        }
        (_, Gesture::Absolute(value)) => scale(value, min, max)?,
        // Sensitivity multiplies the steps after the encoding's own curve. A Float moves a share
        // of the span per step; at 100 % that is the 1/256 Live measured.
        (ControlKind::Float, Gesture::Steps(steps)) => {
            current + f64::from(steps) * mapping.sensitivity * (max - min) / 256.0
        }
        (_, Gesture::Steps(steps)) => {
            let direction = if min > max { -1.0 } else { 1.0 };
            let whole = whole_steps(steps, mapping.sensitivity, carry);
            if whole == 0 {
                // A fraction owed towards an end the value already sits on can never be paid:
                // drop it, so a spin past the end leaves no debt for the reversal to work off.
                // A value outside Min/Max does not sit on an end: its carry builds up and the
                // whole unit then pulls it in.
                let owed = direction * carry.signum();
                if *carry != 0.0 && fit(current + owed, mapping, descriptor) == current {
                    *carry = 0.0;
                }
                return None;
            }
            let moved = current + f64::from(whole) * direction;
            // The clamp changed what the click asked for: the end absorbed the rest it owed.
            if fit(moved, mapping, descriptor) != moved {
                *carry = 0.0;
            }
            moved
        }
    };
    if !value.is_finite() {
        return None;
    }
    // Switch windows are input ranges; relative/note switch policy always yields 0/1.
    Some(if descriptor.kind == ControlKind::Toggle {
        value
    } else {
        fit(value, mapping, descriptor)
    })
}

/// A value held to what the control can store: inside the mapping's Min and Max and the
/// descriptor's own range, and whole for an Int or a Choice.
fn fit(value: f64, mapping: &RemoteMapping, descriptor: &ControlValue) -> f64 {
    let mut value = value.clamp(mapping.min.min(mapping.max), mapping.min.max(mapping.max));
    if let Some([low, high]) = descriptor.range {
        value = value.clamp(low, high);
    }
    if matches!(descriptor.kind, ControlKind::Int | ControlKind::Enum) {
        value = value.round();
    }
    value
}

pub(crate) fn scale(value: f64, min: f64, max: f64) -> Option<f64> {
    Mapping::new([0.0, 1.0], [min, max], 1.0, 0.0)
        .ok()?
        .map(value)
        .ok()
}

#[cfg(test)]
mod tests;
