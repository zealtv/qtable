//! The one rule: a 1 px line that separates groups, rows and areas, in a named ink level.
//! Panels, device bodies, strip bands, menus and fields all draw their dividers here, so one
//! change recolours them all. Iced's own `rule` widget takes its ink from the iced palette
//! rather than the theme, so nothing in the GUI uses it.
use super::Theme;
use iced::widget::{Space, container};
use iced::{Color, Element, Length};

/// Which way the line runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    /// Fills the width of its column; `thickness` tall.
    Horizontal,
    /// Fills the height of its row; `thickness` wide.
    Vertical,
}

/// How a rule reads against the surface it sits on. Levels are named by what the line does,
/// and each is one theme ink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Level {
    /// Between groups on a panel or device body, and around cells: a groove darker than the
    /// surface, as Live's band borders are (`row_rule`).
    Group,
    /// Where a recessed control or area meets its neighbour: the darkest line, the same ink as
    /// a readout's frame (`control_contrast_frame`).
    Deep,
    /// A lighter line on a dark ground: a popup list's separators, and the divider inside a
    /// field (`surface_highlight`).
    Light,
}

impl Theme {
    /// The ink of a rule at this level.
    pub(crate) fn rule_ink(&self, level: Level) -> Color {
        match level {
            Level::Group => self.row_rule,
            Level::Deep => self.control_contrast_frame,
            Level::Light => self.surface_highlight,
        }
    }
}

/// A 1 px rule.
pub(crate) fn rule<'a, M: 'a>(theme: &Theme, axis: Axis, level: Level) -> Element<'a, M> {
    rule_sized(theme, axis, level, 1.0)
}

/// A rule `thickness` px across, for the few places a heavier line separates rows.
pub(crate) fn rule_sized<'a, M: 'a>(
    theme: &Theme,
    axis: Axis,
    level: Level,
    thickness: f32,
) -> Element<'a, M> {
    let ink = theme.rule_ink(level);
    let (width, height) = match axis {
        Axis::Horizontal => (Length::Fill, Length::Fixed(thickness)),
        Axis::Vertical => (Length::Fixed(thickness), Length::Fill),
    };
    container(Space::new())
        .width(width)
        .height(height)
        .style(move |_| container::Style {
            background: Some(ink.into()),
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests;
