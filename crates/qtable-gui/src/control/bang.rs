//! Momentary event trigger: an icon-only theme-coloured starburst. The event's title sits beside it.
use super::{Button, ControlKind, Face, Interaction, Look, SizeRole, State, Theme};
use iced::widget::canvas::{Frame, Path};
use iced::{Point, Rectangle};

pub(crate) struct Bang {
    /// An inactive Target keeps its controls editable, but dims their faces.
    pub active: bool,
}

impl ControlKind for Bang {
    fn look(&self, theme: &Theme, state: State, interaction: Interaction) -> Look {
        let mut look = Button {
            face: Face::Text(String::new()),
            latched: true,
        }
        .look(theme, state, interaction);
        if !self.active {
            look.ink = theme.text_disabled;
        }
        look
    }

    fn role(&self) -> SizeRole {
        SizeRole::Content
    }

    fn paint(&self, frame: &mut Frame, bounds: Rectangle, look: &Look) {
        frame.fill_rectangle(bounds.position(), bounds.size(), look.fill);
        let size = 14.0_f32.min(bounds.width).min(bounds.height);
        let origin = Point::new(
            bounds.center_x() - size / 2.0,
            bounds.center_y() - size / 2.0,
        );
        let burst = |points: &[(f32, f32)]| {
            Path::new(|p| {
                for (index, &(x, y)) in points.iter().enumerate() {
                    let point = Point::new(origin.x + x * size, origin.y + y * size);
                    if index == 0 {
                        p.move_to(point);
                    } else {
                        p.line_to(point);
                    }
                }
                p.close();
            })
        };
        frame.fill(
            &burst(&[
                (0.08, 0.03),
                (0.40, 0.23),
                (0.55, 0.00),
                (0.62, 0.29),
                (0.94, 0.12),
                (0.77, 0.43),
                (1.00, 0.54),
                (0.73, 0.65),
                (0.90, 0.96),
                (0.58, 0.77),
                (0.42, 1.00),
                (0.36, 0.73),
                (0.03, 0.86),
                (0.23, 0.57),
                (0.00, 0.40),
                (0.29, 0.35),
            ]),
            look.ink,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;

    #[test]
    fn bang_paints_a_centred_icon_and_no_text() {
        let bang = Bang { active: true };
        let look = bang.look(
            &super::super::LIVE_DARK,
            State::default(),
            Interaction::Idle,
        );
        let renderer = crate::grid::test_renderer();
        let mut frame = Frame::new(&renderer, Size::new(18.0, 18.0));
        bang.paint(
            &mut frame,
            Rectangle::with_size(Size::new(18.0, 18.0)),
            &look,
        );
        let geometry = format!("{:?}", frame.into_geometry());
        assert!(geometry.contains("text: []"), "{geometry}");
        // The first starburst point sits at 0.08, 0.03 of the 14 px icon, centred in 18 px.
        assert!(geometry.contains("M 3.12 2.42"), "{geometry}");
    }

    #[test]
    fn a_remote_flash_uses_the_pointer_pressed_colours() {
        let bang = Bang { active: true };
        let theme = &super::super::LIVE_DARK;
        let pressed = bang.look(theme, State::default(), Interaction::Pressed);
        let flash = bang.look(
            theme,
            State {
                lit: super::super::Lit::On,
                ..State::default()
            },
            Interaction::Idle,
        );
        assert_eq!(flash.fill, pressed.fill);
        assert_eq!(flash.ink, pressed.ink);
        assert_ne!(
            flash.fill,
            bang.look(theme, State::default(), Interaction::Idle).fill
        );
    }
}
