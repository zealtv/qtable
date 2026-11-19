//! MIDI and Key Map Modes, after Live's: the MIDI/KEY switches, blue/red overlays on controls
//! a controller can drive, click-then-input learn, source labels, and the mapping list in
//! the Browser's place.
//!
//! The mappable controls come from the controls themselves: each declares `Identity.drives`
//! (`control::help`), and the window's `map_layer` finds them with a widget operation. Nothing
//! lists them here. A mapping is only ever changed through `ProjectEdit::EditMapping`, so Map
//! Mode is one more client of the engine's command layer.
//! Mode and Takeover are
//! set per mapping, a deliberate departure from Live. As of v3.10 the list keeps Live's
//! identifying columns and the inspector below it holds every setting of the selected row.
#[cfg(any(test, feature = "dev-hooks"))]
mod capture;
mod inspector;
pub(crate) mod keys;
pub(crate) mod labels;
mod list;

use crate::app::App;
use crate::control::map_layer::Mark;
use crate::drafts::DraftKey;
use qtable_engine::edit::{MappingEdit, ProjectEdit};
use qtable_engine::engine::Command;
use qtable_engine::model::{
    ControlDescriptor, ControlRef, MappingId, MidiMessage, RemoteMapping, Source, SourceMode,
    Takeover, describe_control,
};
use qtable_remote::{Envelope, Gesture, Input, midi::decode};
use std::collections::HashMap;

#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) use capture::capture;
pub(crate) use list::view as list_view;

/// Mutually exclusive learn modes, as in Live.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Midi,
    Key,
}

/// Session state of Map Mode; not part of the set.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// Whether either learn mode is active.
    pub on: bool,
    pub mode: Mode,
    /// The control waiting for a controller message: the next one maps it.
    pub selected_control: Option<ControlRef>,
    /// The selected row of the mapping list; Delete removes it.
    pub selected_row: Option<MappingId>,
    /// What the list last saw of its rows' scrolling, to keep the selected row in view.
    pub scroll: ListScroll,
}

/// The list's rows viewport and what the update loop already scrolled for, after the Session
/// grid's `scene_follow`: the list follows its selection once per change of selection or
/// viewport height, so wheel scrolling elsewhere is left alone.
#[derive(Debug, Default)]
pub(crate) struct ListScroll {
    /// The rows' viewport height in px, written by the list's view.
    pub viewport: std::cell::Cell<f32>,
    offset: f32,
    handled: (Option<usize>, u32),
    seen: Option<ListKey>,
}

/// What `follow` last looked at: a change of any of these can move the selected row, and
/// nothing else can, so while it holds still `follow` does no work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ListKey {
    row: Option<MappingId>,
    revision: u64,
    mode: Mode,
    viewport: u32,
}

impl State {
    /// A replacement project keeps the switch but drops what pointed into the old one.
    pub(crate) fn reset_for_project(&mut self) {
        self.selected_control = None;
        self.selected_row = None;
    }
}

/// A setting of the selected mapping, as the inspector shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Cell {
    Min,
    Max,
    Mode,
    Takeover,
    Sensitivity,
    Invert,
}

#[derive(Debug, Clone)]
pub(crate) enum Action {
    /// The MIDI switch.
    Toggle,
    ToggleKey,
    /// A mappable control was clicked in Map Mode.
    Pick(ControlRef),
    SelectRow(MappingId),
    /// Up or Down while the list has focus: the next row above or below the selection.
    MoveSelection {
        down: bool,
    },
    /// A key Map Mode claims but has no use for: nothing happens, and nothing falls through.
    Ignore,
    /// The list's rows scrolled to this offset in px.
    ListScrolled(f32),
    /// Delete: remove the selected mapping, whether picked from the list or on its control.
    /// With none selected it does nothing, and nothing falls through to editing.
    DeleteSelected,
    /// A typed draft of Min, Max or Sensitivity.
    FieldChanged(MappingId, Cell, String),
    FieldSubmitted(MappingId, Cell),
    /// A Min or Max chosen from a list (an Enum's choice, a switch's Off or On).
    LimitChosen(MappingId, Cell, f64),
    SetMode(MappingId, SourceMode),
    SetTakeover(MappingId, Takeover),
    /// Swap Min and Max, as Live's Invert command does: Min above Max is the inverted mapping.
    Invert(MappingId),
}

