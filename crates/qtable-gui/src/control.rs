//! One control descriptor that iced widgets and canvas drawings both read, so size, colour,
//! state and focus cannot drift between them.
//!
//! `Control` holds what every control has; a `ControlKind` is one kind of control (toggle,
//! number, solo, launch…); `Theme` holds the colours; `widgets` builds iced elements and
//! `canvas` paints the same kinds on a canvas. A new kind is one new type and one `impl`.
mod activity_dot;
pub(crate) use activity_dot::ActivityDot;
pub(crate) mod adjustment;
pub(crate) mod automation_mark;
mod bang;
mod canvas;
mod canvas_kinds;
pub(crate) mod device_layout;
pub(crate) mod dial;
mod double_click;
pub(crate) mod fader;
pub(crate) mod height_reserve;
pub(crate) mod help;
pub(crate) mod key_gate;
mod kinds;
mod load_meter;
pub(crate) mod map_layer;
pub(crate) mod mapping;
pub(crate) mod measure;
pub(crate) mod menu;
pub(crate) mod multi_select;
mod number_input;
pub(crate) mod path_field;
pub(crate) mod rule;
pub(crate) mod slider;
pub(crate) mod text_field;
pub(crate) mod theme;
pub(crate) mod time_axis;
pub(crate) mod time_field;
pub(crate) mod timeline_brace;
pub(crate) use time_field::TimeField;
pub(crate) mod viewport;
pub(crate) mod widgets;

pub(crate) use adjustment::Nudge;
pub(crate) use bang::Bang;
pub(crate) use canvas::outline;
pub(crate) use canvas_kinds::{Fold, FollowToggle, Launch, Solo, Stop, StripIcon};
pub(crate) use dial::Dial;
pub(crate) use fader::Fader;
pub(crate) use kinds::{
    Activator, Button, Dropdown, Face, Glyph, Number, Readout, Text, Toggle, ViewTab,
};
pub(crate) use load_meter::LoadMeter;
pub(crate) use multi_select::MultiFace;
pub(crate) use path_field::PathField;
pub(crate) use slider::Slider;
pub(crate) use theme::{LIVE_DARK, Theme};
pub(crate) use widgets::{Controls, tooltip, tooltip_if};

use crate::control_focus::{ControlId, ControlKey};
use iced::widget::canvas::Frame;
use iced::{Color, Length, Point, Rectangle};
use qtable_engine::model::{FieldDef, FieldType, OscArgument, ParameterSpec};

/// The attributes every control shares, after Live's `live.*` objects
/// (Cycling '74 live.numbox and live.text references, consulted 2026-10-12).
pub(crate) struct Control<K, Key: ControlKey> {
    pub id: ControlId<Key>,
    /// What the control is: its help entry (required, so no control ships without help) and
    /// what it drives, if a controller could.
    pub identity: help::Identity<Key::Help>,
    pub kind: K,
    pub role: SizeRole,
    pub state: State,
    /// The id of a field's text input when it is not the one its `ControlId` names: a draft
    /// that a focus operation must find under another id (the Target face's exact draft).
    pub input_id: Option<iced::widget::Id>,
    /// Where a double click returns the control, in the units the widget publishes (a fraction
    /// for Slider, Dial and fader). `None` leaves a double click alone.
    pub default: Option<f64>,
    /// Keyboard adjustment in the units this control publishes.
    pub nudge: Option<Nudge>,
}

impl<K: ControlKind, Key: ControlKey> Control<K, Key> {
    pub(crate) fn new(id: ControlId<Key>, help: Key::Help, kind: K) -> Self {
        Self {
            id,
            identity: help::Identity::new(help),
            role: kind.role(),
            kind,
            state: State::default(),
            input_id: None,
            default: None,
            nudge: None,
        }
    }

    /// Declare what this control drives, by stable ids, so Map Mode can light and map it.
    pub(crate) fn drives(mut self, drives: qtable_engine::model::ControlRef) -> Self {
        self.identity.drives = Some(drives);
        self
    }

    /// `drives`, for a control that may drive nothing a controller could (a text argument).
    pub(crate) fn drives_when(mut self, drives: Option<qtable_engine::model::ControlRef>) -> Self {
        self.identity.drives = drives;
        self
    }

