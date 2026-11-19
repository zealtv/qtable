//! Canvas adapter: paint a kind, hit-test it, and draw the focus outline on its own rectangle.
//! Canvases that lay out many controls (the grid) keep their own geometry and call `outline`
//! with the same rectangle they hit-test; a single kind is hosted with `Hosted`.
use super::{Control, ControlKind, Interaction, Theme};
use crate::control_focus::ControlKey;
use iced::widget::canvas::{self, Frame, Geometry, Path, Program, Stroke};
use iced::{Rectangle, Renderer, mouse};

/// The 1 px focus stroke, inset half a pixel so it stays inside the control's rectangle.
pub(crate) fn outline(frame: &mut Frame, bounds: Rectangle, theme: &Theme) {
    let inset = Rectangle::new(
        iced::Point::new(bounds.x + 0.5, bounds.y + 0.5),
        iced::Size::new(bounds.width - 1.0, bounds.height - 1.0),
    );
    frame.stroke(
        &Path::rectangle(inset.position(), inset.size()),
        Stroke::default()
            .with_color(theme.focus_stroke)
            .with_width(1.0),
    );
}

/// One kind drawn on its own canvas, publishing `on_press` when a press lands on it;
/// without `on_press` it is a readout.
/// The focus outline comes from the `Focusable` wrapper around it, as for iced controls.
pub(super) struct Hosted<K, M, Key: ControlKey> {
    pub control: Control<K, Key>,
    pub theme: Theme,
    pub on_press: Option<M>,
}

pub(super) fn interaction(cursor: mouse::Cursor, bounds: Rectangle, pressed: bool) -> Interaction {
    match (cursor.is_over(bounds), pressed) {
        (true, true) => Interaction::Pressed,
        (true, false) => Interaction::Hovered,
        (false, _) => Interaction::Idle,
    }
}

impl<K: ControlKind, M: Clone, Key: ControlKey> Program<M> for Hosted<K, M, Key> {
    type State = bool;

    fn update(
        &self,
        pressed: &mut bool,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<M>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let on_press = self.on_press.clone()?;
                let local = cursor.position_in(bounds)?;
                let area = Rectangle::with_size(bounds.size());
                if !self.control.state.enabled || !self.control.kind.hit(area, local) {
                    return None;
                }
                *pressed = true;
                Some(canvas::Action::publish(on_press).and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if *pressed => {
                *pressed = false;
                Some(canvas::Action::request_redraw())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        pressed: &bool,
        renderer: &Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let look = self.control.kind.look(
            &self.theme,
            self.control.state,
            interaction(cursor, bounds, *pressed),
        );
        self.control
            .kind
            .paint(&mut frame, Rectangle::with_size(bounds.size()), &look);
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _: &bool,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let over = cursor.position_in(bounds).is_some_and(|p| {
            self.control
                .kind
                .hit(Rectangle::with_size(bounds.size()), p)
        });
        if over && self.control.state.enabled && self.on_press.is_some() {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}
