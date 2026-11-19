//! GUI selection and command routing for the event-lane editor. A lane is an
//! address; its hits carry a position and their own arguments.
use super::{Message, clip_panel_update::clip, event_timeline_math::Grid};
use crate::app::App;
use crate::control::time_axis::Window;
use crate::drafts::{ClipField, DraftKey};
use crate::edit_actions::{execute_edit, run_edit};
use qtable_engine::edit::{EnvelopeEdit, LaneEdit, ProjectEdit};
use qtable_engine::engine::Command;
use qtable_engine::model::{
    ArgumentRef, BeatTime, Breakpoint, Clip, ClipId, Event, EventRef, FieldRef, TrackId,
};

#[cfg(test)]
use qtable_engine::model::OscAddress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClipKey {
    pub(crate) track: TrackId,
    pub(crate) slot: usize,
    pub(crate) id: ClipId,
    pub(crate) epoch: u64,
}

pub(crate) mod content;
pub(crate) mod drawing;
mod hits;
pub(crate) mod lanes;
pub(crate) mod menu;
pub(crate) mod regions;
pub(crate) mod shapes;
pub(crate) use drawing::Draw;
mod points;
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) use hits::capture;
pub(crate) use hits::{HitTarget, valid_target};
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) use points::capture as capture_points;
pub(crate) use points::{
    choose_value, commit_text, forget_drafts as forget_point_drafts, valid_point_target,
};

/// An envelope lane at a project revision: what a point gesture was started against, so a
/// release against any other list publishes nothing (as `HitTarget` does for hits).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PointTarget {
    pub(crate) key: ClipKey,
    pub(crate) revision: u64,
    pub(crate) target: ArgumentRef,
}

/// The one selected point of the selected envelope lane, at the revision it was selected.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PointSelection {
    pub(crate) lane: PointTarget,
    pub(crate) index: usize,
}

#[derive(Debug, Clone)]
pub(crate) enum Action {
    Grid(Grid),
    Focus(ClipKey),
    Markers(ClipKey, u64, qtable_engine::model::region::Region),
    LaneRegion(PointTarget, Option<qtable_engine::model::region::Region>),
    LaneRegionText(PointTarget, super::lane_view::region::Field, String),
    Content(content::Selection),
    PickPoint(PointTarget, usize),
    RegionMove(content::Move),
    ClearContent,
    /// Insert Shape: one shape over a time selection or a grid step, as one `ReplaceRange`.
    InsertShape(ClipKey, u64, qtable_engine::model::Shape, shapes::Place),
    /// Simplify Envelope: one `ReplaceRange` over the lanes it thinned.
    Simplify(ClipKey, u64),
    /// A finished Draw Mode stroke: one `ReplaceRange`.
    Draw(Draw),
    /// The clip's zoom window, from a ruler drag or a wheel (`None` is the whole clip). GUI
    /// session state only: no engine command, no epoch change, no undo step.
    Zoom(ClipKey, Option<Window>),
    /// Double-click on the ruler: show exactly the time selection, or fit the whole clip when
    /// there is none or the window already shows it.
    ZoomTo(ClipKey),

    Header(ClipKey, usize),
    LaneMenu(lanes::Target),
    LaneCommand(lanes::Target, crate::clip_shortcuts::Shortcut),
    MoveLanes(lanes::Target, std::ops::Range<usize>, usize),
    Select(ClipKey, Option<usize>),
    SelectLane(ClipKey, usize),
    /// A click on a grey automation dot.
    ReEnable(FieldRef),
    Move(HitTarget, BeatTime),
    SelectAt(HitTarget),
    Cursor(ClipKey, BeatTime, Option<usize>),
    /// A right-click on a point, a hit or lane space: settle the selection, open its menu.
    ContentMenu(menu::Click),
    /// A row of an open content menu.
    ContentCommand(menu::ContentTarget, menu::ContentCommand),
    Duplicate(HitTarget, BeatTime),
    Add(ClipKey, BeatTime, EventRef),
    /// A released point gesture: the whole new list, one `SetPoints`, and the point to select.
    SetPoints(PointTarget, Vec<Breakpoint>, Option<usize>),
    /// The first point drawn on an argument that has no lane: the lane is created with it
    /// (`EnvelopeEdit::Add`, then `SetPoints`).
    StartEnvelope(PointTarget, Vec<Breakpoint>, Option<usize>),
    /// The value to show beside the pointer, from hovering or dragging on an envelope lane.
    Hover(Option<String>),
}

