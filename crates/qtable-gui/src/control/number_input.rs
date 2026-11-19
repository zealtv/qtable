//! Numeric selection is a control state, not a text selection. iced supplies keyboard
//! editing and focus operations; this adapter owns the numeric face and typing caret.
mod drag;
mod segments;
use super::Look;
use super::kinds::NumberSurface;
use super::text_field::TextField;
use super::widgets::Looks;
use crate::style::VALUE_FONT;
use iced::advanced::text::{self, Paragraph as _};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, renderer};
use iced::keyboard::{self, Key, key::Named};
use iced::widget::text_input;
use iced::{
    Border, Color, Event, Font, Length, Pixels, Point, Rectangle, Renderer, Size, Theme, alignment,
    mouse,
};
use std::time::{Duration, Instant};

type InputState = text_input::State<<Renderer as text::Renderer>::Paragraph>;
type Paragraph = <Renderer as text::Renderer>::Paragraph;
const CARET_HALF_PERIOD: Duration = Duration::from_millis(500);
const CARET_FRAME_INTERVAL: Duration = Duration::from_millis(16);
/// Keep scrolling aligned with the editing backend's caret/IME margin.
const CARET_MARGIN: f32 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Selected,
    Editing,
}

struct State {
    mode: Mode,
    segments: segments::Segments,
    drag: Option<drag::ValueDrag>,
    modifiers: keyboard::Modifiers,
    pressed: bool,
    window_active: bool,
    epoch: Instant,
    now: Instant,
    paragraph: text::paragraph::Plain<Paragraph>,
}
impl Default for State {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            mode: Mode::Selected,
            segments: Default::default(),
            drag: None,
            modifiers: keyboard::Modifiers::NONE,
            pressed: false,
            window_active: true,
            epoch: now,
            now,
            paragraph: Default::default(),
        }
    }
}
impl State {
    fn editing(&mut self) {
        self.mode = Mode::Editing;
        self.epoch = Instant::now();
        self.now = self.epoch;
    }
    fn caret_color(&self, looks: Looks) -> Color {
        let phase = self.now.saturating_duration_since(self.epoch).as_secs_f64()
            / CARET_HALF_PERIOD.as_secs_f64();
        // Ease between the theme endpoints: white -> red in 0.5 s, back in 0.5 s.
        let amount = ((1.0 - (phase * std::f64::consts::PI).cos()) * 0.5) as f32;
        let white = looks.theme.numeric_cursor;
        let red = looks.theme.chosen_record;
        Color::from_rgba(
            white.r * (1.0 - amount) + red.r * amount,
            white.g * (1.0 - amount) + red.g * amount,
            white.b * (1.0 - amount) + red.b * amount,
            white.a * (1.0 - amount) + red.a * amount,
        )
    }
}

struct NudgeAction<'a, M> {
    axis: super::Nudge,
    changed: Box<dyn Fn(String) -> M + 'a>,
    commit: M,
}

