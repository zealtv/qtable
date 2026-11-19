//! The clip editor's three tabs, as Live's Notes | Envelopes | MPE:
//! **Overview**, the stacked lanes in
//! `event_timeline`; **Envelopes**, one argument's lane at full height, chosen with the Device and
//! Control choosers as in Live's clip view; and **Events**, only the clip's event lanes.
//! This module holds the choice (keyed by clip, so each clip remembers
//! its own tab and envelope), the handlers that change it, and the view for the one lane; the
//! point gestures are `point_gesture`'s, driven unchanged.
//!
//! Choosing an argument without a lane shows its current value dotted ("Click to start an
//! envelope"), and the lane appears with the first point.
use super::{event_editor::ClipKey, lane_model::EnvelopeRow};
use crate::app::App;
use crate::info::Id;
#[cfg(any(test, feature = "dev-hooks"))]
use qtable_engine::model::BeatTime;
use qtable_engine::model::{ArgumentRef, ClipId, DeviceRef, Project, TrackId};
use std::collections::HashMap;

mod canvas;
pub(crate) mod chooser;
pub(crate) mod region;
mod tabs;

pub(super) use canvas::{Input, view};
pub(super) use tabs::bar as tab_bar;

/// The clip editor's tabs, in the bar's order. A clip seen for the first time opens on
/// Overview (thread decision 2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum EditorTab {
    /// The clip's event lanes, Live's Notes tab for OSC events.
    Events,
    /// One envelope at full height, picked with the Device and Control choosers.
    Envelopes,
    /// Every event and envelope lane stacked.
    #[default]
    Overview,
}

impl EditorTab {
    pub(crate) const ALL: [Self; 3] = [Self::Events, Self::Envelopes, Self::Overview];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Events => "Events",
            Self::Envelopes => "Envelopes",
            Self::Overview => "Overview",
        }
    }

    pub(crate) fn help(self) -> Id {
        match self {
            Self::Events => Id::EditorTabEvents,
            Self::Envelopes => Id::EditorTabEnvelopes,
            Self::Overview => Id::EditorTabOverview,
        }
    }

    /// The tab's control name, for its focus id and the capture hooks.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Events => "tab-events",
            Self::Envelopes => "tab-envelopes",
            Self::Overview => "tab-overview",
        }
    }
}

/// What the editor shows for one clip: its tab, and the envelope the Envelopes tab edits.
/// The envelope pick is kept while another tab shows, so coming back finds it again.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Choice {
    pub(crate) tab: EditorTab,
    /// The Envelopes tab's device: a chain device or the Mixer. `None` only until the tab
    /// first opens, or when every device has gone.
    pub(crate) device: Option<DeviceRef>,
    /// The chosen control of that device. `None` while a device without arguments is chosen.
    pub(crate) argument: Option<ArgumentRef>,
    /// The Control chooser lists only the lanes this clip has (Live's *Only show adjusted
    /// envelopes*).
    pub(crate) only_adjusted: bool,
}

impl Choice {
    /// The argument whose lane the editor shows at full height: only on the Envelopes tab.
    pub(crate) fn one_lane(&self) -> Option<&ArgumentRef> {
        if self.tab == EditorTab::Envelopes {
            self.device.and(self.argument.as_ref())
        } else {
            None
        }
    }

    /// The identities visible to content selection and keyboard operations in this tab.
    pub(crate) fn visible_lanes(
        &self,
        track: Option<&qtable_engine::model::Track>,
        clip: &qtable_engine::model::Clip,
    ) -> Vec<super::event_editor::lanes::Identity> {
        use super::event_editor::lanes::{self, Identity};
        if let Some(target) = self.one_lane() {
            return vec![Identity::Envelope(target.clone())];
        }
        lanes::displayed(track, clip)
            .into_iter()
            .filter(|id| self.tab != EditorTab::Events || matches!(id, Identity::Event(_)))
            .collect()
    }

