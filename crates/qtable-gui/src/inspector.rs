use crate::drafts::{ClipField, DraftKey, SceneField};
use qtable_engine::model::{FollowAction, SceneId, TrackId};

mod clip;
pub(crate) mod clip_markers;
pub(crate) mod clip_panel_update;
pub(crate) mod clip_panels;
pub(crate) mod draw;
pub(crate) mod event_editor;
mod event_strip;
mod event_timeline;
mod event_timeline_math;
mod follow_controls;
pub(crate) mod lane_model;
pub(crate) mod lane_update;
pub(crate) mod lane_view;
pub(crate) mod musical_time;
mod parse;
pub(crate) mod point_gesture;
mod scene;
pub(crate) mod scene_update;
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) use clip::LANE_SCROLL;
pub(crate) use clip::view;
pub(crate) use parse::parse_argument;
pub(crate) use scene::scene_view;
mod beats;
mod choices;
#[cfg(test)]
pub(crate) use beats::parse_beats;
pub use choices::{ArgKind, LaunchQuantChoice};

#[derive(Debug, Clone)]
pub enum Message {
    Info(crate::info::Hover),
    CommitNumber,
    /// Enter submitted a text field in the Detail View; the window handles it as `Message::FieldEntered`.
    FieldEntered,
    /// A Clip or Scene control was clicked; it takes the window's focus outline.
    ControlFocused(crate::drafts::ControlId),
    Timeline(event_editor::Action),
    SceneDraft(DraftKey, String),
    SceneName(SceneId, String),
    SceneEnabled(SceneId, bool),
    SceneAction(SceneId, bool, FollowAction),
    SceneChance(SceneId, bool, String),
    ScenePercent(SceneId, bool, String),
    SceneChanceSplit(SceneId, u8),
    SceneTime(SceneId, String),
    Draft(DraftKey, String),
    ClipName(TrackId, usize, String),
    ToggleSection(crate::inspector_panel::Tab, crate::detail_layout::SectionId),
    TogglePalette,
    ChanceSplit(TrackId, usize, u8),
    FollowPercent(TrackId, usize, bool, String),
    Length(TrackId, usize, String),
    Marker(TrackId, usize, ClipField, String),
    SetMarker(TrackId, usize, ClipField),
    /// Reveals the Target device that defines an event, and its entry, in the Track tab.
    GoToDevice(TrackId, qtable_engine::model::EventRef),
    /// Reveals the Target device that defines a parameter (the Mixer for Volume), and its
    /// entry, in the Track tab.
    GoToParameter(TrackId, qtable_engine::model::ArgumentRef),
    /// Opens the Device or the Control chooser at the cursor.
    OpenChooser(TrackId, usize, lane_view::chooser::Which),
    /// A clip editor tab's press: Events, Envelopes or Overview.
    ChooseTab(TrackId, usize, lane_view::EditorTab),
    /// The Device chooser's pick.
    ChooseDevice(TrackId, usize, qtable_engine::model::DeviceRef),
    /// The Control chooser's pick: an argument to edit on the Envelopes tab.
    ChooseControl(TrackId, usize, qtable_engine::model::ArgumentRef),
    /// Toggles *Only show adjusted envelopes* in the Control chooser.
    OnlyAdjusted(TrackId, usize),
    /// Typed text for the selected point's Position, Value or Curve.
    PointText(TrackId, usize, ClipField, String),
    /// A level chosen for the selected point's Value (a Toggle press or an Enum option).
    PointValue(TrackId, usize, f64),
    /// Removes one hit.
    RemoveEvent(TrackId, usize, usize),
    ToggleLoop(TrackId, usize),
    ToggleLegato(TrackId, usize),
    Quantisation(TrackId, usize, LaunchQuantChoice),
    FollowEnabled(TrackId, usize, bool),
    FollowA(TrackId, usize, FollowAction),
    FollowB(TrackId, usize, FollowAction),
    FollowTime(TrackId, usize, String),
    FollowChance(TrackId, usize, bool, String),
    Multiplier(TrackId, usize, String),
    FollowLinked(TrackId, usize, bool),
    Colour(TrackId, usize, qtable_engine::model::Colour),
    Argument(TrackId, usize, usize, usize, String, String),
}

pub(crate) fn scene_key(scene: SceneId, field: SceneField) -> DraftKey {
    DraftKey::scene(scene, field)
}

pub(crate) fn field_key(track: TrackId, slot: usize, field: ClipField) -> DraftKey {
    DraftKey::clip(track, slot, field)
}

#[cfg(test)]
mod tests;

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
