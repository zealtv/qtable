//! What a mapping can set for its source and control: the one rule the inspector, validation
//! and the resolver share, so none of them re-derives which settings apply.
//!
//! `RemoteMapping::facets` answers, for each setting a mapping carries, whether it is absent
//! (`Hidden`: it can never apply to this source and control), `Dimmed` (it applies, but not in
//! the mapping's current mode, so it keeps its stored value and does nothing) or `Enabled`.
//! `control_limits` and `learn_range` say what Min and Max mean for the control and where a freshly
//! learned mapping starts them.
use super::*;

/// Whether one of a mapping's settings applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facet {
    /// It can never apply to this source and control: the setting is not offered.
    Hidden,
    /// It applies to this source and control, but not in the mapping's current mode.
    Dimmed,
    Enabled,
}

impl Facet {
    /// `Enabled` while `active`, else `Dimmed`: a setting that belongs to one mode.
    fn when(active: bool) -> Self {
        if active { Self::Enabled } else { Self::Dimmed }
    }

    pub fn is_hidden(self) -> bool {
        self == Self::Hidden
    }

    pub fn is_enabled(self) -> bool {
        self == Self::Enabled
    }
}

/// What a row's Min and Max mean for its control, in the control's own units. The rule lives
/// here once, so the list, the inspector and the resolver agree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Limits {
    /// A launch, stop or bang: nothing to scale, so the row has no Min or Max (as Live's slot row).
    None,
    /// An optional `[low, high]` the control accepts; `integer` rounds to whole units.
    Number {
        integer: bool,
        range: Option<[f64; 2]>,
    },
    /// An Enum: the first and last choice, as indexes; the hardware range splits into equal
    /// bins between them. `count` is the number of choices (the labels belong to the
    /// descriptor).
    Choice { count: usize },
    /// A switch (Solo, Follow Actions): Off or On at each end of the hardware range.
    Switch,
    /// An Activator: the window of hardware values (0–100%) in which the track is active.
    Window,
}

/// The settings of one mapping, each with whether it applies. The design's table is §3.4.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Facets {
    /// The mode chooser: the nine of a CC (Absolute and eight relative encodings), or Latch and
    /// Gate for a note or key. Hidden for pitch bend (always absolute) and where Gate is not
    /// offered (a trigger holds no value).
    pub mode: Facet,
    pub takeover: Facet,
    pub sensitivity: Facet,
    /// Invert is Min above Max; this says whether swapping the two does anything.
    pub invert: Facet,
    pub min: Facet,
    pub max: Facet,
    pub limits: Limits,
}

impl Facets {
    const ABSENT: Self = Self {
        mode: Facet::Hidden,
        takeover: Facet::Hidden,
        sensitivity: Facet::Hidden,
        invert: Facet::Hidden,
        min: Facet::Hidden,
        max: Facet::Hidden,
        limits: Limits::None,
    };
}

/// How a control takes a value, for the purposes of the table.
#[derive(Clone, Copy, PartialEq)]
enum Target {
    /// Float or Int, including Tempo.
    Number,
    /// An Enum argument or Quantisation.
    Choice,
    /// Solo, Follow Actions or a Bool argument.
    Switch,
    Activator,
    SceneSelect,
    /// A launch, stop, bang or anything else with no value to hold; also Text, which cannot be mapped.
    Trigger,
}

impl Target {
    fn of(control: &ControlRef, value: &ControlValue) -> Self {
        match (control, value.kind) {
            (ControlRef::SceneNavigation(SceneNavigation::Select), _) => Self::SceneSelect,
            (ControlRef::TrackActivator(_), _) => Self::Activator,
            (_, ControlKind::Float | ControlKind::Int) => Self::Number,
            (_, ControlKind::Enum) => Self::Choice,
            (_, ControlKind::Toggle) => Self::Switch,
            (_, ControlKind::Text | ControlKind::Bang | ControlKind::Trigger) => Self::Trigger,
        }
    }
}

/// What a row's Min and Max mean for `control`: a Number for a range, the choices for an Enum,
/// Off and On for a switch, a percentage window for an Activator, and nothing for a launch or
/// Scene Select (which always spans every scene).
pub fn control_limits(control: &ControlRef, value: &ControlValue) -> Limits {
    match Target::of(control, value) {
        Target::Trigger | Target::SceneSelect => Limits::None,
        Target::Activator => Limits::Window,
        Target::Switch => Limits::Switch,
        Target::Choice => Limits::Choice {
            count: value.range.map_or(0, |[_, high]| high.round() as usize + 1),
        },
        Target::Number => Limits::Number {
            integer: value.kind == ControlKind::Int,
            range: value.range,
        },
    }
}