    /// The choice as it can be honoured on `track` now. A device that has gone, or an argument
    /// that no longer resolves, is forgotten; the Envelopes tab then shows `first_pick`, as when
    /// it opens without a remembered pick.
    fn resolved(&self, project: &Project, track: TrackId, slot: Option<usize>) -> Self {
        let owner = project.track(track);
        let device_exists = |device: &DeviceRef| match device {
            DeviceRef::Mixer => owner.is_some(),
            DeviceRef::Chain(id) => owner.is_some_and(|t| t.device(*id).is_some()),
        };
        let Some(device) = self.device.filter(device_exists) else {
            let mut fallback = Self {
                tab: self.tab,
                only_adjusted: self.only_adjusted,
                ..Self::default()
            };
            if self.tab == EditorTab::Envelopes {
                fallback.pick(slot.and_then(|slot| first_pick(project, track, slot)));
            }
            return fallback;
        };
        Self {
            tab: self.tab,
            device: Some(device),
            argument: self.argument.clone().filter(|argument| {
                argument.device == device
                    && owner.is_some_and(|t| t.argument_spec(argument).is_some())
            }),
            only_adjusted: self.only_adjusted,
        }
    }

    /// Picks `argument` and its device for the Envelopes tab.
    fn pick(&mut self, argument: Option<ArgumentRef>) {
        self.device = argument.as_ref().map(|a| a.device);
        self.argument = argument;
    }
}

/// What the Envelopes tab shows when it opens without a remembered pick (thread decision 4):
/// the clip's first envelope, else the first automatable control of the track's first device
/// that has one (the Control chooser's order: Targets in chain order, then the Mixer, whose
/// Volume is always there).
fn first_pick(project: &Project, track: TrackId, slot: usize) -> Option<ArgumentRef> {
    let owner = project.track(track)?;
    let clip = project.clip(track, slot)?;
    clip.lanes
        .iter()
        .filter_map(|lane| lane.envelope())
        .map(|envelope| &envelope.target)
        .find(|target| owner.argument_spec(target).is_some())
        .cloned()
        .or_else(|| {
            chooser::entries::entries(project, track, slot)
                .into_iter()
                .flat_map(|device| device.items)
                .map(|item| item.target)
                .next()
        })
}

/// Each clip's zoom window, so a clip comes back as it was left. Session state only: the
/// project never holds it, no command carries it and undo does not touch it. A clip with no
/// entry is fitted. Events, Envelopes and Overview all read the one window because they share
/// the content axis.
#[derive(Default)]
pub(crate) struct Zooms(HashMap<(TrackId, ClipId), crate::control::time_axis::Window>);

impl Zooms {
    /// The clip's window held to `extent`: `None` (fitted) when it shows all of it, as after a
    /// clip shrank to its window's width.
    pub(crate) fn window(
        &self,
        track: TrackId,
        clip: ClipId,
        extent: qtable_engine::model::BeatTime,
    ) -> Option<crate::control::time_axis::Window> {
        self.0.get(&(track, clip))?.normalised(extent)
    }

    pub(crate) fn set(
        &mut self,
        track: TrackId,
        clip: ClipId,
        window: Option<crate::control::time_axis::Window>,
    ) {
        match window {
            Some(window) => self.0.insert((track, clip), window),
            None => self.0.remove(&(track, clip)),
        };
    }

    /// Forget clips the project no longer holds (a load, a deleted clip).
    pub(crate) fn retain_existing(&mut self, project: &Project) {
        self.0
            .retain(|(track, clip), _| holds(project, *track, *clip));
    }
}

fn holds(project: &Project, track: TrackId, clip: ClipId) -> bool {
    project.track(track).is_some_and(|t| {
        t.slots
            .iter()
            .any(|s| s.clip.as_ref().is_some_and(|c| c.id == clip))
    })
}

/// Each clip's choice, so changing clips and coming back finds the same lane showing. Editor
/// state is keyed by clip, not global, so several clips can later draw into one ruler.
#[derive(Default)]
pub(crate) struct Views(HashMap<(TrackId, ClipId), Choice>);

impl Views {
    pub(crate) fn choice(&self, project: &Project, track: TrackId, clip: ClipId) -> Choice {
        let slot = project.track(track).and_then(|t| {
            t.slots
                .iter()
                .position(|s| s.clip.as_ref().is_some_and(|c| c.id == clip))
        });
        self.0
            .get(&(track, clip))
            .map_or_else(Choice::default, |c| c.resolved(project, track, slot))
    }

    fn set(&mut self, track: TrackId, clip: ClipId, choice: Choice) {
        if choice == Choice::default() {
            self.0.remove(&(track, clip));
        } else {
            self.0.insert((track, clip), choice);
        }
    }

    /// Forget clips the project no longer holds (a load, a deleted clip).
    pub(crate) fn retain_existing(&mut self, project: &Project) {
        self.0
            .retain(|(track, clip), _| holds(project, *track, *clip));
    }
}

