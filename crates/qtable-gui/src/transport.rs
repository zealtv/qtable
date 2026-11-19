//! Control bar after Live 12's: tempo, meter and global quantisation at the
//! left, then the transport position with Play and Stop. File buttons stay at the
//! far left.
mod position;

use crate::control::{Button, Dropdown, Face, Glyph, Number, ValueFormat};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::{Control, ControlId};
use crate::info::{Help, Id};
use iced::widget::{Space, container, row, text};
use iced::{Alignment, Element, Font, Length};
use qtable_engine::model::{ControlRef, GlobalQuantisation, TransportControl};

use crate::{App, FileMessage, Message};

/// A menu entry; `Display` gives the label shown in the dropdown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct QuantChoice(pub GlobalQuantisation);

impl std::fmt::Display for QuantChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.label())
    }
}

pub(super) fn view(app: &App) -> Element<'_, Message> {
    let controls = app.controls();
    let file = |name: &'static str, help: Id, message: FileMessage| {
        controls.button(
            Control::new(
                ControlId::action(View::ControlBar, name, ControlOwner::Song),
                help,
                Button {
                    face: Face::Text(name.to_owned()),
                    latched: false,
                },
            ),
            Message::Files(message),
        )
    };
    let transport = |name: &'static str,
                     help: Id,
                     drives: TransportControl,
                     glyph: Glyph,
                     lit: bool,
                     message: Message| {
        controls.button(
            Control::new(
                ControlId::action(View::ControlBar, name, ControlOwner::Song),
                help,
                Button {
                    face: Face::Glyph(glyph),
                    latched: false,
                },
            )
            .drives(ControlRef::Transport(drives))
            .lit(lit),
            message,
        )
    };
    let bar = row![
        file("New", Id::New, FileMessage::New),
        file("Open", Id::Open, FileMessage::Open),
        file("Save", Id::Save, FileMessage::Save),
        file("Save As", Id::SaveAs, FileMessage::SaveAs),
        crate::settings_pane::button(app),
        tempo_field(app),
        meter_readout(app),
        quantisation_field(app, View::ControlBar),
        position_readout(app),
        // Lit while the transport runs, in Live's ChosenPlay green.
        transport(
            "Play",
            Id::Play,
            TransportControl::Play,
            Glyph::Play,
            app.snapshot.playing,
            Message::Play
        ),
        transport(
            "Stop",
            Id::Stop,
            TransportControl::Stop,
            Glyph::Stop,
            false,
            Message::Stop
        ),
        controls.button(
            Control::new(
                ControlId::action(View::ControlBar, "re-enable-automation", ControlOwner::Song),
                Id::ReEnableAutomation,
                Button {
                    face: Face::Text("Re-Enable Automation".into()),
                    latched: false,
                }
            )
            .drives(ControlRef::Transport(TransportControl::ReEnableAutomation))
            .lit(any_overridden(app))
            .enabled(any_overridden(app)),
            Message::ReEnableAutomation(None),
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    // Draw Mode sits with the mapping switches, immediately left of Key, as in Live 12.
    let draw_mode = controls.button(
        Control::new(
            ControlId::action(View::ControlBar, "draw-mode", ControlOwner::Song),
            Id::DrawMode,
            Button {
                face: Face::Glyph(Glyph::Pencil),
                latched: true,
            },
        )
        .lit(app.clip_view.editor.draw.on),
        Message::ToggleDrawMode,
    );
    container(
        row![
            bar,
            Space::new().width(Length::Fill),
            draw_mode,
            crate::map_mode::switch(app, crate::map_mode::Mode::Key),
            crate::map_mode::switch(app, crate::map_mode::Mode::Midi),
            crate::remote::indicator(app)
        ]
        .spacing(crate::style::detail::LABEL_GAP)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .into()
}

/// Whether any argument is overridden: Re-Enable Automation lights and enables meanwhile.
pub(crate) fn any_overridden(app: &App) -> bool {
    crate::parameter_gesture::any_overridden(&app.snapshot.automation)
}

/// Tempo with two to four decimals, like Live's transport (a 2026-10-12 screenshot). `app.tempo_input` holds the shown text.
pub(crate) const TEMPO_FORMAT: ValueFormat = ValueFormat::Float;

pub(crate) fn tempo_field(app: &App) -> Element<'_, Message> {
    let key = crate::drafts::DraftKey::Tempo;
    app.controls().number(
        Control::new(
            ControlId::field(View::ControlBar, key),
            Id::Tempo,
            Number::new("BPM").chrome(),
        )
        .drives(ControlRef::Transport(TransportControl::Tempo))
        .nudge(crate::control::Nudge::continuous(0.01, f64::MAX, 1.0))
        .invalid(app.drafts.errors.contains_key(&key)),
        &app.tempo_input,
        Message::TempoChanged,
        Message::TempoSubmitted,
        None,
    )
}

pub(crate) fn meter_readout(app: &App) -> Element<'_, Message> {
    text(format!(
        "{}/{}",
        app.snapshot.time_signature.numerator(),
        app.snapshot.time_signature.denominator()
    ))
    .size(crate::style::detail::HEADER_SIZE)
    .info(Id::Meter, Message::Info)
}
pub(crate) fn quantisation_field(app: &App, view: View) -> Element<'_, Message> {
    let choices: Vec<QuantChoice> = GlobalQuantisation::CHOICES
        .iter()
        .copied()
        .map(QuantChoice)
        .collect();
    app.controls().dropdown(
        Control::new(
            ControlId::action(view, "global-quantisation", ControlOwner::Song),
            Id::GlobalQuantisation,
            Dropdown,
        )
        .drives(ControlRef::Transport(TransportControl::Quantisation)),
        choices,
        Some(QuantChoice(app.project().global_quantisation)),
        |choice| Message::GlobalQuantisation(choice.0),
    )
}
pub(crate) fn position_readout(app: &App) -> Element<'_, Message> {
    container(
        app.controls().number_readout(
            Control::new(
                ControlId::action(View::ControlBar, "position", ControlOwner::Song),
                Id::Position,
                Number::new("").chrome(),
            )
            .role(crate::control::SizeRole::Fill),
            &position::format_position(
                app.snapshot.current_beat.ticks(),
                app.snapshot.time_signature,
            ),
            Font::MONOSPACE,
            12,
        ),
    )
    .width(Length::Fixed(position::READOUT_WIDTH))
    .align_x(iced::alignment::Horizontal::Right)
    .into()
}

