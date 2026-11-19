//! Which window region has keyboard focus. Like Live, only one region is active at a time;
//! it wears a light outline, and the arrow keys act on the Session grid only while it is active.
use crate::messages::Message;
use iced::keyboard::{Key, Modifiers, key::Named};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Focus {
    #[default]
    Grid,
    Detail,
    Browser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// A keyboard command for the Session grid, ignored unless the grid has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GridKey {
    /// Arrow keys: move the selection between track titles, clip slots and scenes.
    Move(Direction),
    /// Command-arrow: move the selected track (left/right) or scene (up/down).
    Reorder(Direction),
    /// Return: launch the selected clip or scene.
    Launch,
}

/// Live 12 manual, Accessibility and Keyboard Navigation (fetched 2026-10-11):
/// Option-1 focuses the Session View, Option-3 the Clip/Detail View, Option-5 the Browser.
/// Match the base key, since Option turns digits into other characters.
pub(crate) fn focus_shortcut(key: &Key, modifiers: Modifiers, repeat: bool) -> Option<Focus> {
    if repeat || modifiers != Modifiers::ALT {
        return None;
    }
    match key.as_ref() {
        Key::Character("1") => Some(Focus::Grid),
        Key::Character("3") => Some(Focus::Detail),
        Key::Character("5") => Some(Focus::Browser),
        _ => None,
    }
}

/// Arrow keys and Return. A text field that consumed the key keeps it, so callers pass
/// whether the event is still unhandled.
pub(crate) fn grid_key(key: &Key, modifiers: Modifiers, ignored: bool) -> Option<GridKey> {
    grid_key_for_platform(key, modifiers, ignored, cfg!(target_os = "macos"))
}

// Keep the platform mapping explicit so both paths can be tested on either host.
fn grid_key_for_platform(
    key: &Key,
    modifiers: Modifiers,
    ignored: bool,
    macos: bool,
) -> Option<GridKey> {
    if !ignored || modifiers.alt() || (macos && modifiers.control()) || modifiers.shift() {
        return None;
    }
    // iced's Command is Logo on macOS and Ctrl elsewhere; Ctrl must not be rejected there.
    let command = if macos {
        modifiers.logo()
    } else {
        modifiers.control()
    };
    let direction = match key.as_ref() {
        Key::Named(Named::ArrowLeft) => Some(Direction::Left),
        Key::Named(Named::ArrowRight) => Some(Direction::Right),
        Key::Named(Named::ArrowUp) => Some(Direction::Up),
        Key::Named(Named::ArrowDown) => Some(Direction::Down),
        _ => None,
    };
    match (direction, command) {
        (Some(direction), false) => Some(GridKey::Move(direction)),
        (Some(direction), true) => Some(GridKey::Reorder(direction)),
        (None, false) if matches!(key.as_ref(), Key::Named(Named::Enter)) => Some(GridKey::Launch),
        _ => None,
    }
}

/// Route one keyboard event to a message: focus moves anywhere, grid keys only reach the grid.
pub(crate) fn event_message(
    key: &Key,
    modifiers: Modifiers,
    repeat: bool,
    ignored: bool,
) -> Option<Message> {
    focus_shortcut(key, modifiers, repeat)
        .map(Message::SetFocus)
        .or_else(|| {
            crate::clip_shortcuts::key(key, modifiers, repeat, ignored).map(Message::ClipShortcut)
        })
        .or_else(|| grid_key(key, modifiers, ignored).map(Message::GridKey))
        .or_else(|| crate::mixer_band::key(key, modifiers, repeat, ignored).map(Message::Mixer))
        .or_else(|| match key.as_ref() {
            Key::Named(Named::Escape) => Some(Message::CancelField),
            Key::Named(Named::Tab) => Some(Message::TabField(modifiers.shift())),
            _ => None,
        })
}

/// The outline the active region wears. Provisional: no Live screenshot of
/// its focus highlight was to hand on 2026-10-11, so this uses `ControlForeground` (#b5b5b5) in place of
/// the usual `SurfaceHighlight` border.
pub(crate) const OUTLINE: iced::Color = crate::style::TEXT;

