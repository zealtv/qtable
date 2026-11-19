//! The Settings pane: machine-wide settings in tabs,
//! opened by the Settings button that ends the New / Open / Save / Save As run. v3.6 builds the
//! MIDI tab only; OSC input and theming each add a `Tab` variant, its label and its body.
use crate::control::{Button, Face, ViewTab};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::{Control, ControlId};
use crate::info::Id;
use crate::overlay::Overlay;
use crate::{App, Message};
use iced::widget::{Space, column, container, mouse_area, opaque, row, stack};
use iced::{Alignment, Element, Length};

/// One tab of the pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Midi,
}

impl Tab {
    pub(crate) const ALL: [Tab; 1] = [Tab::Midi];

    fn label(self) -> &'static str {
        match self {
            Tab::Midi => "MIDI",
        }
    }

    /// The stable control name, so each tab keeps its own focus.
    fn name(self) -> &'static str {
        match self {
            Tab::Midi => "settings-tab-midi",
        }
    }

    fn help(self) -> Id {
        match self {
            Tab::Midi => Id::SettingsMidi,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Action {
    /// The Settings button: open the pane, or close it when it is open.
    Toggle,
    ChooseTab(Tab),
}

pub(crate) fn update(app: &mut App, action: Action) {
    match action {
        Action::Toggle => {
            if matches!(app.overlay, Some(Overlay::Settings(_))) {
                app.close_overlay();
            } else {
                app.open_overlay(Overlay::Settings(Tab::Midi));
            }
        }
        Action::ChooseTab(tab) => app.open_overlay(Overlay::Settings(tab)),
    }
}

/// The Settings button, lit while the pane is open.
pub(crate) fn button(app: &App) -> Element<'_, Message> {
    app.controls().button(
        Control::new(
            ControlId::action(View::ControlBar, "settings", ControlOwner::Song),
            Id::Settings,
            Button {
                face: Face::Text("Settings".into()),
                latched: true,
            },
        )
        .lit(matches!(app.overlay, Some(Overlay::Settings(_)))),
        Message::Settings(Action::Toggle),
    )
}

/// The pane over a click-away layer: tabs across the top, the shown tab's body below.
pub(crate) fn view(app: &App, shown: Tab) -> Element<'_, Message> {
    let controls = app.controls();
    let tabs = controls.view_tabs(Tab::ALL.map(|tab| {
        (
            Control::new(
                ControlId::action(View::Settings, tab.name(), ControlOwner::Song),
                tab.help(),
                ViewTab {
                    label: tab.label().into(),
                },
            )
            .lit(tab == shown),
            Message::Settings(Action::ChooseTab(tab)),
        )
    }));
    let close = controls.button(
        Control::new(
            ControlId::action(View::Settings, "settings-close", ControlOwner::Song),
            Id::SettingsClose,
            Button {
                face: Face::Text("Close".into()),
                latched: false,
            },
        ),
        Message::CloseOverlay,
    );
    let body = match shown {
        Tab::Midi => crate::remote::tab(app),
    };
    let content = column![
        row![
            controls
                .readout("Settings", crate::control::State::default())
                .width(Length::Fill),
            close
        ]
        .align_y(Alignment::Center),
        crate::control::rule::rule(
            &app.theme,
            crate::control::rule::Axis::Horizontal,
            crate::control::rule::Level::Light
        ),
        tabs,
        body,
    ]
    .spacing(crate::style::detail::DETAIL_INSET)
    .padding(crate::style::detail::DETAIL_INSET);
    let theme = app.theme;
    let pane = container(content)
        .width(Length::Fill)
        .max_width(crate::style::detail::SETTINGS_PANE_WIDTH)
        .height(crate::style::detail::SETTINGS_PANE_HEIGHT)
        .style(move |_| crate::control::menu::panel_style(&theme));
    stack![
        mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
            .on_press(Message::CloseOverlay),
        container(opaque(pane)).center(Length::Fill),
    ]
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtable_engine::engine::Engine;

    fn app() -> App {
        App::new(
            Engine::new(crate::files::new_project()).unwrap(),
            "120".into(),
            None,
        )
    }

    #[test]
    fn the_settings_button_opens_the_midi_tab_and_a_second_press_closes_it() {
        let mut app = app();
        let _ = crate::update::update(&mut app, Message::Settings(Action::Toggle));
        assert_eq!(app.overlay, Some(Overlay::Settings(Tab::Midi)));
        let _ = crate::update::update(&mut app, Message::Settings(Action::Toggle));
        assert_eq!(app.overlay, None);
    }

    #[test]
    fn choosing_a_tab_keeps_the_pane_open_on_that_tab() {
        let mut app = app();
        let _ = crate::update::update(&mut app, Message::Settings(Action::ChooseTab(Tab::Midi)));
        assert_eq!(app.overlay, Some(Overlay::Settings(Tab::Midi)));
    }

    #[test]
    fn the_settings_button_adds_no_mappable_control_to_the_bar() {
        let app = app();
        let mut bar = crate::transport::view(&app);
        let declared =
            crate::control_focus::test_registry::declared(&mut bar, iced::Size::new(1600.0, 40.0));
        // The Settings button drives no model control, so the bar still declares its five.
        assert_eq!(declared.len(), 5, "{declared:?}");
    }
}
