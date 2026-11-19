//! Event lanes are the OSC counterpart of Live's note rows: the header column names
//! each lane's address as Live's key column names each pitch, and the selected lane is
//! highlighted there and across its row.
use super::event_editor::{content, lanes, menu};
use super::{
    Message,
    event_editor::{Action, ClipKey, Draw, HitTarget, PointTarget},
    event_timeline_math::{Geometry as Layout, Grid, LABEL_WIDTH, MARKER_HALF, RULER_HEIGHT},
    lane_model::{EnvelopeRow, LaneModel, RowKind},
    point_gesture::Gesture,
};
use crate::info::Id;
use crate::parameter_gesture::Effective;
use crate::style::{self, detail};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Element, Length, Point, Rectangle, Renderer, Size, Theme, mouse};
use qtable_engine::model::{ArgumentRef, BeatTime, Clip, EventRef, TimeSignature};
use std::cell::RefCell;
use std::time::{Duration, Instant};

mod headers;
mod interaction;
mod markers;
mod points;
pub(super) mod ruler;

#[derive(Clone, Default)]
pub(super) struct Context {
    pub(super) events_only: bool,
    pub(super) content: Option<super::event_editor::content::Selection>,
    pub(super) revision: u64,
    pub(super) alt_down: bool,
    pub(super) shift_down: bool,
    pub(super) command_down: bool,
    pub(super) draw: bool,
    pub(super) focused: bool,
    pub(super) lane_range: Option<(usize, usize)>,
    /// The clip's zoom window; `None` shows all of it.
    pub(super) window: Option<crate::control::time_axis::Window>,
}

/// The two canvases of the Events and Overview editor: the ruler header, which stays where it is,
/// and the lanes under it, which the caller scrolls vertically. Both read one clip, one zoom window
/// and one width, so they stay aligned; the lanes canvas keeps the header's height as blank space
/// at its top, so the header covers exactly that band.
pub(super) struct Parts {
    pub(super) header: Element<'static, Message>,
    pub(super) lanes: Element<'static, Message>,
}

/// The lanes under the clip's brace, event and envelope, from one `LaneModel`. `live` holds
/// one entry per row (`LaneModel::live`); `selection` is the selected event lane (an index
/// into `Clip::lane_refs`) and hit, the selected envelope lane and its selected point.
/// `hover` is the value to read out beside the pointer (`control::tooltip`).
pub(super) fn view(
    (clip, model, live): (
        std::rc::Rc<Clip>,
        std::rc::Rc<LaneModel>,
        Vec<Option<Effective>>,
    ),
    key: ClipKey,
    selection: (
        Option<usize>,
        Option<usize>,
        Option<ArgumentRef>,
        Option<usize>,
    ),
    (grid, signature, playhead, lane_playheads): (
        Grid,
        TimeSignature,
        Option<BeatTime>,
        Vec<(ArgumentRef, BeatTime)>,
    ),
    (context, hover): (Context, Option<String>),
) -> Parts {
    let (lane, selected, envelope, point) = selection;
    let height = (RULER_HEIGHT + model.height()).max(134.0);
    let timeline = Timeline {
        clip,
        key,
        lane,
        selected,
        envelope,
        point,
        grid,
        signature,
        model,
        live,
        playhead,
        lane_playheads,
        context,
        part: Part::Lanes,
    };
    let header_timeline = Timeline {
        part: Part::Header,
        ..timeline.clone()
    };
    let probe = timeline.clone();
    let lanes = crate::info::region(
        canvas::Canvas::new(timeline)
            .width(Length::Fill)
            .height(height),
        move |at, size| Some(probe.info_at(at, size)),
        Message::Info,
    );
    let probe = header_timeline.clone();
    let header = crate::info::region(
        canvas::Canvas::new(header_timeline)
            .width(Length::Fill)
            .height(RULER_HEIGHT),
        move |at, size| Some(probe.info_at(at, size)),
        Message::Info,
    );
    // The scrollable that holds the lanes reserves a gutter for its scrollbar; the header leaves
    // the same one, so both canvases have one width.
    let header = iced::widget::row![
        header,
        iced::widget::Space::new().width(detail::SCROLLBAR_GUTTER)
    ]
    .height(RULER_HEIGHT);
    Parts {
        header: header.into(),
        lanes: crate::control::tooltip_if(
            lanes,
            hover,
            iced::widget::tooltip::Position::FollowCursor,
        ),
    }
}

