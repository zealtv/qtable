//! One capability-driven header look and selected body tint in every region.
use super::layout::{Column, ColumnKey, column_at};
use super::{Action, Grid, TrackId};
use super::{FOLD_W, GAP, cache, draw};
use crate::control::theme::ink;
use crate::control::{ControlKind, Fold, Look as ControlLook};
use crate::selection::{ColumnOwner, SelectionState, SelectionView};
use iced::widget::canvas::{self, Frame};
use iced::{Color, Point, Rectangle, Size, mouse};
use qtable_engine::model::Track;

pub(super) struct HeaderFacts<'a> {
    pub name: &'a str,
    pub colour: Color,
    pub folded: bool,
}

impl<'a> HeaderFacts<'a> {
    pub fn track(track: &'a Track) -> Self {
        Self {
            name: &track.name,
            colour: draw::colour(track.colour),
            folded: track.fold_state,
        }
    }
}

pub(super) struct Look {
    pub selected: bool,
    pub fill: Color,
    pub band_colours: Vec<Color>,
    pub title: String,
    pub folded: bool,
}

pub(super) fn look(col: &Column, facts: &HeaderFacts<'_>, selection: &SelectionView<'_>) -> Look {
    let selected = selection.column_selected(col);
    Look {
        selected,
        fill: title_fill(facts.colour, selected, selection.active_column(col)),
        band_colours: col
            .ancestors
            .iter()
            .map(|(id, colour)| {
                title_fill(
                    *colour,
                    SelectionState::group_selected(selection.tracks, selection.models, *id),
                    selection.active_owner(ColumnOwner::Group(*id)),
                )
            })
            .collect(),
        title: col.number.map_or_else(
            || facts.name.to_owned(),
            |number| format!("{number} {}", facts.name),
        ),
        folded: facts.folded,
    }
}

fn title_fill(colour: Color, selected: bool, active: bool) -> Color {
    if selected && !active {
        crate::control::LIVE_DARK.selection_background
    } else {
        colour
    }
}

pub(super) fn draw(frame: &mut Frame, col: &Column, x: f32, look: &Look, names: &cache::Names) {
    draw::rect(frame, col.title(x), look.fill);
    for ((band, _), colour) in col.bands(x).zip(&look.band_colours) {
        draw::rect(frame, band, *colour);
    }
    let ink = ink(look.fill);
    let room = col.width - 10.0 - if col.caps.foldable { FOLD_W } else { 0.0 };
    draw::label(
        frame,
        names.fit(&look.title, room),
        x + 6.0,
        col.label_y(),
        ink,
        false,
    );
    if col.caps.foldable {
        Fold {
            folded: look.folded,
        }
        .paint(
            frame,
            col.fold(x),
            &ControlLook::glyph(Color::TRANSPARENT, ink),
        );
    }
}

