//! A compact CC Control-style cell. Vertical dragging changes the held value without jumping.
use super::{Control, ControlKind, Interaction, Look, SizeRole, State, Theme};
use crate::control_focus::{ControlId, ControlKey};
use iced::widget::canvas::{self, Frame, Path, Program, Text};
use iced::{Point, Radians, Rectangle, Renderer, keyboard, mouse};

pub(crate) const WIDTH: f32 = 55.0;
pub(crate) const HEIGHT: f32 = 68.0;
const TRAVEL: f64 = 100.0;

/// `fraction` is the value's place in the range; `label` and `value` are the texts above and
/// below the knob.
pub(crate) struct Dial {
    pub fraction: f64,
    pub label: String,
    pub value: String,
}

impl ControlKind for Dial {
    fn look(&self, theme: &Theme, state: State, _: Interaction) -> Look {
        Look {
            fill: theme.control_background,
            ink: if state.enabled {
                theme.control_foreground
            } else {
                theme.text_disabled
            },
            frame: theme.fader_edge,
            // Control On Variant, or its inactive twin for a device that is off or a control
            // that doesn't apply.
            accent: if state.enabled && !state.inactive {
                theme.value_arc
            } else {
                theme.value_arc_inactive
            },
        }
    }

    fn role(&self) -> SizeRole {
        SizeRole::Fixed {
            width: WIDTH,
            height: HEIGHT,
        }
    }

    fn paint(&self, frame: &mut Frame, bounds: Rectangle, look: &Look) {
        let center = Point::new(bounds.x + bounds.width / 2.0, bounds.y + 34.0);
        let start = 135.0_f32.to_radians();
        let sweep = 270.0_f32.to_radians();
        let angle = start + sweep * self.fraction.clamp(0.0, 1.0) as f32;
        let arc = |end| {
            Path::new(|p| {
                p.arc(canvas::path::Arc {
                    center,
                    radius: 16.0,
                    start_angle: Radians(start),
                    end_angle: Radians(end),
                });
            })
        };
        frame.stroke(
            &arc(start + sweep),
            canvas::Stroke::default()
                .with_color(look.frame)
                .with_width(2.0),
        );
        if self.fraction > 0.0 {
            frame.stroke(
                &arc(angle),
                canvas::Stroke::default()
                    .with_color(look.accent)
                    .with_width(2.0),
            );
        }
        let tip = Point::new(center.x + 13.0 * angle.cos(), center.y + 13.0 * angle.sin());
        frame.stroke(
            &Path::line(center, tip),
            canvas::Stroke::default()
                .with_color(look.ink)
                .with_width(2.0),
        );
        for (content, y, font) in [
            (&self.label, 8.0, iced::Font::DEFAULT),
            (&self.value, 59.0, crate::style::VALUE_FONT),
        ] {
            frame.with_clip(
                Rectangle {
                    x: bounds.x + 2.0,
                    y: bounds.y + y - 7.0,
                    width: bounds.width - 4.0,
                    height: 14.0,
                },
                |frame| {
                    frame.fill_text(Text {
                        content: content.clone(),
                        // Clip regions retain the canvas coordinate system.
                        position: Point::new(bounds.x + bounds.width / 2.0, bounds.y + y),
                        color: look.ink,
                        size: (crate::style::detail::TEXT_SIZE as f32).into(),
                        font,
                        align_x: iced::alignment::Horizontal::Center.into(),
                        align_y: iced::alignment::Vertical::Center,
                        ..Text::default()
                    })
                },
            );
        }
    }
}

/// The Dial on its own canvas: a vertical drag moves the held value relative to where it
/// started. `on_change` carries each step and `on_release` the final value once.
pub(super) struct Hosted<F, G, Key: ControlKey> {
    pub control: Control<Dial, Key>,
    pub theme: Theme,
    pub focused: bool,
    pub shift: bool,
    pub on_change: F,
    pub on_release: G,
}

pub(super) struct Gesture<Key> {
    id: Option<ControlId<Key>>,
    drag: Option<Drag>,
    click: super::double_click::Presses,
    modifiers: keyboard::Modifiers,
    keyboard: super::adjustment::Keyboard,
}

impl<Key> Default for Gesture<Key> {
    fn default() -> Self {
        Self {
            id: None,
            drag: None,
            click: Default::default(),
            modifiers: keyboard::Modifiers::NONE,
            keyboard: Default::default(),
        }
    }
}

struct Drag {
    y: f32,
    fraction: f64,
}

impl Drag {
    fn move_to(&mut self, y: f32, fine: bool) -> bool {
        let fraction = (self.fraction
            + f64::from(self.y - y) / TRAVEL
                * if fine {
                    super::adjustment::FINE_FACTOR
                } else {
                    1.0
                })
        .clamp(0.0, 1.0);
        self.y = y;
        if self.fraction == fraction {
            return false;
        }
        self.fraction = fraction;
        true
    }
}

impl<M, F: Fn(f64) -> M, G: Fn(f64) -> M, Key: ControlKey> Program<M> for Hosted<F, G, Key> {
    type State = Gesture<Key>;

    fn update(
        &self,
        state: &mut Gesture<Key>,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<M>> {
        if state.id != Some(self.control.id) {
            *state = Gesture::default();
            state.id = Some(self.control.id);
        }
        if !self.control.state.enabled {
            state.drag = None;
            state.keyboard = Default::default();
            return None;
        }
        if let Some((adjustment, capture)) = state.keyboard.update(
            event,
            self.focused,
            state.drag.is_some(),
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
                    state.drag = None;
                    return Some(canvas::Action::publish((self.on_release)(default)).and_capture());
                }
                state.modifiers = if self.shift {
                    keyboard::Modifiers::SHIFT
                } else {
                    keyboard::Modifiers::NONE
                };
                state.drag = Some(Drag {
                    y: position.y,
                    fraction: self.control.kind.fraction,
                });
                Some(canvas::Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                // Scrollables translate the cursor into content coordinates while the
                // event retains window coordinates. Match the frame used at press.
                let position = cursor.land().position()?;
                let drag = state.drag.as_mut()?;
                if !drag.move_to(position.y, state.modifiers.shift()) {
                    return Some(canvas::Action::capture());
                }
                Some(canvas::Action::publish((self.on_change)(drag.fraction)).and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let drag = state.drag.take()?;
                Some(canvas::Action::publish((self.on_release)(drag.fraction)).and_capture())
            }
            canvas::Event::Window(iced::window::Event::Unfocused) => {
                state.modifiers = keyboard::Modifiers::NONE;
                let drag = state.drag.take()?;
                Some(canvas::Action::publish((self.on_release)(drag.fraction)))
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Gesture<Key>,
        renderer: &Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let look = self.control.kind.look(
            &self.theme,
            self.control.state,
            super::canvas::interaction(cursor, bounds, state.drag.is_some()),
        );
        self.control
            .kind
            .paint(&mut frame, Rectangle::with_size(bounds.size()), &look);
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests;