fn key_for(app: &App, track: TrackId, slot: usize) -> Option<ClipKey> {
    let clip = super::clip_panel_update::clip(app.project(), track, slot)?;
    Some(ClipKey {
        track,
        slot,
        id: clip.id,
        epoch: app.clip_view.editor.epoch,
    })
}

/// The choice for the clip at `slot`, as it stands now.
pub(crate) fn current(app: &App, track: TrackId, slot: usize) -> Choice {
    super::clip_panel_update::clip(app.project(), track, slot).map_or_else(Choice::default, |c| {
        app.clip_view
            .editor
            .views
            .choice(app.project(), track, c.id)
    })
}

/// Changes the clip's choice. Like a clip change, it ends any gesture in flight (a new epoch
/// makes every key from before stale) and drops the selections, then selects the chosen lane
/// when it has one.
fn choose(app: &mut App, track: TrackId, slot: usize, choice: Choice) {
    let Some(clip) = super::clip_panel_update::clip(app.project(), track, slot) else {
        return;
    };
    let id = clip.id;
    let laned = choice.one_lane().is_some_and(|target| {
        clip.lanes
            .iter()
            .any(|l| l.envelope().is_some_and(|e| e.target == *target))
    });
    let editor = &mut app.clip_view.editor;
    editor.views.set(track, id, choice.clone());
    editor.epoch = editor.epoch.wrapping_add(1);
    super::event_editor::content::clear(editor);
    editor.selected = None;
    editor.lanes = None;
    editor.point = None;
    editor.hover = None;
    super::event_editor::forget_point_drafts(app, track, slot);
    region::forget_drafts(app, track, slot);
    if let (Some(key), Some(target)) = (key_for(app, track, slot), choice.one_lane())
        && laned
    {
        app.clip_view.editor.select_lane(
            key,
            super::event_editor::lanes::Identity::Envelope(target.clone()),
        );
    }
}

/// A tab's press. Overview and Events share hit selections; entering Events drops hidden
/// envelope content from the selection. Opening or leaving Envelopes changes which
/// lanes show, so it goes through `choose`, as a chooser pick does. Envelopes opened without a
/// remembered pick shows `first_pick`. The shown tab's press does nothing.
pub(crate) fn choose_tab(app: &mut App, track: TrackId, slot: usize, tab: EditorTab) {
    app.close_overlay();
    let Some(id) = super::clip_panel_update::clip(app.project(), track, slot).map(|c| c.id) else {
        return;
    };
    let old = current(app, track, slot);
    if old.tab == tab {
        return;
    }
    let mut choice = Choice { tab, ..old.clone() };
    if tab == EditorTab::Envelopes && choice.device.is_none() {
        choice.pick(first_pick(app.project(), track, slot));
    }
    if choice.one_lane() == old.one_lane() {
        app.clip_view.editor.views.set(track, id, choice);
        if tab == EditorTab::Events {
            super::event_editor::content::events_only(app, track, slot);
        }
    } else {
        choose(app, track, slot, choice);
    }
}

/// The Device chooser's pick: the device with its first control already chosen, preferring one
/// that has a lane here.
pub(crate) fn choose_device(app: &mut App, track: TrackId, slot: usize, device: DeviceRef) {
    app.close_overlay();
    let all = chooser::entries::entries(app.project(), track, slot);
    let items = all
        .iter()
        .find(|d| d.device == device)
        .map_or(&[][..], |d| &d.items[..]);
    let arguments = |laned: bool| {
        items
            .iter()
            .find(|item| item.laned || !laned)
            .map(|item| item.target.clone())
    };
    let argument = arguments(true).or_else(|| arguments(false));
    let choice = Choice {
        tab: EditorTab::Envelopes,
        device: Some(device),
        argument,
        only_adjusted: current(app, track, slot).only_adjusted,
    };
    choose(app, track, slot, choice);
}

/// The Control chooser's pick, and *Show Envelope*'s: that argument on the Envelopes tab.
pub(crate) fn choose_control(app: &mut App, track: TrackId, slot: usize, target: ArgumentRef) {
    app.close_overlay();
    let mut choice = current(app, track, slot);
    choice.tab = EditorTab::Envelopes;
    choice.pick(Some(target));
    choose(app, track, slot, choice);
}

/// Why *Show Envelope* has no clip to open a lane in. Decided 2026-10-21: only ever the selected
/// clip; a playing clip elsewhere never stands in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unavailable {
    NoClip,
    OtherTrack,
}