#[derive(Default)]
pub(crate) struct State {
    pub(crate) content: Option<content::Selection>,
    /// The selected hit, as an index into the clip's events.
    pub(crate) selected: Option<(ClipKey, usize)>,
    /// One selection for both lane kinds; event indices remain event-only in the strip.
    pub(crate) lanes: Option<lanes::Selection>,
    pub(crate) lane_clipboard: Option<lanes::Clipboard>,
    pub(crate) grid: Grid,
    pub(crate) focused: bool,
    pub(crate) epoch: u64,
    pub(crate) playhead: Option<BeatTime>,
    /// Each unlinked lane's own playhead, on its own region, kept like `playhead`.
    pub(crate) lane_playheads: Vec<(ArgumentRef, BeatTime)>,
    pub(crate) playing: bool,
    pub(crate) clipboard: Option<regions::Clipboard>,
    pub(crate) revision: u64,
    pub(crate) model_cache: super::lane_model::ModelCache,
    /// The engine's effective values for lane headers, refreshed with the playhead.
    pub(crate) automation: Vec<qtable_engine::scheduler::AutomationValue>,
    pub(crate) alt_down: bool,
    pub(crate) shift_down: bool,
    pub(crate) command_down: bool,
    /// Draw Mode: the switch, B and the canvases read this one flag.
    pub(crate) draw: super::draw::Mode,
    /// The selected point; it belongs to the selected envelope lane.
    pub(crate) point: Option<PointSelection>,
    /// The value read out beside the pointer over an envelope lane.
    pub(crate) hover: Option<String>,
    /// Each clip's chosen lane (All lanes or one argument), kept across clip changes.
    pub(crate) views: super::lane_view::Views,
    /// Each clip's zoom window, kept across clip changes and, like `views`, not saved.
    pub(crate) zooms: super::lane_view::Zooms,
    pub(super) owner: Option<(TrackId, usize, ClipId)>,
}
impl State {
    /// The playhead an envelope lane draws: its own when unlinked, otherwise the clip's.
    pub(crate) fn playhead_for(&self, target: &ArgumentRef, unlinked: bool) -> Option<BeatTime> {
        if unlinked {
            self.lane_playheads
                .iter()
                .find(|(t, _)| t == target)
                .map(|(_, position)| *position)
        } else {
            self.playhead
        }
    }
    pub(crate) fn selected_for(&self, key: ClipKey) -> Option<usize> {
        self.selected
            .filter(|(owner, _)| *owner == key)
            .map(|(_, index)| index)
    }
    /// Resolve stable event selection to the strip's current event-only index.
    pub(crate) fn lane_for(&self, key: ClipKey, clip: &Clip) -> Option<usize> {
        if let Some(content) = self.content.as_ref().filter(|s| s.key == key) {
            let identity = if content.count() == 1 {
                content
                    .hits
                    .first()
                    .and_then(|i| clip.events.get(*i))
                    .map(|e| lanes::Identity::Event(e.definition.clone()))
            } else {
                content.lanes.first().cloned()
            };
            return match identity.as_ref()? {
                lanes::Identity::Event(r) => clip.lane_refs().iter().position(|i| i == r),
                _ => None,
            };
        }
        match self
            .lanes
            .as_ref()
            .filter(|s| s.key == key)?
            .items
            .first()?
        {
            lanes::Identity::Event(reference) => {
                clip.lane_refs().iter().position(|r| r == reference)
            }
            _ => None,
        }
    }
    pub(crate) fn envelope_for(&self, key: ClipKey) -> Option<ArgumentRef> {
        if let Some(content) = self.content.as_ref().filter(|s| s.key == key) {
            if content.count() == 1 {
                return content.points.first().map(|(t, _)| t.clone());
            }
            return match content.lanes.first()? {
                lanes::Identity::Envelope(t) => Some(t.clone()),
                _ => None,
            };
        }
        match self
            .lanes
            .as_ref()
            .filter(|s| s.key == key)?
            .items
            .first()?
        {
            lanes::Identity::Envelope(target) => Some(target.clone()),
            _ => None,
        }
    }
    pub(crate) fn select_lane(&mut self, key: ClipKey, identity: lanes::Identity) {
        self.content = None;
        self.lanes = Some(lanes::Selection {
            key,
            anchor: identity.clone(),
            items: vec![identity],
            headers: false,
        });
    }
    pub(crate) fn point_for(&self, key: ClipKey) -> Option<usize> {
        self.point
            .as_ref()
            .filter(|p| p.lane.key == key)
            .map(|p| p.index)
    }
    /// Select a hit and the lane it sits on.
    pub(crate) fn select_hit(&mut self, clip: &Clip, key: ClipKey, index: Option<usize>) {
        self.content = None;
        self.lanes = None;
        self.point = None;
        self.selected = index.map(|index| (key, index));
        if let Some(event) = index.and_then(|i| clip.events.get(i)) {
            let item = content::Selection::items(clip)
                .into_iter()
                .find(|(item, i)| {
                    matches!(item.lane, lanes::Identity::Event(_)) && *i == index.unwrap()
                })
                .unwrap()
                .0;
            let mut selection = content::Selection::time(
                key,
                event.position..event.position,
                vec![item.lane.clone()],
            );
            selection.picks = Some(vec![item]);
            selection.resolve(clip);
            self.content = Some(selection);
        }
    }
}

