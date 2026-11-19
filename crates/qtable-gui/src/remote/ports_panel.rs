//! The Settings pane's MIDI tab (Live's Link, Tempo & MIDI): the input ports table, with
//! per-machine Remote only and no MIDI Track or Sync columns, and the top-right MIDI In indicator.
use super::{Action, State};
use crate::control::{self, Button, Face, State as ControlState};
use crate::info::Id;
use crate::{App, Message};
use iced::widget::tooltip::Position;
use iced::widget::{column, container, row, scrollable, text};
use iced::{Alignment, Element, Length};

fn switch<'a>(app: &App, label: &str, lit: bool, action: Message) -> Element<'a, Message> {
    let kind = Button {
        face: Face::Text(label.to_owned()),
        latched: true,
    };
    let looks = control::widgets::Looks::of(
        &kind,
        &app.theme,
        ControlState {
            lit: if lit {
                control::Lit::On
            } else {
                control::Lit::Off
            },
            ..Default::default()
        },
    );
    crate::style::flat(label.to_owned())
        .style(control::widgets::flat_style(looks))
        .on_press(action)
        .into()
}

/// The MIDI In indicator: a bare flashing square, as in Live's top right. The Info View names it.
pub(crate) fn indicator(app: &App) -> Element<'_, Message> {
    use crate::control::help::Help;
    control::ActivityDot {
        level: app.remote.activity_level(),
        ink: app.theme.orange_vu_meter,
    }
    .view(&app.theme)
    .info(Id::MidiInIndicator, Message::Info)
}

/// Include saved, disconnected ports so their Remote setting can be changed offline.
fn rows(state: &State) -> Vec<(String, bool, bool)> {
    let mut rows: Vec<_> = state
        .ports
        .ports
        .iter()
        .map(|port| {
            (
                port.name.clone(),
                state
                    .settings
                    .midi
                    .remote
                    .get(&port.name)
                    .copied()
                    .unwrap_or(false),
                true,
            )
        })
        .collect();
    rows.extend(
        state
            .settings
            .midi
            .remote
            .iter()
            .filter(|(name, _)| !state.ports.ports.iter().any(|port| port.name == **name))
            .map(|(name, enabled)| (name.clone(), *enabled, false)),
    );
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

/// The MIDI tab: every input port with its Remote switch, saved for this computer.
pub(crate) fn tab(app: &App) -> Element<'_, Message> {
    let controls = app.controls();
    let remote_width = crate::style::detail::DEVICE_NUMBER_WIDTH;
    let entries = rows(&app.remote);
    let mut ports = column![row![
        controls
            .readout("Input Ports", ControlState::default())
            .width(Length::Fill),
        container(controls.readout("Remote", ControlState::default())).width(remote_width)
    ]]
    .spacing(crate::style::detail::LABEL_GAP);
    if entries.is_empty() {
        ports = ports.push(controls.readout("No MIDI input ports", ControlState::default()));
    }
    for (name, enabled, available) in entries {
        let label = if available {
            name.clone()
        } else {
            format!("{name} (disconnected)")
        };
        ports = ports.push(
            row![
                container(
                    controls
                        .readout(
                            label,
                            ControlState {
                                enabled: available,
                                ..Default::default()
                            }
                        )
                        .wrapping(text::Wrapping::None)
                )
                .width(Length::Fill)
                .clip(true),
                container(control::tooltip(
                    switch(
                        app,
                        if enabled { "On" } else { "Off" },
                        enabled,
                        Message::Remote(Action::SetRemote(name, !enabled))
                    ),
                    "Enable this input for remote control; saved on this computer",
                    Position::Bottom
                ))
                .width(remote_width),
            ]
            .align_y(Alignment::Center),
        );
    }
    scrollable(ports).height(Length::Fill).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtable_remote::{Port, PortSnapshot};
    #[test]
    fn unplugged_preferences_remain_visible_and_hotplug_has_no_duplicate_row() {
        let mut state = State::default();
        state.settings.midi.remote.insert("Keys".into(), true);
        assert_eq!(rows(&state), vec![("Keys".into(), true, false)]);
        state.ports = PortSnapshot {
            ports: vec![Port {
                id: "new-id".into(),
                name: "Keys".into(),
                remote: true,
                connected: true,
                error: None,
            }],
            error: None,
        };
        assert_eq!(rows(&state), vec![("Keys".into(), true, true)]);
    }
}