/// The slot of the selected clip when it sits on `track` (envelopes are track-local).
pub(crate) fn envelope_slot(app: &App, track: TrackId) -> Result<usize, Unavailable> {
    match app.selection.clip {
        Some(selected) if selected.track == track => Ok(selected.slot),
        Some(_) => Err(Unavailable::OtherTrack),
        None => Err(Unavailable::NoClip),
    }
}

/// *Show Envelope*: the selected clip's Envelopes tab on the control's argument, in the open
/// Detail View. It reuses `choose_control`, so it adds no lane and selects no clip.
pub(crate) fn show_envelope(app: &mut App, field: &qtable_engine::model::FieldRef) {
    app.close_overlay();
    let track = field.parameter.track;
    let Ok(slot) = envelope_slot(app, track) else {
        return;
    };
    if super::clip_panel_update::clip(app.project(), track, slot).is_none() {
        return;
    }
    let argument = ArgumentRef {
        device: field.parameter.device,
        parameter: field.parameter.key.clone(),
        field: field.field.clone(),
    };
    choose_control(app, track, slot, argument);
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    app.chrome.detail_open = true;
}

/// Toggles *Only show adjusted envelopes*; the list stays open.
pub(crate) fn toggle_only_adjusted(app: &mut App, track: TrackId, slot: usize) {
    let Some(id) = super::clip_panel_update::clip(app.project(), track, slot).map(|c| c.id) else {
        return;
    };
    let mut choice = current(app, track, slot);
    choice.only_adjusted = !choice.only_adjusted;
    app.clip_view.editor.views.set(track, id, choice);
}

/// Opens one of the two choosers at the cursor.
pub(crate) fn open_chooser(app: &mut App, track: TrackId, slot: usize, which: chooser::Which) {
    app.open_overlay(crate::overlay::Overlay::Chooser(chooser::OpenChooser {
        at: app.cursor,
        track,
        slot,
        which,
    }));
}

/// The row for the chosen argument: the clip's lane when it has one, else an empty row of the
/// same axis and range.
pub(super) fn chosen_row(
    project: &Project,
    track: TrackId,
    model: &super::lane_model::LaneModel,
    target: &ArgumentRef,
) -> std::rc::Rc<EnvelopeRow> {
    model
        .rows
        .iter()
        .find_map(|row| match &row.kind {
            super::lane_model::RowKind::Envelope(envelope) if envelope.target == *target => {
                Some(envelope.clone())
            }
            _ => None,
        })
        .unwrap_or_else(|| std::rc::Rc::new(EnvelopeRow::of_argument(project, track, target)))
}

/// Capture-only (`tools/gui-capture.sh --show-one-lane`, `--show-tab-*` and their siblings):
/// picks a tab or a lane, or opens a chooser, on the lanes fixture, through the same handlers a
/// click reaches.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture(app: &mut App, scenario: &str) {
    use qtable_engine::model::{DeviceRef, FieldKey, ParameterKey};
    super::event_editor::reconcile(app);
    if matches!(scenario, "menu-enabled" | "menu-disabled" | "show-envelope") {
        let first = app
            .project()
            .clip_tracks()
            .next()
            .and_then(|t| Some((t.id, t.devices.first()?.id)));
        if let Some((track, device)) = first {
            capture_show_envelope(app, track, device, scenario);
        }
        return;
    }
    let Some(selected) = app.selection.clip else {
        return;
    };
    let (track, slot) = (selected.track, selected.slot);
    if scenario == "events-hits" {
        choose_tab(app, track, slot, EditorTab::Events);
        super::event_editor::handle(app, super::event_editor::Action::ClearContent);
        return;
    }
    if let Some(tab) = EditorTab::ALL.into_iter().find(|t| t.name() == scenario) {
        choose_tab(app, track, slot, tab);
        return;
    }
    let Some(device) = app
        .project()
        .track(track)
        .and_then(|t| t.devices.first().map(|d| d.id))
    else {
        return;
    };
    let argument = |parameter: &str, field: &str| ArgumentRef {
        device: DeviceRef::Chain(device),
        parameter: ParameterKey(parameter.into()),
        field: FieldKey(field.into()),
    };
    // At the bottom-left chooser faces in the 1100 pt capture window. The overlay clamps
    // the list above the lower window edge, as it does for a pointer-opened chooser.
    let at = iced::Point::new(264.0, 790.0);
    match scenario {
        "volume" => choose_control(app, track, slot, ArgumentRef::volume()),
        "no-lane" => choose_control(app, track, slot, argument("position", "x")),
        _ => {
            choose_control(app, track, slot, argument("opacity", "value"));
            // The point at 1.2 beats' 0.9, ringed as a gesture would leave it.
            if let Some(key) = key_for(app, track, slot) {
                let lane = super::event_editor::PointTarget {
                    key,
                    revision: app.project_revision(),
                    target: argument("opacity", "value"),
                };
                if scenario == "opacity-unlinked" {
                    let region = app.project().clip(track, slot).unwrap().region();
                    region::submit(app, lane.clone(), Some(region), None);
                }
                let lane = super::event_editor::PointTarget {
                    revision: app.project_revision(),
                    ..lane
                };
                app.clip_view.editor.point =
                    Some(super::event_editor::PointSelection { lane, index: 1 });
                app.clip_view.editor.focused = true;
            }
            match scenario {
                "device-chooser" => open_chooser_at(app, track, slot, chooser::Which::Device, at),
                "control-chooser" => open_chooser_at(
                    app,
                    track,
                    slot,
                    chooser::Which::Control,
                    iced::Point::new(at.x + 114.0, at.y),
                ),
                "menu-point" | "menu-space" | "menu-selection" => {
                    capture_content_menu(app, track, slot, scenario, argument("opacity", "value"))
                }
                _ => {}
            }
        }
    }
}

