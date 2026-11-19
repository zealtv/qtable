//! Shared device cells and fixed label rows on the device guideline's grid.
use super::rule::{Axis, Level, rule_sized};
use super::{Controls, State, Theme};
use crate::style::detail;
use iced::widget::{column, container, text};
use iced::{Element, Length};

pub(crate) fn labelled<'a, M: 'a, Key: crate::control_focus::ControlKey>(
    controls: Controls<'_, Key>,
    label: impl text::IntoFragment<'a>,
    state: State,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    column![
        container(
            controls
                .readout(label, state)
                .wrapping(text::Wrapping::None)
        )
        .height(detail::DEVICE_LABEL_ROW)
        .clip(true),
        container(content).height(detail::CONTROL_HEIGHT)
    ]
    .spacing(detail::LABEL_GAP)
    .width(Length::Fill)
    .into()
}

pub(crate) fn cell<'a, M: 'a>(
    theme: Theme,
    title: &'a str,
    width: f32,
    height: f32,
    body: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    cell_with(
        theme,
        Title::Text(title, true),
        width,
        height,
        Some(body.into()),
        false,
    )
}

/// What a cell's title strip holds.
pub(crate) enum Title<'a, M> {
    /// Plain text, inked by the owner's state (`true` is enabled).
    Text(&'a str, bool),
    /// The owner's own element, flush to the strip's right edge (a selectable title and a Bang).
    Element(Element<'a, M>),
}

/// The title ink of an owner that may be inactive.
pub(crate) fn title_ink(theme: Theme, enabled: bool) -> iced::Color {
    if enabled {
        theme.control_foreground
    } else {
        theme.text_disabled
    }
}

/// A cell title's text, inked by the owner's state.
pub(crate) fn title_text<'a>(theme: Theme, label: String, enabled: bool) -> text::Text<'a> {
    text(label)
        .size(detail::TEXT_SIZE)
        .color(title_ink(theme, enabled))
}

/// A title strip's border: square, except the top corners follow the 1 px edge around the
/// owner, whose own corners have radius `outer`, so the strip doesn't poke past the curve.
pub(crate) fn strip_border(outer: f32) -> iced::Border {
    iced::Border {
        radius: iced::border::top((outer - detail::DEVICE_CELL_EDGE).max(0.0)),
        ..Default::default()
    }
}

/// A cell on the device guideline's grid. `selected` colours its edge; the widget tree is the
/// same either way. A cell without a body is its strip alone, and `height` may shrink to fit.
pub(crate) fn cell_with<'a, M: 'a>(
    theme: Theme,
    title: Title<'a, M>,
    width: f32,
    height: impl Into<Length>,
    body: Option<Element<'a, M>>,
    selected: bool,
) -> Element<'a, M> {
    let (title, left, right) = match title {
        Title::Text(label, enabled) => (
            text(label)
                .size(detail::TEXT_SIZE)
                .color(title_ink(theme, enabled))
                .into(),
            detail::DEVICE_CELL_PADDING,
            detail::DEVICE_CELL_PADDING,
        ),
        Title::Element(element) => (element, detail::DEVICE_CELL_PADDING, 0.0),
    };
    let mut cell = column![
        container(title)
            .height(detail::DEVICE_CELL_STRIP)
            .width(Length::Fill)
            .align_y(iced::alignment::Vertical::Center)
            .padding(iced::Padding {
                top: 0.0,
                right,
                bottom: 0.0,
                left,
            })
            .style(move |_| container::Style {
                background: Some(theme.device_title_background.into()),
                border: strip_border(theme.corner_cell),
                ..Default::default()
            })
    ];
    if let Some(body) = body {
        cell = cell.push(container(body).padding(iced::Padding {
            top: detail::DEVICE_ROW_GAP,
            right: detail::DEVICE_CELL_PADDING,
            bottom: detail::DEVICE_CELL_PADDING,
            left: detail::DEVICE_CELL_PADDING,
        }));
    }
    container(cell)
        .width(width)
        .height(height)
        .padding(detail::DEVICE_CELL_EDGE)
        .style(move |_| container::Style {
            background: Some(theme.detail_view_background.into()),
            border: iced::Border {
                color: if selected {
                    theme.selection_background
                } else {
                    theme.rule_ink(Level::Group)
                },
                width: detail::DEVICE_CELL_EDGE,
                radius: theme.corner_cell.into(),
            },
            ..Default::default()
        })
        .into()
}

