//! Horizontal track traits: one mixed header selection, clipboard and guarded edits.
use super::*;
use crate::clip_shortcuts::Shortcut;
use crate::context_menu::{MenuTarget, OpenMenu};
use qtable_engine::edit::Carried;
use qtable_engine::model::Lane;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Identity {
    Event(EventRef),
    Envelope(ArgumentRef),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Selection {
    pub key: ClipKey,
    pub anchor: Identity,
    pub items: Vec<Identity>,
    /// Header commands take priority over the point/hit editor only after a header press.
    pub headers: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Target {
    pub key: ClipKey,
    pub revision: u64,
    pub row: usize,
}
#[derive(Clone)]
pub(crate) struct Clipboard {
    lanes: Vec<Lane>,
    hits: Vec<Event>,
    carried: Vec<Carried>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IdentityRef<'a> {
    Event(&'a EventRef),
    Envelope(&'a ArgumentRef),
}
impl IdentityRef<'_> {
    pub(crate) fn owned(self) -> Identity {
        match self {
            Self::Event(event) => Identity::Event(event.clone()),
            Self::Envelope(argument) => Identity::Envelope(argument.clone()),
        }
    }
    pub(crate) fn matches(self, identity: &Identity) -> bool {
        match (self, identity) {
            (Self::Event(a), Identity::Event(b)) => a == b,
            (Self::Envelope(a), Identity::Envelope(b)) => a == b,
            _ => false,
        }
    }
}

pub(crate) fn identities(clip: &Clip) -> Vec<IdentityRef<'_>> {
    let mut ids: Vec<_> = clip
        .lanes
        .iter()
        .map(|lane| match lane {
            Lane::Events(event) => IdentityRef::Event(&event.definition),
            Lane::Envelope(envelope) => IdentityRef::Envelope(&envelope.target),
        })
        .collect();
    // Preserve lane_refs' first-hit order for event lanes without an explicit row.
    for event in &clip.events {
        let id = IdentityRef::Event(&event.definition);
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// Rows available on the track, independent of this clip's stored event lanes or hits.
/// Envelopes follow the generated event rows in their existing clip order.
pub(crate) fn displayed(track: Option<&qtable_engine::model::Track>, clip: &Clip) -> Vec<Identity> {
    let mut ids = Vec::new();
    if let Some(track) = track {
        for (device, target) in track.targets() {
            ids.extend(target.events.iter().map(|event| {
                Identity::Event(EventRef {
                    device: device.id,
                    key: event.key.clone(),
                })
            }));
        }
    }
    ids.extend(
        clip.lanes
            .iter()
            .filter_map(|lane| lane.envelope())
            .map(|envelope| Identity::Envelope(envelope.target.clone())),
    );
    ids
}

fn displayed_in(app: &App, key: ClipKey, clip: &Clip) -> Vec<Identity> {
    displayed(app.project().track(key.track), clip)
}

/// Generated event headers have no structural lane commands.
pub(crate) fn structural_selection(app: &App, target: Target) -> bool {
    if !valid(app, target) {
        return false;
    }
    let clip = clip(app.project(), target.key.track, target.key.slot).unwrap();
    let ids = displayed_in(app, target.key, clip);
    matches!(ids.get(target.row), Some(Identity::Envelope(_)))
        && !app
            .clip_view
            .editor
            .lanes
            .as_ref()
            .is_some_and(|s| s.items.iter().any(|id| matches!(id, Identity::Event(_))))
}

fn values(clip: &Clip) -> Vec<Lane> {
    let mut lanes = clip.lanes.clone();
    for reference in clip.lane_refs() {
        if !lanes
            .iter()
            .any(|l| matches!(l, Lane::Events(r) if r.definition == reference))
        {
            lanes.push(Lane::Events(qtable_engine::model::EventLane {
                definition: reference,
            }));
        }
    }
    lanes
}
pub(crate) fn valid(app: &App, target: Target) -> bool {
    active(app, target.key)
        && app.chrome.detail_open
        && app.selection.inspector_tab == crate::inspector_panel::Tab::Clip
        && target.revision == app.project_revision()
        && clip(app.project(), target.key.track, target.key.slot)
            .is_some_and(|c| target.row < displayed_in(app, target.key, c).len())
}
pub(super) fn reconcile(app: &mut App) {
    let valid = app.clip_view.editor.lanes.as_ref().is_some_and(|s| {
        active(app, s.key)
            && clip(app.project(), s.key.track, s.key.slot).is_some_and(|c| {
                let ids = displayed_in(app, s.key, c);
                s.items.iter().all(|id| ids.contains(id))
            })
    });
    if !valid {
        app.clip_view.editor.lanes = None;
    }
}
pub(crate) fn rows(
    editor: &State,
    key: ClipKey,
    track: Option<&qtable_engine::model::Track>,
    clip: &Clip,
) -> Vec<usize> {
    let Some(selection) = editor.lanes.as_ref().filter(|s| s.key == key) else {
        return vec![];
    };
    displayed(track, clip)
        .iter()
        .enumerate()
        .filter_map(|(i, id)| selection.items.iter().any(|item| id == item).then_some(i))
        .collect()
}
pub(crate) fn range(
    editor: &State,
    key: ClipKey,
    track: Option<&qtable_engine::model::Track>,
    clip: &Clip,
) -> Option<(usize, usize)> {
    let rows = rows(editor, key, track, clip);
    Some((*rows.first()?, rows.last()? + 1))
}
pub(crate) fn select(app: &mut App, key: ClipKey, row: usize, extend: bool) {
    if !active(app, key) {
        return;
    }
    let Some(clip) = clip_owned(app, key) else {
        return;
    };
    let all = displayed_in(app, key, &clip);
    let Some(identity) = all.get(row).cloned() else {
        return;
    };
    let choice = app
        .clip_view
        .editor
        .views
        .choice(app.project(), key.track, key.id);
    let ids: Vec<_> = all
        .into_iter()
        .filter(|id| {
            choice.tab != super::super::lane_view::EditorTab::Events
                || matches!(id, Identity::Event(_))
        })
        .collect();
    let Some(row) = ids.iter().position(|id| *id == identity) else {
        return;
    };
    let editor = &mut app.clip_view.editor;
    let anchor = editor
        .lanes
        .as_ref()
        .filter(|s| s.key == key)
        .and_then(|s| ids.iter().position(|i| *i == s.anchor))
        .unwrap_or(row);
    let extending = extend && editor.shift_down;
    let indices = crate::selection::header_range(if extending { anchor } else { row }, row);
    content::clear(editor);
    editor.lanes = Some(Selection {
        key,
        anchor: ids[if extending { anchor } else { row }].clone(),
        items: ids[indices].to_vec(),
        headers: true,
    });
    editor.selected = None;
    editor.point = None;
    editor.hover = None;
    editor.focused = true;
}
/// The argument an envelope lane header drives; `None` for an event lane or a stale target.
pub(crate) fn envelope_argument(app: &App, target: Target) -> Option<ArgumentRef> {
    if !valid(app, target) {
        return None;
    }
    let clip = clip(app.project(), target.key.track, target.key.slot)?;
    match displayed_in(app, target.key, clip)
        .into_iter()
        .nth(target.row)?
    {
        Identity::Envelope(argument) => Some(argument),
        Identity::Event(_) => None,
    }
}
/// [`envelope_argument`], on the open clip's track.
pub(crate) fn envelope_field(app: &App, target: Target) -> Option<qtable_engine::model::FieldRef> {
    Some(envelope_argument(app, target)?.on(target.key.track))
}
/// The definition an event row's header stands for, as of the open clip's displayed rows.
pub(crate) fn event_definition(app: &App, target: Target) -> Option<EventRef> {
    if !valid(app, target) {
        return None;
    }
    let clip = clip(app.project(), target.key.track, target.key.slot)?;
    match displayed_in(app, target.key, clip)
        .into_iter()
        .nth(target.row)?
    {
        Identity::Event(reference) => Some(reference),
        Identity::Envelope(_) => None,
    }
}
pub(crate) fn open_menu(app: &mut App, target: Target) {
    if !valid(app, target) {
        return;
    }
    let clip = clip_owned(app, target.key).unwrap();
    if !rows(
        &app.clip_view.editor,
        target.key,
        app.project().track(target.key.track),
        &clip,
    )
    .contains(&target.row)
    {
        select(app, target.key, target.row, false);
    }
    if let Some(selection) = &mut app.clip_view.editor.lanes {
        selection.headers = true;
    }
    app.clip_view.editor.focused = true;
    app.open_overlay(crate::overlay::Overlay::Context(OpenMenu {
        at: app.cursor,
        target: MenuTarget::Lane(target),
    }));
}
fn copy(app: &App, key: ClipKey) -> Option<Clipboard> {
    let clip = clip(app.project(), key.track, key.slot)?;
    let track = app.project().track(key.track)?;
    let values = values(clip);
    let stored = identities(clip);
    let lanes: Vec<_> = app
        .clip_view
        .editor
        .lanes
        .as_ref()?
        .items
        .iter()
        .filter_map(|id| stored.iter().position(|stored| stored.matches(id)))
        .map(|i| values[i].clone())
        .collect();
    if lanes.is_empty() {
        return None;
    }
    let hits = clip
        .events
        .iter()
        .filter(|e| {
            lanes
                .iter()
                .any(|l| matches!(l, Lane::Events(r) if r.definition == e.definition))
        })
        .cloned()
        .collect();
    Some(Clipboard {
        lanes,
        hits,
        carried: Carried::from_clip(track, clip),
    })
}
fn submit(app: &mut App, key: ClipKey, edit: LaneEdit) -> bool {
    crate::edit_actions::execute_structural_edit(
        app,
        Command::Edit(ProjectEdit::EditLane {
            track: key.track,
            slot: key.slot,
            edit,
        }),
    )
}
pub(crate) fn shortcut(app: &mut App, command: Shortcut) -> bool {
    let editor = &app.clip_view.editor;
    if !editor.focused || app.pending_field.is_some() {
        return false;
    }
    let selection = editor.lanes.as_ref().filter(|s| s.headers);
    let key = if let Some(selection) = selection {
        selection.key
    } else if command == Shortcut::Paste
        && editor.content.is_none()
        && editor.lane_clipboard.is_some()
        && editor.selected.is_none()
        && editor.point.is_none()
    {
        let Some((track, slot, id)) = editor.owner else {
            return false;
        };
        ClipKey {
            track,
            slot,
            id,
            epoch: editor.epoch,
        }
    } else {
        return false;
    };
    if !active(app, key) {
        return false;
    }
    let Some(clip) = clip_owned(app, key) else {
        return false;
    };
    // Consume structural shortcuts on generated rows, including mixed header ranges.
    if selection.is_some_and(|s| s.items.iter().any(|id| matches!(id, Identity::Event(_)))) {
        return matches!(
            command,
            Shortcut::Copy
                | Shortcut::Cut
                | Shortcut::Delete
                | Shortcut::Duplicate
                | Shortcut::Paste
        );
    }
    if command == Shortcut::Paste
        && app
            .clip_view
            .editor
            .views
            .choice(app.project(), key.track, key.id)
            .tab
            == super::super::lane_view::EditorTab::Events
    {
        return true;
    }
    let stored = identities(&clip);
    let selected: Vec<_> = selection
        .into_iter()
        .flat_map(|s| &s.items)
        .filter_map(|id| stored.iter().position(|candidate| candidate.matches(id)))
        .collect();
    let first = selected.first().copied().unwrap_or(0);
    match command {
        Shortcut::Copy => {
            app.clip_view.editor.lane_clipboard = copy(app, key);
        }
        Shortcut::Cut | Shortcut::Delete => {
            if command == Shortcut::Cut {
                app.clip_view.editor.lane_clipboard = copy(app, key);
            }
            if submit(app, key, LaneEdit::RemoveMany { lanes: selected }) {
                app.clip_view.editor.lanes = None;
                app.clip_view.editor.point = None;
                app.clip_view.editor.selected = None;
            }
        }
        // Consume this shortcut on headers so it cannot fall through to hit duplication.
        Shortcut::Duplicate => return true,
        Shortcut::Paste => {
            let Some(payload) = app.clip_view.editor.lane_clipboard.clone() else {
                return true;
            };
            if payload
                .lanes
                .iter()
                .any(|lane| matches!(lane, Lane::Events(_)))
            {
                return true;
            }
            let at = crate::track_clipboard::insertion_index(
                first,
                selected.last().copied().unwrap_or(first),
                false,
            );
            let old = values(&clip);
            if submit(
                app,
                key,
                LaneEdit::Paste {
                    at,
                    lanes: payload.lanes,
                    hits: payload.hits,
                    carried: payload.carried,
                },
            ) {
                let new = clip_owned(app, key).unwrap();
                let values = values(&new);
                let fresh: Vec<_> = values
                    .iter()
                    .enumerate()
                    .filter_map(|(i, l)| (!old.iter().any(|o| same_lane(o, l))).then_some(i))
                    .collect();
                if let Some(&row) = fresh.first() {
                    let stored_ids = identities(&new);
                    let fresh_ids: Vec<_> = fresh.iter().map(|i| stored_ids[*i].owned()).collect();
                    let ids = displayed_in(app, key, &new);
                    let row = ids
                        .iter()
                        .position(|id| *id == stored_ids[row].owned())
                        .unwrap();
                    select(app, key, row, false);
                    if let Some(s) = &mut app.clip_view.editor.lanes {
                        s.items = fresh_ids;
                    }
                }
            }
        }
        _ => return false,
    }
    app.clip_view.editor.focused = true;
    true
}
fn same_lane(a: &Lane, b: &Lane) -> bool {
    match (a, b) {
        (Lane::Events(a), Lane::Events(b)) => a == b,
        (Lane::Envelope(a), Lane::Envelope(b)) => a.target == b.target,
        _ => false,
    }
}
pub(crate) fn move_lanes(app: &mut App, target: Target, from: std::ops::Range<usize>, to: usize) {
    if !valid(app, target) {
        return;
    }
    let clip = clip_owned(app, target.key).unwrap();
    let ids = displayed_in(app, target.key, &clip);
    let Some(moving) = ids.get(from.clone()) else {
        return;
    };
    if moving.is_empty() || moving.iter().any(|id| matches!(id, Identity::Event(_))) {
        return;
    }
    let stored = identities(&clip);
    let indices: Vec<_> = moving
        .iter()
        .filter_map(|id| stored.iter().position(|s| s.matches(id)))
        .collect();
    let start = indices[0];
    if indices.iter().copied().ne(start..start + indices.len()) {
        return;
    }
    let mut remaining = ids.clone();
    remaining.drain(from);
    // Events are a fixed prefix; envelopes cannot be dropped into it.
    if remaining
        .get(to)
        .is_some_and(|id| matches!(id, Identity::Event(_)))
    {
        return;
    }
    let remaining_stored: Vec<_> = stored
        .iter()
        .filter(|s| !moving.iter().any(|id| s.matches(id)))
        .collect();
    let destination = remaining
        .get(to)
        .and_then(|id| remaining_stored.iter().position(|s| s.matches(id)))
        .unwrap_or(remaining_stored.len());
    let moving = moving.to_vec();
    if submit(
        app,
        target.key,
        LaneEdit::Move {
            from: start..start + indices.len(),
            to: destination,
        },
    ) {
        let editor = &mut app.clip_view.editor;
        content::clear(editor);
        editor.lanes = Some(Selection {
            key: target.key,
            anchor: moving[0].clone(),
            items: moving,
            headers: true,
        });
        editor.selected = None;
        editor.point = None;
        editor.focused = true;
    }
}

/// Capture setup uses the same selection and clipboard handlers as the keyboard.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture(app: &mut App, scenario: &str) {
    let Some(selected) = app.selection.clip else {
        return;
    };
    let Some(clip) = clip(app.project(), selected.track, selected.slot) else {
        return;
    };
    let key = ClipKey {
        track: selected.track,
        slot: selected.slot,
        id: clip.id,
        epoch: app.clip_view.editor.epoch,
    };
    let lane_count = displayed_in(app, key, clip).len();
    select(app, key, 0, false);
    app.clip_view.editor.shift_down = true;
    select(app, key, 2, true);
    app.clip_view.editor.shift_down = false;
    match scenario {
        "menu" => {
            shortcut(app, Shortcut::Copy);
            app.cursor = iced::Point::new(640.0, 340.0);
            open_menu(
                app,
                Target {
                    key,
                    revision: app.project_revision(),
                    row: 1,
                },
            );
        }
        // An event row's header menu (Go to Device), or the Track tab after pressing it.
        "event-menu" | "go-to-device" => {
            let revision = app.project_revision();
            let found = (0..lane_count).find_map(|row| {
                event_definition(app, Target { key, revision, row }).map(|d| (row, d))
            });
            if let Some((row, definition)) = found {
                select(app, key, row, false);
                if scenario == "event-menu" {
                    app.cursor = iced::Point::new(640.0, 340.0);
                    open_menu(app, Target { key, revision, row });
                } else {
                    crate::device_chain::go_to_event(app, key.track, definition);
                }
            }
        }
        // A parameter row's header menu (Go to Device) in the Overview tab, or the Track tab after
        // pressing it: Opacity for the menu, Position's second field for the reveal.
        "parameter-menu" | "volume-menu" | "go-to-parameter" => {
            let revision = app.project_revision();
            let wanted = match scenario {
                "parameter-menu" => "opacity",
                "volume-menu" => "volume",
                _ => "position",
            };
            let found = (0..lane_count).find_map(|row| {
                envelope_argument(app, Target { key, revision, row })
                    .filter(|a| a.parameter.0 == wanted)
                    .map(|a| (row, a))
            });
            if let Some((row, argument)) = found {
                select(app, key, row, false);
                if scenario == "go-to-parameter" {
                    crate::device_chain::go_to_argument(app, key.track, &argument);
                } else {
                    app.cursor = iced::Point::new(640.0, 340.0);
                    open_menu(app, Target { key, revision, row });
                }
            }
        }
        // The Mixer Volume lane, overridden by the capture's manual change: Re-Enable Automation
        // is enabled in its header menu.
        "reenable" => {
            let volume = crate::mixer_device::volume_field(key.track);
            let revision = app.project_revision();
            let row = (0..lane_count).find(|&row| {
                envelope_field(app, Target { key, revision, row }).as_ref() == Some(&volume)
            });
            if let Some(row) = row {
                select(app, key, row, false);
                app.cursor = iced::Point::new(640.0, 340.0);
                open_menu(app, Target { key, revision, row });
            }
        }
        "paste" => {
            shortcut(app, Shortcut::Copy);
            shortcut(app, Shortcut::Delete);
            // A header selection supplies the insertion boundary, just like tracks.
            select(app, key, 0, false);
            shortcut(app, Shortcut::Paste);
        }
        _ => {}
    }
    app.focus = crate::focus::Focus::Detail;
}

#[cfg(test)]
mod tests;
