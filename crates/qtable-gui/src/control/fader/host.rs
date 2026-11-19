//! A relative drag host. Pointer motion edits the model; the painter only reads it.
use super::{Fader, Geometry, unit};
use crate::control::{Control, ControlKind, Theme};
use crate::control_focus::{ControlId, ControlKey};
use iced::widget::canvas::{self, Frame, Program};
use iced::{Rectangle, Renderer, keyboard, mouse};

/// `on_change` carries each step of a drag. `on_release` carries the final value once, when the
/// drag ends or the window loses focus, so the receiver decides whether steps took effect.
pub(crate) struct Hosted<F, G, Key: ControlKey> {
    pub control: Control<Fader, Key>,
    pub value: f64,
    pub focused: bool,
    /// Shift as the app last saw it, for a drag that starts before any `ModifiersChanged`.
    pub shift: bool,
    pub theme: Theme,
    pub on_change: F,
    pub on_release: G,
}

pub(crate) struct State<Key> {
    id: Option<ControlId<Key>>,
    modifiers: keyboard::Modifiers,
    drag: Option<Drag>,
    click: crate::control::double_click::Presses,
    keyboard: crate::control::adjustment::Keyboard,
}

impl<Key> Default for State<Key> {
    fn default() -> Self {
        Self {
            id: None,
            modifiers: keyboard::Modifiers::default(),
            drag: None,
            click: Default::default(),
            keyboard: Default::default(),
        }
    }
}
#[derive(Clone, Copy)]
struct Drag {
    /// The last raw pointer y; unset until the first move, so press and move share one frame.
    y: Option<f32>,
    value: f64,
    published: f64,
}

/// What a drag publishes: the ends exactly, anything between rounded to 0.001.
fn published_value(value: f64) -> f64 {
    if value <= 0.0 || value >= 1.0 {
        unit(value)
    } else {
        (value * 1000.0).round() / 1000.0
    }
}

/// Publish a changed value when it moved a thousandth, or when it reached an end.
fn worth_publishing(next: f64, published: f64) -> bool {
    next != published && ((next - published).abs() >= 0.001 || next <= 0.0 || next >= 1.0)
}

fn drag_value(value: f64, dy: f32, travel: f32, fine: bool) -> f64 {
    if travel <= 0.0 || !travel.is_finite() || !dy.is_finite() {
        return unit(value);
    }
    unit(
        value
            - f64::from(dy / travel)
                * if fine {
                    crate::control::adjustment::FINE_FACTOR
                } else {
                    1.0
                },
    )
}

impl<F, G, M, Key: ControlKey> Program<M> for Hosted<F, G, Key>
where
    F: Fn(f64) -> M,
    G: Fn(f64) -> M,
{
    type State = State<Key>;

    fn update(
        &self,
        state: &mut State<Key>,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<M>> {
        // iced may reuse a canvas state when a strip is removed or reordered.
        if state.id != Some(self.control.id) || !self.control.state.enabled {
            state.drag = None;
            state.keyboard = Default::default();
            state.id = Some(self.control.id);
        }
        if let Some((adjustment, capture)) = state.keyboard.update(
            event,
            self.focused && self.control.state.enabled,
            state.drag.is_some(),
            self.value,
            self.control
                .nudge
                .unwrap_or(crate::control::Nudge::NORMALIZED),
        ) {
            return Some(adjustment.action(&self.on_change, &self.on_release, capture));
        }
        match event {
            canvas::Event::Window(iced::window::Event::Unfocused) => {
                let drag = state.drag.take();
                state.modifiers = keyboard::Modifiers::NONE;
                Some(match drag {
                    Some(drag) => {
                        canvas::Action::publish((self.on_release)(published_value(drag.value)))
                    }
                    None => canvas::Action::request_redraw(),
                })
            }
            canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if self.control.state.enabled && cursor.is_over(bounds) =>
            {
                if let Some(default) = self.control.default
                    && let Some(position) = cursor.position()
                    && state.click.register(position)
                {
                    state.drag = None;
                    return Some(
                        canvas::Action::publish((self.on_release)(published_value(default)))
                            .and_capture(),
                    );
                }
                state.modifiers = if self.shift {
                    keyboard::Modifiers::SHIFT
                } else {
                    keyboard::Modifiers::NONE
                };
                state.drag = Some(Drag {
                    y: None,
                    value: unit(self.value),
                    published: unit(self.value),
                });
                // The focus wrapper claims this control; pressing never changes its value.
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position })
                if state.drag.is_some() =>
            {
                let geometry = Geometry::new(bounds);
                let fine = state.modifiers.shift();
                let drag = state.drag.as_mut()?;
                let Some(last) = drag.y.replace(position.y) else {
                    return Some(canvas::Action::capture());
                };
                drag.value = drag_value(
                    drag.value,
                    position.y - last,
                    geometry.bottom - geometry.top,
                    fine,
                );
                // Incremental motion allows changing Shift mid-drag without a jump,
                // and reversing immediately at an endpoint. Subthreshold motion accumulates.
                let next = published_value(drag.value);
                if worth_publishing(next, drag.published) {
                    drag.published = next;
                    Some(canvas::Action::publish((self.on_change)(next)).and_capture())
                } else {
                    Some(canvas::Action::capture())
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.drag.is_some() =>
            {
                // The sub-threshold remainder lands with the release.
                let drag = state.drag.take()?;
                Some(
                    canvas::Action::publish((self.on_release)(published_value(drag.value)))
                        .and_capture(),
                )
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &State<Key>,
        renderer: &Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let mut kind = self.control.kind;
        kind.value = self.value;
        let look = kind.look(
            &self.theme,
            self.control.state,
            crate::control::canvas::interaction(cursor, bounds, state.drag.is_some()),
        );
        kind.paint_with(
            &mut frame,
            Rectangle::with_size(bounds.size()),
            &look,
            &self.theme,
        );
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _: &State<Key>,
        _: Rectangle,
        _: mouse::Cursor,
    ) -> mouse::Interaction {
        // Volume editing keeps the normal arrow, including a drag outside the well.
        mouse::Interaction::default()
    }
}

#[cfg(test)]
mod tests;
