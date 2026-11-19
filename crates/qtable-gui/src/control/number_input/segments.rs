//! Musical gestures layered on the shared numeric backend and painter.
use super::*;
use crate::control::time_field::{self, TimeField};

#[derive(Default)]
pub(super) struct Segments {
    pub identity: Option<iced::widget::Id>,
    pub segment: usize,
    pub whole: bool,
    pub pending: bool,
    pub drag: Option<Drag>,
}
pub(super) struct Drag {
    y: f32,
    initial: String,
    steps: i64,
    finger: Option<iced::touch::Finger>,
}
impl<'a, M: Clone + 'a> NumberInput<'a, M> {
    pub(in crate::control) fn time_field(
        mut self,
        field: Option<TimeField>,
        identity: Option<iced::widget::Id>,
    ) -> Self {
        self.time_field = field;
        self.time_identity = identity;
        self
    }
    pub(super) fn select_segment(&self, tree: &mut Tree) {
        let text = self.value.to_string();
        let state = tree.state.downcast_ref::<State>();
        if !state.segments.whole
            && let Some(range) = time_field::ranges(&text).get(state.segments.segment)
        {
            input_state_mut(tree).select_range(
                text_input::Value::new(&text[..range.start]).len(),
                text_input::Value::new(&text[..range.end]).len(),
            );
        } else {
            input_state_mut(tree).select_all();
        }
    }
    /// Text origin and segment geometry use the same measured paragraph as the painter.
    pub(super) fn segment_rects(&self, tree: &Tree, layout: Layout<'_>) -> Vec<Rectangle> {
        let state = tree.state.downcast_ref::<State>();
        let text = self.value.to_string();
        let bounds = layout.children().next().unwrap().bounds();
        let paragraph = state.paragraph.raw();
        let x = self.text_origin(tree, layout);
        time_field::ranges(&text)
            .into_iter()
            .map(|range| {
                let position = |i| {
                    paragraph
                        .grapheme_position(0, text_input::Value::new(&text[..i]).len())
                        .unwrap_or(Point::ORIGIN)
                        .x
                };
                Rectangle {
                    x: x + position(range.start),
                    width: position(range.end) - position(range.start),
                    ..bounds
                }
            })
            .collect()
    }
    pub(super) fn segment_event(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        inside: bool,
        shell: &mut Shell<'_, M>,
    ) -> bool {
        if self.time_field.is_none() {
            return false;
        }
        if self.segment_pointer_event(tree, event, layout, cursor, inside, shell) {
            return true;
        }
        if !self.focused(tree) {
            return false;
        }
        if self.navigate_segment(tree, event, shell) {
            return true;
        }
        if self.whole_entry_event(tree, event, shell) {
            return true;
        }
        if self.segment_caret_event(tree, event, shell) {
            return true;
        }
        let field = self.time_field.unwrap();
        if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event
            && let Some(up) = super::super::adjustment::arrow(key, *modifiers)
        {
            let state = tree.state.downcast_mut::<State>();
            if state.mode == Mode::Selected
                && !state.segments.pending
                && let Some(next) = field.step(
                    &self.value.to_string(),
                    state.segments.segment,
                    if up { 1 } else { -1 },
                )
                && next != self.value.to_string()
                && let Some(action) = &self.nudge
            {
                shell.publish((action.changed)(next));
                shell.publish(action.commit.clone());
                state.segments.whole = false;
                shell.request_redraw();
            }
            shell.capture_event();
            return true;
        }
        false
    }
    fn segment_pointer_event(
        &self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        inside: bool,
        shell: &mut Shell<'_, M>,
    ) -> bool {
        let field = self.time_field.unwrap();
        let pressed = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if inside => {
                cursor.position().map(|point| (point, None))
            }
            Event::Touch(iced::touch::Event::FingerPressed { id, position }) if inside => {
                Some((*position, Some(*id)))
            }
            _ => None,
        };
        if let Some((point, finger)) = pressed {
            if tree
                .state
                .downcast_ref::<State>()
                .segments
                .drag
                .as_ref()
                .is_some_and(|drag| drag.finger != finger)
            {
                shell.capture_event();
                return true;
            }
            let x = point.x;
            let rects = self.segment_rects(tree, layout);
            // Choose by gaps between measured segments, with padding belonging to the end segment.
            let segment = rects
                .windows(2)
                .position(|pair| x < (pair[0].x + pair[0].width + pair[1].x) / 2.0)
                .unwrap_or(rects.len().saturating_sub(1));
            let state = tree.state.downcast_mut::<State>();
            state.segments.segment = segment.min(2);
            state.segments.whole = false;
            state.segments.drag = Some(Drag {
                y: point.y,
                initial: self.value.to_string(),
                steps: 0,
                finger,
            });
            return false; // Let the backend acquire its ordinary single focus target.
        }
        let moved = match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => Some((*position, None)),
            Event::Touch(iced::touch::Event::FingerMoved { id, position }) => {
                Some((*position, Some(*id)))
            }
            _ => None,
        };
        if let Some((position, finger)) = moved {
            let state = tree.state.downcast_mut::<State>();
            if let Some(drag) = &mut state.segments.drag
                && drag.finger == finger
            {
                let steps =
                    ((drag.y - position.y) / crate::style::detail::TIME_DRAG_STEP).trunc() as i64;
                if steps != drag.steps {
                    drag.steps = steps;
                    if let Some(next) = field.step(&drag.initial, state.segments.segment, steps)
                        && next != self.value.to_string()
                        && let Some(action) = &self.nudge
                    {
                        shell.publish((action.changed)(next));
                        shell.publish(action.commit.clone());
                        state.mode = Mode::Selected;
                        state.segments.pending = false;
                        shell.request_redraw();
                    }
                }
                shell.capture_event();
                return true;
            }
        }
        let released = match event {
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => Some(None),
            Event::Touch(
                iced::touch::Event::FingerLifted { id, .. }
                | iced::touch::Event::FingerLost { id, .. },
            ) => Some(Some(*id)),
            _ => None,
        };
        let state = tree.state.downcast_mut::<State>();
        if matches!(event, Event::Window(iced::window::Event::Unfocused))
            || released.is_some_and(|finger| {
                state
                    .segments
                    .drag
                    .as_ref()
                    .is_some_and(|drag| drag.finger == finger)
            })
        {
            state.segments.drag = None;
        }
        false
    }
    fn navigate_segment(&self, tree: &mut Tree, event: &Event, shell: &mut Shell<'_, M>) -> bool {
        let field = self.time_field.unwrap();
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(key @ (Named::ArrowLeft | Named::ArrowRight)),
            ..
        }) = event
        {
            if time_field::ranges(&self.value.to_string()).len() < 3
                && let Ok(value) = field.parse(&self.value.to_string())
                && let Some(action) = &self.nudge
            {
                shell.publish((action.changed)(field.format(value)));
            }
            let state = tree.state.downcast_mut::<State>();
            state.segments.segment = if *key == Named::ArrowLeft {
                state.segments.segment.saturating_sub(1)
            } else {
                (state.segments.segment + 1).min(2)
            };
            state.segments.whole = false;
            state.mode = Mode::Selected;
            self.select_segment(tree);
            shell.capture_event();
            shell.request_redraw();
            return true;
        }
        false
    }
    /// Editing keys stay inside a segment until explicit whole-value entry begins.
    fn segment_caret_event(
        &self,
        tree: &mut Tree,
        event: &Event,
        shell: &mut Shell<'_, M>,
    ) -> bool {
        let state = tree.state.downcast_ref::<State>();
        if state.mode != Mode::Editing || state.segments.whole {
            return false;
        }
        let Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Named(key),
            modifiers,
            ..
        }) = event
        else {
            return false;
        };
        let text = self.value.to_string();
        let Some(range) = time_field::ranges(&text)
            .get(state.segments.segment)
            .cloned()
        else {
            return false;
        };
        let start = text_input::Value::new(&text[..range.start]).len();
        let end = text_input::Value::new(&text[..range.end]).len();
        let caret = input_state(tree).cursor().state(&self.value);
        let index = match caret {
            text_input::cursor::State::Index(i) => i,
            text_input::cursor::State::Selection { end, .. } => end,
        };
        if matches!(key, Named::Home | Named::End) {
            let boundary = if *key == Named::Home { start } else { end };
            if modifiers.shift() {
                input_state_mut(tree).select_range(index.clamp(start, end), boundary);
            } else {
                input_state_mut(tree).move_cursor_to(boundary);
            }
            shell.capture_event();
            shell.request_redraw();
            return true;
        }
        if let text_input::cursor::State::Index(index) = caret
            && matches!(key, Named::Backspace | Named::Delete)
        {
            let boundary = if *key == Named::Backspace { start } else { end };
            if index == boundary {
                shell.capture_event();
                return true;
            }
            if modifiers.command() || modifiers.control() || modifiers.alt() {
                input_state_mut(tree).select_range(index.clamp(start, end), boundary);
            }
        }
        false
    }
    fn whole_entry_event(&self, tree: &mut Tree, event: &Event, shell: &mut Shell<'_, M>) -> bool {
        if matches!(event, Event::Keyboard(keyboard::Event::KeyPressed { key: Key::Character(key), modifiers, .. }) if (modifiers.command() || modifiers.control()) && key.as_str() == "a")
        {
            tree.state.downcast_mut::<State>().segments.whole = true;
        }
        // Pasting always supplies a whole value, even with one or two parts.
        let pasted = matches!(event, Event::Keyboard(keyboard::Event::KeyPressed { key: Key::Character(key), modifiers, .. }) if (modifiers.command() || modifiers.control()) && key.as_str() == "v");
        let dotted = match event {
            Event::Keyboard(keyboard::Event::KeyPressed {
                text: Some(text),
                modifiers,
                ..
            }) if !modifiers.command() && !modifiers.control() && text.contains('.') => {
                Some(text.as_str())
            }
            Event::InputMethod(iced::advanced::input_method::Event::Commit(text))
                if text.contains('.') =>
            {
                Some(text.as_str())
            }
            _ => None,
        };
        if pasted {
            input_state_mut(tree).select_all();
            tree.state.downcast_mut::<State>().segments.whole = true;
        } else if dotted.is_some() && !tree.state.downcast_ref::<State>().segments.whole {
            // A dot after segment digits switches to whole-string entry ("5.2"),
            // retaining the digits just typed, not the field's untouched suffix.
            let state = tree.state.downcast_mut::<State>();
            let prefix = if state.mode == Mode::Editing && dotted == Some(".") {
                time_field::ranges(&self.value.to_string())
                    .get(state.segments.segment)
                    .map(|r| self.value.to_string()[r.clone()].to_owned())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            state.segments.whole = true;
            input_state_mut(tree).select_all();
            if !prefix.is_empty()
                && let Some(action) = &self.nudge
            {
                shell.publish((action.changed)(format!("{prefix}.")));
                input_state_mut(tree).move_cursor_to_end();
                tree.state.downcast_mut::<State>().editing();
                tree.state.downcast_mut::<State>().segments.pending = true;
                shell.capture_event();
                shell.request_redraw();
                return true;
            }
        }
        false
    }
}
