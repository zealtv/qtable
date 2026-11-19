//! iced adapter: one constructor per kind. Each builds the widget, sizes it from its role,
//! styles it from the kind's `look`, registers its help and wraps it for focus, so call sites
//! no longer repeat those steps.
use super::canvas::Hosted;
use super::help::{self, HelpId, Hover, Identity};
use super::path_field::{CURSOR_FLOOR, PathFrame, PrefixText, measure};
use super::text_field::TextField;
use super::{
    Activator, Button, Control, ControlKind, Dropdown, Face, Glyph, Interaction, LIVE_DARK, Look,
    MultiFace, NAME_MAX_WIDTH, Number, PathField, Readout, State, Text, Theme, Toggle, ViewTab,
};
use crate::control_focus::{ControlFocus, ControlId, ControlKey};
use crate::style::VALUE_FONT;
use crate::style::detail::{COMPACT_TEXT_SIZE, CONTROL_HEIGHT, INLINE_ROW_HEIGHT, TEXT_SIZE};
use iced::widget::Canvas;
use iced::widget::canvas::{Frame, Geometry, Program};
use iced::widget::tooltip::{Position, Tooltip};
use iced::widget::{PickList, Space, button, container, pick_list, text, text_input};
use iced::{Border, Color, Element, Length, Rectangle, Renderer, alignment, mouse};
use std::borrow::Borrow;

/// Padding inside every tooltip bubble (unified from 4 and 6 px, shared-tooltip 2026-10-16).
const TOOLTIP_PADDING: u16 = 6;
/// Widest a tooltip bubble grows before its text wraps.
const TOOLTIP_MAX_WIDTH: u32 = 600;

/// The one tooltip: `trigger` with a bubble of `label` at `position`, in the shared bubble style
/// and `TEXT_SIZE`. Hand-built `iced::widget::tooltip` bubbles are a departure from the library.
pub(crate) fn tooltip<'a, M: 'a>(
    trigger: impl Into<Element<'a, M>>,
    label: impl text::IntoFragment<'a>,
    position: Position,
) -> Element<'a, M> {
    Tooltip::new(
        trigger,
        container(text(label).size(TEXT_SIZE))
            .padding(TOOLTIP_PADDING)
            .max_width(TOOLTIP_MAX_WIDTH)
            .style(crate::style::tooltip),
        position,
    )
    .into()
}

/// `tooltip` whose bubble comes and goes with the text: `None` shows nothing, but the widget
/// keeps its place in the tree, so a trigger with state of its own (a canvas) keeps that state.
pub(crate) fn tooltip_if<'a, M: 'a>(
    trigger: impl Into<Element<'a, M>>,
    label: Option<String>,
    position: Position,
) -> Element<'a, M> {
    match label {
        Some(label) => tooltip(trigger, label, position),
        None => Tooltip::new(trigger, iced::widget::Space::new(), position).into(),
    }
}

/// A kind's colours for each pointer state, computed once so style closures stay `'static`.
#[derive(Clone, Copy)]
pub(crate) struct Looks {
    pub(super) idle: Look,
    pub(super) hovered: Look,
    pub(super) pressed: Look,
    pub(super) disabled: Look,
    pub(super) theme: Theme,
}

impl Looks {
    pub(crate) fn of(kind: &dyn ControlKind, theme: &Theme, state: State) -> Self {
        let disabled = State {
            enabled: false,
            ..state
        };
        Self {
            idle: kind.look(theme, state, Interaction::Idle),
            hovered: kind.look(theme, state, Interaction::Hovered),
            pressed: kind.look(theme, state, Interaction::Pressed),
            disabled: kind.look(theme, disabled, Interaction::Idle),
            theme: *theme,
        }
    }

    fn for_button(&self, status: button::Status) -> Look {
        match status {
            button::Status::Active => self.idle,
            button::Status::Hovered => self.hovered,
            button::Status::Pressed => self.pressed,
            button::Status::Disabled => self.disabled,
        }
    }
}

/// Latched toggle: a 1 px frame and square corners.
pub(crate) fn toggle_style(looks: Looks) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let look = looks.for_button(status);
        button::Style {
            background: Some(look.fill.into()),
            text_color: look.ink,
            border: Border {
                color: look.frame,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        }
    }
}

/// Round latched switch: the Toggle's colours on a circle. It keeps its idle look under the
/// pointer, as the device Activator always has.
pub(crate) fn activator_style(
    looks: Looks,
    size: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let look = match status {
            button::Status::Disabled => looks.disabled,
            _ => looks.idle,
        };
        button::Style {
            background: Some(look.fill.into()),
            border: Border {
                color: look.frame,
                width: 1.0,
                radius: (size / 2.0).into(),
            },
            ..Default::default()
        }
    }
}

