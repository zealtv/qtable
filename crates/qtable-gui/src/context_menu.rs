//! Context menus, after Live 12's clip menu: the commands qtable supports, a shortcut column on the
//! right, then the colour palette. A clip gets Cut to Rename; an empty slot gets Insert Clip
//! and Paste; both get Toggle Stop Button. A track title gets Rename, Delete, Insert Track and
//! Group Tracks; a group header gets Rename and Ungroup; a scene in Main gets Rename and
//! Insert Scene. These entries, with their shortcuts, replace the Detail View's old button row.
use crate::app::App;
use crate::control::menu::{self, Anchor, Item};
use crate::header_actions;
use crate::info::{Help, Id};
use crate::messages::{EditMessage, Message, OutputMessage};
use crate::outputs::to_colour;
use crate::style::TEXT;
use iced::widget::{Space, button, column, container, row};
use iced::{Border, Color, Element, Length, Point, Size};
use qtable_engine::model::TrackId;
use qtable_engine::model::{Capabilities, Colour, LIVE_CLIP_PALETTE, Region, SceneId};

const MENU_W: f32 = 262.0;
/// Live lays its 70 clip colours out 14 across.
const SWATCHES_PER_ROW: usize = 14;
const SWATCH_W: f32 = 16.0;
const SWATCH_H: f32 = 14.0;
const SWATCH_GAP: f32 = 2.0;
const PALETTE_PAD: f32 = 6.0;

/// What a menu acts on.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MenuTarget {
    /// The selected clip slot.
    Slot,
    Argument {
        subject: crate::info::Subject,
        revision: u64,
    },
    /// A point, a hit, lane space or a time selection in a clip-editor canvas.
    Content(crate::inspector::event_editor::menu::ContentTarget),
    Header(TrackId),
    Lane(crate::inspector::event_editor::lanes::Target),
    Device(qtable_engine::model::DeviceId),
    Scene(SceneId),
    /// Empty space in a region (the outputs area): what that region inserts.
    Region(Region),
}

/// The open menu: where it appears and what it acts on. GUI session state.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OpenMenu {
    pub at: Point,
    pub target: MenuTarget,
}

