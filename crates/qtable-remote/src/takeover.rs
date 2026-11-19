//! Caught state belongs to a mapping, hardware history to (endpoint, source).
use crate::{Endpoint, translate::scale};
use qtable_engine::model::{
    ControlKind, ControlValue, MidiMessage, MidiSource, RemoteMapping, Source, Takeover,
};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Reconnected/removed devices must not grow the hub indefinitely. Eviction simply
/// loses a crossing opportunity; it cannot invent one. Current source state is newest.
const HISTORY_LIMIT: usize = 2048;

#[derive(Default)]
pub(crate) struct HardwareHistory {
    values: HashMap<Endpoint, HashMap<Source, (f64, u64)>>,
    // Update order is separate from value lookup. Moving an existing entry reuses its
    // owned keys; neither lookup nor refresh scans/clones the endpoint strings.
    order: BTreeMap<u64, (Endpoint, Source)>,
    next: u64,
}

impl HardwareHistory {
    pub fn previous(&self, endpoint: &Endpoint, source: &Source) -> Option<f64> {
        self.values
            .get(endpoint)?
            .get(source)
            .map(|(value, _)| *value)
    }

    pub fn record(&mut self, endpoint: &Endpoint, source: &Source, value: f64) {
        let key = if let Some(sample) = self
            .values
            .get_mut(endpoint)
            .and_then(|values| values.get_mut(source))
        {
            let key = self.order.remove(&sample.1).unwrap();
            *sample = (value, self.next);
            key
        } else {
            if self.order.len() == HISTORY_LIMIT {
                let (_, (port, source)) = self.order.pop_first().unwrap();
                let values = self.values.get_mut(&port).unwrap();
                values.remove(&source);
                if values.is_empty() {
                    self.values.remove(&port);
                }
            }
            self.values
                .entry(endpoint.clone())
                .or_default()
                .insert(source.clone(), (value, self.next));
            (endpoint.clone(), source.clone())
        };
        self.order.insert(self.next, key);
        self.next += 1;
    }

    pub fn retain_sources(&mut self, sources: &HashSet<&Source>) {
        self.values.retain(|_, values| {
            values.retain(|source, _| sources.contains(source));
            !values.is_empty()
        });
        self.order.retain(|_, (_, source)| sources.contains(source));
    }
}

#[derive(Default)]
pub(crate) struct PickUp {
    last_sent: Option<f64>,
    caught: bool,
}

impl PickUp {
    pub fn allows(
        &mut self,
        mapping: &RemoteMapping,
        current: f64,
        previous: Option<f64>,
        hardware: f64,
    ) -> bool {
        // Includes external edits, automation and command rejection: no speculative
        // working value survives into a subsequent owner drain.
        if self.last_sent != Some(current) {
            self.caught = false;
        }
        let Some(value) = scale(hardware, mapping.min, mapping.max) else {
            return false;
        };
        self.caught |= value == current
            || previous
                .and_then(|previous| scale(previous, mapping.min, mapping.max))
                .is_some_and(|previous| {
                    (previous.min(value)..=previous.max(value)).contains(&current)
                });
        self.caught
    }

    pub fn sent(&mut self, value: f64) {
        self.last_sent = Some(value);
    }
}

/// The takeover that acts on a message to `mapping`. A stored takeover acts only where the
/// mapping's source has one (an absolute CC or pitch bend) and `facets` offers it for the
/// control: an Activator, Scene Select and the triggers take none, whatever the file holds.
/// Value Scaling converges through a Float or Int; a Choice or switch has no in-between to
/// converge through, so there it acts as Pick-Up.
pub(crate) fn active(mapping: &RemoteMapping, descriptor: &ControlValue) -> Takeover {
    if !mapping.source.uses_takeover(mapping.mode)
        || mapping
            .facets(&mapping.control, descriptor)
            .takeover
            .is_hidden()
    {
        return Takeover::None;
    }
    match mapping.takeover {
        Takeover::ValueScaling
            if !matches!(descriptor.kind, ControlKind::Float | ControlKind::Int) =>
        {
            Takeover::PickUp
        }
        takeover => takeover,
    }
}

/// Value Scaling (design 4.3): the control converges on the hardware instead of waiting to
/// cross it. It works in hardware travel, 0 to 1, where `position` is the hardware value that
/// would produce the control's value under None. A move of the hardware carries the position
/// the same share of the way to the end it is heading for as the hardware has covered of its
/// own way there, so both reach each end together, it never jumps on the first touch and it
/// always moves the hardware's way. Once the two meet, the control tracks the hardware 1:1.
///
/// The position stays unrounded between messages, which keeps an Int target moving when one CC
/// step is under a unit. Any value the resolver did not send resets it from the engine.
#[derive(Default)]
pub(crate) struct ValueScaling {
    last_sent: Option<f64>,
    caught: bool,
    position: Option<f64>,
}