/// Flat latched toggle for region switches. The hover surface replaces the boxed
/// toggle's hover frame, while a lit switch retains its on fill.
pub(crate) fn flat_toggle_style(
    theme: &Theme,
    state: State,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    let mut looks = Looks::of(
        &Toggle {
            label: String::new(),
        },
        theme,
        state,
    );
    if state.enabled && state.lit == super::Lit::Off {
        looks.hovered.fill = theme.surface_highlight;
    }
    flat_style(looks)
}

/// Flat action button: no frame, 1 px rounding, as in Live's top bar.
pub(crate) fn flat_style(looks: Looks) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let look = looks.for_button(status);
        button::Style {
            background: Some(look.fill.into()),
            text_color: look.ink,
            border: Border::default().rounded(LIVE_DARK.corner_control),
            ..Default::default()
        }
    }
}

/// A view tab: square, unframed (the bar draws the rules between tabs).
fn view_tab_style(looks: Looks) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let look = looks.for_button(status);
        button::Style {
            background: Some(look.fill.into()),
            text_color: look.ink,
            border: Border::default(),
            ..Default::default()
        }
    }
}

pub(crate) fn field_style(
    looks: Looks,
) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
    move |_, status| {
        let look = match status {
            text_input::Status::Active => looks.idle,
            text_input::Status::Hovered => looks.hovered,
            text_input::Status::Focused { .. } => looks.pressed,
            text_input::Status::Disabled => looks.disabled,
        };
        text_input::Style {
            background: look.fill.into(),
            border: Border {
                color: look.frame,
                width: 1.0,
                radius: LIVE_DARK.corner_control.into(),
            },
            icon: look.ink,
            placeholder: looks.theme.text_disabled,
            value: look.ink,
            selection: Color {
                a: 0.4,
                ..look.accent
            },
        }
    }
}

/// The ▼ glyph in a boxed picker, smaller than its text as Live draws it.
const DROPDOWN_ARROW_SIZE: f32 = 8.0;

/// The boxed ▼ picker and its menu.
fn dropdown_arrow() -> pick_list::Icon<iced::Font> {
    pick_list::Icon {
        font: <Renderer as iced::advanced::text::Renderer>::ICON_FONT,
        code_point: <Renderer as iced::advanced::text::Renderer>::ARROW_DOWN_ICON,
        size: Some(iced::Pixels(DROPDOWN_ARROW_SIZE)),
        line_height: Default::default(),
        shaping: iced::advanced::text::Shaping::Basic,
    }
}

pub(crate) fn picker<'a, T, L, M>(
    looks: Looks,
    options: L,
    selected: Option<T>,
    on_select: impl Fn(T) -> M + 'a,
) -> PickList<'a, T, L, T, M>
where
    T: ToString + PartialEq + Clone + 'a,
    L: Borrow<[T]> + 'a,
    M: Clone + 'a,
{
    let frame = |color| Border {
        color,
        width: 1.0,
        radius: 0.0.into(),
    };
    pick_list(options, selected, on_select)
        .font(VALUE_FONT)
        .text_size(TEXT_SIZE)
        .padding([2, 4])
        .handle(pick_list::Handle::Static(dropdown_arrow()))
        .style(move |_, status| {
            let look = match status {
                pick_list::Status::Opened { .. } => looks.pressed,
                pick_list::Status::Hovered => looks.hovered,
                pick_list::Status::Active => looks.idle,
            };
            pick_list::Style {
                text_color: look.ink,
                placeholder_color: looks.theme.text_disabled,
                handle_color: look.ink,
                background: look.fill.into(),
                border: frame(look.frame),
            }
        })
        // iced draws this list itself: it takes the shared popup panel and ink
        // (`control::menu`), and keeps its own row padding and its selected-row colours.
        .menu_style(move |_| {
            let panel = super::menu::panel_style(&looks.theme);
            iced::widget::overlay::menu::Style {
                background: panel.background.unwrap_or(looks.idle.fill.into()),
                border: panel.border,
                text_color: super::menu::ink(&looks.theme, false),
                selected_text_color: looks.theme.control_on_foreground,
                selected_background: looks.idle.accent.into(),
                shadow: iced::Shadow::default(),
            }
        })
}

/// The ▼ at the right of a dropdown's closed face.
fn arrow_glyph<'a>() -> text::Text<'a> {
    let arrow = dropdown_arrow();
    text(arrow.code_point.to_string())
        .font(arrow.font)
        .size(arrow.size.expect("dropdown arrow has a size"))
        .line_height(arrow.line_height)
        .shaping(arrow.shaping)
}