pub(super) fn tint(frame: &mut Frame, col: &Column, x: f32, body_h: f32, look: &Look) {
    if look.selected {
        draw::rect(
            frame,
            Rectangle::new(Point::new(x, col.header_h), Size::new(col.width, body_h)),
            draw::COLUMN_TINT,
        );
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ColumnDrag {
    pub key: ColumnKey,
    pub selected: Vec<TrackId>,
    pub collapse_on_click: bool,
    pub origin: Point,
    pub cursor: Point,
    pub revision: u64,
    pub inside: bool,
    started: bool,
}
impl ColumnDrag {
    pub fn dragging(&self) -> bool {
        self.started || crate::selection::header_dragging(self.origin, self.cursor)
    }
}

pub(super) fn press(
    col: &Column,
    at: Point,
    left: f32,
    extend: bool,
    revision: u64,
) -> (Action, Option<ColumnDrag>) {
    if col.caps.foldable
        && col.fold(left).contains(at)
        && let ColumnKey::Track(id) = col.key
    {
        return (Action::ToggleGroup(id), None);
    }
    (
        selection_action(col, extend),
        col.caps.movable.then_some(ColumnDrag {
            key: col.key,
            selected: vec![id(col)],
            collapse_on_click: false,
            origin: at,
            cursor: at,
            revision,
            inside: true,
            started: false,
        }),
    )
}

fn selection_action(col: &Column, extend: bool) -> Action {
    match col.key {
        ColumnKey::Track(id) if col.caps.foldable => Action::SelectGroup(id, extend),
        ColumnKey::Track(_) if col.caps.destination => Action::SelectColumn(col.owner(), None),
        ColumnKey::Track(id) => Action::SelectTrack(id, extend),
    }
}

pub(super) fn menu(col: &Column) -> Action {
    Action::HeaderMenu(match col.key {
        ColumnKey::Track(id) => id,
    })
}

// Planning is shared by the preview and release; model edits remain in the engine.
#[cfg(any(test, feature = "dev-hooks"))]
mod multi_capture;
mod placement;
#[cfg(test)]
pub(super) use placement::Marker;
#[cfg(test)]
pub(super) use placement::drop_at;
pub(super) use placement::{Drop, DropContinuation, draw_drop, drop_selected_at};
fn id(col: &Column) -> TrackId {
    let ColumnKey::Track(id) = col.key;
    id
}

/// One gesture is shared by the pinned header and slot canvas, so its marker
/// spans both and event propagation cannot run the release twice.
pub(super) type Gesture = std::rc::Rc<GestureState>;

#[derive(Default)]
pub(super) struct GestureState {
    drag: std::cell::RefCell<Option<ColumnDrag>>,
    edge: std::cell::Cell<super::edge_scroll::EdgeScroll>,
    requested_scroll: std::cell::Cell<Option<u32>>,
    /// Only the unattended startup hook freezes preview state.
    capture_hold: std::cell::Cell<bool>,
    pub(super) capture: std::cell::Cell<Option<super::drag_capture::Capture>>,
}
impl GestureState {
    pub(super) fn get(&self) -> Option<ColumnDrag> {
        self.drag.borrow().clone()
    }
    pub(super) fn set(&self, drag: Option<ColumnDrag>) {
        if drag.is_none() || self.drag.borrow().is_none() {
            self.edge.set(Default::default());
            self.requested_scroll.set(None);
        }
        *self.drag.borrow_mut() = drag;
    }
}

pub(super) struct Surface {
    pub bounds: Rectangle,
    pub visible: Option<Rectangle>,
    pub pinned: bool,
}

impl Surface {
    fn visible(&self) -> Rectangle {
        let mut visible = self.visible.unwrap_or(self.bounds);
        if self.pinned {
            // The pinned header drives the gesture throughout its region's body.
            visible.height = self.bounds.height;
        }
        visible
    }
}

#[derive(Clone, Default)]
pub(crate) struct Gestures {
    pub(super) tracks: Gesture,
    pub(super) outputs: Gesture,
}

impl Gestures {
    pub(crate) fn select_for_drag(
        &self,
        region: qtable_engine::model::Region,
        selected: Vec<TrackId>,
        collapse: bool,
    ) {
        use qtable_engine::model::Region;
        let gesture = match region {
            Region::Tracks => &self.tracks,
            Region::Outputs => &self.outputs,
            Region::Main => return,
        };
        if let Some(mut drag) = gesture.get() {
            drag.selected = selected;
            drag.collapse_on_click = collapse;
            gesture.set(Some(drag));
        }
    }

    /// A redraw may queue a drag request before its scrollable reports the
    /// preceding offset. Mirroring that old notification would undo the request.
    pub(crate) fn accepts_scroll(&self, region: qtable_engine::model::Region, offset: u32) -> bool {
        use qtable_engine::model::Region;
        let gesture = match region {
            Region::Tracks => &self.tracks,
            Region::Outputs => &self.outputs,
            Region::Main => return true,
        };
        gesture
            .requested_scroll
            .get()
            .is_none_or(|requested| requested == offset)
    }
}

pub(super) fn shared_drag_update(
    model: (&[Track], u64),
    cols: &[Column],
    xs: &[f32],
    gesture: &Gesture,
    event: &canvas::Event,
    cursor: mouse::Cursor,
    surface: Surface,
) -> Option<canvas::Action<Action>> {
    if gesture.capture_hold.get() {
        return None;
    }
    let mut drag = gesture.get();
    let bounds = surface.bounds;
    let visible = surface.visible();
    let mut at = cursor.land().position();
    if let canvas::Event::Window(iced::window::Event::RedrawRequested(now)) = event {
        // Only the pinned copy advances the shared timer, once per frame.
        if !surface.pinned {
            return None;
        }
        let mut finish_capture = None;
        if let Some(mut capture) = gesture.capture.get() {
            let sample = capture.sample(*now, visible, cols.first()?.header_h, true);
            if sample.first {
                let origin = Point::new(xs[0] + 15., cols[0].header_h - 4.);
                drag = press(&cols[0], origin, xs[0], false, model.1).1;
            }
            at = Some(sample.at);
            finish_capture = sample.finish.then_some(capture.drop);
            gesture.capture.set(Some(capture));
        }
        let current = drag.as_mut()?;
        if current.revision != model.1
            || !cols.iter().any(|col| col.key == current.key)
            || at.is_none()
        {
            gesture.set(None);
            return Some(canvas::Action::request_redraw());
        }
        refresh_cursor(current, at.unwrap(), bounds, visible);
        if let Some(dropping) = finish_capture {
            #[cfg(any(test, feature = "dev-hooks"))]
            let preview = drop_selected_at(
                model.0,
                cols,
                xs,
                &current.selected,
                current.key,
                current.cursor,
            );
            #[cfg(any(test, feature = "dev-hooks"))]
            eprintln!(
                "autoscroll-track offset={} cursor={:?} preview={preview:?} drop={dropping}",
                visible.x - bounds.x,
                current.cursor
            );
            let action = dropping
                .then(|| release_action(model.0, cols, xs, current, current.cursor, bounds))
                .flatten();
            #[cfg(any(test, feature = "dev-hooks"))]
            eprintln!("autoscroll-track release={action:?}");
            gesture.set(if dropping { None } else { drag });
            gesture.capture_hold.set(true);
            return Some(
                action.map_or_else(canvas::Action::request_redraw, canvas::Action::publish),
            );
        }
        let mut edge = gesture.edge.get();
        let step = if current.inside && current.dragging() {
            edge.step(
                *now,
                at.unwrap().x,
                visible.x..visible.x + visible.width,
                (visible.x - bounds.x).max(0.0),
                (bounds.width - visible.width).max(0.0),
            )
        } else {
            None
        };
        if step.is_none() {
            edge = Default::default();
            gesture.requested_scroll.set(None);
        }
        gesture.edge.set(edge);
        gesture.set(drag);
        return step.map(|step| {
            let offset = (visible.x - bounds.x).max(0.0);
            if step.offset.round() != offset.round() {
                gesture
                    .requested_scroll
                    .set(Some(step.offset.round() as u32));
                canvas::Action::publish(Action::ScrollColumns {
                    region: cols[0].caps.region,
                    footer: true,
                    offset: step.offset.round() as u32,
                })
            } else {
                canvas::Action::request_redraw_at(step.next)
            }
        });
    }
    if gesture.capture.get().is_some() {
        return None;
    }
    // The toolkit preserves content coordinates outside a scrollable. Those
    // coordinates must never make a drop in a neighbouring fixed region valid.
    let release_cursor = if at.is_some_and(|at| visible.contains(at)) {
        cursor
    } else {
        mouse::Cursor::Unavailable
    };
    let action = drag_update(model, cols, xs, &mut drag, event, release_cursor, bounds);
    if let Some(current) = drag.as_mut() {
        if let Some(at) = at {
            refresh_cursor(current, at, bounds, visible);
        }
        if !current.inside {
            gesture.edge.set(Default::default());
            gesture.requested_scroll.set(None);
        }
    }
    gesture.set(drag);
    action.map(canvas::Action::and_capture)
}

fn refresh_cursor(drag: &mut ColumnDrag, at: Point, bounds: Rectangle, visible: Rectangle) {
    drag.cursor = Point::new(at.x - bounds.x, at.y - bounds.y);
    drag.started |= crate::selection::header_dragging(drag.origin, drag.cursor);
    drag.inside = visible.contains(at);
}

pub(super) fn drag_update(
    model: (&[Track], u64),
    cols: &[Column],
    xs: &[f32],
    state: &mut Option<ColumnDrag>,
    event: &canvas::Event,
    cursor: mouse::Cursor,
    bounds: Rectangle,
) -> Option<canvas::Action<Action>> {
    let drag = state.as_mut()?;
    if drag.revision != model.1 || !cols.iter().any(|col| col.key == drag.key) {
        *state = None;
        return Some(canvas::Action::request_redraw());
    }
    match event {
        canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
            // Scrollable levitates its child cursor outside the visible viewport.
            // land() keeps its translated content coordinates so hidden boundaries remain reachable.
            drag.inside = cursor.land().position().is_some_and(|point| {
                drag.cursor = Point::new(point.x - bounds.x, point.y - bounds.y);
                drag.started |= crate::selection::header_dragging(drag.origin, drag.cursor);
                drag.cursor.x >= 0.0
                    && drag.cursor.x <= bounds.width
                    && drag.cursor.y >= 0.0
                    && drag.cursor.y <= bounds.height
            });
            Some(canvas::Action::request_redraw())
        }
        canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            let mut drag = state.take()?;
            // Resolve the actual release, never a remembered valid hover. Levitating
            // cursors retain scroll-content coordinates but cannot cross regions.
            let at = cursor
                .land()
                .position()
                .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y));
            Some(
                match at.and_then(|at| release_action(model.0, cols, xs, &mut drag, at, bounds)) {
                    Some(action) => canvas::Action::publish(action),
                    None => canvas::Action::request_redraw(),
                },
            )
        }
        canvas::Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            ..
        })
        | canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
        | canvas::Event::Mouse(mouse::Event::CursorLeft)
        | canvas::Event::Window(iced::window::Event::Unfocused) => {
            *state = None;
            Some(canvas::Action::request_redraw())
        }
        _ => None,
    }
}

