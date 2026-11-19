//! Live-style A/B chance rail with an upward triangular thumb (2026-10-11 refs).
//! The caller supplies percentage B and converts edits to engine weights.
use super::{CONTROL_GAP, CONTROL_HEIGHT, TEXT_SIZE};
use crate::control::LIVE_DARK;
use crate::style::{ACCENT, DISABLED, DISPLAY, HIGHLIGHT, TEXT};
use iced::widget::canvas::{self, Frame, Geometry, Path, Program, Stroke};
use iced::widget::{container, row, text};
use iced::{Element, Length, Point, Rectangle, Renderer, Theme, alignment, mouse, touch};

const FRAME: iced::Color = LIVE_DARK.control_contrast_frame;
const THUMB_HALF: f32 = 5.0;
const READOUT_WIDTH: f32 = 42.0;

/// Pass None for `on_change` to show disabled chances without accepting input.
/// At the left end A is 100%; at the right end B is 100%.
pub fn view<'a, M: 'a>(
    percent_b: f32,
    on_change: Option<Box<dyn Fn(f32) -> M + 'a>>,
) -> Element<'a, M> {
    let percent_b = normalise(percent_b);
    let ink = if on_change.is_some() { TEXT } else { DISABLED };
    let readout = move |value| {
        container(text(format!("{value:.0} %")).size(TEXT_SIZE).color(ink))
            .width(READOUT_WIDTH)
            .height(CONTROL_HEIGHT)
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
            .style(|_| container::Style {
                background: Some(DISPLAY.into()),
                border: iced::Border {
                    color: FRAME,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
    };
    with_readouts(
        percent_b,
        on_change,
        readout(100.0 - percent_b),
        readout(percent_b),
    )
}

/// Caller-owned, focusable percentage fields can replace the static readouts.
pub fn with_readouts<'a, M: 'a>(
    percent_b: f32,
    on_change: Option<Box<dyn Fn(f32) -> M + 'a>>,
    left: impl Into<Element<'a, M>>,
    right: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    row![
        left.into(),
        canvas::Canvas::new(Chance {
            percent_b: normalise(percent_b),
            on_change
        })
        .width(Length::Fill)
        .height(CONTROL_HEIGHT),
        right.into(),
    ]
    .spacing(CONTROL_GAP)
    .align_y(alignment::Vertical::Center)
    .into()
}

fn normalise(percent: f32) -> f32 {
    if percent.is_finite() {
        percent.clamp(0.0, 100.0)
    } else {
        0.0
    }
}

fn percent_at(x: f32, width: f32) -> f32 {
    let span = width - 2.0 * THUMB_HALF;
    if span <= 0.0 {
        return 0.0;
    }
    normalise((x - THUMB_HALF) / span * 100.0).round()
}

struct Chance<'a, M> {
    percent_b: f32,
    on_change: Option<Box<dyn Fn(f32) -> M + 'a>>,
}

#[derive(Default, Debug, PartialEq)]
enum Drag {
    #[default]
    Idle,
    Mouse,
    Touch(touch::Finger),
}

impl<M> Program<M> for Chance<'_, M> {
    type State = Drag;

    fn update(
        &self,
        state: &mut Drag,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<M>> {
        let Some(on_change) = &self.on_change else {
            *state = Drag::Idle;
            return None;
        };
        let position = match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                *state = Drag::Mouse;
                cursor.position()
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position })
                if *state == Drag::Mouse =>
            {
                Some(*position)
            }
            canvas::Event::Touch(touch::Event::FingerPressed { id, position })
                if *state == Drag::Idle && bounds.contains(*position) =>
            {
                *state = Drag::Touch(*id);
                Some(*position)
            }
            canvas::Event::Touch(touch::Event::FingerMoved { id, position })
                if *state == Drag::Touch(*id) =>
            {
                Some(*position)
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if *state == Drag::Mouse =>
            {
                *state = Drag::Idle;
                return Some(canvas::Action::request_redraw().and_capture());
            }
            canvas::Event::Touch(
                touch::Event::FingerLifted { id, .. } | touch::Event::FingerLost { id, .. },
            ) if *state == Drag::Touch(*id) => {
                *state = Drag::Idle;
                return Some(canvas::Action::request_redraw().and_capture());
            }
            canvas::Event::Mouse(mouse::Event::CursorLeft)
            | canvas::Event::Window(iced::window::Event::Unfocused) => {
                *state = Drag::Idle;
                return Some(canvas::Action::request_redraw());
            }
            _ => return None,
        }?;
        Some(
            canvas::Action::publish(on_change(percent_at(position.x - bounds.x, bounds.width)))
                .and_capture(),
        )
    }

    fn draw(
        &self,
        state: &Drag,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let centre_y = bounds.height / 2.0;
        let right = (bounds.width - THUMB_HALF).max(THUMB_HALF);
        frame.stroke(
            &Path::line(
                Point::new(THUMB_HALF, centre_y - 3.0),
                Point::new(right, centre_y - 3.0),
            ),
            Stroke::default().with_color(FRAME).with_width(2.0),
        );
        let x = THUMB_HALF + (right - THUMB_HALF) * self.percent_b / 100.0;
        let triangle = Path::new(|path| {
            path.move_to(Point::new(x, centre_y - THUMB_HALF));
            path.line_to(Point::new(x + THUMB_HALF, centre_y + THUMB_HALF));
            path.line_to(Point::new(x - THUMB_HALF, centre_y + THUMB_HALF));
            path.close();
        });
        let fill = if self.on_change.is_none() {
            DISABLED
        } else if *state != Drag::Idle {
            ACCENT
        } else if cursor.is_over(bounds) {
            TEXT
        } else {
            HIGHLIGHT
        };
        frame.fill(&triangle, fill);
        frame.stroke(
            &triangle,
            Stroke::default().with_color(FRAME).with_width(1.0),
        );
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &Drag,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.on_change.is_some() && (*state != Drag::Idle || cursor.is_over(bounds)) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chance_uses_thumb_centres_and_clamps_drag_beyond_rail() {
        assert_eq!(percent_at(THUMB_HALF, 110.0), 0.0);
        assert_eq!(percent_at(55.0, 110.0), 50.0);
        assert_eq!(percent_at(105.0, 110.0), 100.0);
        assert_eq!(percent_at(-40.0, 110.0), 0.0);
        assert_eq!(percent_at(300.0, 110.0), 100.0);
        assert_eq!(percent_at(0.0, 0.0), 0.0);
        assert_eq!(normalise(f32::NAN), 0.0);
    }

    #[test]
    fn disabled_control_ignores_press_and_release_ends_drag() {
        let bounds = Rectangle::new(Point::new(10.0, 20.0), iced::Size::new(110.0, 20.0));
        let cursor = mouse::Cursor::Available(Point::new(65.0, 30.0));
        let press = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let mut drag = Drag::Idle;
        let disabled = Chance::<f32> {
            percent_b: 50.0,
            on_change: None,
        };
        assert!(disabled.update(&mut drag, &press, bounds, cursor).is_none());
        assert_eq!(drag, Drag::Idle);
        let enabled = Chance {
            percent_b: 50.0,
            on_change: Some(Box::new(|value| value)),
        };
        let (message, _, status) = enabled
            .update(&mut drag, &press, bounds, cursor)
            .unwrap()
            .into_inner();
        assert_eq!(message, Some(50.0));
        assert_eq!(status, iced::event::Status::Captured);
        assert_eq!(drag, Drag::Mouse);
        let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        assert!(
            enabled
                .update(&mut drag, &release, bounds, mouse::Cursor::Unavailable)
                .is_some()
        );
        assert_eq!(drag, Drag::Idle);
    }

    #[test]
    fn touch_drag_keeps_its_finger_and_clamps_outside_the_control() {
        let bounds = Rectangle::new(Point::new(10.0, 20.0), iced::Size::new(110.0, 20.0));
        let enabled = Chance {
            percent_b: 50.0,
            on_change: Some(Box::new(|value| value)),
        };
        let mut drag = Drag::Idle;
        let cursor = mouse::Cursor::Unavailable;
        let finger = touch::Finger(1);
        let press = canvas::Event::Touch(touch::Event::FingerPressed {
            id: finger,
            position: Point::new(65.0, 30.0),
        });
        assert_eq!(
            enabled
                .update(&mut drag, &press, bounds, cursor)
                .unwrap()
                .into_inner()
                .0,
            Some(50.0),
        );
        let wrong_finger = canvas::Event::Touch(touch::Event::FingerMoved {
            id: touch::Finger(2),
            position: Point::new(200.0, 30.0),
        });
        assert!(
            enabled
                .update(&mut drag, &wrong_finger, bounds, cursor)
                .is_none()
        );
        let moved = canvas::Event::Touch(touch::Event::FingerMoved {
            id: finger,
            position: Point::new(200.0, 30.0),
        });
        assert_eq!(
            enabled
                .update(&mut drag, &moved, bounds, cursor)
                .unwrap()
                .into_inner()
                .0,
            Some(100.0),
        );
        let lost = canvas::Event::Touch(touch::Event::FingerLost {
            id: finger,
            position: Point::new(200.0, 30.0),
        });
        assert!(enabled.update(&mut drag, &lost, bounds, cursor).is_some());
        assert_eq!(drag, Drag::Idle);
        assert!(enabled.update(&mut drag, &moved, bounds, cursor).is_none());
    }
}