    /// Double-click returns the control to `value`, published as a release (one edit that also
    /// ends any On Release hold).
    pub(crate) fn default_to(mut self, value: f64) -> Self {
        self.default = Some(value);
        self
    }

    pub(crate) fn nudge(mut self, nudge: Nudge) -> Self {
        self.nudge = Some(nudge);
        self
    }

    /// Name the text input; `None` keeps the id the `ControlId` gives it.
    pub(crate) fn input_id(mut self, id: Option<iced::widget::Id>) -> Self {
        self.input_id = id;
        self
    }

    pub(crate) fn role(mut self, role: SizeRole) -> Self {
        self.role = role;
        self
    }

    pub(crate) fn lit(mut self, on: bool) -> Self {
        self.state.lit = if on { Lit::On } else { Lit::Off };
        self
    }

    pub(crate) fn lit_as(mut self, lit: Lit) -> Self {
        self.state.lit = lit;
        self
    }

    pub(crate) fn enabled(mut self, enabled: bool) -> Self {
        self.state.enabled = enabled;
        self
    }

    /// An inactive device's control: still editable, drawn in its parts' inactive colours.
    pub(crate) fn inactive(mut self, inactive: bool) -> Self {
        self.state.inactive = inactive;
        self
    }

    pub(crate) fn invalid(mut self, invalid: bool) -> Self {
        self.state.invalid = invalid;
        self
    }
}

/// Stored state. Focus is not here: adapters ask `ControlFocus`, so it never goes stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct State {
    pub enabled: bool,
    /// Editable but drawn in inactive colours: a control of a deactivated device, or one that
    /// doesn't apply. Only a control's own parts dim; a label keeps its ink (`enabled` dims it).
    pub inactive: bool,
    pub lit: Lit,
    /// A rejected draft: the alert border wins over focus.
    pub invalid: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            enabled: true,
            inactive: false,
            lit: Lit::Off,
            invalid: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lit {
    Off,
    On,
    /// Live's half-lit group Solo (manual 18.3).
    Partial,
}

/// Pointer state, supplied by the adapter each frame rather than stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Interaction {
    Idle,
    Hovered,
    Pressed,
}

/// The prescribed control widths, held in one place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SizeRole {
    Float,
    /// Half-cell numeric values in device bodies.
    DeviceNumber,
    /// Short device choices, including their dropdown arrow.
    DeviceChoice,
    /// Compact values in the hit argument row.
    HitNumber,
    HitString,
    HitBool,
    Port,
    Tag,
    /// Names, hosts and OSC paths: fill the column, at most 220 px.
    Name,
    /// Fill the column of a Detail View panel.
    Fill,
    /// Fill a fixed Session column (104 px), with compact strip typography.
    Strip,
    /// Shrink to content: pickers and text buttons.
    Content,
    Fixed {
        width: f32,
        height: f32,
    },
}

pub(crate) const NAME_MAX_WIDTH: f32 = 220.0;

impl SizeRole {
    pub(crate) fn width(self) -> Length {
        match self {
            Self::Float => Length::Fixed(72.0),
            Self::DeviceNumber => Length::Fixed(crate::style::detail::DEVICE_NUMBER_WIDTH),
            Self::DeviceChoice => Length::Fixed(crate::style::detail::DEVICE_CHOICE_WIDTH),
            Self::HitNumber => Length::Fixed(64.0),
            Self::HitString => Length::Fixed(140.0),
            Self::HitBool => Length::Fixed(36.0),
            Self::Port => Length::Fixed(60.0),
            Self::Tag => Length::Fixed(44.0),
            Self::Name | Self::Fill | Self::Strip => Length::Fill,
            Self::Content => Length::Shrink,
            Self::Fixed { width, .. } => Length::Fixed(width),
        }
    }

    pub(crate) fn height(self) -> Option<f32> {
        match self {
            Self::Fixed { height, .. } => Some(height),
            _ => None,
        }
    }

    /// Session strip controls use 10 px regular text with tight padding (mixer-live-layout).
    pub(crate) fn is_compact(self) -> bool {
        matches!(self, Self::Strip | Self::HitBool | Self::Fixed { .. })
    }
}