/// A text or number field with the shared value typography.
pub(crate) fn field<'a, M: Clone + 'a>(
    looks: Looks,
    placeholder: &str,
    value: &str,
) -> TextField<'a, M> {
    TextField::new(placeholder, value)
        .font(VALUE_FONT)
        .size(TEXT_SIZE)
        .padding([2, 4])
        .style(field_style(looks))
}

/// A toggle's label, centred, in value typography (or compact strip text).
pub(crate) fn toggle_label<'a>(
    label: impl text::IntoFragment<'a>,
    compact: bool,
) -> text::Text<'a> {
    // Fill the button so the label centres in a wide toggle.
    let label = text(label)
        .align_x(alignment::Horizontal::Center)
        .width(Length::Fill);
    if compact {
        label
            .size(COMPACT_TEXT_SIZE)
            .height(Length::Fill)
            .align_y(alignment::Vertical::Center)
    } else {
        label.font(VALUE_FONT).size(TEXT_SIZE)
    }
}

/// The transport glyph inside a flat button.
struct GlyphCanvas {
    glyph: Glyph,
    ink: Color,
}

impl<M> Program<M> for GlyphCanvas {
    type State = ();

    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        self.glyph
            .paint(&mut frame, Rectangle::with_size(bounds.size()), self.ink);
        vec![frame.into_geometry()]
    }
}

/// What a control publishes for help hover and focus, in each view's message type.
pub(crate) trait ControlMessage<Key: ControlKey>: Clone {
    fn info(hover: Hover<Key::Help>) -> Self;
    fn focused(id: ControlId<Key>) -> Self;
    fn commit_number() -> Self;
    /// Enter submitted a text field: the app releases the keyboard if the value was accepted.
    fn field_entered() -> Self;
}

/// Builds controls for one window from its theme and focus.
#[derive(Clone, Copy)]
pub(crate) struct Controls<'a, Key: ControlKey> {
    theme: Theme,
    focus: &'a ControlFocus<Key>,
    subject: Option<&'a <Key::Help as HelpId>::Subject>,
    shift: bool,
}

impl<'a, Key: ControlKey> Controls<'a, Key> {
    pub(crate) fn new(theme: Theme, focus: &'a ControlFocus<Key>) -> Self {
        Self {
            theme,
            focus,
            subject: None,
            shift: false,
        }
    }

