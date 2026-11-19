//! Five fixed segments for comparing output load; a readout never claims focus.
use super::{ControlKind, Interaction, Look, SizeRole, State, Theme};
use iced::widget::canvas::Frame;
use iced::{Rectangle, Size};

pub(crate) struct LoadMeter {
    pub fraction: f64,
    pub deferring: bool,
}

impl LoadMeter {
    fn segments(&self) -> usize {
        (self.fraction.clamp(0.0, 1.0) * 5.0).ceil() as usize
    }
}

impl ControlKind for LoadMeter {
    fn look(&self, theme: &Theme, state: State, _: Interaction) -> Look {
        Look {
            fill: theme.display_background,
            ink: theme.control_foreground,
            frame: theme.control_contrast_frame,
            accent: if !state.enabled {
                theme.text_disabled
            } else if self.deferring {
                theme.chosen_default
            } else {
                theme.control_foreground
            },
        }
    }

    fn role(&self) -> SizeRole {
        SizeRole::Fixed {
            width: 29.0,
            height: 14.0,
        }
    }

    fn paint(&self, frame: &mut Frame, bounds: Rectangle, look: &Look) {
        for index in 0..5 {
            frame.fill_rectangle(
                iced::Point::new(bounds.x + index as f32 * 6.0, bounds.y + 3.0),
                Size::new(5.0, 8.0),
                if index < self.segments() {
                    look.accent
                } else {
                    look.fill
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_clamp_and_only_current_deferring_uses_orange() {
        let mut meter = LoadMeter {
            fraction: 0.0,
            deferring: false,
        };
        assert_eq!(meter.segments(), 0);
        meter.fraction = 0.01;
        assert_eq!(meter.segments(), 1);
        meter.fraction = 0.4;
        assert_eq!(meter.segments(), 2);
        meter.fraction = 1.5;
        assert_eq!(meter.segments(), 5);
        let look = |meter: &LoadMeter| {
            meter.look(
                &super::super::LIVE_DARK,
                State::default(),
                Interaction::Idle,
            )
        };
        assert_eq!(
            look(&meter).accent,
            super::super::LIVE_DARK.control_foreground
        );
        meter.deferring = true;
        assert_eq!(look(&meter).accent, super::super::LIVE_DARK.chosen_default);
    }
}