/// Selection never survives a different clip, a replacement at the same slot, or a load.
pub(crate) fn reconcile(app: &mut App) {
    let owner = app
        .selection
        .clip
        .and_then(|s| clip(app.project(), s.track, s.slot).map(|c| (s.track, s.slot, c.id)));
    let revision_changed = app.clip_view.editor.revision != app.project_revision();
    app.clip_view.editor.revision = app.project_revision();
    if app.clip_view.editor.owner != owner {
        content::clear(&mut app.clip_view.editor);
        app.clip_view.editor.owner = owner;
        app.clip_view.editor.playhead = None;
        app.clip_view.editor.lane_playheads.clear();
        app.clip_view.editor.epoch = app.clip_view.editor.epoch.wrapping_add(1);
        app.clip_view.editor.selected = None;
        app.clip_view.editor.lanes = None;
        app.clip_view.editor.focused = false;
    }
    // Positions are content coordinates owned by the scheduler. A missing voice means
    // stopped (including a track stop); keep the last published position until a new
    // publication or a different shown clip, without extrapolating or wrapping it here.
    app.clip_view.editor.playing = owner.is_some_and(|(track, _, id)| {
        app.snapshot
            .clip_positions
            .iter()
            .any(|p| p.track == track && p.clip == id)
    });
    if let Some(position) = owner.and_then(|(track, _, id)| {
        app.snapshot
            .clip_positions
            .iter()
            .find(|p| p.track == track && p.clip == id)
            .map(|p| p.position)
    }) {
        app.clip_view.editor.playhead = Some(position);
    }
    if let Some((track, _, id)) = owner {
        let lanes = &mut app.clip_view.editor.lane_playheads;
        for published in app
            .snapshot
            .lane_positions
            .iter()
            .filter(|p| p.track == track && p.clip == id)
        {
            match lanes
                .iter_mut()
                .find(|(target, _)| *target == published.target)
            {
                Some(lane) => lane.1 = published.position,
                None => lanes.push((published.target.clone(), published.position)),
            }
        }
    }
    let valid = app.clip_view.editor.selected.is_some_and(|(key, index)| {
        active(app, key)
            && clip(app.project(), key.track, key.slot).is_some_and(|c| index < c.events.len())
    });
    if !valid {
        app.clip_view.editor.selected = None;
    }
    lanes::reconcile(app);
    if revision_changed && let Some((track, slot, _)) = owner {
        // Borrow the engine independently: no clip or envelope copies on playback frames.
        let project = app.engine.project();
        if let Some(clip) = clip(project, track, slot) {
            content::sync(&mut app.clip_view.editor, clip, project);
        }
    }
    // The point follows its lane and its revision: any other edit may have moved it.
    let editor = &app.clip_view.editor;
    if let Some(selection) = &editor.point {
        let (track, slot) = (selection.lane.key.track, selection.lane.key.slot);
        if editor.envelope_for(selection.lane.key) != Some(selection.lane.target.clone())
            || points::selected(app).is_none()
        {
            app.clip_view.editor.point = None;
            app.clip_view.editor.hover = None;
            points::forget_drafts(app, track, slot);
        }
    }
    app.clip_view.editor.automation = app.snapshot.automation.clone();
    app.clip_view
        .editor
        .views
        .retain_existing(app.engine.project());
    app.clip_view
        .editor
        .zooms
        .retain_existing(app.engine.project());
}
fn active(app: &App, key: ClipKey) -> bool {
    key.epoch == app.clip_view.editor.epoch
        && app
            .selection
            .clip
            .is_some_and(|s| (s.track, s.slot) == (key.track, key.slot))
        && clip(app.project(), key.track, key.slot).is_some_and(|c| c.id == key.id)
}