pub(super) struct NumberInput<'a, M> {
    input: TextField<'a, M>,
    value: text_input::Value,
    placeholder: &'static str,
    looks: Looks,
    surface: NumberSurface,
    size: Pixels,
    alignment: alignment::Horizontal,
    font: Font,
    enabled: bool,
    editable: bool,
    adjust_text: Option<super::adjustment::TextAdjustment>,
    nudge: Option<NudgeAction<'a, M>>,
    time_field: Option<super::TimeField>,
    time_identity: Option<iced::widget::Id>,
}
impl<'a, M: Clone + 'a> NumberInput<'a, M> {
    pub(super) fn new(
        input: TextField<'a, M>,
        value: &str,
        placeholder: &'static str,
        looks: Looks,
        surface: NumberSurface,
        size: u32,
    ) -> Self {
        Self {
            input,
            value: text_input::Value::new(value),
            placeholder,
            looks,
            surface,
            size: Pixels(size as f32),
            alignment: alignment::Horizontal::Right,
            font: VALUE_FONT,
            enabled: true,
            editable: true,
            adjust_text: None,
            nudge: None,
            time_field: None,
            time_identity: None,
        }
    }
    pub(super) fn adjust_text(mut self, adjust: Option<super::adjustment::TextAdjustment>) -> Self {
        self.adjust_text = adjust;
        self
    }
    pub(super) fn nudge(
        mut self,
        nudge: super::Nudge,
        changed: impl Fn(String) -> M + 'a,
        commit: M,
    ) -> Self {
        self.nudge = Some(NudgeAction {
            axis: nudge,
            changed: Box::new(changed),
            commit,
        });
        self
    }
    pub(super) fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
    pub(super) fn centered(mut self) -> Self {
        self.alignment = alignment::Horizontal::Center;
        self
    }
    pub(super) fn read_only(mut self) -> Self {
        self.editable = false;
        self
    }
    pub(super) fn font(mut self, font: Font) -> Self {
        self.font = font;
        self.input = self.input.font(font);
        self
    }
    fn text_origin(&self, tree: &Tree, layout: Layout<'_>) -> f32 {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.children().next().unwrap().bounds();
        let paragraph = state.paragraph.raw();
        let editing = self.focused(tree) && state.mode == Mode::Editing && state.window_active;
        let available = bounds.width - if editing { 1.0 } else { 0.0 };
        let width = paragraph.min_bounds().width;
        let alignment = if width > available {
            0.0
        } else {
            match self.alignment {
                alignment::Horizontal::Left => 0.0,
                alignment::Horizontal::Center => (available - width) / 2.0,
                alignment::Horizontal::Right => available - width,
            }
        };
        let end = match input_state(tree).cursor().state(&self.value) {
            text_input::cursor::State::Index(i) => i,
            text_input::cursor::State::Selection { end, .. } => end,
        };
        let scroll = if editing {
            (paragraph
                .grapheme_position(0, end)
                .unwrap_or(Point::ORIGIN)
                .x
                + CARET_MARGIN
                - bounds.width)
                .max(0.0)
        } else if self.time_field.is_some() && self.focused(tree) && !state.segments.whole {
            // A selected segment must remain visible even when exact fractions or many
            // bars make the value wider than a compact region field.
            super::time_field::ranges(&self.value.to_string())
                .get(state.segments.segment)
                .map(|range| {
                    let end = text_input::Value::new(&self.value.to_string()[..range.end]).len();
                    (paragraph
                        .grapheme_position(0, end)
                        .unwrap_or(Point::ORIGIN)
                        .x
                        - bounds.width)
                        .max(0.0)
                })
                .unwrap_or(0.0)
        } else {
            0.0
        };
        bounds.x + alignment - scroll
    }
    fn focused(&self, tree: &Tree) -> bool {
        self.enabled && self.editable && input_state(tree).is_focused()
    }
    fn look(&self, state: &State, focused: bool, hovered: bool) -> Look {
        let mut look = if !self.enabled {
            self.looks.disabled
        } else if state.pressed || focused {
            self.looks.pressed
        } else if hovered {
            self.looks.hovered
        } else {
            self.looks.idle
        };
        if self.surface == NumberSurface::Chrome && focused && !state.pressed {
            look.fill = self.looks.theme.transport_selection_background;
            if look.frame != self.looks.theme.chosen_alert {
                look.frame = look.fill;
            }
        }
        look
    }
}
fn input_state(tree: &Tree) -> &InputState {
    tree.children[0].state.downcast_ref::<InputState>()
}
fn input_state_mut(tree: &mut Tree) -> &mut InputState {
    tree.children[0].state.downcast_mut::<InputState>()
}

fn starts_typing(event: &Event) -> bool {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(Named::Backspace | Named::Delete),
            ..
        }) => true,
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Character(key),
            text,
            modifiers,
            ..
        }) => {
            if modifiers.command() || modifiers.control() {
                matches!(key.as_str(), "v" | "x")
            } else {
                text.as_ref().is_some_and(|text| !text.is_empty())
            }
        }
        Event::InputMethod(iced::advanced::input_method::Event::Commit(text)) => !text.is_empty(),
        _ => false,
    }
}