/// Commits the tempo draft through Enter's path.
pub(crate) fn commit_tempo(app: &mut crate::app::App) {
    let _ = crate::update::update(app, Message::TempoSubmitted);
    app.pending_field = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_entry_is_a_valid_global_quantisation() {
        for value in GlobalQuantisation::CHOICES {
            assert!(value.validate().is_ok(), "{value:?}");
        }
    }

    #[test]
    fn the_control_bar_declares_every_transport_control_it_shows() {
        use qtable_engine::engine::Engine;
        let app = App::new(
            Engine::new(crate::files::new_project()).unwrap(),
            "120".into(),
            None,
        );
        let mut bar = view(&app);
        let declared =
            crate::control_focus::test_registry::declared(&mut bar, iced::Size::new(1600.0, 40.0));
        for control in [
            TransportControl::Play,
            TransportControl::Stop,
            TransportControl::Tempo,
            TransportControl::Quantisation,
            TransportControl::ReEnableAutomation,
        ] {
            let control = ControlRef::Transport(control);
            assert!(declared.contains(&control), "{control:?}");
            assert!(
                qtable_engine::model::describe_control(app.project(), &control).is_some(),
                "{control:?} describes"
            );
        }
        assert_eq!(declared.len(), 5, "{declared:?}");
    }

    #[test]
    fn draw_mode_sits_with_the_mapping_switches_immediately_left_of_key() {
        use crate::control_focus::ControlOwner;
        use crate::drafts::ControlId;
        use qtable_engine::engine::Engine;
        let app = App::new(
            Engine::new(crate::files::new_project()).unwrap(),
            "120".into(),
            None,
        );
        let mut bar = view(&app);
        let rectangles = crate::control_focus::test_registry::rectangles_in(
            &mut bar,
            iced::Size::new(1600.0, 40.0),
        );
        let at = |name: &'static str| {
            let wanted = ControlId::action(View::ControlBar, name, ControlOwner::Song);
            rectangles
                .iter()
                .find(|(id, _)| *id == wanted)
                .map(|(_, rectangle)| *rectangle)
                .unwrap_or_else(|| panic!("{name} is in the Control Bar"))
        };
        let (draw, key, midi) = (at("draw-mode"), at("key-map"), at("midi-map"));
        let (stop, re_enable) = (at("Stop"), at("re-enable-automation"));
        // Right-hand group: Draw Mode, Key, MIDI, in that order, on one row.
        assert!(draw.x + draw.width <= key.x && key.x + key.width <= midi.x);
        assert!(
            key.x - (draw.x + draw.width) < 12.0,
            "immediately left of Key"
        );
        // No longer between Stop and Re-Enable Automation.
        assert!(stop.x + stop.width <= re_enable.x);
        assert!(re_enable.x + re_enable.width < draw.x);
        assert!((draw.center_y() - key.center_y()).abs() < 1.0, "one row");
    }

    #[test]
    fn menu_labels_follow_lives_chooser() {
        let labels: Vec<String> = GlobalQuantisation::CHOICES
            .iter()
            .map(|&v| QuantChoice(v).to_string())
            .collect();
        assert_eq!(labels[0], "None");
        assert_eq!(labels[4], "1 Bar");
        assert_eq!(labels[2], "4 Bars");
        assert_eq!(labels[6], "1/2T");
    }
}