fn release_action(
    tracks: &[Track],
    cols: &[Column],
    xs: &[f32],
    drag: &mut ColumnDrag,
    at: Point,
    bounds: Rectangle,
) -> Option<Action> {
    if at.x < 0. || at.x > bounds.width || at.y < 0. || at.y > bounds.height {
        return None;
    }
    drag.cursor = at;
    if !drag.dragging() && drag.collapse_on_click {
        let col = cols.iter().find(|c| c.key == drag.key)?;
        return Some(selection_action(col, false));
    }
    let drop = drag
        .dragging()
        .then(|| drop_selected_at(tracks, cols, xs, &drag.selected, drag.key, at))
        .flatten()?;
    Some(if drag.selected.len() == 1 {
        Action::PlaceTrack {
            track: drag.selected[0],
            before: drop.before,
            group: drop.group,
        }
    } else {
        Action::PlaceTracks {
            tracks: drag.selected.clone(),
            before: drop.before,
            group: drop.group,
        }
    })
}

impl Grid<'_> {
    pub(super) fn pending_drop(&self, drag: &ColumnDrag) -> Option<Drop> {
        (drag.revision == self.revision && drag.inside && drag.dragging())
            .then(|| {
                drop_selected_at(
                    self.tracks,
                    &self.layout.columns,
                    &self.layout.xs,
                    &drag.selected,
                    drag.key,
                    drag.cursor,
                )
            })
            .flatten()
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod drag_tests;

#[cfg(test)]
mod multi_drag_tests;
#[cfg(test)]
mod nested_drag_tests;

/// Startup-only capture: seed the production gesture and placement planner,
/// routing selection/placement actions through normal update without pointer events.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture_session(app: &mut crate::app::App, scenario: &str) {
    if multi_capture::setup(app, scenario)
        || super::drag_capture::setup(app, scenario)
        || crate::scene_stop_feedback::capture_session(app, scenario)
    {
        return;
    }
    // Select/fold through ordinary update messages, without pointer input.
    let selected_group = match scenario {
        "select-root" | "select-root-folded" => Some(1),
        "select-middle" | "select-middle-inner-folded" => Some(2),
        "select-inner" | "select-inner-folded" | "select-inner-hidden" => Some(3),
        _ => None,
    };
    if let Some(id) = selected_group {
        let mut actions = vec![Action::SelectGroup(TrackId(id), false)];
        match scenario {
            "select-root-folded" => actions.push(Action::ToggleGroup(TrackId(1))),
            "select-middle-inner-folded" | "select-inner-folded" => {
                actions.push(Action::ToggleGroup(TrackId(3)));
            }
            "select-inner-hidden" => actions.push(Action::ToggleGroup(TrackId(2))),
            _ => {}
        }
        for action in actions {
            let _ = crate::update::update(app, crate::messages::Message::Grid(action));
        }
        return;
    }
    // Acceptance-only native stills use ordinary selection and header commands.
    // The fixture is the header builder's two-expanded/three-expanded project.
    if matches!(
        scenario,
        "nested-wrap" | "single-wrap" | "inner-menu" | "inner-ungroup" | "all-ungroup"
    ) {
        use crate::messages::{Message, OutputMessage};
        let select = |app: &mut crate::app::App, action| {
            let _ = crate::update::update(app, Message::Grid(action));
        };
        match scenario {
            "inner-menu" => {
                app.cursor = Point::new(340., 100.);
                select(app, Action::HeaderMenu(TrackId(3)));
            }
            "nested-wrap" | "single-wrap" => {
                if scenario == "nested-wrap" {
                    select(app, Action::SelectGroup(TrackId(3), false));
                    select(app, Action::SelectTrack(TrackId(6), true));
                } else {
                    select(app, Action::SelectTrack(TrackId(8), false));
                }
                let _ = crate::update::update(
                    app,
                    Message::Outputs(OutputMessage::CreateGroupAroundSelected),
                );
            }
            "inner-ungroup" => {
                select(app, Action::SelectGroup(TrackId(3), false));
                let _ = crate::update::update(
                    app,
                    Message::Outputs(OutputMessage::Ungroup(TrackId(3))),
                );
            }
            "all-ungroup" => loop {
                let group = app.project().group_tracks().next().map(|g| g.id);
                let Some(group) = group else { break };
                select(app, Action::SelectGroup(group, false));
                let _ = crate::update::update(app, Message::Outputs(OutputMessage::Ungroup(group)));
            },
            _ => unreachable!(),
        }
        return;
    }
    let (source, target, fraction) = match scenario.trim_end_matches("-drop") {
        "drag-inner" => (8, 5, 0.25),
        "drag-first" => (8, 3, 0.75),
        "drag-outside" => (4, 5, 0.75),
        "drag-root" => (4, 1, 0.25),
        "drag-group" => (3, 8, 0.25),
        "drag-folded-left" => (8, 3, 0.25),
        "drag-folded-right" => (8, 3, 0.75),
        _ => return,
    };
    let layout = app.grid_layout.get(app.project(), app.project_revision());
    let ci = |id| {
        layout
            .columns
            .iter()
            .position(|c| c.key == ColumnKey::Track(TrackId(id)))
            .expect("capture fixture track")
    };
    let from = ci(source);
    let to = ci(target);
    let origin = Point::new(layout.xs[from] + 10., layout.header_h - 4.);
    let at = Point::new(
        layout.xs[to] + layout.columns[to].width * fraction,
        layout.header_h - 4.,
    );
    let (action, mut drag) = press(
        &layout.columns[from],
        origin,
        layout.xs[from],
        false,
        app.project_revision(),
    );
    let _ = crate::update::update(app, crate::messages::Message::Grid(action));
    let bounds = Rectangle::with_size(Size::new(layout.end() + super::TRACK_W, layout.header_h));
    if let Some(current) = drag.as_mut() {
        current.cursor = at;
    }
    if scenario.ends_with("-drop") {
        let action = drag.as_mut().and_then(|current| {
            release_action(
                app.project().tracks.as_slice(),
                &layout.columns,
                &layout.xs,
                current,
                at,
                bounds,
            )
        });
        if let Some(action) = action {
            let _ = crate::update::update(app, crate::messages::Message::Grid(action));
        }
        drag = None;
    }
    app.header_gestures.tracks.capture_hold.set(drag.is_some());
    app.header_gestures.tracks.set(drag);
}