/// One line of the menu.
#[derive(Debug, Clone)]
pub(crate) enum Entry {
    Command {
        label: &'static str,
        shortcut: &'static str,
        message: Message,
        info: Id,
        enabled: bool,
        /// A tick in the check column, as Live marks a mode that is on.
        checked: bool,
        /// An icon between the check column and the label.
        glyph: Option<crate::control::Glyph>,
    },
    /// A label row that names the rows beneath it: no press, no hover, never reached by keys.
    Heading(&'static str),
    Separator,
    /// The colour grid, with the current colour outlined. A swatch recolours the header
    /// `owner`, or the selected clip when it is `None`.
    Palette {
        current: Colour,
        owner: Option<TrackId>,
        info: Id,
    },
}

pub(crate) fn command(
    label: &'static str,
    shortcut: &'static str,
    message: Message,
    info: Id,
    enabled: bool,
) -> Entry {
    Entry::Command {
        label,
        shortcut,
        message,
        info,
        enabled,
        checked: false,
        glyph: None,
    }
}

fn edit(
    label: &'static str,
    shortcut: &'static str,
    message: EditMessage,
    info: Id,
    enabled: bool,
) -> Entry {
    command(label, shortcut, Message::Edit(message), info, enabled)
}

/// Go to Device, after a separator that is left out when nothing precedes it. It sits at the
/// bottom, as Show Envelope does in the Argument menu: navigation after the edit commands.
pub(super) fn go_to_device_entry(
    track: TrackId,
    definition: Option<qtable_engine::model::EventRef>,
    enabled: bool,
    separated: bool,
) -> Vec<Entry> {
    let Some(reference) = definition else {
        return Vec::new();
    };
    let go = command(
        "Go to Device",
        "",
        Message::Inspector(crate::inspector::Message::GoToDevice(track, reference)),
        Id::GoToDevice,
        enabled,
    );
    if separated {
        vec![Entry::Separator, go]
    } else {
        vec![go]
    }
}

/// The menu for a filled slot (`clip_colour` is the clip's) or an empty one (`None`).
pub(crate) fn slot_entries(
    clip_colour: Option<Colour>,
    clipboard: bool,
    tracks: bool,
) -> Vec<Entry> {
    let paste = || {
        edit(
            if tracks { "Paste Tracks" } else { "Paste" },
            "⌘V",
            EditMessage::PasteClip,
            if tracks {
                Id::PasteTrack
            } else {
                Id::PasteClip
            },
            clipboard,
        )
    };
    let toggle_stop = || {
        edit(
            "Toggle Stop Button",
            "⌘E",
            EditMessage::ToggleSelectedStop,
            Id::ToggleSlotStop,
            true,
        )
    };
    match clip_colour {
        Some(current) => vec![
            edit("Cut", "⌘X", EditMessage::CutSelectedClip, Id::CutClip, true),
            edit(
                "Copy",
                "⌘C",
                EditMessage::CopySelectedClip,
                Id::CopyClip,
                true,
            ),
            paste(),
            edit(
                "Duplicate",
                "⌘D",
                EditMessage::DuplicateSelectedClip,
                Id::DuplicateClip,
                true,
            ),
            edit(
                "Delete",
                "Del",
                EditMessage::DeleteSelectedClip,
                Id::DeleteClip,
                true,
            ),
            Entry::Separator,
            edit(
                "Rename",
                "⌘R",
                EditMessage::BeginRenameClip,
                Id::ClipName,
                true,
            ),
            edit(
                "Move Clip…",
                "",
                EditMessage::ToggleMoveClipMode,
                Id::MoveClip,
                true,
            ),
            toggle_stop(),
            Entry::Separator,
            Entry::Palette {
                current,
                owner: None,
                info: Id::ClipColour,
            },
        ],
        None => vec![
            edit(
                "Insert Clip",
                "⇧⌘M",
                EditMessage::CreateSelectedClip,
                Id::CreateClip,
                true,
            ),
            paste(),
            toggle_stop(),
        ],
    }
}

/// The menu for a column header, built from what its kind can do (`TrackKind::capabilities`):
/// entries differ by omission, so an output's menu is a track's without grouping.
/// `can_group` comes from the engine's consecutive sibling subtree planner; `can_paste` when the clipboard
/// holds copied tracks of the matching family. Main's caps leave out Copy, Paste and Duplicate.
pub(crate) fn header_entries(
    owner: TrackId,
    caps: Capabilities,
    current: Colour,
    can_group: bool,
    can_paste: bool,
) -> Vec<Entry> {
    let (rename_info, palette_info, delete_info) = if caps.foldable {
        (Id::GroupName, Id::GroupColour, Id::DeleteTrack)
    } else if caps.destination {
        (Id::OutputName, Id::OutputColour, Id::DeleteOutput)
    } else {
        (Id::TrackName, Id::TrackColour, Id::DeleteTrack)
    };
    let mut entries = Vec::new();
    if caps.renamable {
        entries.push(edit(
            "Rename",
            "⌘R",
            EditMessage::BeginRenameHeader(owner),
            rename_info,
            true,
        ));
    }
    if caps.copyable {
        entries.extend(clipboard_entries(
            [
                Message::Edit(EditMessage::CopyTracks),
                Message::Edit(EditMessage::PasteTracks),
                Message::Edit(EditMessage::DuplicateTracks),
            ],
            [Id::CopyTrack, Id::PasteTrack, Id::DuplicateTrack],
            can_paste,
        ));
    }
    if caps.removable {
        entries.push(edit(
            "Delete",
            "Del",
            EditMessage::DeleteTrack(owner),
            delete_info,
            true,
        ));
    }
    entries.push(Entry::Separator);
    entries.extend(region_entries(caps.region));
    if caps.groupable {
        entries.push(command(
            "Group Tracks",
            "⌘G",
            Message::Outputs(OutputMessage::CreateGroupAroundSelected),
            Id::GroupTracks,
            can_group,
        ));
    }
    // A group can be ungrouped or wrapped in another group.
    if caps.foldable {
        entries.push(command(
            "Ungroup Tracks",
            "⇧⌘G",
            Message::Outputs(OutputMessage::Ungroup(owner)),
            Id::Ungroup,
            true,
        ));
    }
    if caps.recolourable {
        entries.extend([
            Entry::Separator,
            Entry::Palette {
                current,
                owner: Some(owner),
                info: palette_info,
            },
        ]);
    }
    entries
}

/// What clicking empty space in a region inserts: Insert Track, or Insert Output.
pub(crate) fn region_entries(region: Region) -> Vec<Entry> {
    match region {
        Region::Outputs => vec![command(
            "Insert Output",
            "⌘⌥T",
            Message::Outputs(OutputMessage::AddOutput),
            Id::AddOutput,
            true,
        )],
        Region::Tracks | Region::Main => vec![edit(
            "Insert Track",
            "⌘T",
            EditMessage::AddTrack,
            Id::AddTrack,
            true,
        )],
    }
}

/// The menu for a scene in Main.
pub(crate) fn scene_entries(scene: SceneId) -> Vec<Entry> {
    vec![
        edit(
            "Delete",
            "⌫",
            EditMessage::DeleteScenes(scene),
            Id::SceneSelect,
            true,
        ),
        edit(
            "Rename",
            "⌘R",
            EditMessage::BeginRenameScene(scene),
            Id::SceneName,
            true,
        ),
        edit(
            "Insert Scene",
            "⌘I",
            EditMessage::AddScene,
            Id::AddScene,
            true,
        ),
    ]
}

fn palette_h() -> f32 {
    let rows = LIVE_CLIP_PALETTE.len().div_ceil(SWATCHES_PER_ROW) as f32;
    2.0 * PALETTE_PAD + rows * SWATCH_H + (rows - 1.0) * SWATCH_GAP
}

fn height(entries: &[Entry]) -> f32 {
    menu::panel_height(
        entries
            .iter()
            .map(|entry| match entry {
                Entry::Command { .. } | Entry::Heading(_) => menu::content_height(1, 0),
                Entry::Separator => menu::content_height(0, 1),
                Entry::Palette { .. } => palette_h(),
            })
            .sum::<f32>(),
    )
}

/// *Show Envelope* for an automatable control, after a rule decided 2026-10-21: it opens the
/// selected clip's lane, so it stays listed but disabled, its tooltip giving the reason, unless
/// that clip sits on the control's track. Tracks without clip slots don't offer it.
fn show_envelope_entry(
    app: &App,
    subject: &crate::info::Subject,
    field: Option<qtable_engine::model::FieldRef>,
) -> Option<Entry> {
    use crate::inspector::lane_view::{Unavailable, envelope_slot};
    let track = crate::parameter_gesture::subject_track(subject);
    if !app
        .project()
        .track_capabilities(track)
        .is_some_and(|caps| caps.holds_clips)
    {
        return None;
    }
    let slot = envelope_slot(app, track);
    let (info, message) = match (&slot, field) {
        (Ok(_), Some(field)) => (Id::ShowEnvelope, Message::ShowEnvelope(field)),
        (Err(Unavailable::OtherTrack), _) => (Id::ShowEnvelopeOtherTrack, Message::CloseOverlay),
        // Nothing selected, or the project moved on under the open menu.
        _ => (Id::ShowEnvelopeNoClip, Message::CloseOverlay),
    };
    Some(command(
        "Show Envelope",
        "",
        message,
        info,
        info == Id::ShowEnvelope,
    ))
}

/// The entries for what the open menu acts on.
pub(crate) fn current_entries(app: &App, target: MenuTarget) -> Vec<Entry> {
    match target {
        MenuTarget::Argument { subject, revision } => {
            let field = (revision == app.engine.project_revision())
                .then(|| crate::parameter_gesture::subject_field(app, &subject))
                .flatten();
            let enabled = field.as_ref().is_some_and(|field| {
                crate::parameter_gesture::automation_state(app, field)
                    == crate::parameter_gesture::AutomationState::Overridden
            });
            let mut entries = vec![command(
                "Re-Enable Automation",
                "",
                Message::ReEnableAutomation(field.clone()),
                Id::ReEnableAutomation,
                enabled,
            )];
            entries.extend(show_envelope_entry(app, &subject, field));
            entries
        }
        MenuTarget::Device(_) => crate::device_chain::menu_entries(app),
        MenuTarget::Lane(target) => lane_entries(app, target),
        MenuTarget::Content(target) => content::content_entries(app, &target),
        MenuTarget::Slot => {
            let colour = app
                .selection
                .clip
                .and_then(|sel| Some(app.project().clip(sel.track, sel.slot)?.colour));
            slot_entries(
                colour,
                crate::clip_actions::can_paste(app),
                crate::track_clipboard::holds_tracks(app),
            )
        }
        MenuTarget::Scene(scene) => scene_entries(scene),
        MenuTarget::Header(owner) => header_actions::name_and_colour(app, owner)
            .zip(app.project().track_capabilities(owner))
            .map(|((_, colour), caps)| {
                header_entries(
                    owner,
                    caps,
                    colour,
                    header_actions::can_group(app),
                    crate::track_clipboard::can_paste_region(app, caps.region),
                )
            })
            .unwrap_or_default(),
        MenuTarget::Region(region) => region_entries(region),
    }
}

/// The rows the keyboard reaches: the enabled commands, top to bottom (`menu_keys.rs`). The
/// colour swatches stay with the mouse.
pub(crate) fn rows(app: &App, open: &OpenMenu) -> crate::menu_keys::Rows {
    let presses = current_entries(app, open.target.clone())
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Command {
                message,
                enabled: true,
                ..
            } => Some(message),
            _ => None,
        })
        .collect();
    crate::menu_keys::Rows {
        presses,
        current: None,
    }
}