pub(crate) fn handle(app: &mut App, action: Action) {
    match action {
        Action::Focus(key) if active(app, key) => app.clip_view.editor.focused = true,
        Action::Markers(key, revision, markers)
            if active(app, key) && app.project_revision() == revision =>
        {
            execute_edit(
                app,
                Command::Edit(ProjectEdit::SetClipMarkers {
                    track: key.track,
                    slot: key.slot,
                    markers,
                }),
            );
        }
        Action::LaneRegion(target, region) => {
            super::lane_view::region::submit(app, target, region, None)
        }
        Action::LaneRegionText(target, field, text) => {
            super::lane_view::region::text(app, target, field, text)
        }
        Action::Content(selection) => content::apply(app, selection),
        Action::ClearContent => content::clear(&mut app.clip_view.editor),
        Action::Draw(draw) => drawing::commit(app, draw),
        Action::Zoom(key, window) if active(app, key) => zoom(app, key, window),
        Action::ZoomTo(key) if active(app, key) => zoom_to(app, key),
        Action::PickPoint(target, index) if valid_point_target(app, &target) => {
            if let Some(clip) = clip_owned(app, target.key)
                && let Some((item, _)) =
                    content::Selection::items(&clip)
                        .into_iter()
                        .find(|(item, i)| {
                            item.lane == lanes::Identity::Envelope(target.target.clone())
                                && *i == index
                        })
            {
                content::pick(app, target.key, item, app.clip_view.editor.shift_down);
            }
        }
        Action::RegionMove(gesture)
            if active(app, gesture.key) && gesture.revision == app.project_revision() =>
        {
            regions::commit_move(app, gesture);
        }
        Action::InsertShape(key, revision, shape, place)
            if active(app, key) && revision == app.project_revision() =>
        {
            shapes::insert(app, key, shape, place);
        }
        Action::Simplify(key, revision)
            if active(app, key) && revision == app.project_revision() =>
        {
            shapes::simplify(app, key);
        }
        Action::Header(key, row) => lanes::select(app, key, row, true),
        Action::LaneMenu(target) => lanes::open_menu(app, target),
        Action::LaneCommand(target, command) => {
            app.close_overlay();
            if lanes::valid(app, target) {
                lanes::shortcut(app, command);
            }
        }
        Action::MoveLanes(target, from, to) => lanes::move_lanes(app, target, from, to),
        Action::SelectAt(target) if valid_target(app, target) => {
            handle(app, Action::Select(target.key, Some(target.index)));
        }
        Action::Cursor(key, position, lane) if active(app, key) => {
            handle(app, Action::Select(key, None));
            if let Some(lane) = lane {
                handle(app, Action::SelectLane(key, lane));
            }
            if let Some(clip) = clip_owned(app, key) {
                let ids = lanes::displayed(app.project().track(key.track), &clip);
                let lanes = lane
                    .and_then(|index| clip.lane_refs().get(index).cloned())
                    .map(|r| vec![lanes::Identity::Event(r)])
                    .unwrap_or_else(|| ids.first().map(|i| vec![i.clone()]).unwrap_or_default());
                content::apply(
                    app,
                    content::Selection::time(key, position..position, lanes),
                );
            }
        }
        Action::ContentMenu(click) => {
            menu::open(app, click);
        }
        Action::ContentCommand(target, command) => menu::command(app, target, command),
        Action::Duplicate(target, position) => hits::duplicate_at(app, target, position),
        Action::Grid(grid) => app.clip_view.editor.grid = grid,
        Action::Select(key, index) if active(app, key) => {
            if let Some(clip) = clip(app.project(), key.track, key.slot).cloned() {
                if let Some(index) = index.filter(|_| app.clip_view.editor.shift_down) {
                    if let Some((item, _)) =
                        content::Selection::items(&clip)
                            .into_iter()
                            .find(|(item, i)| {
                                matches!(item.lane, lanes::Identity::Event(_)) && *i == index
                            })
                    {
                        content::pick(app, key, item, true);
                    }
                } else {
                    app.clip_view.editor.select_hit(&clip, key, index);
                }
            }
            app.clip_view.editor.focused = true;
            reconcile(app);
        }
        Action::ReEnable(field) => {
            crate::edit_actions::execute(app, Command::ReEnableAutomation { field: Some(field) })
        }
        Action::SelectLane(key, lane) if active(app, key) => {
            select_event_lane(app, key, lane);
            app.clip_view.editor.focused = true;
            reconcile(app);
        }
        Action::Move(target, position) if valid_target(app, target) => {
            regions::move_hit(app, target, position, false);
        }

        Action::Add(key, position, lane) if active(app, key) => add(app, key, position, lane),
        Action::SetPoints(lane, new, select) if valid_point_target(app, &lane) => {
            points::set_points(app, lane, new, select, None);
        }
        Action::StartEnvelope(lane, new, select) if points::valid_start(app, &lane) => {
            points::start_envelope(app, lane, new, select);
        }
        // A pointer over an open menu is not reading a value beneath it.
        Action::Hover(text) if app.overlay.is_none() => app.clip_view.editor.hover = text,
        _ => {}
    }
}

