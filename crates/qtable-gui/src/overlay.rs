//! The one overlay slot: the app
//! holds at most one popover. Opening one replaces the other, and after every update the open
//! one is checked against the model and closed when its target has gone.
use crate::app::App;
use crate::context_menu::{MenuTarget, OpenMenu};
use crate::control::menu::{self, Anchor};
use crate::messages::Message;
use iced::widget::stack;
use iced::{Element, Size};
use qtable_engine::model::TrackId;

/// Width of a dropdown list, frame included.
const DROPDOWN_W: f32 = 192.0;

/// The popover that is open. GUI session state, not part of the project.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Overlay {
    Context(OpenMenu),
    /// The output band's menu: `Some(None)` from the empty area, `Some(Some(id))` from a column.
    SectionsMenu,
    /// The Settings pane, on the tab it shows.
    Settings(crate::settings_pane::Tab),
    Dropdown(OpenDropdown),
    /// The clip editor's Device or Control chooser list.
    Chooser(crate::inspector::lane_view::chooser::OpenChooser),
}

/// A multi-select list: where it opens and whose choices it edits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpenDropdown {
    /// The click, or the face once anchored from the keyboard.
    pub at: Anchor,
    pub owner: DropdownOwner,
    pub highlight: Option<usize>,
    pub anchor_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum DropdownOwner {
    /// The outputs a track or group sends to; inputs will join as a variant.
    Route(TrackId),
}

impl App {
    /// Open `overlay`, closing whichever popover was open.
    pub(crate) fn open_overlay(&mut self, overlay: Overlay) {
        self.menu_highlight = None;
        self.overlay = Some(overlay);
    }

    pub(crate) fn close_overlay(&mut self) {
        self.menu_highlight = None;
        self.overlay = None;
    }

    #[cfg(test)]
    pub(crate) fn menu_target(&self) -> Option<MenuTarget> {
        self.context_menu().map(|menu| menu.target)
    }

    #[cfg(test)]
    pub(crate) fn context_menu(&self) -> Option<OpenMenu> {
        match &self.overlay {
            Some(Overlay::Context(menu)) => Some(menu.clone()),
            _ => None,
        }
    }

    /// Close the open overlay when what it points at no longer exists.
    pub(crate) fn validate_overlay(&mut self) {
        if self
            .overlay
            .as_ref()
            .is_some_and(|overlay| !still_valid(self, overlay))
        {
            self.close_overlay();
        }
    }
}

fn still_valid(app: &App, overlay: &Overlay) -> bool {
    let project = app.project();
    match overlay {
        Overlay::Context(menu) => match &menu.target {
            MenuTarget::Lane(target) => crate::inspector::event_editor::lanes::valid(app, *target),
            MenuTarget::Content(target) => {
                crate::inspector::event_editor::menu::valid_content(app, target)
            }
            MenuTarget::Slot => app.selection.clip.is_some_and(|sel| {
                project
                    .track(sel.track)
                    .is_some_and(|track| sel.slot < track.slots.len())
            }),
            MenuTarget::Argument { subject, revision } => {
                *revision == app.engine.project_revision()
                    && crate::parameter_gesture::subject_field(app, subject).is_some()
            }
            MenuTarget::Device(id) => project.device(*id).is_some(),
            MenuTarget::Header(owner) => project.track_capabilities(*owner).is_some(),
            MenuTarget::Scene(id) => project.scenes.iter().any(|s| s.id == *id),
            MenuTarget::Region(_) => true,
        },
        Overlay::SectionsMenu | Overlay::Settings(_) => true,
        Overlay::Dropdown(dropdown) => dropdown.owner.still_valid(app),
        Overlay::Chooser(chooser) => chooser.still_valid(app),
    }
}

/// The window with the open overlay stacked over it.
///
/// The window is always the first child of one `stack!`, open overlay or not. iced diffs the
/// widget tree frame to frame by widget kind, and a window that moved from being the root to
/// being a stack's child would lose all its state (a scrolled list's offset, say) on the frame a
/// menu opens.
pub(crate) fn view<'a>(app: &'a App, window: Element<'a, Message>) -> Element<'a, Message> {
    let layer: Option<Element<'a, Message>> = match &app.overlay {
        Some(Overlay::Context(menu)) => Some(crate::context_menu::view(app, menu.clone())),
        Some(Overlay::Settings(tab)) => Some(crate::settings_pane::view(app, *tab)),
        Some(Overlay::SectionsMenu) => Some(crate::sections_menu::view(app)),
        Some(Overlay::Chooser(open)) => {
            Some(crate::inspector::lane_view::chooser::view(app, *open))
        }
        Some(Overlay::Dropdown(dropdown)) if !dropdown.anchor_pending => {
            Some(dropdown_layer(app, *dropdown))
        }
        Some(Overlay::Dropdown(_)) | None => None,
    };
    match layer {
        Some(layer) => stack![window, layer].into(),
        None => stack![window].into(),
    }
}

/// The open list over a click-away layer, on the side of its anchor with room.
fn dropdown_layer<'a>(app: &'a App, dropdown: OpenDropdown) -> Element<'a, Message> {
    menu::layer(dropdown.at, Message::CloseOverlay, move || {
        let (list, height) = dropdown.owner.list(app, dropdown.highlight);
        let size = Size::new(DROPDOWN_W, menu::panel_height(height));
        (menu::panel(list, size.width, &app.theme).into(), size)
    })
}

#[cfg(test)]
mod tests;