    /// A 1 px rule in this control set's theme (`control::rule`).
    pub(crate) fn rule<'e, M: 'e>(
        &self,
        axis: super::rule::Axis,
        level: super::rule::Level,
    ) -> Element<'e, M> {
        super::rule::rule(&self.theme, axis, level)
    }

    pub(crate) fn shift(mut self, shift: bool) -> Self {
        self.shift = shift;
        self
    }

    /// Give this group of controls contextual help about a model subject.
    pub(crate) fn about(mut self, subject: &'a <Key::Help as HelpId>::Subject) -> Self {
        self.subject = Some(subject);
        self
    }

    fn help<'e, M: ControlMessage<Key> + 'e>(
        &self,
        element: impl Into<Element<'e, M>>,
        identity: Identity<Key::Help>,
    ) -> Element<'e, M> {
        help::widget_with(element, identity, self.subject.cloned(), M::info)
    }

    /// The ink of a label or readout: the field kind's enabled/disabled ink. Only `enabled`
    /// dims it; `inactive` (an inactive device's editable control) leaves it alone, as in Live.
    pub(crate) fn label_ink(&self, state: State) -> Color {
        Text {
            placeholder: "".into(),
        }
        .look(&self.theme, state, Interaction::Idle)
        .ink
    }

    /// Labels and readouts share the field kind's enabled/disabled ink.
    pub(crate) fn readout<'e>(
        &self,
        label: impl text::IntoFragment<'e>,
        state: State,
    ) -> text::Text<'e> {
        text(label).size(TEXT_SIZE).color(self.label_ink(state))
    }

    pub(crate) fn labelled<'e, M: 'e>(
        &self,
        label: impl text::IntoFragment<'e>,
        state: State,
        content: impl Into<Element<'e, M>>,
    ) -> iced::widget::Column<'e, M> {
        iced::widget::column![self.readout(label, state), content.into()]
            .spacing(crate::style::detail::LABEL_GAP)
            .width(Length::Fill)
    }

    /// A clipped label of `label_width` at the left of one 22 px row, as the Target editor has.
    pub(crate) fn inline<'e, M: 'e>(
        &self,
        label: impl text::IntoFragment<'e>,
        label_width: f32,
        state: State,
        content: impl Into<Element<'e, M>>,
    ) -> iced::widget::Row<'e, M> {
        iced::widget::row![
            container(self.readout(label, state).wrapping(text::Wrapping::None))
                .width(label_width)
                .clip(true),
            content.into()
        ]
        .spacing(crate::style::detail::LABEL_GAP)
        .align_y(iced::Alignment::Center)
        .height(INLINE_ROW_HEIGHT)
        .width(Length::Fill)
    }

    /// Help and focus, the last two steps of every constructor.
    fn finish<'e, K, M: ControlMessage<Key> + 'e>(
        &self,
        control: &Control<K, Key>,
        element: impl Into<Element<'e, M>>,
    ) -> Element<'e, M> {
        let element = self.help(element, control.identity.clone());
        self.focus.wrap(
            control.id,
            control.state.enabled,
            control.state.invalid,
            element,
            M::focused,
        )
    }

    /// Focus for a control whose widget needs its own styling (the clip name on its colour).
    pub(crate) fn wrap<'e, M: ControlMessage<Key> + 'e>(
        &self,
        id: ControlId<Key>,
        invalid: bool,
        element: impl Into<Element<'e, M>>,
    ) -> Element<'e, M> {
        self.focus.wrap(id, true, invalid, element, M::focused)
    }

    pub(crate) fn toggle<'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<Toggle, Key>,
        on_press: M,
    ) -> Element<'e, M> {
        let compact = control.role.is_compact();
        let looks = Looks::of(&control.kind, &self.theme, control.state);
        let mut widget = button(toggle_label(control.kind.label.clone(), compact))
            .width(control.role.width())
            .height(control.role.height().unwrap_or(CONTROL_HEIGHT))
            .padding(if compact { [0, 0] } else { [2, 4] })
            .style(toggle_style(looks));
        if control.state.enabled {
            widget = widget.on_press(on_press.clone());
        }
        let widget = self.help(widget, control.identity.clone());
        self.focus.wrap_enter(
            control.id,
            control.state.enabled,
            control.state.invalid,
            widget,
            M::focused,
            on_press,
        )
    }

    /// A round, unlabelled on/off switch (a device's Activator), sized by its kind's role.
    /// Enter does not press it, and it has no hover look.
    pub(crate) fn activator<'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<Activator, Key>,
        on_press: M,
    ) -> Element<'e, M> {
        let size = match control.role {
            super::SizeRole::Fixed { width, .. } => width,
            _ => crate::style::detail::DEVICE_ACTIVATOR_SIZE,
        };
        let looks = Looks::of(&control.kind, &self.theme, control.state);
        let mut widget = button(Space::new().width(size).height(size))
            .width(size)
            .height(size)
            .padding(0)
            .style(activator_style(looks, size));
        if control.state.enabled {
            widget = widget.on_press(on_press);
        }
        let element = self.help(widget, control.identity.clone());
        // The focus stroke follows the circle, as the button's own frame does.
        self.focus.wrap_as(
            control.id,
            control.state.enabled,
            control.state.invalid,
            crate::control_focus::Outline::Round,
            element,
            M::focused,
        )
    }

    pub(crate) fn button<'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<Button, Key>,
        on_press: M,
    ) -> Element<'e, M> {
        let looks = Looks::of(&control.kind, &self.theme, control.state);
        let (content, padding): (Element<'e, M>, [u16; 2]) = match &control.kind.face {
            Face::Text(label) => (
                text(label.clone()).size(TEXT_SIZE).font(VALUE_FONT).into(),
                [2, 8],
            ),
            Face::Link(linked) => (
                iced::widget::row![
                    Canvas::new(GlyphCanvas {
                        glyph: if *linked {
                            Glyph::Linked
                        } else {
                            Glyph::Unlinked
                        },
                        ink: looks.idle.ink
                    })
                    .width(Glyph::SIZE)
                    .height(Glyph::SIZE),
                    text(if *linked { "Linked" } else { "Unlinked" }).size(TEXT_SIZE)
                ]
                .spacing(4)
                .align_y(alignment::Vertical::Center)
                .into(),
                [2, 6],
            ),
            Face::Glyph(glyph) => (
                Canvas::new(GlyphCanvas {
                    glyph: *glyph,
                    ink: looks.idle.ink,
                })
                .width(Length::Fixed(Glyph::SIZE))
                .height(Length::Fixed(Glyph::SIZE))
                .into(),
                [3, 10],
            ),
        };
        let mut widget = button(content)
            .width(control.role.width())
            .padding(padding)
            .style(flat_style(looks));
        if control.state.enabled {
            widget = widget.on_press(on_press);
        }
        self.finish(&control, widget)
    }

    /// One focus target for a musical time, using the numeric backend for drafts/IME.
    /// The descriptor supplies help; the kind knows nothing of the Info View.
    pub(crate) fn time_field<'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<super::TimeField, Key>,
        value: &str,
        on_input: impl Fn(String) -> M + 'e,
        on_submit: M,
    ) -> Element<'e, M> {
        let number = Number {
            time_field: Some(control.kind),
            ..Number::new("")
        };
        self.number(
            Control {
                id: control.id,
                identity: control.identity,
                kind: number,
                role: control.role,
                state: control.state,
                input_id: control.input_id,
                default: None,
                nudge: None,
            },
            value,
            on_input,
            on_submit,
            None,
        )
    }

    pub(crate) fn number<'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<Number, Key>,
        value: &str,
        on_input: impl Fn(String) -> M + 'e,
        on_submit: M,
        on_reset: Option<M>,
    ) -> Element<'e, M> {
        let looks = Looks::of(&control.kind, &self.theme, control.state);
        let on_input = std::rc::Rc::new(on_input);
        let changed = on_input.clone();
        let drafted = on_input.clone();
        let reset_input = move |text: String| drafted(text);
        let mut widget = field(looks, control.kind.placeholder, value).width(control.role.width());
        if control.state.enabled {
            widget = widget
                .on_input(move |s| changed(s))
                .on_submit(on_submit)
                .on_enter(M::field_entered());
        }
        let input_id = control
            .input_id
            .clone()
            .or_else(|| control.id.field_key().map(|k| k.input_id()));
        if let Some(id) = input_id.clone() {
            widget = widget.id(id);
        }
        widget = if control.role.is_compact() {
            widget
                .size(COMPACT_TEXT_SIZE)
                .padding([2, 3])
                .align_x(alignment::Horizontal::Center)
        } else {
            widget.align_x(alignment::Horizontal::Right)
        };
        let number = super::number_input::NumberInput::new(
            widget,
            value,
            control.kind.placeholder,
            looks,
            control.kind.surface,
            if control.role.is_compact() {
                COMPACT_TEXT_SIZE
            } else {
                TEXT_SIZE
            },
        )
        .enabled(control.state.enabled)
        .nudge(
            control.nudge.unwrap_or(super::Nudge::FLOAT),
            move |s| on_input(s),
            M::commit_number(),
        )
        .adjust_text(control.kind.adjust_text.clone())
        .time_field(control.kind.time_field, input_id);
        let widget = Element::new(if control.role.is_compact() {
            number.centered()
        } else {
            number
        });
        // A double click returns the field to its default. It is checked before the text input
        // sees the press, so it replaces the word selection a double click would make.
        // The caller's own reset message wins; otherwise a declared default is typed into the
        // draft and committed, as a drag step is.
        let on_reset: Option<Vec<M>> = on_reset.map(|m| vec![m]).or_else(|| {
            control.default.map(|default| {
                vec![
                    reset_input(qtable_engine::model::format_float(default, Some(4))),
                    M::commit_number(),
                ]
            })
        });
        let widget = match on_reset.filter(|_| control.state.enabled) {
            Some(on_reset) => {
                let reset = super::double_click::ResetOnDoubleClick::new(widget, on_reset);
                Element::new(reset)
            }
            None => widget,
        };
        let outline = if control.kind.time_field.is_some() {
            crate::control_focus::Outline::Corners
        } else if control.kind.surface == super::kinds::NumberSurface::Chrome {
            crate::control_focus::Outline::None
        } else {
            crate::control_focus::Outline::Box
        };
        let widget = self.help(widget, control.identity.clone());
        self.focus.wrap_as(
            control.id,
            control.state.enabled,
            control.state.invalid,
            outline,
            widget,
            M::focused,
        )
    }

    /// A numeric readout uses the same painter and surface as an editable Number,
    /// without taking focus or accepting input.
    pub(crate) fn number_readout<'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<Number, Key>,
        value: &str,
        font: iced::Font,
        size: u32,
    ) -> Element<'e, M> {
        let looks = Looks::of(&control.kind, &self.theme, control.state);
        let input = field(looks, control.kind.placeholder, value)
            .width(control.role.width())
            .size(size)
            .align_x(alignment::Horizontal::Right);
        let widget = super::number_input::NumberInput::new(
            input,
            value,
            control.kind.placeholder,
            looks,
            control.kind.surface,
            size,
        )
        .enabled(control.state.enabled)
        .read_only()
        .font(font);
        self.help(Element::new(widget), control.identity.clone())
    }

    /// A value in a recessed box that takes no focus and accepts no input. The one readout:
    /// regular roles sit on the 20 px field row in `TEXT_SIZE`; a compact role (the Session
    /// strip) uses `COMPACT_TEXT_SIZE` and tighter padding. Disabled dims the ink.
    pub(crate) fn readout_box<'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<Readout, Key>,
        value: impl Into<String>,
    ) -> Element<'e, M> {
        let look = Looks::of(&control.kind, &self.theme, control.state).idle;
        let compact = control.role.is_compact();
        let (size, padding) = if compact {
            (COMPACT_TEXT_SIZE, [2, 3])
        } else {
            (TEXT_SIZE, [2, 4])
        };
        let value = text(value.into())
            .size(size)
            .font(VALUE_FONT)
            .color(look.ink)
            .width(Length::Fill)
            .align_x(control.kind.align)
            .wrapping(text::Wrapping::None);
        let mut face = container(value)
            .padding(padding)
            .width(control.role.width())
            .style(move |_| container::Style {
                background: Some(look.fill.into()),
                border: Border {
                    color: look.frame,
                    width: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            });
        if !compact {
            face = face
                .height(CONTROL_HEIGHT)
                .align_y(alignment::Vertical::Center);
        }
        self.help(face, control.identity.clone())
    }

    pub(crate) fn text<'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<Text, Key>,
        value: &str,
        on_input: impl Fn(String) -> M + 'e,
        on_submit: M,
    ) -> Element<'e, M> {
        let looks = Looks::of(&control.kind, &self.theme, control.state);
        let mut widget = field(looks, &control.kind.placeholder, value).width(control.role.width());
        if control.state.enabled {
            widget = widget
                .on_input(on_input)
                .on_submit(on_submit)
                .on_enter(M::field_entered());
        }
        if let Some(id) = control
            .input_id
            .clone()
            .or_else(|| control.id.field_key().map(|k| k.input_id()))
        {
            widget = widget.id(id);
        }
        if control.role == super::SizeRole::Name {
            let bounded = container(widget).max_width(NAME_MAX_WIDTH);
            return self.finish(&control, bounded);
        }
        self.finish(&control, widget)
    }

    /// A path field: the dim inherited prefix, the editable piece, "+N" for further
    /// destinations and an optional Absolute toggle, in one frame.
    /// `on_focus` runs when the dim part is pressed; it should focus the input and put the
    /// cursor at its start.
    pub(crate) fn path<
        'e,
        M: ControlMessage<Key> + 'e,
        S: Clone + Into<<Key::Help as HelpId>::Subject>,
    >(
        &self,
        control: Control<PathField<S>, Key>,
        value: &str,
        on_input: impl Fn(String) -> M + 'e,
        on_submit: M,
        on_focus: M,
        on_absolute: Option<M>,
    ) -> Element<'e, M> {
        let compact = control.role.is_compact();
        let size = if compact {
            COMPACT_TEXT_SIZE
        } else {
            TEXT_SIZE
        };
        let looks = Looks::of(&control.kind, &self.theme, control.state);
        let dim = self.theme.text_disabled;
        let style = field_style(looks);
        let mut input = TextField::new(control.kind.input_placeholder(), value)
            .font(VALUE_FONT)
            .size(size)
            .padding(iced::Padding {
                top: 2.0,
                right: if compact { 3.0 } else { 4.0 },
                bottom: 2.0,
                left: 0.0,
            })
            .width(Length::Fill)
            .style(move |theme, status| text_input::Style {
                background: Color::TRANSPARENT.into(),
                border: Border::default(),
                ..style(theme, status)
            });
        if control.state.enabled {
            input = input
                .on_input(on_input)
                .on_submit(on_submit)
                .on_enter(M::field_entered());
        }
        if let Some(id) = control
            .input_id
            .clone()
            .or_else(|| control.id.field_key().map(|k| k.input_id()))
        {
            input = input.id(id);
        }
        let more = (control.kind.prefix.more > 0).then(|| format!("+{}", control.kind.prefix.more));
        let reserved =
            (measure(value, iced::Pixels(size as f32)) + if compact { 3.0 } else { 4.0 } + 1.0)
                .ceil()
                .max(CURSOR_FLOOR)
                + more
                    .as_ref()
                    .map_or(0.0, |s| measure(s, iced::Pixels(size as f32)).ceil() + 6.0)
                + if control.kind.absolute.is_some() {
                    55.0
                } else {
                    0.0
                };
        let mut row = iced::widget::row![
            iced::widget::Space::new().width(4),
            Element::from(PrefixText::new(
                control.kind.prefix.segments.clone(),
                reserved,
                size,
                self.theme,
                control.state.enabled
            )),
            input,
        ]
        .align_y(iced::Alignment::Center);
        if let Some(more) = more {
            row = row.push(
                container(text(more).font(VALUE_FONT).size(size).color(dim))
                    .padding([0, 3])
                    .height(Length::Fill)
                    .align_y(iced::alignment::Vertical::Center),
            );
        }
        if let Some(on) = control.kind.absolute {
            let toggle = Toggle {
                label: "Absolute".into(),
            };
            let state = State {
                lit: if on { super::Lit::On } else { super::Lit::Off },
                invalid: false,
                ..control.state
            };
            let looks = Looks::of(&toggle, &self.theme, state);
            let mut button = button(toggle_label(toggle.label, true))
                .width(Length::Fixed(54.0))
                .height(CONTROL_HEIGHT - 2.0)
                .padding([0, 0])
                .style(toggle_style(looks));
            if let (true, Some(message)) = (control.state.enabled, on_absolute) {
                button = button.on_press(message);
            }
            let divider =
                container(self.rule(super::rule::Axis::Vertical, super::rule::Level::Light))
                    .height(CONTROL_HEIGHT - 2.0);
            row = row.push(divider).push(button);
        }
        let frame = PathFrame::new(
            row,
            looks,
            control.role.width(),
            control.state.enabled.then_some(on_focus),
            control.kind.absolute.is_some(),
        );
        let element = help::widget_with(
            Element::from(frame),
            control.identity.clone(),
            Some(control.kind.subject.clone().into()),
            M::info,
        );
        self.focus.wrap(
            control.id,
            control.state.enabled,
            control.state.invalid,
            element,
            M::focused,
        )
    }

    /// Live's view switch (the Detail View's Notes | Envelopes | MPE): equal tabs side by side
    /// with a 1 px dark rule around and between them, the shown one (lit) lifted. Each tab is a
    /// control of its own, with its own id, help and focus; Enter on a selected tab presses it.
    pub(crate) fn view_tabs<'e, M: ControlMessage<Key> + 'e>(
        &self,
        tabs: impl IntoIterator<Item = (Control<ViewTab, Key>, M)>,
    ) -> Element<'e, M> {
        let mut bar = iced::widget::row![].spacing(1);
        let mut rule = None;
        for (control, on_press) in tabs {
            let looks = Looks::of(&control.kind, &self.theme, control.state);
            rule.get_or_insert(looks.idle.frame);
            let label = text(control.kind.label.clone())
                .size(TEXT_SIZE)
                .wrapping(text::Wrapping::None)
                .align_x(alignment::Horizontal::Center)
                .width(Length::Fill);
            let mut widget = button(label)
                .width(control.role.width())
                .height(control.role.height().unwrap_or(CONTROL_HEIGHT))
                .padding([2, 4])
                .style(view_tab_style(looks));
            if control.state.enabled {
                widget = widget.on_press(on_press.clone());
            }
            let widget = self.help(widget, control.identity.clone());
            bar = bar.push(self.focus.wrap_enter(
                control.id,
                control.state.enabled,
                control.state.invalid,
                widget,
                M::focused,
                on_press,
            ));
        }
        let rule = rule.unwrap_or(self.theme.control_contrast_frame);
        container(bar)
            .padding(1)
            .style(move |_| container::Style {
                background: Some(rule.into()),
                ..Default::default()
            })
            .into()
    }

    /// The closed face of the checklist multi-select: pips, then the text and a ▾.
    pub(crate) fn multi_face<'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<MultiFace, Key>,
        on_open: M,
    ) -> Element<'e, M> {
        use super::multi_select::MAX_PIPS;
        let looks = Looks::of(&control.kind, &self.theme, control.state);
        let mut content = iced::widget::row![]
            .spacing(2)
            .align_y(iced::Alignment::Center);
        if let Some(mark) = control.kind.led.slot() {
            content = content.push(
                iced::widget::container(super::automation_mark::dot(mark, &self.theme)).padding(
                    iced::Padding {
                        right: super::automation_mark::FACE_GAP,
                        ..iced::Padding::ZERO
                    },
                ),
            );
        }
        for pip in control.kind.pips.iter().take(MAX_PIPS) {
            content = content.push(super::multi_select::pip(*pip));
        }
        let mut label = text(control.kind.text.clone())
            .size(TEXT_SIZE)
            .font(VALUE_FONT)
            .wrapping(text::Wrapping::None);
        if let Some(colour) = control.kind.text_colour {
            label = label.color(colour);
        }
        let mut widget = button(
            content
                .push(label)
                .push(iced::widget::Space::new().width(Length::Fill))
                .push(arrow_glyph()),
        )
        .width(control.role.width())
        .height(CONTROL_HEIGHT)
        .padding([2, 4])
        .style(toggle_style(looks));
        if control.state.enabled {
            widget = widget.on_press(on_open);
        }
        self.finish(&control, widget)
    }

    pub(crate) fn dropdown<'e, T, L, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<Dropdown, Key>,
        options: L,
        selected: Option<T>,
        on_select: impl Fn(T) -> M + 'e,
    ) -> Element<'e, M>
    where
        T: ToString + PartialEq + Clone + 'e,
        L: Borrow<[T]> + 'e,
    {
        let looks = Looks::of(&control.kind, &self.theme, control.state);
        if !control.state.enabled {
            // A pick list cannot be disabled, so show its closed face without the list.
            let face = selected.map(|value| value.to_string()).unwrap_or_default();
            let widget = button(
                iced::widget::row![
                    text(face).size(TEXT_SIZE).font(VALUE_FONT),
                    iced::widget::Space::new().width(Length::Fill),
                    arrow_glyph(),
                ]
                .align_y(iced::Alignment::Center),
            )
            .width(control.role.width())
            .padding([2, 4])
            .style(toggle_style(looks));
            return self.finish(&control, widget);
        }
        let widget = picker(looks, options, selected, on_select).width(control.role.width());
        self.finish(&control, widget)
    }

    /// Relative normalized volume editing, sharing the ordinary help and focus wrappers.
    pub(crate) fn fader<'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<super::Fader, Key>,
        value: f64,
        shift: bool,
        on_change: impl Fn(f64) -> M + 'e,
        on_release: impl Fn(f64) -> M + 'e,
    ) -> Element<'e, M> {
        let role = control.role;
        let (id, help, state) = (control.id, control.identity.clone(), control.state);
        let widget = Canvas::new(super::fader::host::Hosted {
            focused: self.focus.has(id),
            control,
            value,
            shift,
            theme: self.theme,
            on_change,
            on_release,
        })
        .width(role.width())
        .height(role.height().map_or(Length::Shrink, Length::Fixed));
        let widget = self.help(widget, help);
        self.focus.wrap_as(
            id,
            state.enabled,
            state.invalid,
            crate::control_focus::Outline::Corners,
            widget,
            M::focused,
        )
    }

    /// A horizontal slider with its value over it. Dragging publishes `on_change` relative to
    /// the press; release or window blur publishes `on_release` only after a value change.
    /// A click only claims focus, leaving the existing exact-value typing path available.
    pub(crate) fn slider<'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<super::Slider, Key>,
        on_change: impl Fn(f64) -> M + 'e,
        on_release: impl Fn(f64) -> M + 'e,
    ) -> Element<'e, M> {
        let role = control.role;
        let (id, help, state) = (control.id, control.identity.clone(), control.state);
        let widget = Canvas::new(super::slider::Hosted {
            focused: self.focus.has(id),
            shift: self.shift,
            control,
            theme: self.theme,
            on_change,
            on_release,
        })
        .width(role.width())
        .height(role.height().map_or(Length::Shrink, Length::Fixed));
        let widget = self.help(widget, help);
        self.focus.wrap_as(
            id,
            state.enabled,
            state.invalid,
            crate::control_focus::Outline::Corners,
            widget,
            M::focused,
        )
    }

    /// A rotary knob with its label above and value below. A vertical drag moves the held value
    /// relative to where it started: `on_change` carries each step, `on_release` the final value.
    pub(crate) fn dial<'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<super::Dial, Key>,
        on_change: impl Fn(f64) -> M + 'e,
        on_release: impl Fn(f64) -> M + 'e,
    ) -> Element<'e, M> {
        let role = control.role;
        let (id, help, state) = (control.id, control.identity.clone(), control.state);
        let widget = Canvas::new(super::dial::Hosted {
            focused: self.focus.has(id),
            shift: self.shift,
            control,
            theme: self.theme,
            on_change,
            on_release,
        })
        .width(role.width())
        .height(role.height().map_or(Length::Shrink, Length::Fixed));
        let widget = self.help(widget, help);
        self.focus.wrap_as(
            id,
            state.enabled,
            state.invalid,
            crate::control_focus::Outline::Corners,
            widget,
            M::focused,
        )
    }

    /// A canvas kind (Solo, a meter) as its own element. Without `on_press` it is a readout
    /// and takes no focus.
    pub(crate) fn canvas<'e, K: ControlKind + 'e, M: ControlMessage<Key> + 'e>(
        &self,
        control: Control<K, Key>,
        on_press: Option<M>,
    ) -> Element<'e, M> {
        let role = control.role;
        let (id, help, state) = (control.id, control.identity.clone(), control.state);
        let readout = on_press.is_none();
        let activation = on_press
            .clone()
            .filter(|_| control.kind.activates_on_enter());
        let widget = Canvas::new(Hosted {
            control,
            theme: self.theme,
            on_press,
        })
        .width(role.width())
        .height(role.height().map_or(Length::Shrink, Length::Fixed));
        let widget = self.help(widget, help);
        if readout {
            return widget;
        }
        if let Some(activate) = activation {
            self.focus.wrap_enter(
                id,
                state.enabled,
                state.invalid,
                widget,
                M::focused,
                activate,
            )
        } else {
            self.focus
                .wrap(id, state.enabled, state.invalid, widget, M::focused)
        }
    }
}