/// How a number reads on screen. Formatting never edits the model: an untouched display
/// submitted again is a no-op, so float presentation preserves model precision.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ValueFormat {
    Parameter(&'static ParameterSpec),
    /// Two to four decimals, for floats without a `ParameterSpec` yet (tempo, mapping).
    Float,
    /// A Target field's value by its type: Int rounds to a whole number, the rest read as
    /// `Float`. How the face's controls show a value.
    Field(FieldType),
    /// A Target field's Min or Max by its type, as stored: an Int bound as written, a Float
    /// bound to its shortest decimals (two at least). How the definition editor shows a range.
    FieldBound(FieldType),
}

impl ValueFormat {
    pub(crate) fn display(self, value: f64) -> String {
        match self {
            Self::Parameter(spec) => spec.format(value),
            Self::Float => qtable_engine::model::format_float(value, Some(4)),
            Self::Field(FieldType::Int) => value.round().to_string(),
            Self::Field(_) => Self::Float.display(value),
            Self::FieldBound(FieldType::Int) => value.to_string(),
            Self::FieldBound(_) => qtable_engine::model::format_float(value, None),
        }
    }

    /// How a field's value reads on its face.
    pub(crate) fn field(field: &FieldDef) -> Self {
        Self::Field(field.value_type)
    }

    /// How a field's Min and Max read in the definition editor.
    pub(crate) fn field_bound(field: &FieldDef) -> Self {
        Self::FieldBound(field.value_type)
    }

    /// The text equals what `display` shows for `model`, so submitting it changes nothing.
    pub(crate) fn is_untouched_display(self, input: &str, model: f64) -> bool {
        match self {
            Self::Parameter(spec) => spec.is_untouched_display(input, model),
            _ => input.trim() == self.display(model),
        }
    }
}

/// An OSC argument as text, the one reading shared by the Events strip and the definition
/// editor: a Float in its shortest round-tripping decimals (two at least), the rest as written.
pub(crate) fn argument_text(argument: &OscArgument) -> String {
    match argument {
        OscArgument::Int(v) => v.to_string(),
        OscArgument::Float(v) => qtable_engine::model::format_float(v, None),
        OscArgument::String(v) => v.clone(),
        OscArgument::Bool(v) => v.to_string(),
    }
}

/// Resolved colours for one control in one state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Look {
    pub fill: Color,
    pub ink: Color,
    pub frame: Color,
    /// The lit part: Solo's half fill, a field's edit border, a toggle's on colour.
    pub accent: Color,
}

impl Look {
    /// Canvas buttons whose colours follow playback rather than control state.
    pub(crate) fn glyph(fill: Color, ink: Color) -> Self {
        Self {
            fill,
            ink,
            frame: fill,
            accent: ink,
        }
    }
}

/// One kind of control: the "derived element" of the parent-class intent (2026-10-12).
/// Object-safe, so user-defined control-devices can later be `Box<dyn ControlKind>`.
pub(crate) trait ControlKind {
    /// Colours for this kind in this state. Both adapters read it, so iced and canvas agree.
    fn look(&self, theme: &Theme, state: State, interaction: Interaction) -> Look;

    /// Default size when the caller names no role.
    fn role(&self) -> SizeRole;

    /// Draw inside `bounds` on a canvas frame. The default is a framed box.
    fn paint(&self, frame: &mut Frame, bounds: Rectangle, look: &Look) {
        frame.fill_rectangle(bounds.position(), bounds.size(), look.frame);
        frame.fill_rectangle(
            Point::new(bounds.x + 1.0, bounds.y + 1.0),
            iced::Size::new(bounds.width - 2.0, bounds.height - 2.0),
            look.fill,
        );
    }

    /// Toggle-like canvas controls can use Enter as their ordinary click action.
    fn activates_on_enter(&self) -> bool {
        false
    }

    /// Whether a press at `point` lands on the control.
    fn hit(&self, bounds: Rectangle, point: Point) -> bool {
        bounds.contains(point)
    }
}

#[cfg(test)]
mod tests;