/// The extent the zoom window is held to: the clip's content, as the canvases draw it.
fn extent(app: &App, key: ClipKey) -> Option<BeatTime> {
    clip(app.project(), key.track, key.slot)
        .map(|c| super::clip_markers::extent(c, app.project().time_signature))
}

/// Sets the clip's window. It touches the editor's session state alone: no engine command, no
/// epoch bump, no selection change, so it makes no undo step and an undo keeps it.
fn zoom(app: &mut App, key: ClipKey, window: Option<Window>) {
    if let Some(extent) = extent(app, key) {
        let window = window.and_then(|w| w.normalised(extent));
        app.clip_view.editor.zooms.set(key.track, key.id, window);
    }
}

/// Shows exactly the time selection; with none, or when it is already shown, the whole clip.
fn zoom_to(app: &mut App, key: ClipKey) {
    let Some(extent) = extent(app, key) else {
        return;
    };
    let editor = &app.clip_view.editor;
    let current = editor.zooms.window(key.track, key.id, extent);
    let same = |a: Option<Window>, b: Option<Window>| {
        let (a, b) = (Window::resolve(a, extent), Window::resolve(b, extent));
        (a.start - b.start).abs() < 0.5 && (a.visible - b.visible).abs() < 0.5
    };
    let next = editor
        .content
        .as_ref()
        .filter(|s| s.key == key && s.picks.is_none() && !s.range.is_empty())
        // A range as wide as the clip is the fitted view, which `to_range` answers with `None`.
        .map(|s| Window::to_range(extent, &s.range))
        .filter(|target| !same(*target, current))
        .flatten();
    zoom(app, key, next);
}

