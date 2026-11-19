//! Route dropdown interaction belongs to the routing band; the overlay only holds and paints it.
use super::*;
use crate::control::menu::Anchor;
use crate::overlay::{DropdownOwner, OpenDropdown, Overlay};
use iced::keyboard::{Key, Modifiers, key::Named};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteKey {
    Up,
    Down,
    Space,
    Return,
    Escape,
}

pub(crate) fn key(key: &Key, modifiers: Modifiers, ignored: bool) -> Option<RouteKey> {
    if !ignored || modifiers != Modifiers::NONE {
        return None;
    }
    match key.as_ref() {
        Key::Named(Named::ArrowUp) => Some(RouteKey::Up),
        Key::Named(Named::ArrowDown) => Some(RouteKey::Down),
        Key::Named(Named::Space) | Key::Character(" ") => Some(RouteKey::Space),
        Key::Named(Named::Enter) => Some(RouteKey::Return),
        Key::Named(Named::Escape) => Some(RouteKey::Escape),
        _ => None,
    }
}

pub(crate) fn keyboard(
    app: &mut App,
    event_key: &Key,
    modifiers: Modifiers,
    ignored: bool,
) -> Option<iced::Task<Message>> {
    let key = key(event_key, modifiers, ignored)?;
    if let Some(Overlay::Dropdown(mut dropdown)) = app.overlay {
        let DropdownOwner::Route(owner) = dropdown.owner;
        let (rows, choices) = route_rows(app.project(), &route_targets(app, owner));
        match key {
            RouteKey::Escape => app.close_overlay(),
            RouteKey::Up | RouteKey::Down => {
                dropdown.highlight =
                    multi_select::step(&rows, dropdown.highlight, key == RouteKey::Down);
                app.overlay = Some(Overlay::Dropdown(dropdown));
            }
            RouteKey::Space | RouteKey::Return => {
                if let Some(choice) = dropdown
                    .highlight
                    .and_then(|index| choices.get(index).copied().flatten())
                {
                    return Some(crate::update::update(
                        app,
                        Message::Outputs(crate::messages::OutputMessage::ChooseRoute(
                            owner, choice,
                        )),
                    ));
                }
            }
        }
        return Some(iced::Task::none());
    }
    if matches!(key, RouteKey::Space | RouteKey::Return | RouteKey::Down)
        && app.pending_field.is_none()
        && let Some(owner) = app
            .control_focus
            .track_action(crate::control_focus::View::Mixer, "route")
        && app
            .project()
            .track_capabilities(owner)
            .is_some_and(|caps| caps.routes)
    {
        return Some(app.open_route_dropdown_at_face(owner));
    }
    None
}

impl App {
    pub(crate) fn open_route_dropdown_at_face(&mut self, owner: TrackId) -> iced::Task<Message> {
        self.open_route_dropdown(owner);
        if let Some(Overlay::Dropdown(dropdown)) = &mut self.overlay {
            dropdown.anchor_pending = true;
        }
        let generation = self.project_generation;
        let id = crate::drafts::ControlId::action(
            crate::control_focus::View::Mixer,
            "route",
            crate::control_focus::ControlOwner::Track(owner),
        );
        crate::control_focus::bounds(id)
            .map(move |bounds| Message::RouteAnchored(owner, generation, bounds))
    }

    pub(crate) fn anchor_route_dropdown(
        &mut self,
        owner: TrackId,
        generation: u64,
        bounds: Option<iced::Rectangle>,
    ) {
        if generation != self.project_generation {
            return;
        }
        if let Some(Overlay::Dropdown(dropdown)) = &mut self.overlay
            && dropdown.owner == DropdownOwner::Route(owner)
            && dropdown.anchor_pending
        {
            if let Some(bounds) = bounds {
                dropdown.at = Anchor::below(bounds, 2.0);
                dropdown.anchor_pending = false;
            } else {
                self.close_overlay();
            }
        }
    }
    pub(crate) fn open_route_dropdown(&mut self, owner: TrackId) {
        let (rows, _) = route_rows(self.project(), &route_targets(self, owner));
        let highlight = rows
            .iter()
            .position(|row| matches!(row, Row::Item(item) if item.mark == Mark::Ticked))
            .or_else(|| multi_select::step(&rows, None, true));
        self.open_overlay(Overlay::Dropdown(OpenDropdown {
            at: Anchor::point(self.cursor),
            owner: DropdownOwner::Route(owner),
            highlight,
            anchor_pending: false,
        }));
    }
    pub(crate) fn route_dropdown(&self) -> Option<TrackId> {
        match self.overlay {
            Some(Overlay::Dropdown(OpenDropdown {
                owner: DropdownOwner::Route(owner),
                ..
            })) => Some(owner),
            _ => None,
        }
    }
}

impl DropdownOwner {
    pub(crate) fn list(self, app: &App, highlight: Option<usize>) -> (Element<'_, Message>, f32) {
        match self {
            Self::Route(owner) => route_list(app, owner, highlight),
        }
    }
    pub(crate) fn still_valid(self, app: &App) -> bool {
        match self {
            Self::Route(owner) => app.project().track(owner).is_some(),
        }
    }
}
