//! Reusable input/output activity light, a bare square as in Live's top right; level and ink belong to the caller, appearance
//! belongs to the control library. It is a readout and never claims focus.
use super::{ControlKind, Interaction, Look, SizeRole, State, Theme};
use iced::widget::canvas::{Frame, Path};
use iced::{Color, Rectangle};

pub(crate) struct ActivityDot {
    pub level: f32,
    pub ink: Color,
}
impl ControlKind for ActivityDot {
    fn look(&self, theme: &Theme, _: State, _: Interaction) -> Look {
        let level = self.level.clamp(0.0, 1.0);
        let off = theme.display_background;
        Look {
            fill: off,
            frame: theme.control_contrast_frame,
            ink: theme.control_foreground,
            accent: Color {
                r: off.r + (self.ink.r - off.r) * level,
                g: off.g + (self.ink.g - off.g) * level,
                b: off.b + (self.ink.b - off.b) * level,
                a: 1.0,
            },
        }
    }
    fn role(&self) -> SizeRole {
        SizeRole::Fixed {
            width: crate::style::detail::ACTIVITY_DOT_SIZE,
            height: crate::style::detail::ACTIVITY_DOT_SIZE,
        }
    }
    fn paint(&self, frame: &mut Frame, bounds: Rectangle, look: &Look) {
        frame.fill(
            &Path::rectangle(bounds.position(), bounds.size()),
            look.accent,
        );
    }
}

impl ActivityDot {
    pub(crate) fn view<'a, M: 'a>(self, theme: &Theme) -> iced::Element<'a, M> {
        let role = self.role();
        iced::widget::Canvas::new(Dot {
            dot: self,
            theme: *theme,
        })
        .width(role.width())
        .height(role.height().unwrap())
        .into()
    }
}
struct Dot {
    dot: ActivityDot,
    theme: Theme,
}
impl<M> iced::widget::canvas::Program<M> for Dot {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &iced::Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<iced::widget::canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let look = self
            .dot
            .look(&self.theme, State::default(), Interaction::Idle);
        self.dot
            .paint(&mut frame, Rectangle::with_size(bounds.size()), &look);
        vec![frame.into_geometry()]
    }
}