/// The content menu of the Envelopes tab, opened through the action a right-click publishes:
/// on Opacity's second point, on empty space, or inside a time selection.
#[cfg(any(test, feature = "dev-hooks"))]
fn capture_content_menu(
    app: &mut App,
    track: TrackId,
    slot: usize,
    scenario: &str,
    argument: ArgumentRef,
) {
    use super::event_editor::{Action, content, menu};
    let Some(key) = key_for(app, track, slot) else {
        return;
    };
    let lane = super::event_editor::lanes::Identity::Envelope(argument);
    let under = match scenario {
        "menu-point" => menu::Under::Point(1),
        "menu-space" => menu::Under::Space(BeatTime(2880)),
        _ => {
            let range = BeatTime(960)..BeatTime(2400);
            super::event_editor::handle(
                app,
                Action::Content(content::Selection::time(key, range, vec![lane.clone()])),
            );
            menu::Under::Space(BeatTime(1500))
        }
    };
    app.cursor = iced::Point::new(640.0, 430.0);
    super::event_editor::handle(
        app,
        Action::ContentMenu(menu::Click {
            key,
            revision: app.project_revision(),
            lane,
            under,
        }),
    );
}

/// *Show Envelope* on the Target face's Opacity: its menu with the clip selected (enabled) or
/// not (disabled, the lanes clip still playing), or the view it opens.
#[cfg(any(test, feature = "dev-hooks"))]
fn capture_show_envelope(
    app: &mut App,
    track: TrackId,
    device: qtable_engine::model::DeviceId,
    scenario: &str,
) {
    // The device's first parameter and its first argument, by their keys.
    let Some((entry, field_key)) = app.project().device(device).and_then(|(_, found)| {
        let parameter = found.target()?.parameters.first()?;
        Some((
            crate::info::EntryRef::target(true, track, device, &parameter.key),
            parameter.fields.first()?.key.clone(),
        ))
    }) else {
        return;
    };
    let subject = crate::info::Subject::TargetField(entry, field_key);
    let Some(field) = crate::parameter_gesture::subject_field(app, &subject) else {
        return;
    };
    if scenario == "show-envelope" {
        app.selection.inspector_tab = crate::inspector_panel::Tab::Track;
        app.chrome.detail_open = true;
        show_envelope(app, &field);
        return;
    }
    app.open_overlay(crate::overlay::Overlay::Context(
        crate::context_menu::OpenMenu {
            at: iced::Point::new(420.0, 470.0),
            target: crate::context_menu::MenuTarget::Argument {
                subject,
                revision: app.project_revision(),
            },
        },
    ));
}

#[cfg(any(test, feature = "dev-hooks"))]
fn open_chooser_at(
    app: &mut App,
    track: TrackId,
    slot: usize,
    which: chooser::Which,
    at: iced::Point,
) {
    app.open_overlay(crate::overlay::Overlay::Chooser(chooser::OpenChooser {
        at,
        track,
        slot,
        which,
    }));
}

#[cfg(test)]
mod tests;
