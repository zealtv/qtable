//! A horizontal slider with its value drawn over it, as Live's generic plug-in panel draws a
//! parameter. It publishes the position as a fraction of the range; the caller maps it.
use super::{Control, ControlKind, Interaction, Look, SizeRole, State, Theme};
use crate::control_focus::{ControlId, ControlKey};
use iced::widget::canvas::{self, Frame, Path, Program, Text};
use iced::{Point, Rectangle, Renderer, Size, keyboard, mouse};

/// `fraction` is the value's place in the range; `label` is the text drawn over the bar.
pub(crate) struct Slider {
    pub fraction: f64,
    pub label: String,
}

impl ControlKind for Slider {
    fn look(&self, theme: &Theme, state: State, _: Interaction) -> Look {
        Look {
            fill: theme.fader_well,
            ink: if state.enabled {
                theme.control_foreground
            } else {
                theme.text_disabled
            },
            frame: theme.fader_edge,
            // Slider Range Value, or its inactive twin for a device that is off or a control
            // that doesn't apply.
            accent: if state.enabled && !state.inactive {
                theme.value_bar
            } else {
                theme.value_arc_inactive
            },
        }
    }

    fn role(&self) -> SizeRole {
        SizeRole::Fixed {
            width: crate::style::detail::DEVICE_CONTROL_WIDTH,
            height: crate::style::detail::CONTROL_HEIGHT,
        }
    }

    fn paint(&self, frame: &mut Frame, bounds: Rectangle, look: &Look) {
        let path = Path::rounded_rectangle(
            bounds.position(),
            bounds.size(),
            super::LIVE_DARK.corner_control.into(),
        );
        frame.fill(&path, look.fill);
        let width = (bounds.width - 2.0).max(0.0) * self.fraction.clamp(0.0, 1.0) as f32;
        if width > 0.0 {
            frame.fill_rectangle(
                Point::new(bounds.x + 1.0, bounds.y + 1.0),
                Size::new(width, (bounds.height - 2.0).max(0.0)),
                look.accent,
            );
        }
        frame.stroke(
            &path,
            canvas::Stroke::default()
                .with_color(look.frame)
                .with_width(1.0),
        );
        frame.fill_text(Text {
            content: self.label.clone(),
            position: Point::new(bounds.x + bounds.width - 4.0, bounds.center_y()),
            color: look.ink,
            size: (crate::style::detail::TEXT_SIZE as f32).into(),
            font: crate::style::VALUE_FONT,
            align_x: iced::alignment::Horizontal::Right.into(),
            align_y: iced::alignment::Vertical::Center,
            ..Text::default()
        });
    }
}

/// A horizontal drag changes the held value relative to the press, without sending on
/// a click. Release (or window blur) publishes the final value only after movement.
pub(super) struct Hosted<F, G, Key: ControlKey> {
    pub control: Control<Slider, Key>,
    pub theme: Theme,
    pub focused: bool,
    pub shift: bool,
    pub on_change: F,
    pub on_release: G,
}

pub(super) struct Drag<Key> {
    id: Option<ControlId<Key>>,
    dragging: Option<Movement>,
    click: super::double_click::Presses,
    modifiers: keyboard::Modifiers,
    keyboard: super::adjustment::Keyboard,
}

impl<Key> Default for Drag<Key> {
    fn default() -> Self {
        Self {
            id: None,
            dragging: None,
            click: Default::default(),
            modifiers: keyboard::Modifiers::NONE,
            keyboard: Default::default(),
        }
    }
}

struct Movement {
    x: f32,
    width: f32,
    origin: f64,
    fraction: f64,
    changed: bool,
    initial: f64,
}

impl Movement {
    fn move_to(&mut self, x: f32, fine: bool) -> bool {
        if self.width <= 0.0 {
            return false;
        }
        let delta = f64::from((x - self.x) / self.width)
            * if fine {
                super::adjustment::FINE_FACTOR
            } else {
                1.0
            };
        self.x = x;
        self.origin = (self.origin + delta).clamp(0.0, 1.0);
        let fraction = if self.origin <= 0.0 || self.origin >= 1.0 {
            self.origin
        } else {
            (self.initial + ((self.origin - self.initial) * 1000.0).round() / 1000.0)
                .clamp(0.0, 1.0)
        };
        if self.fraction == fraction {
            return false;
        }
        self.fraction = fraction;
        self.changed = true;
        true
    }
}

impl<M, F: Fn(f64) -> M, G: Fn(f64) -> M, Key: ControlKey> Program<M> for Hosted<F, G, Key> {
    type State = Drag<Key>;

    fn update(
        &self,
        state: &mut Drag<Key>,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<M>> {
        if state.id != Some(self.control.id) {
            *state = Drag::default();
            state.id = Some(self.control.id);
        }
        if !self.control.state.enabled {
            state.dragging = None;
            state.keyboard = Default::default();
            return None;
        }
        if let Some((adjustment, capture)) = state.keyboard.update(
            event,
            self.focused,
            state.dragging.is_some(),
            self.control.kind.fraction,
            self.control.nudge.unwrap_or(super::Nudge::NORMALIZED),
        ) {
            return Some(adjustment.action(&self.on_change, &self.on_release, capture));
        }
        match event {
            canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                let position = cursor.position()?;
                if let Some(default) = self.control.default
                    && state.click.register(position)
                {
                    state.dragging = None;
                    return Some(canvas::Action::publish((self.on_release)(default)).and_capture());
                }
                state.modifiers = if self.shift {
                    keyboard::Modifiers::SHIFT
                } else {
                    keyboard::Modifiers::NONE
                };
                let fraction = self.control.kind.fraction;
                state.dragging = Some(Movement {
                    x: position.x,
                    width: bounds.width,
                    origin: fraction,
                    fraction,
                    changed: false,
                    initial: fraction,
                });
                Some(canvas::Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging.is_some() => {
                // Scrollables translate the cursor, but leave the event's position in
                // window coordinates. Use the same coordinate system as the press, also
                // while the pointer is outside the scroll area's viewport.
                let position = cursor.land().position()?;
                let drag = state.dragging.as_mut()?;
                if !drag.move_to(position.x, state.modifiers.shift()) {
                    return Some(canvas::Action::capture());
                }
                Some(canvas::Action::publish((self.on_change)(drag.fraction)).and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.dragging.is_some() =>
            {
                let drag = state.dragging.take()?;
                Some(if drag.changed {
                    canvas::Action::publish((self.on_release)(drag.fraction)).and_capture()
                } else {
                    canvas::Action::capture()
                })
            }
            canvas::Event::Window(iced::window::Event::Unfocused) => {
                state.modifiers = keyboard::Modifiers::NONE;
                let drag = state.dragging.take()?;
                drag.changed
                    .then(|| canvas::Action::publish((self.on_release)(drag.fraction)))
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Drag<Key>,
        renderer: &Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let look = self.control.kind.look(
            &self.theme,
            self.control.state,
            super::canvas::interaction(cursor, bounds, state.dragging.is_some()),
        );
        self.control
            .kind
            .paint(&mut frame, Rectangle::with_size(bounds.size()), &look);
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests;