/// Capture hook: zoom the shown clip to `start,visible` ticks (`QTABLE_CAPTURE_ZOOM=960,1920`),
/// through the ordinary zoom action.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture_zoom(app: &mut App, window: &str) {
    reconcile(app);
    let Some((track, slot, id)) = app.clip_view.editor.owner else {
        return;
    };
    let key = ClipKey {
        track,
        slot,
        id,
        epoch: app.clip_view.editor.epoch,
    };
    let mut numbers = window
        .split(',')
        .filter_map(|n| n.trim().parse::<f64>().ok());
    if let (Some(start), Some(visible)) = (numbers.next(), numbers.next()) {
        handle(app, Action::Zoom(key, Some(Window { start, visible })));
    }
    // QTABLE_CAPTURE_ZOOM_SELECT=<start>,<end>: a time selection across every lane, as a drag
    // would make it (the zoom captures of a selection that starts before the window).
    if let Ok(range) = std::env::var("QTABLE_CAPTURE_ZOOM_SELECT") {
        let mut ticks = range
            .split(',')
            .filter_map(|n| n.trim().parse::<u64>().ok());
        if let (Some(start), Some(end), Some(clip)) =
            (ticks.next(), ticks.next(), clip_owned(app, key))
        {
            let lanes = lanes::displayed(app.project().track(track), &clip);
            content::apply(
                app,
                content::Selection::time(key, BeatTime(start)..BeatTime(end), lanes),
            );
        }
    }
}

/// Convert an event-only row at the UI boundary; selection itself keeps the stable ref.
fn select_event_lane(app: &mut App, key: ClipKey, index: usize) {
    let reference =
        clip(app.project(), key.track, key.slot).and_then(|c| c.lane_refs().get(index).cloned());
    let editor = &mut app.clip_view.editor;
    editor.selected = None;
    editor.lanes = None;
    if let Some(reference) = reference {
        editor.select_lane(key, lanes::Identity::Event(reference));
    }
}

/// Test support: add and select a fresh event lane through the engine edit path.
#[cfg(test)]
pub(crate) fn add_lane(app: &mut App, track: TrackId, slot: usize) {
    let Some(clip) = clip(app.project(), track, slot) else {
        return;
    };
    let key = ClipKey {
        track,
        slot,
        id: clip.id,
        epoch: app.clip_view.editor.epoch,
    };
    let index = clip.lane_refs().len();
    let owner = app.project().track(track);
    let path = (1..)
        .map(|n| match n {
            1 => "/event".into(),
            n => format!("/event{n}"),
        })
        .find(|path: &String| {
            owner.is_none_or(|t| t.find_event(&OscAddress::Relative(path.clone())).is_none())
        })
        .unwrap_or_else(|| "/event".into());
    let edit = ProjectEdit::EditLane {
        track,
        slot,
        edit: LaneEdit::AddPath(path),
    };
    if run_edit(app, Command::Edit(edit)) && active(app, key) {
        select_event_lane(app, key, index);
    }
}

/// A new hit copies the arguments of the lane's nearest earlier hit, as Live reuses the last
/// velocity, or else takes its event's field defaults.
fn add(app: &mut App, key: ClipKey, position: BeatTime, lane: EventRef) {
    let Some(clip) = clip(app.project(), key.track, key.slot) else {
        return;
    };
    let index = clip.events.len();
    let arguments = drawing::new_arguments(app.project(), key.track, &clip.events, &lane, position);
    if run_edit(
        app,
        Command::Edit(ProjectEdit::AddEvent {
            track: key.track,
            slot: key.slot,
            event: Event {
                position,
                definition: lane,
                arguments,
            },
            carried: None,
        }),
    ) && active(app, key)
        && let Some(clip) = clip_owned(app, key)
    {
        app.clip_view.editor.select_hit(&clip, key, Some(index));
        app.clip_view.editor.focused = true;
    }
}

