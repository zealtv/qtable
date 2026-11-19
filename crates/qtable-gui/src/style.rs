//! Live 12 "Default Dark Neutral Medium" look: palette aliases and the older widget helpers.
//! Colours come from `control::LIVE_DARK`; the helpers style through the control kinds' looks.
use crate::control::widgets::Looks;
use crate::control::{self, LIVE_DARK, State};
use control::text_field::TextField;
use iced::widget::{Button, button, text};
use iced::{Border, Color, Theme};

pub(crate) mod detail;

/// Portable value weight chosen from 2026-10-12 compact-control references.
pub(crate) const VALUE_FONT: iced::Font = iced::Font {
    weight: iced::font::Weight::Semibold,
    ..iced::Font::DEFAULT
};

pub(crate) const CONTROL: Color = LIVE_DARK.control_background;
pub(crate) const HIGHLIGHT: Color = LIVE_DARK.surface_highlight;
pub(crate) const DISPLAY: Color = LIVE_DARK.display_background;
pub(crate) const TEXT: Color = LIVE_DARK.control_foreground;
pub(crate) const DISABLED: Color = LIVE_DARK.text_disabled;
pub(crate) const ACCENT: Color = LIVE_DARK.chosen_default;

/// The iced palette, derived from the same theme as the controls.
pub fn live_theme(theme: &crate::control::Theme) -> Theme {
    Theme::custom(
        "Live",
        iced::theme::Palette {
            background: theme.surface_background,
            text: theme.control_foreground,
            primary: theme.control_background,
            success: theme.chosen_play,
            warning: theme.chosen_default,
            danger: theme.chosen_alert,
        },
    )
}

/// Small flat button with a dark control background, as in Live's top bar.
pub fn flat<'a, M: Clone + 'a>(label: impl text::IntoFragment<'a>) -> Button<'a, M> {
    let kind = control::Button {
        face: control::Face::Text(String::new()),
        latched: false,
    };
    button(text(label).size(detail::TEXT_SIZE).font(VALUE_FONT))
        .padding([2, 8])
        .style(control::widgets::flat_style(Looks::of(
            &kind,
            &LIVE_DARK,
            State::default(),
        )))
}

/// The Info View's panel: a recessed display with an outline, as behind Live's Arrangement
/// Position fields. (A value readout is `Controls::readout_box`, not this.)
pub fn panel(_: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        text_color: Some(TEXT),
        background: Some(DISPLAY.into()),
        border: Border {
            color: HIGHLIGHT,
            width: 1.0,
            radius: crate::control::LIVE_DARK.corner_control.into(),
        },
        ..Default::default()
    }
}

/// Small dark tooltip bubble.
pub fn tooltip(_: &Theme) -> iced::widget::container::Style {
    panel(&Theme::Dark)
}

fn text_looks(invalid: bool) -> Looks {
    let kind = control::Text {
        placeholder: "".into(),
    };
    Looks::of(
        &kind,
        &LIVE_DARK,
        State {
            invalid,
            ..State::default()
        },
    )
}

/// Dark display-style text field.
pub fn input<'a, M: Clone + 'a>(placeholder: &str, value: &str) -> TextField<'a, M> {
    control::widgets::field(text_looks(false), placeholder, value)
}

/// Keep rejected text visible with the same validation colour in every panel.
pub(crate) fn field_style(
    invalid: bool,
) -> impl Fn(&Theme, iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    control::widgets::field_style(text_looks(invalid))
}

/// The status bar and its controls retain their normal background at every severity.
pub(crate) fn status_strip(theme: control::Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        text_color: Some(theme.control_foreground),
        background: Some(theme.control_background.into()),
        ..Default::default()
    }
}

/// The Detail View's clip colour chooser labels.
pub(crate) struct PaletteColour {
    pub inspector_label: &'static str,
    pub colour: qtable_engine::model::Colour,
}

const fn colour(red: u8, green: u8, blue: u8) -> qtable_engine::model::Colour {
    qtable_engine::model::Colour { red, green, blue }
}

pub(crate) const PALETTE: [PaletteColour; 5] = [
    PaletteColour {
        inspector_label: "Coral",
        colour: colour(225, 90, 80),
    },
    PaletteColour {
        inspector_label: "Amber",
        colour: colour(240, 170, 70),
    },
    PaletteColour {
        inspector_label: "Green",
        colour: colour(125, 195, 110),
    },
    PaletteColour {
        inspector_label: "Blue",
        colour: colour(85, 165, 205),
    },
    PaletteColour {
        inspector_label: "Purple",
        colour: colour(145, 120, 205),
    },
];
