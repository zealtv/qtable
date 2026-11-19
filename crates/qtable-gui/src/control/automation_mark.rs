//! Live's automation LED: a small dot on a control whose argument has an envelope in the
//! playing clip. `AutomationColor` while it follows the envelope, `AutomationDisabled` once a
//! manual change has overridden it (`parameter-curves-and-model`, section 3, "Look").
use super::Theme;
use iced::widget::{Space, container, stack};
use iced::{Border, Color, Element, Length, alignment};

/// Diameter of the dot, in px (qtable choice; Live's exact size is not captured).
pub(crate) const MARK_SIZE: f32 = 4.0;
/// Gap between the dot and the control's top left corner (qtable choice).
pub(crate) const MARK_INSET: f32 = 1.0;
/// Extra room between a dropdown face's LED and its text, beyond the face's own spacing,
/// as in Live's Device and Control choosers.
pub(crate) const FACE_GAP: f32 = 3.0;

/// What a control shows about its envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    Following,
    Overridden,
}

impl Mark {
    pub(crate) fn ink(self, theme: &Theme) -> Color {
        match self {
            Mark::Following => theme.automation_color,
            Mark::Overridden => theme.automation_disabled,
        }
    }
}

/// The bare LED: `MARK_SIZE` square, drawn only for `Some`. Standalone, it sits before a
/// dropdown face's text (`multi_select::FaceLed`), as on Live's envelope choosers.
pub(crate) fn dot<'a, M: 'a>(mark: Option<Mark>, theme: &Theme) -> Element<'a, M> {
    let ink = mark.map(|mark| mark.ink(theme));
    container(Space::new())
        .width(MARK_SIZE)
        .height(MARK_SIZE)
        .style(move |_| container::Style {
            background: ink.map(Into::into),
            border: Border {
                radius: (MARK_SIZE / 2.0).into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// `element` with the LED over its top left corner; nothing is drawn for `None`. The tree has
/// the same shape either way, so a canvas drag or text input survives the state changing
/// under it. The LED takes no clicks.
pub(crate) fn automation_mark<'a, M: 'a>(
    element: impl Into<Element<'a, M>>,
    mark: Option<Mark>,
    theme: &Theme,
) -> Element<'a, M> {
    stack![
        element.into(),
        container(dot(mark, theme))
            .padding(MARK_INSET)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(alignment::Horizontal::Left)
            .align_y(alignment::Vertical::Top),
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::LIVE_DARK;

    #[test]
    fn following_and_overridden_use_the_automation_tokens() {
        assert_eq!(Mark::Following.ink(&LIVE_DARK), LIVE_DARK.automation_color);
        assert_eq!(
            Mark::Overridden.ink(&LIVE_DARK),
            LIVE_DARK.automation_disabled
        );
        assert_ne!(
            Mark::Following.ink(&LIVE_DARK),
            Mark::Overridden.ink(&LIVE_DARK)
        );
    }
}
