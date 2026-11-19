//! The selected track's devices, with its Mixer always the last device in the chain.
use crate::{app::App, control_focus::View, messages::Message, style::detail};
use iced::widget::{container, row, scrollable, text};
use iced::{Element, Length};
use qtable_engine::model::Track;

#[cfg(any(test, feature = "dev-hooks"))]
mod capture;
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) use capture::{capture, hold_drag};
mod face;
pub(crate) use face::width;
mod frame;
pub(crate) mod target_face;
pub(crate) const SCROLL: &str = "device-chain";

pub(crate) fn view<'a>(app: &'a App, track: &'a Track, location: View) -> Element<'a, Message> {
    let mixer = frame::panel(
        app,
        "Mixer",
        crate::mixer_device::width(track),
        scrollable(crate::mixer_device::view(app, track, location))
            .direction(scrollable::Direction::Vertical(detail::scrollbar()))
            .height(Length::Fill)
            .into(),
    );
    let chain: Element<'a, Message> = if track.devices.is_empty() {
        row![
            crate::browser::chain(
                app,
                track,
                container(
                    text(if track.capabilities().target_devices {
                        "Drop a device here from the Browser"
                    } else {
                        "Targets require a Track with clips"
                    })
                    .size(detail::TEXT_SIZE)
                    .color(app.theme.text_disabled),
                )
                .center(Length::Fill)
                .into(),
            ),
            mixer
        ]
        .spacing(detail::CHAIN_GAP)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    } else {
        let devices = row(track
            .devices
            .iter()
            .map(|device| frame::device(app, device, location)))
        .spacing(detail::CHAIN_GAP)
        .height(Length::Fill);
        // The Mixer follows the last device and scrolls with it, outside the drop and reorder regions.
        scrollable(
            row![
                crate::browser::chain(
                    app,
                    track,
                    host::region(devices, track, &app.device_chain, app.cursor),
                ),
                mixer
            ]
            .spacing(detail::CHAIN_GAP)
            .height(Length::Fill),
        )
        .id(iced::widget::Id::new(SCROLL))
        .direction(scrollable::Direction::Horizontal(detail::scrollbar()))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };
    container(
        container(chain)
            .padding(detail::CHAIN_INSET)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_| container::Style {
        background: Some(detail::CHAIN_BACKGROUND.into()),
        ..Default::default()
    })
    .into()
}

use crate::clip_actions::GridClipboard;
use crate::clip_shortcuts::Shortcut;
use crate::focus::Focus;
use crate::inspector_panel::Tab;
use iced::{Point, Task};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::{DeviceId, EntryKey, TrackId};
use std::collections::HashMap;

mod host;

#[derive(Default)]
pub(crate) struct State {
    track: Option<TrackId>,
    pub renaming: Option<DeviceId>,
    pub draft: String,
    pub drag: Option<Drag>,
    /// Each Target's selected entry (`target_face`).
    pub entry: HashMap<DeviceId, EntryKey>,
    /// Each Target's field tab, within its selected entry (`target_face::edited_field`).
    pub field: HashMap<DeviceId, qtable_engine::model::FieldKey>,
    pub face: target_face::collapsed::State,
}

pub(crate) struct Drag {
    pub device: DeviceId,
    origin: Point,
    pub moved: bool,
    pub over: Option<Option<DeviceId>>,
}

#[derive(Debug, Clone)]
pub(crate) enum Action {
    Select(DeviceId),
    Begin(DeviceId, Point),
    Over(Option<Option<DeviceId>>),
    Finish,
    Menu(DeviceId, Point),
    Toggle(DeviceId),
    Shortcut(Shortcut),
    Draft(String),
    CommitRename,
    RenameFocusChecked(DeviceId, String, u64, bool),
    Target(target_face::Action),
}

pub(crate) fn shown(app: &App) -> bool {
    matches!(
        app.selection.inspector_tab,
        Tab::Track | Tab::Output | Tab::Main
    )
}

pub(crate) fn shown_track(app: &App) -> Option<TrackId> {
    match app.selection.inspector_tab {
        Tab::Main => Some(app.project().main.id),
        Tab::Output => app.selection.anchor,
        Tab::Track => app.selection.track(),
        _ => None,
    }
}