impl ValueScaling {
    /// The hardware travel to put through the ordinary absolute path (so Min, Max and Invert
    /// apply as for None), or `None` when this message sends nothing: the first from a port
    /// only records `previous` for the next, and a hardware that stands still moves nothing.
    pub fn travel(
        &mut self,
        mapping: &RemoteMapping,
        current: f64,
        previous: Option<f64>,
        hardware: f64,
    ) -> Option<f64> {
        // Includes external edits, automation and command rejection: no speculative
        // working value survives into a subsequent owner drain.
        if self.last_sent != Some(current) {
            self.caught = false;
            self.position = None;
        }
        // Equal ends hold one value, so there is nothing to converge on: the mode acts as None.
        if mapping.min == mapping.max {
            return Some(hardware);
        }
        // A value outside Min to Max counts as the nearer end.
        let position = *self.position.get_or_insert_with(|| {
            ((current - mapping.min) / (mapping.max - mapping.min)).clamp(0.0, 1.0)
        });
        if self.caught {
            return Some(self.meet(hardware));
        }
        let half_step = half_step(&mapping.source);
        // Where the hardware moved from, when it moved at all, and where that carries the
        // position. The first message from a port has nowhere to move from: it only records.
        let moved = previous.filter(|previous| *previous != hardware);
        let next = moved.map(|from| {
            if hardware > from {
                position + (hardware - from) * (1.0 - position) / (1.0 - from)
            } else {
                position - (from - hardware) * position / from
            }
        });
        let close_now = (position - hardware).abs() <= half_step;
        let close_next = next.is_some_and(|next| (next - hardware).abs() <= half_step);
        let at_end = moved.is_some() && (hardware <= 0.0 || hardware >= 1.0);
        if close_now || close_next || at_end {
            // Caught: the control tracks the hardware from here. The message that closes the
            // last half step still never goes against the hardware's way: when the control
            // sits just ahead of it, it stays put (or takes this move's share) until the next
            // message lands on the hardware.
            let ahead = if close_now {
                position
            } else {
                next.unwrap_or(position)
            };
            let target = match moved {
                Some(from) if hardware > from => hardware.max(ahead),
                Some(_) => hardware.min(ahead),
                None => hardware,
            };
            self.meet(hardware);
            return Some(target.clamp(0.0, 1.0));
        }
        let next = next?.clamp(0.0, 1.0);
        self.position = Some(next);
        Some(next)
    }

    fn meet(&mut self, hardware: f64) -> f64 {
        self.caught = true;
        self.position = Some(hardware);
        hardware
    }

    pub fn sent(&mut self, value: f64) {
        self.last_sent = Some(value);
    }
}