/// The hardware window a learned Activator starts with: a conventional threshold (64 to 127 of
/// 127), so it follows the controller's On half. A 0–100% window accepts every message.
pub const ACTIVATOR_WINDOW: [f64; 2] = [64.0 / 127.0, 1.0];

/// Live's tempo range, for a Tempo mapping: the model sets none.
pub const TEMPO_RANGE: [f64; 2] = [20.0, 999.0];

/// The range a Float with no range of its own (an event argument) starts with.
pub const UNRANGED_FLOAT: [f64; 2] = [0.0, 1.0];

/// The range an Int with no range of its own starts with: a MIDI data byte.
pub const UNRANGED_INT: [f64; 2] = [0.0, 127.0];

/// Min and Max a freshly learned mapping starts with: the control's whole range, as Live does.
/// A control with no range of its own learns Tempo's 20–999 for Tempo, 0–1 for a Float and
/// 0–127 for an Int. Controls that show no Min or Max keep 0–1.
pub fn learn_range(control: &ControlRef, value: &ControlValue) -> (f64, f64) {
    let [low, high] = match (Target::of(control, value), value.range) {
        (Target::Activator, _) => ACTIVATOR_WINDOW,
        (Target::Trigger | Target::SceneSelect, _) => UNRANGED_FLOAT,
        (_, Some(range)) => range,
        (_, None) if matches!(control, ControlRef::Transport(TransportControl::Tempo)) => {
            TEMPO_RANGE
        }
        (_, None) if value.kind == ControlKind::Int => UNRANGED_INT,
        (_, None) => UNRANGED_FLOAT,
    };
    (low, high)
}

impl RemoteMapping {
    /// Which settings apply to this mapping's source (and its mode) and `control`, which
    /// `value` describes. One rule for every client; see the table in the design, §3.4.
    pub fn facets(&self, control: &ControlRef, value: &ControlValue) -> Facets {
        let target = Target::of(control, value);
        let limits = control_limits(control, value);
        let relative = matches!(self.mode, SourceMode::Relative(_));
        let gate = self.mode == SourceMode::Gate;
        let at_end = |settings: Facet| Facets {
            min: settings,
            max: settings,
            limits,
            ..Facets::ABSENT
        };
        match &self.source {
            Source::Midi(MidiSource {
                message: MidiMessage::Cc(_),
                ..
            }) => {
                let mode = Facet::Enabled;
                match target {
                    Target::Number | Target::Choice => Facets {
                        mode,
                        takeover: Facet::when(!relative),
                        sensitivity: Facet::when(relative),
                        invert: Facet::Enabled,
                        ..at_end(Facet::Enabled)
                    },
                    Target::Switch => Facets {
                        mode,
                        takeover: Facet::when(!relative),
                        invert: Facet::when(!relative),
                        ..at_end(Facet::when(!relative))
                    },
                    Target::Activator => Facets {
                        mode,
                        ..at_end(Facet::when(!relative))
                    },
                    Target::SceneSelect => Facets {
                        mode,
                        sensitivity: Facet::when(relative),
                        ..Facets::ABSENT
                    },
                    Target::Trigger => Facets {
                        mode,
                        ..Facets::ABSENT
                    },
                }
            }
            Source::Midi(MidiSource {
                message: MidiMessage::PitchBend,
                ..
            }) => match target {
                Target::Number | Target::Choice | Target::Switch => Facets {
                    takeover: Facet::Enabled,
                    invert: Facet::Enabled,
                    ..at_end(Facet::Enabled)
                },
                Target::Activator => at_end(Facet::Enabled),
                Target::SceneSelect | Target::Trigger => Facets::ABSENT,
            },
            Source::Midi(MidiSource {
                message: MidiMessage::Note(_),
                ..
            })
            | Source::Key(_) => match target {
                Target::Number => Facets {
                    mode: Facet::Enabled,
                    invert: Facet::Enabled,
                    ..at_end(Facet::Enabled)
                },
                Target::Choice => Facets {
                    mode: Facet::Enabled,
                    invert: Facet::when(gate),
                    ..at_end(Facet::Enabled)
                },
                // Latch flips a switch and Gate needs only a direction, so no Off/On pair.
                // The same holds for an Activator, which keeps its stored window.
                Target::Switch | Target::Activator => Facets {
                    mode: Facet::Enabled,
                    invert: Facet::when(gate),
                    limits,
                    ..Facets::ABSENT
                },
                Target::SceneSelect | Target::Trigger => Facets::ABSENT,
            },
        }
    }
}