pub(crate) fn reconcile(app: &mut App) {
    if shown(app) {
        app.dividers.lock_detail();
    }
    let track = shown_track(app).or_else(|| app.selection.track()).or_else(|| {
        matches!(app.selection.owner, Some(crate::selection::ColumnOwner::Track(id)) if id == app.project().main.id)
            .then_some(app.project().main.id)
    });
    if app.device_chain.track != track {
        app.selection.devices.clear();
        app.selection.device_anchor = None;
        app.device_chain = State {
            track,
            ..Default::default()
        };
    }
    let valid: Vec<_> = track
        .and_then(|id| app.project().track(id))
        .map(|track| track.devices.iter().map(|d| d.id).collect())
        .unwrap_or_default();
    app.selection.devices.retain(|id| valid.contains(id));
    if app
        .selection
        .device_anchor
        .is_some_and(|id| !valid.contains(&id))
    {
        app.selection.device_anchor = None;
    }
    if app
        .device_chain
        .renaming
        .is_some_and(|id| !valid.contains(&id))
    {
        app.device_chain.renaming = None;
    }
    target_face::reconcile(app);
    if !shown(app) || !app.chrome.detail_open {
        app.device_chain.drag = None;
    }
    if app
        .device_chain
        .drag
        .as_ref()
        .is_some_and(|drag| !valid.contains(&drag.device))
    {
        app.device_chain.drag = None;
    }
}

fn select(app: &mut App, id: DeviceId) {
    let Some(track) = shown_track(app).and_then(|id| app.project().track(id)) else {
        return;
    };
    let ids: Vec<_> = track.devices.iter().map(|d| d.id).collect();
    let Some(index) = ids.iter().position(|d| *d == id) else {
        return;
    };
    app.device_chain.track = Some(track.id);
    if app.selection.shift_down {
        let anchor = app
            .selection
            .device_anchor
            .and_then(|id| ids.iter().position(|d| *d == id))
            .unwrap_or(index);
        app.selection.devices = ids[anchor.min(index)..=anchor.max(index)].to_vec();
    } else if app.selection.command_down {
        if app.selection.devices.contains(&id) {
            app.selection.devices.retain(|d| *d != id);
        } else {
            app.selection.devices.push(id);
        }
        app.selection.device_anchor = Some(id);
    } else {
        app.selection.devices = vec![id];
        app.selection.device_anchor = Some(id);
    }
    app.focus = Focus::Detail;
    app.control_focus.clear();
}

pub(crate) fn drag_cursor(app: &mut App, cursor: Point) {
    if let Some(drag) = &mut app.device_chain.drag {
        drag.moved |= cursor.distance(drag.origin) >= 4.0;
    }
}

pub(crate) fn update(app: &mut App, action: Action) -> Task<Message> {
    match action {
        Action::Select(id) => select(app, id),
        Action::Target(action) => return target_face::update(app, action),
        Action::Begin(id, origin) => {
            if app.device_chain.renaming.is_some() {
                commit_rename(app);
            }
            select(app, id);
            target_face::clear_entry(app, id);
            app.close_overlay();
            app.device_chain.drag = Some(Drag {
                device: id,
                origin,
                moved: false,
                over: None,
            });
        }
        Action::Over(over) => {
            if let Some(drag) = &mut app.device_chain.drag {
                drag.over = over;
            }
        }
        Action::Finish => {
            if let Some(drag) = app.device_chain.drag.take()
                && drag.moved
                && let Some(before) = drag.over
            {
                crate::edit_actions::run_edit(
                    app,
                    Command::Edit(ProjectEdit::MoveDevice {
                        device: drag.device,
                        before,
                    }),
                );
            }
        }
        Action::Menu(id, at) => {
            if !app.selection.devices.contains(&id) {
                select(app, id);
            }
            app.focus = Focus::Detail;
            app.open_overlay(crate::overlay::Overlay::Context(
                crate::context_menu::OpenMenu {
                    at,
                    target: crate::context_menu::MenuTarget::Device(id),
                },
            ));
        }
        Action::Toggle(id) => {
            app.focus = Focus::Detail;
            if let Some((_, device)) = app.project().device(id) {
                crate::edit_actions::run_edit(
                    app,
                    Command::Edit(ProjectEdit::SetDevicesActive {
                        devices: vec![id],
                        active: !device.active,
                    }),
                );
            }
        }
        Action::Shortcut(key) => {
            app.close_overlay();
            return shortcut(app, key);
        }
        Action::Draft(value) => app.device_chain.draft = value,
        Action::CommitRename => commit_rename(app),
        Action::RenameFocusChecked(id, draft, generation, focused) => {
            if !focused
                && app.project_generation == generation
                && app.device_chain.renaming == Some(id)
                && app.device_chain.draft == draft
            {
                // The user moved on: the commit is a step of its own, apart from whatever
                // the click that took the focus does next.
                crate::gesture_steps::isolated(app, commit_rename);
            }
        }
    }
    Task::none()
}

