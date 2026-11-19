//! Relative vertical drag for a plain Number field, as Live's number boxes behave like
//! sliders without the slider face: press, then drag up to increase and down to decrease,
//! Shift for finer. Nothing is sent by the press itself, and a click that never leaves the
//! dead zone stays an ordinary click that selects the value for typing.
//!
//! One step is one keyboard nudge (`Nudge::next`, or the field's own text adjustment), so
//! the keyboard and the mouse move a field by the same amounts and Shift is the same 4x finer.
use super::*;
use crate::style::detail::{NUMBER_DRAG_DEAD_ZONE, NUMBER_DRAG_STEP};

/// A pointer press on a Number field, from its press to its release.
pub(super) struct ValueDrag {
    /// Pointer height at which the net applied steps read as zero.
    anchor: f32,
    /// The text after the last published step; the press text until the first one.
    current: String,
    /// Net steps published since the drag left its dead zone.
    steps: i64,
    started: bool,
    finger: Option<iced::touch::Finger>,
}

impl<'a, M: Clone + 'a> NumberInput<'a, M> {
    /// One nudge of `text`: the field's text adjustment, else its numeric axis.
    pub(super) fn adjusted(&self, text: &str, up: bool, fine: bool) -> Option<String> {
        let nudge = self.nudge.as_ref()?;
        if let Some(adjust) = &self.adjust_text {
            return adjust(text, up, fine);
        }
        text.trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .and_then(|value| {
                let next = nudge.axis.next(value, up, fine);
                (next != value).then(|| next.to_string())
            })
    }

    /// Returns true when the event belongs to a drag in progress and goes no further.
    pub(super) fn value_drag_event(
        &self,
        tree: &mut Tree,
        event: &Event,
        cursor: mouse::Cursor,
        inside: bool,
        shell: &mut Shell<'_, M>,
    ) -> bool {
        let state = tree.state.downcast_mut::<State>();
        if let Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
        }
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
            if state
                .drag
                .as_ref()
                .is_some_and(|drag| drag.finger != finger)
            {
                shell.capture_event();
                return true;
            }
            state.drag = Some(ValueDrag {
                anchor: point.y,
                current: self.value.to_string(),
                steps: 0,
                started: false,
                finger,
            });
            return false; // The backend still takes focus; a bare click selects the value.
        }
        let moved = match event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => Some((*position, None)),
            Event::Touch(iced::touch::Event::FingerMoved { id, position }) => {
                Some((*position, Some(*id)))
            }
            _ => None,
        };
        if let Some((position, finger)) = moved
            && let Some(drag) = &mut state.drag
            && drag.finger == finger
        {
            let height = drag.anchor - position.y;
            if !drag.started && height.abs() < NUMBER_DRAG_DEAD_ZONE {
                return false;
            }
            drag.started = true;
            let fine = state.modifiers.shift();
            let wanted = (height / NUMBER_DRAG_STEP).trunc() as i64;
            let up = wanted > drag.steps;
            let mut applied = drag.steps;
            let mut text = drag.current.clone();
            while applied != wanted {
                match self.adjusted(&text, up, fine) {
                    Some(next) if next != text => {
                        text = next;
                        applied += if up { 1 } else { -1 };
                    }
                    _ => break, // At a limit: stop here and don't wind up past it.
                }
            }
            if applied != wanted {
                drag.anchor = position.y + applied as f32 * NUMBER_DRAG_STEP;
            }
            drag.steps = applied;
            if text != drag.current {
                drag.current = text.clone();
                if let Some(action) = &self.nudge {
                    shell.publish((action.changed)(text));
                    shell.publish(action.commit.clone());
                }
                state.mode = Mode::Selected;
            }
            shell.request_redraw();
            shell.capture_event();
            return true;
        }
        let released = matches!(
            event,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | Event::Touch(
                    iced::touch::Event::FingerLifted { .. } | iced::touch::Event::FingerLost { .. }
                )
                | Event::Window(iced::window::Event::Unfocused)
        );
        if released {
            state.drag = None;
        }
        false
    }
}