/// The menu layer over the whole window: a click anywhere outside it closes it.
/// The menu layer over the whole window: a click anywhere outside it closes it. The menu opens
/// on the side of the click with room (`menu::Anchor`).
pub(crate) fn view<'a>(app: &App, open: OpenMenu) -> Element<'a, Message> {
    let entries = current_entries(app, open.target);
    let size = Size::new(MENU_W, height(&entries));
    let theme = app.theme;
    let highlight = app.menu_highlight;
    menu::layer(Anchor::point(open.at), Message::CloseOverlay, move || {
        // The keyboard's row counts the enabled commands only, as `rows` does.
        let mut reachable = 0;
        let views = entries.iter().map(|entry| {
            let highlighted = match entry {
                Entry::Command { enabled: true, .. } => {
                    reachable += 1;
                    highlight == Some(reachable - 1)
                }
                _ => false,
            };
            entry_view(&theme, entry, highlighted)
        });
        let panel =
            menu::panel(column(views), size.width, &theme).height(Length::Fixed(size.height));
        (panel.into(), size)
    })
}

fn entry_view<'a>(
    theme: &crate::control::Theme,
    entry: &Entry,
    highlighted: bool,
) -> Element<'a, Message> {
    match entry {
        Entry::Command {
            label,
            shortcut,
            message,
            info,
            enabled,
            checked,
            glyph,
        } => menu::item(
            theme,
            Item::new(*label, *info)
                .shortcut(shortcut)
                .check(if *checked {
                    menu::Check::Tick
                } else {
                    menu::Check::None
                })
                .lead(glyph.map_or(menu::Lead::None, menu::Lead::Glyph))
                .highlighted(highlighted)
                .on_press_maybe(enabled.then(|| message.clone())),
            |row, info| row.info(info, Message::Info),
        ),
        Entry::Heading(label) => menu::heading(theme, label),
        Entry::Separator => menu::separator(theme),
        Entry::Palette {
            current,
            owner,
            info,
        } => palette_view(*current, *owner, *info),
    }
}