/// Container border for a region, light when the region has focus.
pub(crate) fn border(active: bool) -> iced::Border {
    iced::Border {
        color: if active {
            OUTLINE
        } else {
            crate::style::HIGHLIGHT
        },
        width: crate::style::detail::REGION_FRAME,
        radius: crate::control::LIVE_DARK.corner_control.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_digits_pick_a_region_and_nothing_else_does() {
        let one = Key::Character("1".into());
        assert_eq!(
            focus_shortcut(&one, Modifiers::ALT, false),
            Some(Focus::Grid)
        );
        assert_eq!(
            focus_shortcut(&Key::Character("3".into()), Modifiers::ALT, false),
            Some(Focus::Detail)
        );
        assert_eq!(
            focus_shortcut(&Key::Character("5".into()), Modifiers::ALT, false),
            Some(Focus::Browser)
        );
        assert_eq!(focus_shortcut(&one, Modifiers::NONE, false), None);
        assert_eq!(focus_shortcut(&one, Modifiers::ALT, true), None);
        assert_eq!(
            focus_shortcut(&one, Modifiers::ALT | Modifiers::COMMAND, false),
            None
        );
    }

    #[test]
    fn arrows_move_command_arrows_reorder_and_fields_keep_their_keys() {
        let left = Key::Named(Named::ArrowLeft);
        assert_eq!(
            grid_key(&left, Modifiers::NONE, true),
            Some(GridKey::Move(Direction::Left))
        );
        assert_eq!(
            grid_key(&left, Modifiers::COMMAND, true),
            Some(GridKey::Reorder(Direction::Left))
        );
        assert_eq!(grid_key(&left, Modifiers::NONE, false), None);
        assert_eq!(grid_key(&left, Modifiers::SHIFT, true), None);
        assert_eq!(
            grid_key(&Key::Named(Named::Enter), Modifiers::NONE, true),
            Some(GridKey::Launch)
        );
        assert_eq!(
            grid_key(&Key::Named(Named::Enter), Modifiers::COMMAND, true),
            None
        );
    }

    #[test]
    fn grid_modifier_mapping_covers_macos_and_linux_on_either_host() {
        for macos in [true, false] {
            let command = if macos {
                Modifiers::LOGO
            } else {
                Modifiers::CTRL
            };
            for (arrow, direction) in [
                (Named::ArrowLeft, Direction::Left),
                (Named::ArrowRight, Direction::Right),
                (Named::ArrowUp, Direction::Up),
                (Named::ArrowDown, Direction::Down),
            ] {
                let key = Key::Named(arrow);
                assert_eq!(
                    grid_key_for_platform(&key, Modifiers::NONE, true, macos),
                    Some(GridKey::Move(direction))
                );
                assert_eq!(
                    grid_key_for_platform(&key, command, true, macos),
                    Some(GridKey::Reorder(direction))
                );
                for extra in [Modifiers::ALT, Modifiers::SHIFT] {
                    assert_eq!(
                        grid_key_for_platform(&key, command | extra, true, macos),
                        None
                    );
                }
                assert_eq!(grid_key_for_platform(&key, command, false, macos), None);
                if macos {
                    for modifiers in [Modifiers::CTRL, Modifiers::CTRL | Modifiers::LOGO] {
                        assert_eq!(grid_key_for_platform(&key, modifiers, true, macos), None);
                    }
                }
            }
            let enter = Key::Named(Named::Enter);
            assert_eq!(
                grid_key_for_platform(&enter, Modifiers::NONE, true, macos),
                Some(GridKey::Launch)
            );
            assert_eq!(grid_key_for_platform(&enter, command, true, macos), None);
            assert_eq!(
                grid_key_for_platform(&enter, Modifiers::CTRL, true, macos),
                None
            );
        }
    }

    #[test]
    fn unhandled_route_keys_use_the_existing_grid_and_escape_paths() {
        assert!(matches!(
            event_message(&Key::Named(Named::ArrowDown), Modifiers::NONE, false, true),
            Some(Message::GridKey(GridKey::Move(Direction::Down)))
        ));
        assert!(matches!(
            event_message(&Key::Named(Named::Enter), Modifiers::NONE, false, true),
            Some(Message::GridKey(GridKey::Launch))
        ));
        assert!(matches!(
            event_message(&Key::Named(Named::Escape), Modifiers::NONE, false, true),
            Some(Message::CancelField)
        ));
        assert!(event_message(&Key::Named(Named::Enter), Modifiers::NONE, false, false).is_none());
    }
}