pub(crate) fn update(app: &mut App, action: Action) {
    match action {
        Action::Toggle | Action::ToggleKey => {
            let mode = if matches!(action, Action::ToggleKey) {
                Mode::Key
            } else {
                Mode::Midi
            };
            app.map_mode.on = !app.map_mode.on || app.map_mode.mode != mode;
            app.map_mode.mode = mode;
            app.map_mode.selected_control = None;
            app.map_mode.selected_row = None;
            app.control_focus.clear();
            app.close_overlay();
            app.info.update(crate::info::Hover::Clear);
        }
        Action::Pick(control) => {
            app.map_mode.selected_row = app
                .project()
                .mappings
                .iter()
                .find(|row| {
                    source_in_mode(&row.source, app.map_mode.mode) && row.control == control
                })
                .map(|row| row.id);
            app.map_mode.selected_control = Some(control);
        }
        Action::SelectRow(id) => {
            app.map_mode.selected_row = Some(id);
            app.control_focus.clear();
            app.focus = crate::focus::Focus::Browser;
        }
        Action::MoveSelection { down } => {
            let ids: Vec<MappingId> = shown(app).map(|(row, _)| row.id).collect();
            let at = app
                .map_mode
                .selected_row
                .and_then(|id| ids.iter().position(|&row| row == id));
            let next = match at {
                Some(at) if down => (at + 1).min(ids.len().saturating_sub(1)),
                Some(at) => at.saturating_sub(1),
                None if down => 0,
                None => ids.len().saturating_sub(1),
            };
            if let Some(&id) = ids.get(next) {
                app.map_mode.selected_row = Some(id);
            }
        }
        Action::Ignore => {}
        Action::ListScrolled(offset) => app.map_mode.scroll.offset = offset,
        Action::DeleteSelected => {
            // A row already gone (removed from elsewhere) leaves nothing to clear.
            if let Some(id) = app.map_mode.selected_row.take()
                && row(app, id).is_some()
            {
                edit(app, MappingEdit::Remove { mapping: id }, None);
            }
        }
        Action::FieldChanged(id, cell, text) => {
            app.drafts.values.insert(DraftKey::Mapping(id, cell), text);
        }
        Action::FieldSubmitted(id, cell) => submit_field(app, id, cell),
        Action::LimitChosen(id, cell, value) => set_limit(app, id, cell, value),
        Action::SetMode(mapping, mode) => {
            edit(app, MappingEdit::SetMode { mapping, mode }, None);
        }
        Action::SetTakeover(mapping, takeover) => {
            edit(app, MappingEdit::SetTakeover { mapping, takeover }, None);
        }
        Action::Invert(mapping) => {
            if let Some(current) = row(app, mapping) {
                let (min, max) = (current.max, current.min);
                edit(app, MappingEdit::SetRange { mapping, min, max }, None);
            }
        }
    }
}

/// The rows the list shows, in order: the active mode's mappings whose control still resolves,
/// each with its control's descriptor.
pub(crate) fn shown(app: &App) -> impl Iterator<Item = (&RemoteMapping, ControlDescriptor)> {
    let mode = app.map_mode.mode;
    app.project()
        .mappings
        .iter()
        .filter(move |row| source_in_mode(&row.source, mode))
        .filter_map(|row| Some((row, describe_control(app.project(), &row.control)?)))
}

/// Scroll the list when its selected row is out of view, once per change of selection,
/// project revision, mode or viewport height (`ListScroll`). It runs on every Tick, so an
/// unchanged state costs a comparison and describes no mapping.
pub(crate) fn follow(app: &mut App) -> iced::Task<crate::messages::Message> {
    if !app.map_mode.on {
        return iced::Task::none();
    }
    let viewport = app.map_mode.scroll.viewport.get();
    let key = ListKey {
        row: app.map_mode.selected_row,
        revision: app.project_revision(),
        mode: app.map_mode.mode,
        viewport: viewport.round() as u32,
    };
    if viewport <= 0.0 || app.map_mode.scroll.seen == Some(key) {
        return iced::Task::none();
    }
    app.map_mode.scroll.seen = Some(key);
    // Something that can move the row changed: find it, and scroll only if its place or the
    // viewport differs from what was last scrolled for, so an unrelated edit leaves a list
    // the user wheel-scrolled where it is.
    let at = app
        .map_mode
        .selected_row
        .and_then(|id| shown(app).position(|(row, _)| row.id == id));
    let scroll = &mut app.map_mode.scroll;
    let placed = (at, key.viewport);
    if scroll.handled == placed {
        return iced::Task::none();
    }
    scroll.handled = placed;
    let Some(offset) = at.and_then(|at| list::offset_to_show(at, scroll.offset, viewport)) else {
        return iced::Task::none();
    };
    scroll.offset = offset;
    list::scroll_rows(offset)
}

