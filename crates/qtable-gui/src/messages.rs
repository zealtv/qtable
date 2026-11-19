//! Typed message domains keep the update router exhaustive.
use crate::drafts::DraftKey;
use crate::files::Intent;
use crate::files::SaveStamp;
use crate::{grid, inspector};
use qtable_engine::model::{Colour, Project, SceneId, TrackId};
use rfd::MessageDialogResult;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Tick,
    Remote(crate::remote::Action),
    Settings(crate::settings_pane::Action),
    CheckFieldFocus,
    /// A pointer button went down or a finger touched. Heard only by the undo gesture
    /// bookkeeping, which skips key-release boundaries while the pointer is down.
    PointerPressed,
    /// The user moved on (the window lost focus): commit the pending draft as a step of its own.
    CommitField,
    /// A number box applied one step of a drag, a nudge or a reset: commit the draft inside the
    /// current undo step, so a whole drag stays one.
    CommitNumber,
    /// Enter submitted a text field (published right after the field's own submit message):
    /// release the keyboard unless that submit was rejected.
    FieldEntered,
    /// Last window position of the mouse; the clip menu opens there.
    CursorMoved(iced::Point),
    /// Close whichever popover is open (a click away from it).
    CloseOverlay,
    FieldFocusChecked(crate::drafts::DraftKey, String, u64, bool),
    CancelField,
    TabField(bool),
    DismissStatus,
    ToggleRegion(crate::chrome::Region),
    /// Open or close the sections menu above the status bar's mixer icon.
    ToggleSectionsMenu,
    ToggleDetailSection(crate::inspector_panel::Tab, crate::detail_layout::SectionId),
    SetFocus(crate::focus::Focus),
    /// Press on a divider between window regions; the drag follows the cursor until release.
    BeginDrag(crate::dividers::Divider),
    WindowResized(iced::Size),
    GridKey(crate::focus::GridKey),
    KeyPressed {
        key: iced::keyboard::Key,
        modifiers: iced::keyboard::Modifiers,
        repeat: bool,
        ignored: bool,
    },
    MappedKey(crate::control::key_gate::KeyInput),
    KeyReleased(iced::keyboard::Key),
    ReleaseKeys,
    RouteAnchored(TrackId, u64, Option<iced::Rectangle>),
    ClipShortcut(crate::clip_shortcuts::Shortcut),
    Info(crate::info::Hover),
    ControlFocused(crate::drafts::ControlId),
    ReEnableAutomation(Option<qtable_engine::model::FieldRef>),
    AutomationMenu(crate::info::Subject),
    /// Open the argument's one-lane view in the selected clip.
    ShowEnvelope(qtable_engine::model::FieldRef),
    Play,
    Stop,
    GlobalQuantisation(qtable_engine::model::GlobalQuantisation),
    TempoChanged(String),
    TempoSubmitted,
    ModifiersChanged(iced::keyboard::Modifiers),
    /// The Draw Mode switch in the transport bar.
    ToggleDrawMode,
    /// B went up: ends a held (temporary) Draw Mode.
    DrawKeyReleased,
    Grid(grid::Action),
    Inspector(inspector::Message),
    Edit(EditMessage),
    Files(FileMessage),
    Outputs(OutputMessage),
    Mixer(crate::mixer_band::MixerMessage),
    Device(crate::device_chain::Action),
    Browser(crate::browser::Action),
    Map(crate::map_mode::Action),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EditMessage {
    MixerDevice(crate::mixer_device::MixerMessage),
    AddTrack,
    AddScene,
    DeleteSelectedClip,
    DeleteScenes(SceneId),
    CreateSelectedClip,
    ToggleSelectedStop,
    PathChanged(TrackId, String),
    PathSubmitted(TrackId, String),
    /// A press on a field's dim part: focus its input with the cursor at the start.
    FocusField(crate::drafts::DraftKey),
    ReorderTrack(isize),
    ReorderScene(isize),
    DuplicateSelectedClip,
    CutSelectedClip,
    CopySelectedClip,
    PasteClip,
    CopyTracks,
    PasteTracks,
    DuplicateTracks,
    BeginRenameClip,
    RenameDraftChanged(String),
    CommitRenameClip(String),
    BeginRenameHeader(TrackId),
    BeginRenameScene(SceneId),
    RenameHeaderDraftChanged(String),
    CommitRenameHeader(String),
    RecolourHeader(TrackId, Colour),
    DeleteTrack(TrackId),
    ToggleMoveClipMode,
    RecolourSelected(Colour),
}

#[derive(Debug, Clone)]
pub(crate) enum FileMessage {
    New,
    Open,
    Save,
    SaveAs,
    CloseRequested(iced::window::Id),
    DiscardChoice(Intent, MessageDialogResult),
    OpenPicked(Option<PathBuf>),
    SavePicked(Option<PathBuf>, Option<Intent>),
    ProjectLoaded(Result<(PathBuf, Box<Project>), String>),
    ProjectSaved(Result<PathBuf, String>, SaveStamp, Option<Intent>),
}

/// Who owns a route: a track, or a group whose tracks may follow it.

#[derive(Debug, Clone)]
pub(crate) enum OutputMessage {
    /// Click on a row of the route list: choose a destination or Group for the owner's targets.
    ChooseRoute(TrackId, crate::output_band::RouteChoice),
    /// Open the route list (the multi-select dropdown) from the face.
    OpenRouteDropdown(TrackId),
    AddOutput,
    TrackDelayChanged(TrackId, String),
    TrackDelaySubmitted(TrackId, String),
    Ungroup(TrackId),
    CreateGroupAroundSelected,
    OutputHostChanged(TrackId, String),
    OutputHostSubmitted(TrackId, String),
    OutputPortChanged(TrackId, String),
    OutputPortSubmitted(TrackId, String),
    ToggleOutputMode(TrackId),
    OutputLatencyChanged(TrackId, String),
    OutputLatencySubmitted(TrackId, String),
    ToggleOutputEnabled(TrackId),
    StageDraft(DraftKey, String),
    StageSubmit(DraftKey),
    ToggleBudget(TrackId),
    ToggleRefresh(TrackId),
    ToggleBundle(TrackId),
}

impl crate::control::widgets::ControlMessage<crate::drafts::DraftKey> for Message {
    fn info(hover: crate::info::Hover) -> Self {
        Self::Info(hover)
    }
    fn commit_number() -> Self {
        Self::CommitNumber
    }
    fn field_entered() -> Self {
        Self::FieldEntered
    }
    fn focused(id: crate::drafts::ControlId) -> Self {
        Self::ControlFocused(id)
    }
}