/// Half of one hardware step: a CC has 127 of them, pitch bend 16383.
fn half_step(source: &Source) -> f64 {
    match source {
        Source::Midi(MidiSource {
            message: MidiMessage::PitchBend,
            ..
        }) => 1.0 / 32766.0,
        _ => 1.0 / 254.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtable_engine::model::*;

    fn mapping() -> RemoteMapping {
        RemoteMapping::new(
            MappingId(1),
            Source::Midi(MidiSource {
                channel: 1,
                message: MidiMessage::Cc(1),
            }),
            ControlRef::Transport(TransportControl::Tempo),
            0.0,
            1.0,
        )
    }

    #[test]
    fn catches_crossing_and_return_then_releases_on_an_external_change() {
        let mapping = mapping();
        let mut pickup = PickUp::default();
        assert!(!pickup.allows(&mapping, 0.5, None, 0.0));
        assert!(pickup.allows(&mapping, 0.5, Some(0.0), 1.0));
        pickup.sent(1.0);
        assert!(pickup.allows(&mapping, 1.0, Some(1.0), 0.0));
        pickup.sent(0.0);
        assert!(!pickup.allows(&mapping, 0.7, Some(0.0), 0.1));
        assert!(pickup.allows(&mapping, 0.7, Some(0.1), 0.8));
    }

    #[test]
    fn meeting_inverted_and_equal_ranges_and_rejected_commands() {
        let mut mapping = mapping();
        mapping.min = 1.0;
        mapping.max = 0.0;
        let mut pickup = PickUp::default();
        assert!(pickup.allows(&mapping, 0.5, None, 0.5));
        pickup.sent(0.5);
        assert!(!pickup.allows(&mapping, 0.7, Some(0.5), 0.6));
        assert!(pickup.allows(&mapping, 0.7, Some(0.6), 0.1));
        mapping.min = 0.4;
        mapping.max = 0.4;
        assert!(PickUp::default().allows(&mapping, 0.4, None, 0.0));
        assert!(!PickUp::default().allows(&mapping, 0.5, Some(0.0), 1.0));
    }

    #[test]
    fn two_ports_cannot_invent_a_crossing_and_fanout_catches_separately() {
        let mapping = mapping();
        let first = Endpoint::Midi {
            id: "a".into(),
            name: "Knob".into(),
        };
        let second = Endpoint::Midi {
            id: "b".into(),
            name: "Knob".into(),
        };
        let mut history = HardwareHistory::default();
        history.record(&first, &mapping.source, 0.0);
        assert_eq!(history.previous(&second, &mapping.source), None);
        assert!(!PickUp::default().allows(
            &mapping,
            0.5,
            history.previous(&second, &mapping.source),
            1.0
        ));
        let previous = history.previous(&first, &mapping.source);
        assert!(PickUp::default().allows(&mapping, 0.3, previous, 0.4));
        assert!(!PickUp::default().allows(&mapping, 0.7, previous, 0.4));
    }

    /// A mapping over the whole travel, from a CC (`bend` false) or from pitch bend.
    fn scaled(bend: bool) -> RemoteMapping {
        let mut mapping = mapping();
        mapping.takeover = Takeover::ValueScaling;
        if bend {
            mapping.source = Source::Midi(MidiSource {
                channel: 1,
                message: MidiMessage::PitchBend,
            });
        }
        mapping
    }

    /// Runs hardware positions against a value at `start`, applying each result as the engine
    /// would (`sent`), and returns what each message put through the absolute path.
    fn run(mapping: &RemoteMapping, start: f64, hardware: &[f64]) -> Vec<Option<f64>> {
        let mut scaling = ValueScaling::default();
        let (mut current, mut previous) = (start, None);
        hardware
            .iter()
            .map(|&h| {
                let travel = scaling.travel(mapping, current, previous, h);
                if let Some(travel) = travel {
                    current = travel;
                    scaling.sent(travel);
                }
                previous = Some(h);
                travel
            })
            .collect()
    }

    fn close(left: Option<f64>, right: f64) {
        assert!(
            left.is_some_and(|left| (left - right).abs() < 1e-12),
            "{left:?} vs {right}"
        );
    }

    // Design 4.3: hardware 0.2 with the value at 0.8: up to 0.6 gives 0.9, up to 1.0 gives 1.0;
    // or down to 0.1 gives 0.4, down to 0 gives 0. Hardware 0.8 with the value at 0.2: up to 0.9
    // gives 0.6; down to 0.4 gives 0.1.
    #[test]
    fn the_worked_values_in_both_directions_of_mismatch() {
        let mapping = scaled(false);
        let up = run(&mapping, 0.8, &[0.2, 0.6, 1.0]);
        assert_eq!(up[0], None, "the first touch only records");
        close(up[1], 0.9);
        close(up[2], 1.0);
        let down = run(&mapping, 0.8, &[0.2, 0.1, 0.0]);
        close(down[1], 0.4);
        close(down[2], 0.0);
        let up = run(&mapping, 0.2, &[0.8, 0.9, 1.0]);
        close(up[1], 0.6);
        close(up[2], 1.0);
        let down = run(&mapping, 0.2, &[0.8, 0.4, 0.0]);
        close(down[1], 0.1);
        close(down[2], 0.0);
    }

    #[test]
    fn a_hardware_that_stands_still_sends_nothing_until_it_meets_the_value() {
        let mapping = scaled(false);
        let steps = run(&mapping, 0.8, &[0.2, 0.2, 0.2]);
        assert_eq!(steps, [None, None, None]);
        // Already there on the first message: caught at once, and then 1:1.
        let steps = run(&mapping, 0.5, &[0.5, 0.3, 0.9]);
        close(steps[0], 0.5);
        close(steps[1], 0.3);
        close(steps[2], 0.9);
    }

    #[test]
    fn it_catches_within_half_a_step_of_a_cc_and_of_pitch_bend() {
        let (cc, bend) = (scaled(false), scaled(true));
        // 1/254 and 1/32766: just inside catches on the first message, just outside does not.
        close(run(&cc, 0.5, &[0.5 + 0.9 / 254.0])[0], 0.5 + 0.9 / 254.0);
        assert_eq!(run(&cc, 0.5, &[0.5 + 1.1 / 254.0])[0], None);
        close(
            run(&bend, 0.5, &[0.5 + 0.9 / 32766.0])[0],
            0.5 + 0.9 / 32766.0,
        );
        assert_eq!(run(&bend, 0.5, &[0.5 + 1.1 / 32766.0])[0], None);
    }

    #[test]
    fn min_max_and_invert_move_only_the_position_never_the_rule() {
        // Value 0.35 in 0.2 to 0.8 sits at 0.25 of the travel; inverted (0.8 to 0.2) at 0.75.
        let mut mapping = scaled(false);
        mapping.min = 0.2;
        mapping.max = 0.8;
        let steps = run(&mapping, 0.35, &[0.0, 0.5]);
        close(steps[1], 0.25 + 0.5 * 0.75);
        mapping.min = 0.8;
        mapping.max = 0.2;
        let steps = run(&mapping, 0.35, &[0.0, 0.5]);
        close(steps[1], 0.75 + 0.5 * 0.25);
        // Outside Min to Max counts as the nearer end; equal ends act as None.
        let steps = run(&mapping, 0.95, &[0.0, 0.5]);
        close(steps[1], 0.5);
        mapping.min = 0.4;
        mapping.max = 0.4;
        let steps = run(&mapping, 0.4, &[0.9, 0.1]);
        close(steps[0], 0.9);
        close(steps[1], 0.1);
    }

    #[test]
    fn a_value_the_resolver_did_not_send_resets_the_position_and_the_catch() {
        let mapping = scaled(false);
        let mut scaling = ValueScaling::default();
        assert_eq!(scaling.travel(&mapping, 0.2, None, 0.8), None);
        let travel = scaling.travel(&mapping, 0.2, Some(0.8), 0.9).unwrap();
        close(Some(travel), 0.6);
        scaling.sent(travel);
        // Caught (1:1) until something else moves the value.
        let travel = scaling.travel(&mapping, travel, Some(0.9), 1.0).unwrap();
        close(Some(travel), 1.0);
        scaling.sent(travel);
        close(scaling.travel(&mapping, 1.0, Some(1.0), 0.3), 0.3);
        scaling.sent(0.3);
        // A GUI drag to 0.1: the position is the engine's again, and the catch is cleared.
        let travel = scaling.travel(&mapping, 0.1, Some(0.3), 0.4).unwrap();
        close(Some(travel), 0.1 + 0.1 * 0.9 / 0.7);
    }

    #[test]
    fn device_history_has_a_hard_bound_and_refreshes_recent_sources() {
        let source = mapping().source;
        let mut history = HardwareHistory::default();
        for id in 0..HISTORY_LIMIT + 1 {
            history.record(
                &Endpoint::Midi {
                    id: id.to_string(),
                    name: "Knob".into(),
                },
                &source,
                0.0,
            );
        }
        assert_eq!(history.order.len(), HISTORY_LIMIT);
        assert_eq!(
            history.previous(
                &Endpoint::Midi {
                    id: "0".into(),
                    name: "Knob".into()
                },
                &source
            ),
            None
        );
        history.record(
            &Endpoint::Midi {
                id: "1".into(),
                name: "Knob".into(),
            },
            &source,
            0.5,
        );
        assert_eq!(history.order.len(), HISTORY_LIMIT);
        history.record(&Endpoint::Keyboard, &source, 1.0);
        let port = |id: &str| Endpoint::Midi {
            id: id.into(),
            name: "Knob".into(),
        };
        assert_eq!(history.previous(&port("1"), &source), Some(0.5));
        assert_eq!(history.previous(&port("2"), &source), None);
        history.retain_sources(&HashSet::new());
        assert!(history.order.is_empty());
        assert!(history.values.is_empty());
    }

    #[test]
    fn removing_one_source_keeps_other_ports_and_sources_in_lru_order() {
        let first = mapping().source;
        let second = Source::Key(KeySource {
            key: KeyName("a".into()),
        });
        let port = Endpoint::Midi {
            id: "port".into(),
            name: "Controller".into(),
        };
        let mut history = HardwareHistory::default();
        history.record(&Endpoint::Keyboard, &first, 0.1);
        history.record(&port, &first, 0.2);
        history.record(&port, &second, 0.3);
        history.retain_sources(&HashSet::from([&first]));
        assert_eq!(history.previous(&port, &second), None);
        assert_eq!(history.previous(&port, &first), Some(0.2));
        assert_eq!(history.previous(&Endpoint::Keyboard, &first), Some(0.1));
        history.record(&Endpoint::Keyboard, &first, 0.4);
        assert_eq!(history.order.values().next(), Some(&(port, first)));
        assert_eq!(history.order.len(), 2);
    }
}