pub(crate) fn shortcut(app: &mut App, key: Shortcut) -> Task<Message> {
    let devices = app.selection.devices.clone();
    match key {
        Shortcut::Copy if !devices.is_empty() => {
            if let Some(track) = shown_track(app).and_then(|id| app.project().track(id)) {
                app.clip_actions.clipboard = Some(GridClipboard::Devices(
                    track
                        .devices
                        .iter()
                        .filter(|d| devices.contains(&d.id))
                        .cloned()
                        .collect(),
                ));
            }
        }
        Shortcut::Paste => paste(app, true),
        Shortcut::Duplicate if !devices.is_empty() => {
            copies(app, ProjectEdit::DuplicateDevices { devices });
        }
        Shortcut::Delete if target_face::remove_selected(app) => {}
        Shortcut::Delete if !devices.is_empty() => remove(app, devices),
        Shortcut::Rename if !devices.is_empty() => {
            let id = devices[0];
            if let Some((_, device)) = app.project().device(id) {
                app.device_chain.draft = device.name.clone();
                app.device_chain.renaming = Some(id);
                return Task::batch([
                    iced::widget::operation::focus(rename_id(id)),
                    iced::widget::operation::select_all(rename_id(id)),
                ]);
            }
        }
        _ => {}
    }
    Task::none()
}

/// Up and Down move the selected Target's entry selection, when no text is being edited.
pub(crate) fn arrow(app: &mut App, key: &iced::keyboard::Key, ignored: bool) -> bool {
    use iced::keyboard::{Key, key::Named};
    if !ignored || app.focus != Focus::Detail || !shown(app) || app.pending_field.is_some() {
        return false;
    }
    match key {
        Key::Named(Named::ArrowUp) => target_face::move_selection(app, -1),
        Key::Named(Named::ArrowDown) => target_face::move_selection(app, 1),
        _ => false,
    }
}

pub(crate) fn check_rename(app: &App) -> Task<Message> {
    let Some(id) = app.device_chain.renaming else {
        return Task::none();
    };
    let draft = app.device_chain.draft.clone();
    let generation = app.project_generation;
    crate::field_focus::check_widget_focus(rename_id(id)).map(move |focused| {
        Message::Device(Action::RenameFocusChecked(
            id,
            draft.clone(),
            generation,
            focused,
        ))
    })
}

pub(crate) fn rename_id(id: DeviceId) -> iced::widget::Id {
    iced::widget::Id::from(format!("device-name-{}", id.0))
}

pub(crate) fn commit_rename(app: &mut App) {
    if let Some(device) = app.device_chain.renaming
        && crate::edit_actions::run_edit(
            app,
            Command::Edit(ProjectEdit::RenameDevice {
                device,
                name: app.device_chain.draft.clone(),
            }),
        )
    {
        app.device_chain.renaming = None;
    }
}

fn grid_target(app: &App) -> Option<TrackId> {
    use crate::selection::ColumnOwner;
    if app.selection.clip.is_some()
        || app.selection.scene.is_some()
        || app.selection.cell.is_some_and(|c| c.active)
    {
        return None;
    }
    match app.selection.owner? {
        ColumnOwner::Track(id) if id == app.project().main.id => Some(id),
        ColumnOwner::Track(id) if app.selection.tracks == [id] => Some(id),
        ColumnOwner::Group(id) if app.selection.tracks == [id] => Some(id),
        _ => None,
    }
}

pub(crate) fn can_paste_grid(app: &App) -> bool {
    matches!(app.clip_actions.clipboard, Some(GridClipboard::Devices(_)))
        && grid_target(app).is_some()
}

pub(crate) fn paste(app: &mut App, detail: bool) {
    let Some(GridClipboard::Devices(devices)) = app.clip_actions.clipboard.clone() else {
        return;
    };
    let track = if detail {
        shown_track(app)
    } else if can_paste_grid(app) {
        grid_target(app)
    } else {
        None
    };
    let Some(track) = track else {
        return;
    };
    let before = if detail {
        app.project().track(track).and_then(|t| {
            t.devices
                .iter()
                .rposition(|d| app.selection.devices.contains(&d.id))
                .and_then(|index| t.devices.get(index + 1))
                .map(|d| d.id)
        })
    } else {
        None
    };
    copies(
        app,
        ProjectEdit::PasteDevices {
            track,
            devices,
            before,
        },
    );
}

fn copies(app: &mut App, edit: ProjectEdit) {
    let track = match &edit {
        ProjectEdit::PasteDevices { track, .. } => Some(*track),
        _ => shown_track(app),
    };
    let Some(track) = track else {
        return;
    };
    let old: Vec<_> = app
        .project()
        .track(track)
        .map(|t| t.devices.iter().map(|d| d.id).collect())
        .unwrap_or_default();
    if crate::edit_actions::run_edit(app, Command::Edit(edit)) {
        app.selection.devices = app
            .project()
            .track(track)
            .unwrap()
            .devices
            .iter()
            .filter(|d| !old.contains(&d.id))
            .map(|d| d.id)
            .collect();
        app.selection.device_anchor = app.selection.devices.first().copied();
        app.device_chain.track = Some(track);
    }
}