/// A section heading over a run of cells: dim text, a 1 px rule, then the gap to the cells.
pub(crate) fn section_heading<'a, M: 'a>(
    theme: Theme,
    label: &'a str,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    use iced::widget::Space;
    column![
        container(
            text(label)
                .size(detail::HEADING_TEXT_SIZE)
                .color(theme.text_disabled)
                .wrapping(text::Wrapping::None)
        )
        .height(detail::DEVICE_HEADING_ROW)
        .align_y(iced::alignment::Vertical::Center),
        rule_sized(
            &theme,
            Axis::Horizontal,
            Level::Group,
            detail::DEVICE_CELL_EDGE
        ),
        Space::new().height(detail::DEVICE_HEADING_GAP),
        content.into(),
    ]
    .into()
}

/// Enclose dependants below a parent field. Detached dependants keep the bracket dimmed.
pub(crate) fn dependants<'a, M: 'a>(
    theme: Theme,
    attached: bool,
    content: impl Into<Element<'a, M>>,
) -> Element<'a, M> {
    use iced::widget::{Space, row};
    let ink = bracket_ink(theme, attached);
    let line = |width, height| {
        container(Space::new())
            .width(width)
            .height(height)
            .style(move |_| container::Style {
                background: Some(ink.into()),
                ..Default::default()
            })
    };
    row![
        line(Length::Fixed(detail::DEPENDENCY_RULE_WIDTH), Length::Fill),
        column![
            line(
                Length::Fixed(detail::CONTROL_GAP),
                Length::Fixed(detail::DEPENDENCY_RULE_WIDTH)
            ),
            Space::new().height(Length::Fill),
            line(
                Length::Fixed(detail::CONTROL_GAP),
                Length::Fixed(detail::DEPENDENCY_RULE_WIDTH)
            ),
        ]
        .height(Length::Fill),
        content.into(),
    ]
    .height(Length::Fill)
    .into()
}

fn bracket_ink(theme: Theme, attached: bool) -> iced::Color {
    iced::Color {
        a: if attached {
            1.0
        } else {
            detail::DEPENDENCY_DIM_ALPHA
        },
        ..theme.rule_ink(Level::Group)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_height_keeps_the_face_geometry() {
        assert_eq!(detail::DEVICE_HEADING, 16.0);
        assert_eq!(detail::DEVICE_SCROLL_GUTTER, 4.0);
    }

    #[test]
    fn title_ink_follows_the_owner_state() {
        let theme = crate::control::LIVE_DARK;
        assert_eq!(title_ink(theme, true), theme.control_foreground);
        assert_eq!(title_ink(theme, false), theme.text_disabled);
    }

    #[test]
    fn cells_build_with_and_without_selection_or_body() {
        let theme = crate::control::LIVE_DARK;
        for selected in [false, true] {
            let _: Element<'_, ()> = cell_with(
                theme,
                Title::Element(text("t").into()),
                118.0,
                Length::Shrink,
                None,
                selected,
            );
        }
    }

    #[test]
    fn detached_dependants_dim_the_bracket_without_hiding_it() {
        let ink = |attached| bracket_ink(crate::control::LIVE_DARK, attached);
        assert_eq!(ink(true), crate::control::LIVE_DARK.rule_ink(Level::Group));
        assert!(ink(false).a > 0.0 && ink(false).a < ink(true).a);
        assert_eq!(ink(false).r, ink(true).r);
    }
}