pub(crate) fn shortcut(app: &mut App, shortcut: crate::clip_shortcuts::Shortcut) {
    if lanes::shortcut(app, shortcut) {
        reconcile(app);
        return;
    }
    regions::shortcut(app, shortcut);
    reconcile(app);
}

fn clip_owned(app: &App, key: ClipKey) -> Option<Clip> {
    clip(app.project(), key.track, key.slot).cloned()
}

pub(crate) fn remove(app: &mut App, track: TrackId, slot: usize, index: usize) {
    if crate::edit_actions::execute_structural_edit(
        app,
        Command::Edit(ProjectEdit::RemoveEvent {
            track,
            slot,
            event: index,
        }),
    ) && let Some((key, selected)) = app.clip_view.editor.selected
        && (key.track, key.slot) == (track, slot)
    {
        app.clip_view.editor.selected = if selected == index {
            None
        } else {
            Some((key, selected - usize::from(selected > index)))
        };
    }
}

/// Only timeline interactions retain keyboard ownership. Text fields still receive Delete.
pub(crate) fn track_focus(app: &mut App, message: &crate::messages::Message, passive: bool) {
    if !passive
        && !matches!(
            message,
            crate::messages::Message::Inspector(Message::Timeline(_))
        )
    {
        app.clip_view.editor.focused = false;
    }
    if matches!(
        message,
        crate::messages::Message::CommitField
            | crate::messages::Message::CommitNumber
            | crate::messages::Message::CancelField
            | crate::messages::Message::TabField(_)
    ) {
        app.clip_view.editor.focused = false;
    }
}

/// Test support: place a hit on the clip's first lane, adding a lane if it has none.
#[cfg(test)]
pub(crate) fn add_hit(app: &mut App, track: TrackId, slot: usize, position: BeatTime) {
    if clip(app.project(), track, slot).is_some_and(|c| c.lane_refs().is_empty()) {
        add_lane(app, track, slot);
    }
    let Some(clip) = clip(app.project(), track, slot) else {
        return;
    };
    let key = ClipKey {
        track,
        slot,
        id: clip.id,
        epoch: app.clip_view.editor.epoch,
    };
    let lane = clip.lane_refs()[0].clone();
    add(app, key, position, lane);
}

/// Test support: give the event a clip lane plays this signature, conforming every hit on
/// its track, as the Target device's argument edits will (`target-edits`).
#[cfg(test)]
pub(crate) fn sign_lane(
    app: &mut App,
    track: TrackId,
    slot: usize,
    lane: usize,
    signature: &[qtable_engine::model::FieldType],
) {
    let mut project = app.project().clone();
    let owner = project.track_mut(track).unwrap();
    let reference = owner.slots[slot].clip.as_ref().unwrap().lane_refs()[lane].clone();
    for device in &mut owner.devices {
        let id = device.id;
        let Some(target) = device.target_mut() else {
            continue;
        };
        for event in target
            .events
            .iter_mut()
            .filter(|e| id == reference.device && e.key == reference.key)
        {
            event.fields = signature
                .iter()
                .enumerate()
                .map(|(i, kind)| {
                    qtable_engine::model::FieldDef::new(
                        qtable_engine::model::FieldKey(format!("f{}", i + 1)),
                        *kind,
                        false,
                    )
                })
                .collect();
        }
    }
    for clip in owner.slots.iter_mut().filter_map(|s| s.clip.as_mut()) {
        for event in clip.events.iter_mut().filter(|e| e.definition == reference) {
            event.conform(signature);
        }
    }
    assert!(run_edit(app, Command::LoadProject(project)));
}

#[cfg(test)]
mod tests;