fn palette_view<'a>(current: Colour, owner: Option<TrackId>, info: Id) -> Element<'a, Message> {
    let rows = LIVE_CLIP_PALETTE.chunks(SWATCHES_PER_ROW).map(|chunk| {
        row(chunk
            .iter()
            .map(|&colour| swatch(colour, colour == current, owner, info)))
        .spacing(SWATCH_GAP)
        .into()
    });
    container(column(rows).spacing(SWATCH_GAP))
        .padding(PALETTE_PAD)
        .into()
}

fn swatch<'a>(
    colour: Colour,
    current: bool,
    owner: Option<TrackId>,
    info: Id,
) -> Element<'a, Message> {
    let recolour = match owner {
        Some(owner) => EditMessage::RecolourHeader(owner, colour),
        None => EditMessage::RecolourSelected(colour),
    };
    button(Space::new())
        .width(Length::Fixed(SWATCH_W))
        .height(Length::Fixed(SWATCH_H))
        .padding(0)
        .on_press(Message::Edit(recolour))
        .style(move |_, status| button::Style {
            background: Some(to_colour(colour).into()),
            border: Border {
                color: match (current, status) {
                    (true, _) => crate::control::LIVE_DARK.clip_text_light,
                    (false, button::Status::Hovered) => TEXT,
                    _ => Color::TRANSPARENT,
                },
                width: if current { 2.0 } else { 1.0 },
                radius: crate::control::LIVE_DARK.corner_control.into(),
            },
            ..button::Style::default()
        })
        .info(info, Message::Info)
}