/// Which half of the editor a canvas is: the ruler header (brace, flags, insert strip, bar
/// numbers, zoom), or the lanes below it (rows, headers, hits, points).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Part {
    Header,
    Lanes,
}
/// The selected lane's row, lighter than both alternating row shades.
const SELECTED_ROW: iced::Color = crate::control::LIVE_DARK.lane_selected;
#[derive(Clone)]
struct Timeline {
    clip: std::rc::Rc<Clip>,
    key: ClipKey,
    lane: Option<usize>,
    selected: Option<usize>,
    envelope: Option<ArgumentRef>,
    /// The selected point of the selected envelope lane.
    point: Option<usize>,
    grid: Grid,
    signature: TimeSignature,
    model: std::rc::Rc<LaneModel>,
    /// Per row: an envelope's live value and whether a manual change overrode it.
    live: Vec<Option<Effective>>,
    playhead: Option<BeatTime>,
    /// Unlinked rows' own playheads, in their own content coordinates.
    lane_playheads: Vec<(ArgumentRef, BeatTime)>,
    context: Context,
    part: Part,
}
#[derive(Clone, Copy)]
struct Drag {
    index: usize,
    origin_x: f32,
    initial: BeatTime,
    preview: BeatTime,
    moved: bool,
    target: HitTarget,
    copy: bool,
}
/// The curves layer: re-evaluated only when the clip, the project revision, the size or which
/// lanes are overridden change, so a playing clip redraws only its playhead and values.
#[derive(Default)]
struct CurveCache {
    key: Option<CurveKey>,
    cache: canvas::Cache,
}
/// The clip, the project revision, which lanes are overridden, and the lane whose point
/// preview stands in for its committed curve.
type CurveKey = (
    ClipKey,
    u64,
    bool,
    Vec<bool>,
    Option<usize>,
    Option<crate::control::time_axis::Window>,
);
/// A point gesture in progress on an envelope lane's row.
struct PointDrag {
    gesture: Gesture,
    row: usize,
    lane: PointTarget,
}
#[derive(Default)]
pub(super) struct State {
    key: Option<ClipKey>,
    events_only: bool,
    revision: u64,
    drag: Option<Drag>,
    marker: Option<ruler::Drag>,
    /// The ruler's zoom gesture.
    zoom: ruler::Zoom,
    header_drag: Option<HeaderDrag>,
    point: Option<PointDrag>,
    paint: Option<Paint>,
    range: Option<content::Drag>,
    region: Option<content::Move>,
    last_click: Option<(Instant, Point, EventRef)>,
    /// An Alt press on an envelope row, for Alt-double-click.
    last_alt: Option<(Instant, Point, usize)>,
    /// The readout last published, so a still pointer publishes nothing.
    hover: Option<String>,
    pointer: Option<Point>,
    curves: RefCell<CurveCache>,
}
/// A Draw Mode stroke in progress on the row it began in.
struct Paint {
    row: usize,
    painting: super::draw::Painting,
}
struct HeaderDrag {
    target: super::event_editor::lanes::Target,
    from: std::ops::Range<usize>,
    origin: Point,
    cursor: Point,
    boundary: usize,
}
impl HeaderDrag {
    fn dragging(&self) -> bool {
        crate::selection::header_dragging(self.origin, self.cursor)
    }
    fn destination(&self) -> usize {
        if self.boundary <= self.from.start {
            self.boundary
        } else if self.boundary >= self.from.end {
            self.boundary - self.from.len()
        } else {
            self.from.start
        }
    }
}
impl Timeline {
    /// A tab can be redrawn before any input resets its retained canvas state.
    fn state_for_view<'a>(&self, state: &'a State) -> Option<&'a State> {
        (state.events_only == self.context.events_only).then_some(state)
    }

    /// Structural actions address the full mixed lane list, even on Events.
    fn source_row(&self, row: usize) -> usize {
        self.model.rows[row].source
    }

    fn drag(&self, state: &State) -> Option<Drag> {
        state
            .drag
            .filter(|d| d.target.key == self.key && d.target.revision == self.context.revision)
    }
    /// The point gesture still describing this clip and revision. A stale one never previews.
    fn point_drag<'s>(&self, state: &'s State) -> Option<&'s PointDrag> {
        state
            .point
            .as_ref()
            .filter(|d| d.lane.key == self.key && d.lane.revision == self.context.revision)
    }
    /// The row drawn from a preview rather than from the committed list.
    fn previewing(&self, state: &State) -> Option<usize> {
        self.point_drag(state)
            .filter(|d| d.gesture.previewing())
            .map(|d| d.row)
            .or_else(|| {
                state
                    .paint
                    .as_ref()
                    .filter(|p| p.painting.preview().is_some())
                    .map(|p| p.row)
            })
    }
    fn axis(&self, layout: &Layout, colour: iced::Color) -> ruler::Axis {
        ruler::Axis {
            time: layout.axis,
            signature: self.signature,
            grid: self.grid.ticks(),
            markers: self.clip.region(),
            colour,
            dragging: false,
        }
    }
    /// The Info View entry under `at`: a brace part or flag, a linked envelope's line and
    /// points, or the timeline's own entry.
    fn info_at(&self, at: Point, size: Size) -> Id {
        let layout = self.layout(Rectangle::new(Point::ORIGIN, size));
        if let Some(part) = self
            .axis(&layout, style::TEXT)
            .brace(style::TEXT)
            .part_at(at)
        {
            return ruler::info(part);
        }
        if ruler::zoom_zone(&self.axis(&layout, style::TEXT), at) {
            return Id::RulerZoom;
        }
        let on_line = layout.lane_at(at, &self.model).is_some_and(|row| {
            self.envelope(row).is_some_and(|e| e.region.is_none())
                && self
                    .lane(row, &layout)
                    .is_some_and(|lane| !matches!(lane.hit(at), super::point_gesture::Hit::Empty))
        });
        if on_line {
            Id::EnvelopeLine
        } else {
            Id::EventTimeline
        }
    }

    fn layout(&self, bounds: Rectangle) -> Layout {
        Layout::new(
            crate::inspector::clip_markers::extent(&self.clip, self.signature),
            bounds.width,
            self.context.window,
        )
    }
}
impl Timeline {
    /// Row shades and headers: cheap, and what changes with selection.
    fn paint_base(
        &self,
        frame: &mut Frame,
        layout: &Layout,
        bounds: Rectangle,
        colour: iced::Color,
    ) {
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), detail::DETAIL_BACKGROUND);
        let line = |frame: &mut Frame, a, b, colour| {
            frame.stroke(
                &Path::line(a, b),
                Stroke::default().with_color(colour).with_width(1.0),
            )
        };
        for (row, lane) in self.model.rows.iter().enumerate() {
            let y = RULER_HEIGHT + lane.top;
            let chosen = self.chosen(row);
            frame.fill_rectangle(
                Point::new(LABEL_WIDTH, y),
                Size::new(layout.span(), lane.height),
                if chosen {
                    SELECTED_ROW
                } else if row % 2 == 0 {
                    crate::control::LIVE_DARK.surface_background
                } else {
                    detail::DETAIL_BACKGROUND
                },
            );
            if chosen {
                // Like Live's highlighted key: a lighter header with the clip colour at its edge.
                frame.fill_rectangle(
                    Point::new(0.0, y),
                    Size::new(LABEL_WIDTH, lane.height),
                    style::HIGHLIGHT,
                );
                frame.fill_rectangle(Point::new(0.0, y), Size::new(3.0, lane.height), colour);
            }
            line(
                frame,
                Point::new(0.0, y + lane.height),
                Point::new(bounds.width, y + lane.height),
                style::CONTROL,
            );
            headers::header(frame, self, row, chosen);
        }
    }

    /// The ruler header's own frame: the ruler, and the time selection's shading in its band.
    /// A zoomed axis clips both to the time area (see `ruler::paint`).
    fn paint_ruler(
        &self,
        frame: &mut Frame,
        layout: &Layout,
        bounds: Rectangle,
        colour: iced::Color,
    ) {
        ruler::paint(frame, &self.axis(layout, colour), bounds.size());
        if let Some(selection) = self.context.content.as_ref().filter(|s| s.picks.is_none()) {
            let axis = self.axis(layout, colour);
            ruler::selection(frame, &axis, &selection.range, 0.0, 19.0);
        }
    }

    /// The time selection's shading down the selected lanes' rows, a frame of its own above
    /// `paint_base`.
    fn paint_selection_rows(&self, frame: &mut Frame, layout: &Layout, colour: iced::Color) {
        let Some(selection) = self.context.content.as_ref().filter(|s| s.picks.is_none()) else {
            return;
        };
        let axis = self.axis(layout, colour);
        for row in &self.model.rows {
            let identity = match &row.kind {
                RowKind::Event { reference, .. } => lanes::Identity::Event(reference.clone()),
                RowKind::Envelope(e) => lanes::Identity::Envelope(e.target.clone()),
            };
            if selection.lanes.contains(&identity) {
                ruler::selection(
                    frame,
                    &axis,
                    &selection.range,
                    RULER_HEIGHT + row.top,
                    row.height,
                );
            }
        }
    }

    /// What the pinned header draws over its ruler: the insert marker and the playhead's marker.
    fn paint_header_marks(
        &self,
        frame: &mut Frame,
        layout: &Layout,
        bounds: Rectangle,
        colour: iced::Color,
    ) {
        let axis = self.axis(layout, colour);
        if let Some(position) = self
            .context
            .content
            .as_ref()
            .and_then(content::Selection::insert_mark)
        {
            ruler::insert(frame, &axis, position, bounds.height, self.context.focused);
        }
        if let Some(position) = self.playhead {
            ruler::playhead_marker(frame, &axis, position);
        }
    }

    /// The pinned header canvas: the ruler band alone, opaque over whatever scrolls beneath it.
    fn draw_header(&self, state: &State, renderer: &Renderer, bounds: Rectangle) -> Vec<Geometry> {
        let layout = self.layout(bounds);
        let colour = crate::outputs::to_colour(self.clip.colour);
        let mut base = Frame::new(renderer, bounds.size());
        base.fill_rectangle(Point::ORIGIN, bounds.size(), detail::DETAIL_BACKGROUND);
        let mut ruler = Frame::new(renderer, bounds.size());
        self.paint_ruler(&mut ruler, &layout, bounds, colour);
        let mut top = Frame::new(renderer, bounds.size());
        layout.axis.within(&mut top, |top| {
            self.paint_header_marks(top, &layout, bounds, colour)
        });
        if let Some(drag) = &state.marker {
            let mut axis = self.axis(&layout, colour);
            axis.markers = drag.preview;
            axis.dragging = true;
            // The cover and the ruler are two frames: a zoomed ruler is clipped, and iced puts
            // a clipped region under what its own frame drew.
            let mut cover = Frame::new(renderer, bounds.size());
            cover.fill_rectangle(Point::ORIGIN, bounds.size(), detail::DETAIL_BACKGROUND);
            let mut preview = Frame::new(renderer, bounds.size());
            ruler::paint(&mut preview, &axis, bounds.size());
            return vec![
                base.into_geometry(),
                ruler.into_geometry(),
                top.into_geometry(),
                cover.into_geometry(),
                preview.into_geometry(),
            ];
        }
        vec![
            base.into_geometry(),
            ruler.into_geometry(),
            top.into_geometry(),
        ]
    }

    /// Hits, the insert marker, empty-lane guides and the playhead, over the curves.
    fn paint_content(
        &self,
        state: &State,
        frame: &mut Frame,
        layout: &Layout,
        bounds: Rectangle,
        colour: iced::Color,
    ) {
        headers::guides(frame, self, layout);
        for (index, event) in self.clip.events.iter().enumerate() {
            let Some(row) = self.model.event_row(&event.definition) else {
                continue;
            };
            let preview = self
                .drag(state)
                .filter(|d| d.index == index && !d.copy)
                .map_or(event.position, |d| d.preview);
            markers::hit(
                frame,
                layout,
                preview,
                self.model.rows[row].top,
                colour,
                self.context
                    .content
                    .as_ref()
                    .map_or(self.selected == Some(index), |s| s.hits.contains(&index)),
                false,
            );
        }
        if let Some(drag) = self.drag(state).filter(|d| d.copy && d.moved)
            && let Some(event) = self.clip.events.get(drag.index)
            && let Some(row) = self.model.event_row(&event.definition)
        {
            markers::hit(
                frame,
                layout,
                drag.preview,
                self.model.rows[row].top,
                colour,
                true,
                true,
            );
        }
        if self.point_drag(state).is_none()
            && let Some(at) = state.pointer
            && let Some(row) = layout.lane_at(at, &self.model)
            && let Some(lane) = self.lane(row, layout)
            && let super::point_gesture::Hit::Segment(left) = lane.hit(at)
            && let Some(envelope) = self.envelope(row)
            && envelope.region.is_none()
        {
            let points = lane.points();
            let hover = crate::control::LIVE_DARK.automation_mouse_over;
            frame.stroke(
                &envelope.polyline(
                    points,
                    &lane,
                    None,
                    (
                        points[left].time.ticks() as f64,
                        points[left + 1].time.ticks() as f64,
                    ),
                ),
                Stroke::default().with_color(hover).with_width(1.5),
            );
            headers::hovered_points(frame, &lane, &points[left..=left + 1], hover);
        }
        headers::points(frame, self, state, layout);
        if let Some(paint) = &state.paint {
            for position in paint.painting.pending_hits() {
                markers::hit(
                    frame,
                    layout,
                    position,
                    self.model.rows[paint.row].top,
                    colour,
                    true,
                    true,
                );
            }
        }
        ruler::dim(frame, &self.axis(layout, colour), bounds.height);
        if let Some(position) = self
            .context
            .content
            .as_ref()
            .and_then(content::Selection::insert_mark)
        {
            ruler::insert(
                frame,
                &self.axis(layout, colour),
                position,
                bounds.height,
                self.context.focused,
            );
        }
        self.paint_playheads(frame, layout, bounds, colour);
    }

    /// The clip's playhead down every row but the unlinked ones, which each draw their own
    /// at their position on the overview's launch-time projection.
    fn paint_playheads(
        &self,
        frame: &mut Frame,
        layout: &Layout,
        bounds: Rectangle,
        colour: iced::Color,
    ) {
        let axis = self.axis(layout, colour);
        let unlinked = || {
            self.model.rows.iter().filter_map(|row| match &row.kind {
                RowKind::Envelope(e) if e.region.is_some() => Some((row, e)),
                _ => None,
            })
        };
        if let Some(position) = self.playhead {
            ruler::playhead_marker(frame, &axis, position);
            let mut top = RULER_HEIGHT;
            for (row, _) in unlinked() {
                ruler::playhead_line(frame, &axis, position, top, RULER_HEIGHT + row.top);
                top = RULER_HEIGHT + row.top + row.height;
            }
            ruler::playhead_line(frame, &axis, position, top, bounds.height);
        }
        for (row, envelope) in unlinked() {
            if let Some((_, position)) = self
                .lane_playheads
                .iter()
                .find(|(target, _)| *target == envelope.target)
            {
                let top = RULER_HEIGHT + row.top;
                ruler::playhead_line(
                    frame,
                    &axis,
                    envelope.overview_time(self.clip.start, *position),
                    top,
                    top + row.height,
                );
            }
        }
    }

    /// What the curves layer depends on. Live values and the playhead are absent on purpose:
    /// a playing clip changes them every frame and must not redraw its curves.
    fn curve_key(&self, previewing: Option<usize>) -> CurveKey {
        let overridden = self
            .live
            .iter()
            .map(|live| live.is_some_and(|l| l.overridden()))
            .collect();
        (
            self.key,
            self.context.revision,
            self.context.events_only,
            overridden,
            previewing,
            self.context.window,
        )
    }

    /// Whether the row is the selected event lane or the selected envelope lane.
    fn chosen(&self, row: usize) -> bool {
        if let Some((start, end)) = self.context.lane_range {
            return (start..end).contains(&self.source_row(row));
        }
        match &self.model.rows[row].kind {
            RowKind::Event { index, .. } => {
                self.lane.is_some() && self.lane == *index && self.envelope.is_none()
            }
            RowKind::Envelope(e) => self.envelope.as_ref() == Some(&e.target),
        }
    }

    pub(super) fn envelope(&self, row: usize) -> Option<&EnvelopeRow> {
        match &self.model.rows.get(row)?.kind {
            RowKind::Envelope(e) => Some(e),
            RowKind::Event { .. } => None,
        }
    }
}
impl canvas::Program<Message> for Timeline {
    type State = State;
    fn update(
        &self,
        state: &mut State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        self.interact(state, event, bounds, cursor)
    }