fn edit(app: &mut App, edit: MappingEdit, draft: Option<DraftKey>) -> bool {
    crate::edit_actions::run_edit_report(app, Command::Edit(ProjectEdit::EditMapping(edit)), draft)
        .is_some()
}

fn row(app: &App, id: MappingId) -> Option<&RemoteMapping> {
    app.project().mappings.iter().find(|row| row.id == id)
}

/// The meaning of a row's Min and Max, from its control as it stands now.
pub(crate) fn row_limits(app: &App, row: &RemoteMapping) -> Option<labels::Limits> {
    let descriptor = describe_control(app.project(), &row.control)?;
    Some(labels::limits(&row.control, &descriptor))
}

fn set_limit(app: &mut App, id: MappingId, cell: Cell, value: f64) {
    let Some(current) = row(app, id) else { return };
    let (min, max) = match cell {
        Cell::Min => (value, current.max),
        _ => (current.min, value),
    };
    edit(
        app,
        MappingEdit::SetRange {
            mapping: id,
            min,
            max,
        },
        Some(DraftKey::Mapping(id, cell)),
    );
}

fn submit_field(app: &mut App, id: MappingId, cell: Cell) {
    let key = DraftKey::Mapping(id, cell);
    let Some(text) = app.drafts.values.get(&key).cloned() else {
        return;
    };
    let parsed = if cell == Cell::Sensitivity {
        labels::parse_sensitivity(&text)
    } else {
        let Some(limits) = row(app, id).and_then(|row| row_limits(app, row)) else {
            return;
        };
        labels::parse_limit(&limits, &text)
    };
    match parsed {
        Ok(sensitivity) if cell == Cell::Sensitivity => {
            edit(
                app,
                MappingEdit::SetSensitivity {
                    mapping: id,
                    sensitivity,
                },
                Some(key),
            );
        }
        Ok(value) => set_limit(app, id, cell, value),
        Err(error) => {
            app.drafts.errors.insert(key, error.clone());
            app.status.show_error(error);
        }
    }
}

/// Commit a Min, Max or Sensitivity draft whose field lost focus.
pub(crate) fn commit(app: &mut App, id: MappingId, cell: Cell) {
    submit_field(app, id, cell);
}

/// While Map Mode is on, input from its source kind maps the selected control instead of operating the set.
/// The first message that can be a source (a CC, a note-on, pitch bend) adds a row; a source
/// that already drives this control just selects its row. Repeats of a source already mapped
/// to other controls fan out, as in Live, with no prompt. A different source replaces this
/// control's mapping of the same kind through the model edit.
pub(crate) fn learn(app: &mut App, events: &[Envelope]) {
    let Some(control) = app.map_mode.selected_control.clone() else {
        return;
    };
    let Some(source) = events
        .iter()
        .filter_map(learnable)
        .find(|source| source_in_mode(source, app.map_mode.mode))
    else {
        return;
    };
    let Some(descriptor) = describe_control(app.project(), &control) else {
        app.map_mode.selected_control = None;
        return;
    };
    app.map_mode.selected_control = None;
    if let Some(existing) = app
        .project()
        .mappings
        .iter()
        .find(|row| row.source == source && row.control == control)
    {
        app.map_mode.selected_row = Some(existing.id);
        return;
    }
    let (min, max) = labels::learn_range(&control, &descriptor);
    let next = app.project().next_mapping_id().ok();
    if edit(
        app,
        MappingEdit::Add {
            source,
            control,
            min,
            max,
        },
        None,
    ) {
        app.map_mode.selected_row = next;
    }
}

/// The source a message would be learned as. A note-off and an unknown message teach nothing.
fn learnable(event: &Envelope) -> Option<Source> {
    if let Input::Key {
        key,
        pressed: true,
        repeat: false,
    } = &event.input
    {
        return Some(Source::Key(qtable_engine::model::KeySource {
            key: qtable_engine::model::KeyName(key.clone()),
        }));
    }
    let Input::Midi(bytes) = &event.input else {
        return None;
    };
    let message = decode::decode(bytes)?;
    match (
        message.source.message,
        message.gesture(SourceMode::Absolute),
    ) {
        (MidiMessage::Note(_), Gesture::Release) => None,
        _ => Some(Source::Midi(message.source)),
    }
}