fn remove(app: &mut App, devices: Vec<DeviceId>) {
    let name = if devices.len() == 1 {
        app.project()
            .device(devices[0])
            .map(|(_, d)| d.name.clone())
            .unwrap_or_default()
    } else {
        format!("{} devices", devices.len())
    };
    if let Some(report) = crate::edit_actions::run_edit_report(
        app,
        Command::Edit(ProjectEdit::RemoveDevices { devices }),
        None,
    ) {
        app.drafts.clear_inspector();
        crate::edit_actions::notify_removal(app, &report, &name);
        reconcile(app);
        app.validate_overlay();
    }
}

pub(crate) fn menu_entries(app: &App) -> Vec<crate::context_menu::Entry> {
    use crate::info::Id;
    [
        ("Rename", "⌘R", Shortcut::Rename, Id::DeviceName),
        ("Copy", "⌘C", Shortcut::Copy, Id::CopyDevice),
        ("Paste", "⌘V", Shortcut::Paste, Id::PasteDevice),
        ("Duplicate", "⌘D", Shortcut::Duplicate, Id::DuplicateDevice),
        ("Delete", "Del", Shortcut::Delete, Id::DeleteDevice),
    ]
    .into_iter()
    .map(|(label, key, action, info)| {
        crate::context_menu::command(
            label,
            key,
            Message::Device(Action::Shortcut(action)),
            info,
            action != Shortcut::Paste
                || matches!(app.clip_actions.clipboard, Some(GridClipboard::Devices(_))),
        )
    })
    .collect()
}

/// Go to Device: selects the Target device that defines `reference` and reveals that
/// definition's entry in the Track tab. The reference names its device, so two Targets on
/// the track that define events with the same name stay apart.
pub(crate) fn go_to_event(
    app: &mut App,
    track: TrackId,
    reference: qtable_engine::model::EventRef,
) {
    if app
        .project()
        .track(track)
        .and_then(|t| t.event_def(&reference))
        .is_none()
        || !reveal(app, reference.device)
    {
        return;
    }
    target_face::select_entry(app, reference.device, reference.key);
}

/// Go to Device for a parameter row: the Target device that defines the parameter an envelope
/// automates, with its entry and the automated field selected in the Track tab. Volume lives on
/// the Mixer, which has no definition to select, so the Track tab scrolls to it instead. The
/// argument carries its device, so two Targets that define the same parameter name stay apart.
pub(crate) fn go_to_argument(
    app: &mut App,
    track: TrackId,
    argument: &qtable_engine::model::ArgumentRef,
) {
    use qtable_engine::model::DeviceRef;
    match argument.device {
        DeviceRef::Mixer => {
            if app.project().track(track).is_some() {
                open_track_tab(app);
                crate::browser::request_reveal_mixer(app);
            }
        }
        DeviceRef::Chain(device) => {
            if !parameter_defined(app, track, device, argument) || !reveal(app, device) {
                return;
            }
            let key = EntryKey(argument.parameter.0.clone());
            target_face::select_entry(app, device, key.clone());
            target_face::select_argument(app, device, &key, &argument.field);
        }
    }
}

/// Whether `device` on `track` is a Target that still defines the argument's parameter.
pub(crate) fn parameter_defined(
    app: &App,
    track: TrackId,
    device: DeviceId,
    argument: &qtable_engine::model::ArgumentRef,
) -> bool {
    app.project()
        .track(track)
        .and_then(|t| t.device(device))
        .and_then(qtable_engine::model::Device::target)
        .is_some_and(|target| target.parameter(&argument.parameter).is_some())
}

fn open_track_tab(app: &mut App) {
    app.close_overlay();
    app.selection.inspector_tab = Tab::Track;
    app.chrome.detail_open = true;
    reconcile(app);
}

/// Opens the Track tab with `device` expanded and scrolled into view. `false` when it
/// cannot be expanded.
fn reveal(app: &mut App, device: DeviceId) -> bool {
    open_track_tab(app);
    if app
        .project()
        .device(device)
        .is_some_and(|(_, device)| !device.expanded)
        && !crate::edit_actions::run_edit(
            app,
            Command::Edit(ProjectEdit::SetDeviceExpanded {
                device,
                expanded: true,
            }),
        )
    {
        return false;
    }
    crate::browser::request_reveal(app, device);
    true
}

#[cfg(test)]
mod tests;