    fn draw(
        &self,
        state: &State,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let cleared = State::default();
        let state = self.state_for_view(state).unwrap_or(&cleared);
        if self.part == Part::Header {
            return self.draw_header(state, renderer, bounds);
        }
        if let Some(gesture) = state
            .region
            .as_ref()
            .filter(|g| g.moved && g.key == self.key && g.revision == self.context.revision)
            && let Some(selection) = &self.context.content
        {
            let (clip, model, next) = crate::inspector::event_editor::regions::preview(
                &self.model,
                &self.clip,
                selection,
                gesture,
                self.grid,
                self.context.command_down,
            );
            let mut preview = self.clone();
            preview.clip = std::rc::Rc::new(clip);
            preview.model = std::rc::Rc::new(model);
            preview.context.content = Some(next);
            return preview.draw(&State::default(), renderer, &Theme::Dark, bounds, cursor);
        }

        // Capture an in-flight drag by driving the real canvas interaction path.
        let scripted = if crate::capture::lane_drag() {
            let mut scripted = State::default();
            let origin = Point::new(bounds.x + LABEL_WIDTH / 2.0, bounds.y + RULER_HEIGHT + 12.0);
            self.interact(
                &mut scripted,
                &canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(origin),
            );
            let end = Point::new(
                origin.x,
                bounds.y + RULER_HEIGHT + self.model.height() - 4.0,
            );
            self.interact(
                &mut scripted,
                &canvas::Event::Mouse(mouse::Event::CursorMoved { position: end }),
                bounds,
                mouse::Cursor::Available(end),
            );
            Some(scripted)
        } else {
            let layout = self.layout(bounds);
            (0..self.model.rows.len())
                .filter(|row| self.envelope(*row).is_some_and(|e| e.region.is_none()))
                .find_map(|row| self.lane(row, &layout)?.capture_hover())
                .map(|at| State {
                    pointer: Some(at),
                    ..State::default()
                })
        };
        let state = scripted.as_ref().unwrap_or(state);
        let layout = self.layout(bounds);
        let colour = crate::outputs::to_colour(self.clip.colour);
        let mut base = Frame::new(renderer, bounds.size());
        self.paint_base(&mut base, &layout, bounds, colour);
        // The grid lines and the time area's left edge run down from the ruler, so this frame
        // takes the whole ruler too; its band lies under the pinned header, which covers it.
        let mut ruler = Frame::new(renderer, bounds.size());
        ruler::paint(&mut ruler, &self.axis(&layout, colour), bounds.size());
        self.paint_selection_rows(&mut ruler, &layout, colour);
        let previewing = self.previewing(state);
        let key = self.curve_key(previewing);
        let mut curves = state.curves.borrow_mut();
        if curves.key.as_ref() != Some(&key) {
            curves.cache.clear();
            curves.key = Some(key);
        }
        let curves = curves.cache.draw(renderer, bounds.size(), |frame| {
            layout.axis.within(frame, |frame| {
                headers::curves(frame, self, &layout, previewing)
            });
        });
        let mut top = Frame::new(renderer, bounds.size());
        layout.axis.within(&mut top, |top| {
            self.paint_content(state, top, &layout, bounds, colour)
        });
        if let Some(drag) = &state.header_drag
            && drag.dragging()
            && drag.target.key == self.key
            && drag.target.revision == self.context.revision
        {
            let y = RULER_HEIGHT
                + self
                    .model
                    .rows
                    .get(drag.boundary)
                    .map_or(self.model.height(), |r| r.top);
            top.fill_rectangle(
                Point::new(0.0, y - 1.0),
                Size::new(bounds.width, 2.0),
                crate::control::LIVE_DARK.selection_background,
            );
        }
        if self.context.draw
            && let Some(at) = cursor.position_in(bounds).filter(|p| p.y >= RULER_HEIGHT)
        {
            crate::control::Glyph::Pencil.paint_cursor(&mut top, at);
        }
        vec![
            base.into_geometry(),
            ruler.into_geometry(),
            curves,
            top.into_geometry(),
        ]
    }
    fn mouse_interaction(
        &self,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let cleared = State::default();
        let state = self.state_for_view(state).unwrap_or(&cleared);
        let part = cursor.position_in(bounds).and_then(|p| {
            self.axis(&self.layout(bounds), style::TEXT)
                .brace(style::TEXT)
                .part_at(p)
        });
        let in_zone = cursor
            .position_in(bounds)
            .is_some_and(|p| ruler::zoom_zone(&self.axis(&self.layout(bounds), style::TEXT), p));
        if let Some(drag) = &state.marker {
            drag.part().interaction()
        } else if let Some(part) = part {
            part.interaction()
        } else if in_zone || state.zoom.dragging() {
            mouse::Interaction::ZoomIn
        } else if self.context.draw
            && cursor
                .position_in(bounds)
                .is_some_and(|p| p.y >= RULER_HEIGHT)
            && state.header_drag.is_none()
        {
            mouse::Interaction::Hidden
        } else if state.header_drag.as_ref().is_some_and(HeaderDrag::dragging)
            || self.drag(state).is_some()
            || self.previewing(state).is_some()
        {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests;