/// Unattended screenshots: `QTABLE_CAPTURE_CLIP=menu` opens the menu on the first clip,
/// `menu-empty` on the first empty slot, `menu-track` on the first track's title and
/// `menu-group` on the first group's header, `menu-output` on the first output's, `menu-main` on Main's, at a fixed window position, with no mouse input.
/// `QTABLE_CAPTURE_MENU_AT=edge` moves that position near the window's bottom right corner.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture(app: &mut App) {
    let target = match std::env::var("QTABLE_CAPTURE_CLIP").as_deref() {
        Ok("hit-menu" | "hit-copy") => {
            crate::inspector::event_editor::capture(
                app,
                std::env::var("QTABLE_CAPTURE_CLIP").as_deref() == Ok("hit-menu"),
            );
            return;
        }
        Ok("menu") => capture_slot(app, false),
        Ok("menu-empty") => capture_slot(app, true),
        Ok("menu-track") => app
            .project()
            .tracks
            .first()
            .map(|t| MenuTarget::Header(t.id)),
        Ok("menu-group") => app
            .project()
            .group_tracks()
            .next()
            .map(|g| MenuTarget::Header(g.id)),
        Ok("menu-main") => Some(MenuTarget::Header(app.project().main.id)),
        Ok("menu-output") => app
            .project()
            .outputs
            .first()
            .map(|o| MenuTarget::Header(o.id)),
        _ => return,
    };
    // `QTABLE_CAPTURE_MENU_AT=edge` opens it near the bottom right corner of the 1024×768
    // capture window, to show the placement rule.
    let at = match std::env::var("QTABLE_CAPTURE_MENU_AT").as_deref() {
        Ok("edge") => Point::new(1000.0, 740.0),
        _ => Point::new(260.0, 120.0),
    };
    if let Some(target) = target {
        app.open_overlay(crate::overlay::Overlay::Context(OpenMenu { at, target }));
    }
}