impl<'a, M: Clone + 'a> Widget<M, Theme, Renderer> for NumberInput<'a, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.input as &dyn Widget<M, Theme, Renderer>)]
    }
    fn diff(&self, tree: &mut Tree) {
        if self.time_field.is_none() {
            tree.state.downcast_mut::<State>().segments = Default::default();
        } else {
            let state = tree.state.downcast_mut::<State>();
            if state.segments.identity != self.time_identity {
                state.segments = segments::Segments {
                    identity: self.time_identity.clone(),
                    ..Default::default()
                };
                state.mode = Mode::Selected;
                state.pressed = false;
            }
        }
        tree.children[0].diff(&self.input as &dyn Widget<M, Theme, Renderer>);
    }
    fn size(&self) -> Size<Length> {
        Widget::size(&self.input)
    }
    fn size_hint(&self) -> Size<Length> {
        self.input.size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let node = self
            .input
            .layout_value(&mut tree.children[0], renderer, limits, None);
        let text_bounds = Layout::new(&node).children().next().unwrap().bounds();
        let content = if self.value.is_empty() {
            self.placeholder.to_owned()
        } else {
            self.value.to_string()
        };
        tree.state
            .downcast_mut::<State>()
            .paragraph
            .update(text::Text {
                content: &content,
                bounds: Size::new(f32::INFINITY, text_bounds.height),
                size: self.size,
                line_height: text::LineHeight::default(),
                font: self.font,
                align_x: text::Alignment::Default,
                align_y: alignment::Vertical::Center,
                shaping: text::Shaping::Advanced,
                wrapping: text::Wrapping::default(),
            });
        node
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        if !self.enabled || !self.editable {
            return;
        }
        let focused = self.focused(tree);
        let before = input_state(tree).cursor();
        self.input
            .operate(&mut tree.children[0], layout, renderer, operation);
        if self.focused(tree) && !focused {
            input_state_mut(tree).select_all();
            tree.state.downcast_mut::<State>().mode = Mode::Selected;
            tree.state.downcast_mut::<State>().segments.whole = false;
            tree.state.downcast_mut::<State>().segments.pending = false;
            if self.time_field.is_some() {
                self.select_segment(tree);
            }
        } else if focused && before != input_state(tree).cursor() {
            // The exact Slider/Dial draft already contains its opening character. Its
            // explicit cursor operation starts editing; focus probes leave selection alone.
            tree.state.downcast_mut::<State>().editing();
        }
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        if !self.enabled {
            let state = tree.state.downcast_mut::<State>();
            state.segments.drag = None;
            state.pressed = false;
            return;
        }
        let point = match event {
            Event::Touch(
                iced::touch::Event::FingerPressed { position, .. }
                | iced::touch::Event::FingerMoved { position, .. }
                | iced::touch::Event::FingerLifted { position, .. }
                | iced::touch::Event::FingerLost { position, .. },
            ) => Some(*position),
            _ => cursor.position(),
        };
        let inside = point.is_some_and(|p| layout.bounds().contains(p) && viewport.contains(p));
        if !self.editable {
            // Readouts share hover/press presentation without entering the focus chain.
            let state = tree.state.downcast_mut::<State>();
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if inside => {
                    state.pressed = true
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | Event::Window(iced::window::Event::Unfocused) => state.pressed = false,
                _ => {}
            }
            if matches!(
                event,
                Event::Mouse(_) | Event::Window(iced::window::Event::Unfocused)
            ) {
                shell.request_redraw();
            }
            return;
        }
        if self.segment_event(tree, event, layout, cursor, inside, shell) {
            return;
        }
        if self.time_field.is_none() && self.value_drag_event(tree, event, cursor, inside, shell) {
            return;
        }
        let focused = self.focused(tree);
        let selected = tree.state.downcast_ref::<State>().mode == Mode::Selected;
        let press = matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                | Event::Touch(iced::touch::Event::FingerPressed { .. })
        ) && inside;
        if focused
            && selected
            && matches!(
                event,
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Named(
                        Named::ArrowLeft | Named::ArrowRight | Named::Home | Named::End
                    ),
                    ..
                })
            )
        {
            // Live check: navigation alone leaves the whole numeric control selected.
            shell.capture_event();
            return;
        }
        if focused
            && let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event
            && let Some(up) = super::adjustment::arrow(key, *modifiers)
            && let Some(NudgeAction {
                changed, commit, ..
            }) = &self.nudge
        {
            if !selected {
                shell.capture_event();
                return;
            }
            let text = self.value.to_string();
            let adjusted = self.adjusted(&text, up, modifiers.shift());
            if let Some(next) = adjusted
                && next != text
            {
                shell.publish(changed(next));
                shell.publish(commit.clone());
                input_state_mut(tree).select_all();
                tree.state.downcast_mut::<State>().mode = Mode::Selected;
                shell.request_redraw();
            }
            shell.capture_event();
            return;
        }
        if focused && selected && starts_typing(event) && self.time_field.is_some() {
            self.select_segment(tree);
        }
        if focused && starts_typing(event) {
            tree.state.downcast_mut::<State>().editing();
            if self.time_field.is_some() {
                tree.state.downcast_mut::<State>().segments.pending = true;
            }
        }
        self.input.update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if press {
            // End the backend's drag immediately. Tiny native pointer motion between down
            // and up must never turn control selection into a partial text selection.
            self.input.update(
                &mut tree.children[0],
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
            input_state_mut(tree).select_all();
            if self.time_field.is_some() {
                self.select_segment(tree);
            }
            let state = tree.state.downcast_mut::<State>();
            state.mode = Mode::Selected;
            state.pressed = true;
            shell.request_redraw();
        }
        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | Event::Touch(
                    iced::touch::Event::FingerLifted { .. } | iced::touch::Event::FingerLost { .. }
                )
        ) {
            tree.state.downcast_mut::<State>().pressed = false;
            shell.request_redraw();
        }
        if focused
            && matches!(
                event,
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Named(Named::Enter | Named::Escape),
                    ..
                })
            )
        {
            // iced's Escape cleanup blurs a text input. Numeric cancel instead keeps the
            // control selected; the owning panel still receives CancelField and restores
            // its committed value. The next character replaces that restored value.
            input_state_mut(tree).focus();
            input_state_mut(tree).select_all();
            tree.state.downcast_mut::<State>().mode = Mode::Selected;
            tree.state.downcast_mut::<State>().segments.whole = false;
            tree.state.downcast_mut::<State>().segments.pending = false;
            if self.time_field.is_some() {
                self.select_segment(tree);
            }
            shell.request_redraw();
        }
        if !self.focused(tree) {
            let state = tree.state.downcast_mut::<State>();
            state.mode = Mode::Selected;
            state.pressed = false;
            state.segments.pending = false;
        }
        if matches!(event, Event::Window(iced::window::Event::Unfocused)) {
            input_state_mut(tree).select_all();
            tree.state.downcast_mut::<State>().mode = Mode::Selected;
            tree.state.downcast_mut::<State>().segments.whole = false;
            tree.state.downcast_mut::<State>().segments.pending = false;
            if self.time_field.is_some() {
                self.select_segment(tree);
            }
        }
        let still_focused = input_state(tree).is_focused();
        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Window(iced::window::Event::Unfocused) => {
                state.window_active = false;
                state.pressed = false;
            }
            Event::Window(iced::window::Event::Focused) => state.window_active = true,
            Event::Window(iced::window::Event::RedrawRequested(now)) => state.now = *now,
            _ => {}
        }
        if still_focused && state.mode == Mode::Editing && state.window_active {
            shell.request_redraw_at(state.now + CARET_FRAME_INTERVAL);
        }
    }
    fn mouse_interaction(
        &self,
        _: &Tree,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _: &Rectangle,
        _: &Renderer,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::{Renderer as _, text::Renderer as _};
        let state = tree.state.downcast_ref::<State>();
        let focused = self.focused(tree);
        let look = self.look(state, focused, cursor.is_over(layout.bounds()));
        renderer.fill_quad(
            renderer::Quad {
                bounds: layout.bounds(),
                border: Border {
                    color: look.frame,
                    width: 1.0,
                    radius: self.looks.theme.corner_control.into(),
                },
                ..Default::default()
            },
            look.fill,
        );
        let text_bounds = layout.children().next().unwrap().bounds();
        let Some(clip) = text_bounds.intersection(viewport) else {
            return;
        };
        let editing = focused && state.mode == Mode::Editing && state.window_active;
        let caret = input_state(tree).cursor().state(&self.value);
        let paragraph = state.paragraph.raw();
        let position = |index| {
            paragraph
                .grapheme_position(0, index)
                .unwrap_or(Point::ORIGIN)
                .x
        };
        let x = self.text_origin(tree, layout);
        renderer.with_layer(clip, |renderer| {
            if focused
                && self.time_field.is_some()
                && !state.segments.whole
                && let Some(bounds) = self.segment_rects(tree, layout).get(state.segments.segment)
            {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: *bounds,
                        ..Default::default()
                    },
                    self.looks.theme.surface_highlight,
                );
            }
            if editing && let text_input::cursor::State::Selection { start, end } = caret {
                let left = position(start.min(end));
                let right = position(start.max(end));
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: x + left,
                            y: text_bounds.y,
                            width: right - left,
                            height: text_bounds.height,
                        },
                        ..Default::default()
                    },
                    Color {
                        a: 0.4,
                        ..self.looks.theme.selection_background
                    },
                );
            }
            renderer.fill_paragraph(
                paragraph,
                Rectangle { x, ..text_bounds }.anchor(
                    paragraph.min_bounds(),
                    paragraph.align_x(),
                    alignment::Vertical::Center,
                ),
                if self.value.is_empty() {
                    self.looks.theme.text_disabled
                } else {
                    look.ink
                },
                clip,
            );
            if editing && let text_input::cursor::State::Index(index) = caret {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: (x + position(index)).floor(),
                            y: text_bounds.y,
                            width: 1.0,
                            height: text_bounds.height,
                        },
                        ..Default::default()
                    },
                    state.caret_color(self.looks),
                );
            }
        });
    }
}

#[cfg(test)]
mod tests;