/// How the layer marks each mappable control: its source in the active mode, and whether
/// it is the one waiting for a controller message.
pub(crate) fn marks(app: &App) -> HashMap<ControlRef, Mark> {
    let mut marks: HashMap<ControlRef, Mark> = HashMap::new();
    for row in app
        .project()
        .mappings
        .iter()
        .filter(|row| source_in_mode(&row.source, app.map_mode.mode))
    {
        let label = labels::short(&row.source);
        let mark = marks.entry(row.control.clone()).or_default();
        mark.label = Some(label);
    }
    if let Some(control) = &app.map_mode.selected_control {
        marks.entry(control.clone()).or_default().selected = true;
    }
    marks
}

pub(crate) fn source_in_mode(source: &Source, mode: Mode) -> bool {
    matches!(
        (source, mode),
        (Source::Midi(_), Mode::Midi) | (Source::Key(_), Mode::Key)
    )
}

pub(crate) fn ink(app: &App) -> iced::Color {
    match app.map_mode.mode {
        Mode::Midi => app.theme.learn_midi,
        Mode::Key => app.theme.learn_key,
    }
}

/// While Map Mode is on, a bare Delete or Backspace is Map Mode's: it clears the selected
/// mapping of the active mode (a mapped control picked on the window, or a row picked in the
/// list) and never reaches the editing meanings (clip, track, device, fader reset). Up and
/// Down move the list's selection while the list has focus: the Browser's column is focused
/// and no inspector control holds the click outline. With the Browser's column focused, any
/// other Up, Down or Enter is swallowed too, so it cannot reach the hidden Browser. A key a text or number field took
/// (`ignored` false, as a selected number's nudge is) or a field being edited keeps its own
/// meaning.
pub(crate) fn keyboard(
    app: &App,
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
    ignored: bool,
) -> Option<Action> {
    use iced::keyboard::{Key, key::Named};
    let free = app.map_mode.on
        && ignored
        && app.pending_field.is_none()
        && app.clip_actions.renaming.is_none()
        && app.header_actions.renaming.is_none()
        && app.device_chain.renaming.is_none()
        && modifiers.is_empty();
    if !free {
        return None;
    }
    let browser_focused = app.focus == crate::focus::Focus::Browser;
    let list_focused = browser_focused
        && !app
            .control_focus
            .current()
            .is_some_and(|id| id.view() == crate::control_focus::View::Mappings);
    match key.as_ref() {
        Key::Named(Named::Delete | Named::Backspace) => Some(Action::DeleteSelected),
        Key::Named(Named::ArrowUp) if list_focused => Some(Action::MoveSelection { down: false }),
        Key::Named(Named::ArrowDown) if list_focused => Some(Action::MoveSelection { down: true }),
        // The list stands where the Browser stood and shares its focus. A key the list or an
        // inspector control left unused (an arrow with a toggle or dropdown focused, Enter)
        // stops here, so it cannot select or load a device in the Browser underneath.
        Key::Named(Named::ArrowUp | Named::ArrowDown | Named::Enter) if browser_focused => {
            Some(Action::Ignore)
        }
        _ => None,
    }
}

/// ⌘M, Live's MIDI Map Mode switch.
pub(crate) fn shortcut(
    key: &iced::keyboard::Key,
    modifiers: iced::keyboard::Modifiers,
    repeat: bool,
) -> bool {
    !repeat
        && modifiers == iced::keyboard::Modifiers::COMMAND
        && matches!(key.as_ref(), iced::keyboard::Key::Character("m" | "M"))
}

/// A mode switch in the Control Bar, lit while its Map Mode is on.
pub(crate) fn switch(app: &App, mode: Mode) -> iced::Element<'_, crate::messages::Message> {
    use crate::control::{Button, Face};
    use crate::control_focus::{ControlOwner, View};
    use crate::drafts::{Control, ControlId};
    let key = mode == Mode::Key;
    app.controls().button(
        Control::new(
            ControlId::action(
                View::ControlBar,
                if key { "key-map" } else { "midi-map" },
                ControlOwner::Song,
            ),
            if key {
                crate::info::Id::KeyMapSwitch
            } else {
                crate::info::Id::MapSwitch
            },
            Button {
                face: Face::Text(if key { "KEY" } else { "MIDI" }.into()),
                latched: true,
            },
        )
        .lit(app.map_mode.on && app.map_mode.mode == mode),
        crate::messages::Message::Map(if key {
            Action::ToggleKey
        } else {
            Action::Toggle
        }),
    )
}

#[cfg(test)]
mod argument_tests;
#[cfg(test)]
mod tests;