/// Selects the first empty (or filled) slot and returns the slot menu target.
#[cfg(any(test, feature = "dev-hooks"))]
fn capture_slot(app: &mut App, empty: bool) -> Option<MenuTarget> {
    let selected = app.project().clip_tracks().find_map(|track| {
        let slot = track
            .slots
            .iter()
            .position(|slot| slot.clip.is_none() == empty)?;
        Some(crate::grid::Selection {
            track: track.id,
            slot,
        })
    });
    app.selection.clip = selected;
    selected.map(|_| MenuTarget::Slot)
}

mod content;

#[cfg(test)]
mod tests;

/// Track and lane headers share command order, shortcuts and enablement.
fn clipboard_entries<const N: usize>(
    messages: [Message; N],
    info: [Id; N],
    can_paste: bool,
) -> Vec<Entry> {
    [
        ("Copy", "⌘C", true),
        ("Paste", "⌘V", can_paste),
        ("Duplicate", "⌘D", true),
    ]
    .into_iter()
    .zip(messages)
    .zip(info)
    .map(|(((label, shortcut, enabled), message), info)| {
        command(label, shortcut, message, info, enabled)
    })
    .collect()
}

/// Lane headers share the track menu's commands, renderer and shortcut column.
fn lane_entries(app: &App, target: crate::inspector::event_editor::lanes::Target) -> Vec<Entry> {
    use crate::clip_shortcuts::Shortcut;
    use crate::inspector::event_editor::Action;
    let clipboard = app.clip_view.editor.lane_clipboard.is_some();
    let message = |action| {
        Message::Inspector(crate::inspector::Message::Timeline(Action::LaneCommand(
            target, action,
        )))
    };
    let structural = crate::inspector::event_editor::lanes::structural_selection(app, target);
    let mut entries = if structural {
        clipboard_entries(
            [message(Shortcut::Copy), message(Shortcut::Paste)],
            [Id::CopyLanes, Id::PasteLanes],
            clipboard,
        )
    } else {
        Vec::new()
    };
    if structural {
        entries.push(command(
            "Delete",
            "Del",
            message(Shortcut::Delete),
            Id::DeleteLanes,
            true,
        ));
    }
    // An event row's header: the Target device that defines it.
    if let Some(definition) = crate::inspector::event_editor::lanes::event_definition(app, target) {
        let defined = app
            .project()
            .track(target.key.track)
            .is_some_and(|t| t.event_def(&definition).is_some());
        entries.extend(go_to_device_entry(
            target.key.track,
            Some(definition),
            defined,
            !entries.is_empty(),
        ));
    }
    // The clicked envelope lane's argument, as the Argument menu offers (design 3.4, section 8).
    if let Some(field) = crate::inspector::event_editor::lanes::envelope_field(app, target) {
        entries.push(Entry::Separator);
        entries.push(command(
            "Re-Enable Automation",
            "",
            Message::ReEnableAutomation(Some(field.clone())),
            Id::ReEnableAutomation,
            crate::parameter_gesture::automation_state(app, &field)
                == crate::parameter_gesture::AutomationState::Overridden,
        ));
    }
    // The same item as an event row's, last: the device that defines the automated parameter.
    if let Some(argument) = crate::inspector::event_editor::lanes::envelope_argument(app, target) {
        entries.push(Entry::Separator);
        entries.push(go_to_parameter_entry(app, target.key.track, argument));
    }
    entries
}

/// Go to Device for an automated parameter: the device that defines it, disabled once that
/// definition has gone.
pub(super) fn go_to_parameter_entry(
    app: &App,
    track: TrackId,
    argument: qtable_engine::model::ArgumentRef,
) -> Entry {
    let defined = match argument.device {
        qtable_engine::model::DeviceRef::Mixer => true,
        qtable_engine::model::DeviceRef::Chain(device) => {
            crate::device_chain::parameter_defined(app, track, device, &argument)
        }
    };
    command(
        "Go to Device",
        "",
        Message::Inspector(crate::inspector::Message::GoToParameter(track, argument)),
        Id::GoToDevice,
        defined,
    )
}
