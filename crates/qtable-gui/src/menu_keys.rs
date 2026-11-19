//! The keyboard for an open pop-up menu or chooser list, after Live (2026-11-15, tour
//! step 14 and L1): a pop-up takes the keyboard. Up and Down move through the rows, Enter
//! chooses, Escape closes, and every other plain key is swallowed so nothing behind the menu
//! reacts, Space included (it never starts the transport).
//!
//! It serves the three popovers that list pressable rows: a context menu, the Sections menu
//! and a clip-editor chooser. Each supplies `Rows` (the message of every row the keyboard can
//! reach, top to bottom) and draws `App::menu_highlight` through `menu::Item::highlighted`.
//! Choosing sends the very message a click on that row sends, so a click and the Enter key
//! cannot drift apart. The route list has its own keys (`output_band/dropdown.rs`) because it
//! is a checklist that stays open; the Settings pane is a pane, not a menu.
use crate::app::App;
use crate::messages::Message;
use crate::overlay::Overlay;
use iced::Task;
use iced::keyboard::{Key, Modifiers, key::Named};

/// The rows the keyboard can reach in the open menu.
pub(crate) struct Rows {
    /// Each enabled row's message, in display order. Disabled rows and separators are left out.
    pub presses: Vec<Message>,
    /// The row that is current when the menu opens (a chooser's chosen entry): the first Up or
    /// Down steps from it.
    pub current: Option<usize>,
}

/// Whether a menu or chooser list holds the keyboard.
pub(crate) fn is_open(app: &App) -> bool {
    matches!(
        app.overlay,
        Some(Overlay::Context(_) | Overlay::SectionsMenu | Overlay::Chooser(_))
    )
}

fn rows(app: &App) -> Option<Rows> {
    match &app.overlay {
        Some(Overlay::Context(open)) => Some(crate::context_menu::rows(app, open)),
        Some(Overlay::SectionsMenu) => Some(crate::sections_menu::rows()),
        Some(Overlay::Chooser(open)) => {
            Some(crate::inspector::lane_view::chooser::key_rows(app, *open))
        }
        _ => None,
    }
}

/// The row after `from` going down or up, stopping at the ends (as the route list does). With
/// no `from`, Down lands on the first row and Up on the last. `None` when there are no rows.
fn step(len: usize, from: Option<usize>, down: bool) -> Option<usize> {
    match (from, down) {
        _ if len == 0 => None,
        (None, true) => Some(0),
        (None, false) => Some(len - 1),
        (Some(i), true) => Some((i + 1).min(len - 1)),
        (Some(i), false) => Some(i.saturating_sub(1).min(len - 1)),
    }
}

/// One key press while a menu may be open. `Some` means the menu took it. Cmd, Ctrl and Alt
/// chords fall through to the shortcuts; a key a text field already took is left to the field,
/// except Escape, which always closes the menu.
pub(crate) fn keyboard(
    app: &mut App,
    key: &Key,
    modifiers: Modifiers,
    ignored: bool,
) -> Option<Task<Message>> {
    if !is_open(app) || modifiers.control() || modifiers.alt() || modifiers.logo() {
        return None;
    }
    if matches!(key.as_ref(), Key::Named(Named::Escape)) {
        app.close_overlay();
        return Some(Task::none());
    }
    if !ignored {
        return None;
    }
    let rows = rows(app)?;
    match key.as_ref() {
        Key::Named(named @ (Named::ArrowUp | Named::ArrowDown)) => {
            let from = app.menu_highlight.or(rows.current);
            app.menu_highlight = step(rows.presses.len(), from, named == Named::ArrowDown);
        }
        Key::Named(Named::Enter) => {
            if let Some(message) = app
                .menu_highlight
                .and_then(|index| rows.presses.get(index).cloned())
            {
                return Some(crate::update::update(app, message));
            }
        }
        // Space, letters, Tab and the rest: the menu holds the keyboard, so none reaches the
        // grid, the transport or a shortcut behind it.
        _ => {}
    }
    Some(Task::none())
}

/// The highlight to draw on the `index`th pressable row: the keyboard's row once it has moved,
/// otherwise `fallback` (a chooser's current entry).
pub(crate) fn highlighted(app: &App, index: usize, fallback: bool) -> bool {
    match app.menu_highlight {
        Some(highlight) => highlight == index,
        None => fallback,
    }
}

#[cfg(test)]
mod tests;
