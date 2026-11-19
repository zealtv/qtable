//! One argument's lane at full height and the editor's full width. The curve, the points and the
//! gestures are the overview's: `point_gesture` is driven unchanged, this view giving it a `Plot`
//! that fills the editor body but for the value scale at its left (`VALUE_SCALE_WIDTH`), the only
//! thing beside the lane, as in Live's Envelopes editor. The ruler starts where the lane does. Nothing here knows curve maths (`EnvelopeRow::samples` is the engine's own
//! evaluation) or edits the project (a release publishes one `Action`).
//!
//! Ghosts, on by default with no switch (decision B5): the clip's other envelopes at half alpha,
//! each on its own range, plus a tick on the floor for every hit. They take no clicks.
use crate::control::LIVE_DARK;
use crate::control::time_axis::TimeAxis;
use crate::info::Id;
use crate::inspector::Message;
use crate::inspector::draw::{Outcome, Painting};
use crate::inspector::event_editor::{Action, ClipKey, Draw, PointTarget, content, lanes, menu};
use crate::inspector::event_timeline::{Context, ruler};
use crate::inspector::event_timeline_math::{Grid, RULER_HEIGHT};
use crate::inspector::lane_model::{EnvelopeRow, LaneModel, RowKind};
use crate::inspector::point_gesture::{Gesture, Hit, Keys, Lane, Plot, Readout};
use crate::parameter_gesture::Effective;
use crate::style::{self, detail};
use iced::keyboard::{Key, key::Named};
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme, mouse};
use qtable_engine::model::{ArgumentRef, Axis, BeatTime, Breakpoint, Clip, TimeSignature};
use std::cell::RefCell;
use std::time::{Duration, Instant};

/// Space between the ruler and the lane's ceiling (room for the top value label, centred on
/// it), and below its floor (room for the hits' ticks), and the gutter at the right edge.
const PAD_TOP: f32 = 14.0;
const PAD_BOTTOM: f32 = 18.0;
const GUTTER: f32 = 10.0;
/// A point's dot, and the ring on the selected or grabbed one.
const POINT_RADIUS: f32 = 3.5;
const RING_RADIUS: f32 = 6.0;
const CURVE_WIDTH: f32 = 2.0;
/// Levels closer than this stack their labels on each other, so only the ends are labelled.
const MIN_LEVEL_GAP: f32 = 14.0;
/// The floor ticks of the clip's hits.
const TICK: Size = Size::new(3.0, 8.0);

pub(in crate::inspector) struct Input<'a> {
    pub clip: std::rc::Rc<Clip>,
    /// Every lane of the clip, for the ghosts; `live` has one entry per row.
    pub model: std::rc::Rc<LaneModel>,
    pub live: &'a [Option<Effective>],
    pub chosen: std::rc::Rc<EnvelopeRow>,
    pub live_chosen: Option<Effective>,
    pub laned: bool,
    pub key: ClipKey,
    pub point: Option<usize>,
    pub grid: Grid,
    pub signature: TimeSignature,
    pub playhead: Option<BeatTime>,
    pub context: Context,
    pub hover: Option<String>,
}

/// The lane, filling its space; the value being dragged or hovered reads out beside the pointer.
pub(in crate::inspector) fn view(input: Input<'_>) -> Element<'static, Message> {
    let hover = input.hover.clone();
    let lane = OneLane {
        clip: input.clip,
        model: input.model,
        live: input.live.to_vec(),
        chosen: input.chosen,
        live_chosen: input.live_chosen,
        laned: input.laned,
        key: input.key,
        point: input.point,
        grid: input.grid,
        signature: input.signature,
        playhead: input.playhead,
        context: input.context,
    };
    let probe = lane.clone();
    let canvas = crate::info::region(
        canvas::Canvas::new(lane)
            .width(Length::Fill)
            .height(Length::Fill),
        move |at, size| Some(probe.info_at(at, size)),
        Message::Info,
    );
    crate::control::tooltip_if(canvas, hover, iced::widget::tooltip::Position::FollowCursor)
}

#[derive(Clone)]
struct OneLane {
    clip: std::rc::Rc<Clip>,
    model: std::rc::Rc<LaneModel>,
    live: Vec<Option<Effective>>,
    chosen: std::rc::Rc<EnvelopeRow>,
    live_chosen: Option<Effective>,
    laned: bool,
    key: ClipKey,
    point: Option<usize>,
    grid: Grid,
    signature: TimeSignature,
    playhead: Option<BeatTime>,
    context: Context,
}

/// A point gesture in progress, and what it was started against.
struct PointDrag {
    gesture: Gesture,
    lane: PointTarget,
}

/// What the static layer depends on: the clip, its revision, which lanes are overridden (their
/// ghosts grey), the grid (its lines), and the lane the labels describe.
type StaticKey = (
    ClipKey,
    u64,
    Vec<bool>,
    u64,
    ArgumentRef,
    Option<crate::control::time_axis::Window>,
);

#[derive(Default)]
struct StaticLayer {
    key: Option<StaticKey>,
    /// One cache for each frame `paint_static` fills.
    caches: [canvas::Cache; 4],
}

#[derive(Default)]
struct State {
    /// The clip, revision and lane this state was made for; any other and it starts afresh, so
    /// a gesture never outlives its lane, as it never outlives its clip.
    stamp: Option<(ClipKey, u64, ArgumentRef)>,
    drag: Option<PointDrag>,
    marker: Option<ruler::Drag>,
    /// The ruler's zoom gesture.
    zoom: ruler::Zoom,
    paint: Option<Painting>,
    range: Option<content::Drag>,
    region: Option<content::Move>,
    last_alt: Option<(Instant, Point)>,
    /// The readout last published, so a still pointer publishes nothing.
    hover: Option<String>,
    /// What the pointer rests on, for the hovered segment's ink.
    hovered: Option<Hit>,
    layer: RefCell<StaticLayer>,
}

impl OneLane {
    fn plot(&self, size: Size) -> Plot {
        Plot {
            left: detail::VALUE_SCALE_WIDTH,
            width: (size.width - detail::VALUE_SCALE_WIDTH - GUTTER).max(1.0),
            top: RULER_HEIGHT + PAD_TOP,
            height: (size.height - RULER_HEIGHT - PAD_TOP - PAD_BOTTOM).max(24.0),
        }
    }

    fn lane(&self, size: Size) -> Lane {
        let plot = self.plot(size);
        self.chosen
            .lane(
                plot,
                crate::inspector::clip_markers::extent(&self.clip, self.signature),
                self.grid.ticks(),
            )
            .with_time(self.time(&plot))
            .with_default(self.live_chosen.map(|l| l.value))
    }

    /// Where time sits across the plot: the clip's zoom window, or all of it.
    fn time(&self, plot: &Plot) -> TimeAxis {
        TimeAxis::zoomed(
            plot.left,
            plot.width,
            crate::inspector::clip_markers::extent(&self.clip, self.signature),
            self.context.window,
        )
    }

    fn axis(&self, plot: &Plot) -> ruler::Axis {
        ruler::Axis {
            time: self.time(plot),
            signature: self.signature,
            grid: self.grid.ticks(),
            markers: self.chosen.region.unwrap_or_else(|| self.clip.region()),
            colour: if self.chosen.region.is_some() {
                LIVE_DARK.automation_color
            } else {
                crate::outputs::to_colour(self.clip.colour)
            },
            dragging: false,
        }
    }

    fn paint_ruler(&self, frame: &mut Frame, axis: &ruler::Axis, size: Size) {
        ruler::paint_with_flags(
            frame,
            axis,
            size,
            if self.chosen.region.is_some() {
                LIVE_DARK.automation_color
            } else {
                style::TEXT
            },
        );
    }

    fn keys(&self) -> Keys {
        Keys {
            alt: self.context.alt_down,
            shift: self.context.shift_down,
            command: self.context.command_down,
        }
    }

    fn target(&self) -> PointTarget {
        PointTarget {
            key: self.key,
            revision: self.context.revision,
            target: self.chosen.target.clone(),
        }
    }

    /// The gesture still describing this clip, revision and lane. A stale one never previews.
    fn drag<'s>(&self, state: &'s State) -> Option<&'s PointDrag> {
        state.drag.as_ref().filter(|d| d.lane == self.target())
    }

    fn readout(&self, readout: Readout) -> String {
        match readout {
            Readout::Value(value) => self.chosen.value_text(value),
            Readout::Curve(curve) => format!("Curve {curve:.2}"),
        }
    }

    /// The lane's ink: red while it follows the clip, grey once overridden.
    fn ink(live: Option<Effective>) -> Color {
        if live.is_some_and(|l| l.overridden()) {
            LIVE_DARK.automation_disabled
        } else {
            LIVE_DARK.automation_color
        }
    }

    fn static_key(&self) -> StaticKey {
        (
            self.key,
            self.context.revision,
            self.live
                .iter()
                .map(|live| live.is_some_and(|l| l.overridden()))
                .collect(),
            self.grid.ticks(),
            self.chosen.target.clone(),
            self.context.window,
        )
    }

    /// The lane's room, the ruler, the value labels and the ghosts: nothing a pointer moves.
    /// Four layers, each its own frame. A zoomed axis clips the ruler, the ghosts and the
    /// floor ticks, and iced puts a clipped region under every shape its own frame drew, so
    /// the clipped layers cannot share a frame with the fills beneath them.
    fn paint_static(&self, layer: usize, frame: &mut Frame, size: Size) {
        let plot = self.plot(size);
        let axis = self.axis(&plot);
        match layer {
            0 => {
                frame.fill_rectangle(Point::ORIGIN, size, detail::DETAIL_BACKGROUND);
                frame.fill_rectangle(
                    Point::new(plot.left, RULER_HEIGHT),
                    Size::new(plot.width, size.height - RULER_HEIGHT),
                    LIVE_DARK.surface_background,
                );
            }
            1 => {
                self.paint_ruler(frame, &axis, size);
                if self.chosen.region.is_some() {
                    axis.time.within(frame, |frame| {
                        ruler::reference_brace(frame, &axis, self.clip.region())
                    });
                }
            }
            2 => self.paint_labels(frame, &plot),
            _ => axis.time.within(frame, |frame| {
                self.paint_ghosts(frame, &plot);
                let floor = plot.top + plot.height;
                let tick = Color {
                    a: detail::NON_EDITABLE_ALPHA,
                    ..crate::outputs::to_colour(self.clip.colour)
                };
                for event in &self.clip.events {
                    frame.fill_rectangle(
                        Point::new(axis.x(event.position) - TICK.width / 2.0, floor + 2.0),
                        TICK,
                        tick,
                    );
                }
            }),
        }
    }

    /// The value scale's labels and where each sits: right-aligned against the lane's left rule,
    /// centred on its level, as Live's. The minimum and maximum, or each level of a Discrete
    /// argument by name.
    fn scale(&self, plot: &Plot) -> Vec<(String, Point)> {
        let row = &self.chosen;
        let lane = row.lane(
            *plot,
            crate::inspector::clip_markers::extent(&self.clip, self.signature),
            self.grid.ticks(),
        );
        let values: Vec<f64> = match row.axis {
            Axis::Discrete => {
                let levels = (row.max - row.min).round().max(0.0) as usize + 1;
                let room = plot.height / levels.saturating_sub(1).max(1) as f32;
                if room >= MIN_LEVEL_GAP {
                    (0..levels).map(|i| row.min + i as f64).collect()
                } else {
                    vec![row.min, row.max]
                }
            }
            Axis::Continuous | Axis::Stepped => vec![row.min, row.max],
        };
        values
            .into_iter()
            .map(|value| {
                let at = Point::new(plot.left - detail::VALUE_SCALE_INSET, lane.y(value));
                (row.value_text(value), at)
            })
            .collect()
    }

    /// The value scale, with a faint line at each labelled level across the lane.
    fn paint_labels(&self, frame: &mut Frame, plot: &Plot) {
        for (content, at) in self.scale(plot) {
            let y = at.y;
            frame.stroke(
                &Path::line(
                    Point::new(plot.left, y),
                    Point::new(plot.left + plot.width, y),
                ),
                Stroke::default()
                    .with_color(style::HIGHLIGHT)
                    .with_width(1.0),
            );
            frame.fill_text(Text {
                content,
                position: at,
                color: style::DISABLED,
                size: iced::Pixels(detail::TEXT_SIZE as f32),
                align_x: iced::alignment::Horizontal::Right.into(),
                align_y: iced::alignment::Vertical::Center,
                ..Default::default()
            });
        }
    }

    /// The clip's other envelopes at half alpha, each scaled to its own range.
    fn paint_ghosts(&self, frame: &mut Frame, plot: &Plot) {
        for (index, lane) in self.model.rows.iter().enumerate() {
            let RowKind::Envelope(other) = &lane.kind else {
                continue;
            };
            if other.target == self.chosen.target || other.points.is_empty() {
                continue;
            }
            let colour = Color {
                a: detail::NON_EDITABLE_ALPHA,
                ..Self::ink(self.live.get(index).copied().flatten())
            };
            let ghost = other
                .lane(
                    *plot,
                    crate::inspector::clip_markers::extent(&self.clip, self.signature),
                    self.grid.ticks(),
                )
                .with_time(self.time(plot));
            let path = other.polyline(
                &other.points,
                &ghost,
                None,
                (
                    0.0,
                    crate::inspector::clip_markers::extent(&self.clip, self.signature).ticks()
                        as f64,
                ),
            );
            frame.stroke(&path, Stroke::default().with_color(colour).with_width(1.0));
        }
    }

    /// The lane's curve, points, selection, hover and the playhead; redrawn with the pointer.
    /// `at` is the pointer in the canvas's own coordinates.
    fn paint_live(&self, state: &State, frame: &mut Frame, size: Size, at: Option<Point>) {
        let plot = self.plot(size);
        let lane = self.lane(size);
        if let Some(selection) = self.context.content.as_ref().filter(|s| s.picks.is_none())
            && selection
                .lanes
                .contains(&lanes::Identity::Envelope(self.chosen.target.clone()))
        {
            ruler::selection(frame, &self.axis(&plot), &selection.range, 0.0, 19.0);
            ruler::selection(
                frame,
                &self.axis(&plot),
                &selection.range,
                RULER_HEIGHT,
                size.height - RULER_HEIGHT,
            );
        }
        let drag = self.drag(state);
        let painted = state.paint.as_ref().and_then(Painting::preview);
        let preview = drag.and_then(|d| d.gesture.preview());
        let points: &[Breakpoint] = painted
            .as_deref()
            .or(preview)
            .unwrap_or(&self.chosen.points);
        let ink = Self::ink(self.live_chosen);
        let whole = (
            0.0,
            crate::inspector::clip_markers::extent(&self.clip, self.signature).ticks() as f64,
        );
        if points.is_empty() {
            self.paint_invitation(frame, &plot);
        } else {
            if self.chosen.axis == Axis::Stepped {
                // The rounded staircase the engine sends, faintly behind the line.
                frame.stroke(
                    &self
                        .chosen
                        .polyline(points, &lane, Some(Axis::Stepped), whole),
                    Stroke::default()
                        .with_color(Color {
                            a: detail::DIM_ALPHA,
                            ..ink
                        })
                        .with_width(1.0),
                );
            }
            let line_axis = match self.chosen.axis {
                Axis::Stepped => Axis::Continuous,
                axis => axis,
            };
            frame.stroke(
                &self.chosen.polyline(points, &lane, Some(line_axis), whole),
                Stroke::default().with_color(ink).with_width(CURVE_WIDTH),
            );
        }
        // The segment under the pointer and its two points, in AutomationMouseOver; not while a
        // gesture runs.
        if drag.is_none()
            && let Some(Hit::Segment(left)) = at.map(|p| lane.hit(p))
            && let (Some(a), Some(b)) = (points.get(left), points.get(left + 1))
        {
            frame.stroke(
                &self.chosen.polyline(
                    points,
                    &lane,
                    Some(line_axis_of(self.chosen.axis)),
                    (a.time.ticks() as f64, b.time.ticks() as f64),
                ),
                Stroke::default()
                    .with_color(LIVE_DARK.automation_mouse_over)
                    .with_width(CURVE_WIDTH),
            );
        }
        let hovered = drag.is_none().then(|| at.map(|p| lane.hit(p))).flatten();
        for (index, point) in points.iter().enumerate() {
            let centre = Point::new(lane.x(point.time), lane.y(point.value));
            let colour = if hovered == Some(Hit::Point(index))
                || matches!(hovered, Some(Hit::Segment(left)) if index == left || index == left + 1)
            {
                LIVE_DARK.automation_mouse_over
            } else {
                ink
            };
            frame.fill(&Path::circle(centre, POINT_RADIUS), colour);
        }
        // The grabbed point while dragging, else the selected one: ringed in white.
        let ringed = match drag {
            Some(d) => d.gesture.grabbed(),
            None => self.point,
        };
        let selected: Vec<usize> = if let Some(selection) = &self.context.content {
            selection
                .points
                .iter()
                .filter(|(t, _)| *t == self.chosen.target)
                .map(|(_, i)| *i)
                .collect()
        } else {
            ringed.into_iter().collect()
        };
        for point in selected.into_iter().filter_map(|i| points.get(i)) {
            frame.stroke(
                &Path::circle(
                    Point::new(lane.x(point.time), lane.y(point.value)),
                    RING_RADIUS,
                ),
                Stroke::default().with_color(style::TEXT).with_width(1.5),
            );
        }
        ruler::dim(frame, &self.axis(&plot), size.height);
        if let Some(position) = self
            .context
            .content
            .as_ref()
            .and_then(content::Selection::insert_mark)
        {
            ruler::insert(
                frame,
                &self.axis(&plot),
                position,
                size.height,
                self.context.focused,
            );
        }
        if let Some(position) = self.playhead {
            ruler::playhead(frame, &self.axis(&plot), position, size.height);
        }
    }

    /// An argument with no points shows its current value dotted, and says how to begin.
    fn paint_invitation(&self, frame: &mut Frame, plot: &Plot) {
        let Some(live) = self.live_chosen else {
            return;
        };
        let y = self
            .chosen
            .lane(
                *plot,
                crate::inspector::clip_markers::extent(&self.clip, self.signature),
                self.grid.ticks(),
            )
            .y(live.value);
        frame.stroke(
            &Path::line(
                Point::new(plot.left, y),
                Point::new(plot.left + plot.width, y),
            ),
            Stroke {
                line_dash: canvas::LineDash {
                    segments: &[2.0, 4.0],
                    offset: 0,
                },
                ..Stroke::default()
                    .with_color(Color {
                        a: detail::DIM_ALPHA + 0.25,
                        ..Self::ink(Some(live))
                    })
                    .with_width(1.5)
            },
        );
        frame.fill_text(Text {
            content: if self.laned {
                "Click the line to add a point"
            } else {
                "Click the line to start an envelope"
            }
            .into(),
            position: Point::new(
                plot.left + plot.width / 2.0,
                // Above the line, or below it when the line is near the ceiling.
                if y - plot.top > 24.0 {
                    y - 22.0
                } else {
                    y + 10.0
                },
            ),
            color: style::TEXT,
            size: iced::Pixels(detail::HEADER_SIZE as f32),
            align_x: iced::alignment::Horizontal::Center.into(),
            ..Default::default()
        });
    }
}

fn line_axis_of(axis: Axis) -> Axis {
    match axis {
        Axis::Stepped => Axis::Continuous,
        axis => axis,
    }
}

impl canvas::Program<Message> for OneLane {
    type State = State;

    fn update(
        &self,
        state: &mut State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let stamp = (self.key, self.context.revision, self.chosen.target.clone());
        if state.stamp.as_ref() != Some(&stamp) {
            *state = State {
                stamp: Some(stamp),
                ..Default::default()
            };
        }
        let size = bounds.size();
        if let Some(action) = ruler::interact(
            &mut state.marker,
            &self.axis(&self.plot(size)),
            (
                self.key,
                self.context.revision,
                self.chosen.region.map(|_| self.target()),
            ),
            event,
            bounds,
            cursor,
            self.context.command_down,
        ) {
            return Some(action);
        }
        let axis = self.axis(&self.plot(size));
        if let Some(action) = ruler::zoom(
            &mut state.zoom,
            &axis,
            (self.key, self.context.window),
            event,
            bounds,
            cursor,
        ) {
            return Some(action);
        }
        let busy = state.drag.is_some()
            || state.paint.is_some()
            || state.range.is_some()
            || state.region.is_some()
            || state.marker.is_some()
            || state.zoom.dragging();
        if let Some(action) = ruler::wheel(
            &axis,
            (self.key, self.context.window),
            ruler::WheelKeys {
                command: self.context.command_down,
                shift: self.context.shift_down,
            },
            busy,
            event,
            bounds,
            cursor,
        ) {
            return Some(action);
        }
        let publish = |action| canvas::Action::publish(Message::Timeline(action)).and_capture();
        if matches!(event, canvas::Event::Mouse(mouse::Event::ButtonPressed(_))) {
            state.range = None;
            state.region = None;
        }
        if state.range.is_some() {
            match event {
                canvas::Event::Mouse(mouse::Event::CursorMoved { .. })
                | canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    let drag = state.range.as_mut()?;
                    let at = cursor
                        .position()
                        .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y))
                        .unwrap_or(drag.pointer);
                    let time = self.lane(size).time_at(at.x, self.keys());
                    let selection = drag.update(at, time, vec![]);
                    if matches!(event, canvas::Event::Mouse(mouse::Event::ButtonReleased(_))) {
                        state.range = None;
                    }
                    return Some(publish(Action::Content(selection)));
                }
                _ => {}
            }
        }
        if let Some(region) = &mut state.region {
            region.follow(event, bounds, cursor);
            if matches!(
                event,
                canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            ) {
                let region = state.region.take()?;
                if region.moved {
                    state.drag = None;
                    return Some(publish(Action::RegionMove(region)));
                }
            } else if matches!(
                event,
                canvas::Event::Mouse(mouse::Event::CursorMoved { .. })
            ) && region.moved
            {
                return Some(canvas::Action::request_redraw().and_capture());
            }
        }
        match event {
            // A right-click settles the selection as a left click would, then opens the menu
            // of what was clicked. The value scale and the ruler take none.
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
                if cursor.is_over(bounds) =>
            {
                let at = cursor.position_in(bounds)?;
                let plot = self.plot(size);
                state.drag = None;
                state.paint = None;
                state.last_alt = None;
                if at.x < plot.left || at.x > plot.left + plot.width || at.y < RULER_HEIGHT {
                    return None;
                }
                let lane = self.lane(size);
                let under = match lane.hit(at) {
                    Hit::Point(index) => menu::Under::Point(index),
                    _ => menu::Under::Space(lane.time_at(at.x, self.keys())),
                };
                Some(publish(Action::ContentMenu(menu::Click {
                    key: self.key,
                    revision: self.context.revision,
                    lane: lanes::Identity::Envelope(self.chosen.target.clone()),
                    under,
                })))
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                let at = cursor.position_in(bounds)?;
                let plot = self.plot(size);
                // The value scale takes no clicks, as Live's.
                if at.x < plot.left {
                    return None;
                }
                if (crate::inspector::event_timeline_math::INSERT_STRIP_TOP..RULER_HEIGHT)
                    .contains(&at.y)
                    && at.x <= plot.left + plot.width
                {
                    let time = self.lane(size).time_at(at.x, self.keys());
                    return Some(publish(Action::Content(content::Selection::time(
                        self.key,
                        time..time,
                        vec![lanes::Identity::Envelope(self.chosen.target.clone())],
                    ))));
                }
                if at.y < RULER_HEIGHT || at.x > plot.left + plot.width {
                    return None;
                }
                if self.context.draw {
                    state.last_alt = None;
                    state.paint = Some(Painting::envelope(
                        &self.lane(size),
                        self.chosen.threshold,
                        self.keys(),
                        at,
                    ));
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                self.press(state, at, size)
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position })
                if state.paint.is_some() =>
            {
                let at = Point::new(position.x - bounds.x, position.y - bounds.y);
                let (lane, keys) = (self.lane(size), self.keys());
                let paint = state.paint.as_mut()?;
                paint.push(Some(&lane), at, lane.ticks_at(at.x), keys);
                Some(canvas::Action::request_redraw().and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.paint.is_some() =>
            {
                let (lane, keys) = (self.lane(size), self.keys());
                let mut paint = state.paint.take()?;
                if let Some(p) = cursor.position() {
                    let at = Point::new(p.x - bounds.x, p.y - bounds.y);
                    paint.push(Some(&lane), at, lane.ticks_at(at.x), keys);
                }
                Some(match paint.finish() {
                    Some(Outcome::Points(replacement)) => publish(Action::Draw(Draw::Points {
                        lane: self.target(),
                        replacement,
                    })),
                    _ => canvas::Action::request_redraw().and_capture(),
                })
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position })
                if state.drag.is_some() =>
            {
                let local = Point::new(position.x - bounds.x, position.y - bounds.y);
                let (keys, lane) = (self.keys(), self.lane(size));
                let drag = state.drag.as_mut()?;
                drag.gesture.drag(&lane, local, keys);
                let text = drag.gesture.readout(&lane).map(|r| self.readout(r));
                if drag.gesture.previewing() {
                    state.last_alt = None;
                }
                Some(self.hover_action(state, text))
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) if self.context.draw => {
                // The pencil follows the pointer.
                Some(canvas::Action::request_redraw())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let lane = self.lane(size);
                let at = cursor.position_in(bounds);
                let text = at
                    .and_then(|p| lane.hover(p))
                    .map(|value| self.chosen.value_text(value));
                let hit = at.map(|p| lane.hit(p));
                if text != state.hover {
                    state.hover = text.clone();
                    state.hovered = hit;
                    return Some(canvas::Action::publish(Message::Timeline(Action::Hover(
                        text,
                    ))));
                }
                // The hovered segment changes ink without a message.
                (hit != state.hovered).then(|| {
                    state.hovered = hit;
                    canvas::Action::request_redraw()
                })
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.drag.is_some() =>
            {
                let drag = state.drag.take()?;
                let at = cursor.position().map_or(drag.gesture.pointer(), |p| {
                    Point::new(p.x - bounds.x, p.y - bounds.y)
                });
                let marker = drag.gesture.marker_click(at);
                let commit = drag.gesture.release(&self.lane(size), at, self.keys());
                Some(match commit {
                    Some(commit) => {
                        state.hover = None;
                        // The lane appears with its first point; until then there is none to set.
                        publish(if self.laned {
                            Action::SetPoints(drag.lane, commit.points, commit.select)
                        } else {
                            Action::StartEnvelope(drag.lane, commit.points, commit.select)
                        })
                    }
                    None if marker => {
                        let time = self.lane(size).time_at(at.x, self.keys());
                        publish(Action::Content(content::Selection::time(
                            self.key,
                            time..time,
                            vec![lanes::Identity::Envelope(self.chosen.target.clone())],
                        )))
                    }
                    // A release that changed nothing still ends the readout and the preview.
                    None => self.hover_action(state, None).and_capture(),
                })
            }
            canvas::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: Key::Named(Named::Escape),
                ..
            }) if state.drag.is_some()
                || state.paint.is_some()
                || state.range.is_some()
                || state.region.is_some() =>
            {
                state.paint = None;
                state.range = None;
                state.region = None;
                state.drag = None;
                Some(publish(Action::ClearContent))
            }
            canvas::Event::Window(iced::window::Event::Unfocused)
            | canvas::Event::Mouse(mouse::Event::CursorLeft) => {
                state.paint = None;
                state.range = None;
                state.region = None;
                state.drag = None;
                state.hovered = None;
                Some(self.hover_action(state, None))
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &State,
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
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
            if let Some(e) = preview.model.rows.iter().find_map(|r| match &r.kind {
                RowKind::Envelope(e) if e.target == self.chosen.target => Some(e.clone()),
                _ => None,
            }) {
                preview.chosen = e;
            }
            return preview.draw(&State::default(), renderer, &Theme::Dark, bounds, cursor);
        }

        let key = self.static_key();
        let mut layer = state.layer.borrow_mut();
        if layer.key.as_ref() != Some(&key) {
            layer.caches.iter().for_each(canvas::Cache::clear);
            layer.key = Some(key);
        }
        let mut base: Vec<Geometry> = layer
            .caches
            .iter()
            .enumerate()
            .map(|(which, cache)| {
                cache.draw(renderer, bounds.size(), |frame| {
                    self.paint_static(which, frame, bounds.size())
                })
            })
            .collect();
        let mut top = Frame::new(renderer, bounds.size());
        // The pointer relative to the canvas; a capture may place it (QTABLE_CAPTURE_HOVER).
        let at = self
            .lane(bounds.size())
            .capture_hover()
            .or_else(|| cursor.position_in(bounds));
        self.time(&self.plot(bounds.size()))
            .within(&mut top, |top| {
                self.paint_live(state, top, bounds.size(), at)
            });
        if self.context.draw
            && let Some(at) = cursor
                .position_in(bounds)
                .filter(|p| p.y >= RULER_HEIGHT && p.x >= detail::VALUE_SCALE_WIDTH)
        {
            crate::control::Glyph::Pencil.paint_cursor(&mut top, at);
        }
        // Capture-only (`capture::brace_drag`): the brace body dragged a beat right.
        let scripted = crate::capture::brace_drag().then(|| {
            let axis = self.axis(&self.plot(bounds.size()));
            let from = axis.x(axis.markers.loop_start) + 20.0;
            let mut drag = ruler::Drag::new(
                crate::control::timeline_brace::Part::Body,
                from,
                axis.markers,
            );
            drag.follow(&axis.time, from + axis.x(BeatTime(960)) - axis.time.left, 1);
            drag
        });
        if let Some(drag) = state.marker.as_ref().or(scripted.as_ref()) {
            let mut axis = self.axis(&self.plot(bounds.size()));
            axis.markers = drag.preview;
            axis.dragging = true;
            let mut cover = Frame::new(renderer, bounds.size());
            cover.fill_rectangle(
                Point::ORIGIN,
                Size::new(bounds.width, RULER_HEIGHT),
                detail::DETAIL_BACKGROUND,
            );
            let mut preview = Frame::new(renderer, bounds.size());
            self.paint_ruler(&mut preview, &axis, Size::new(bounds.width, RULER_HEIGHT));
            if self.chosen.region.is_some() {
                axis.time.within(&mut preview, |preview| {
                    ruler::reference_brace(preview, &axis, self.clip.region())
                });
            }
            base.extend([
                top.into_geometry(),
                cover.into_geometry(),
                preview.into_geometry(),
            ]);
            return base;
        }
        base.push(top.into_geometry());
        base
    }

    fn mouse_interaction(
        &self,
        state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let part = cursor.position_in(bounds).and_then(|p| {
            self.axis(&self.plot(bounds.size()))
                .brace(style::TEXT)
                .part_at(p)
        });
        let in_zone = cursor
            .position_in(bounds)
            .is_some_and(|p| ruler::zoom_zone(&self.axis(&self.plot(bounds.size())), p));
        if let Some(drag) = &state.marker {
            drag.part().interaction()
        } else if let Some(part) = part {
            part.interaction()
        } else if in_zone || state.zoom.dragging() {
            mouse::Interaction::ZoomIn
        } else if self.drag(state).is_none()
            && state.paint.is_none()
            && cursor
                .position_in(bounds)
                .is_some_and(|p| p.x < detail::VALUE_SCALE_WIDTH)
        {
            mouse::Interaction::default()
        } else if self.context.draw
            && cursor
                .position_in(bounds)
                .is_some_and(|p| p.y >= RULER_HEIGHT)
        {
            mouse::Interaction::Hidden
        } else if self.drag(state).is_some_and(|d| d.gesture.previewing()) {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

impl OneLane {
    /// The Info View entry under `at`: a brace part or flag, the envelope's line and points, or
    /// the view's own entry.
    pub(super) fn info_at(&self, at: Point, size: Size) -> Id {
        if let Some(part) = self.axis(&self.plot(size)).brace(style::TEXT).part_at(at) {
            return ruler::info(part);
        }
        if ruler::zoom_zone(&self.axis(&self.plot(size)), at) {
            return Id::RulerZoom;
        }
        match (at.y >= RULER_HEIGHT).then(|| self.lane(size).hit(at)) {
            Some(Hit::Point(_) | Hit::Segment(_) | Hit::Line | Hit::Step) => Id::EnvelopeLine,
            _ => Id::OneLane,
        }
    }

    /// A press in the lane starts a point gesture, or with Alt held twice straightens the
    /// segment there. Nothing publishes until the release decides.
    fn press(&self, state: &mut State, at: Point, size: Size) -> Option<canvas::Action<Message>> {
        let lane = self.lane(size);
        let keys = self.keys();
        let hit = lane.hit(at);
        if keys.shift
            && let Hit::Point(index) = hit
        {
            return Some(
                canvas::Action::publish(Message::Timeline(Action::PickPoint(self.target(), index)))
                    .and_capture(),
            );
        }
        if hit == Hit::Empty {
            let drag = content::Drag::new(
                self.key,
                at,
                lane.time_at(at.x, keys),
                lanes::Identity::Envelope(self.chosen.target.clone()),
            );
            let selection = drag.selection.clone();
            state.range = Some(drag);
            return Some(
                canvas::Action::publish(Message::Timeline(Action::Content(selection)))
                    .and_capture(),
            );
        }
        let region = self.context.content.as_ref().is_some_and(|s| {
            s.contains(
                &lanes::Identity::Envelope(self.chosen.target.clone()),
                lane.time_at(at.x, keys),
            ) || matches!(hit, Hit::Point(i) if s.points.contains(&(self.chosen.target.clone(),i)))
        });
        if region {
            state.region = Some(content::Move {
                key: self.key,
                revision: self.context.revision,
                from: at,
                to: at,
                copy: keys.alt,
                moved: false,
                time_scale: lane.time.ticks_per_px(),
                value_scale: 1.0 / f64::from(lane.plot.height),
            });
        }
        if keys.alt && !region {
            let now = Instant::now();
            let double = state.last_alt.take().is_some_and(|(time, previous)| {
                now.duration_since(time) <= Duration::from_millis(500)
                    && previous.distance(at) <= 5.0
            });
            if double {
                return Some(match lane.straighten(at) {
                    Some(commit) => canvas::Action::publish(Message::Timeline(Action::SetPoints(
                        self.target(),
                        commit.points,
                        None,
                    )))
                    .and_capture(),
                    None => canvas::Action::request_redraw().and_capture(),
                });
            }
            state.last_alt = Some((now, at));
        } else {
            state.last_alt = None;
        }
        state.drag = Gesture::press(&lane, at, keys, self.point).map(|gesture| PointDrag {
            gesture,
            lane: self.target(),
        });
        Some(canvas::Action::request_redraw().and_capture())
    }

    /// Publishes the readout when it changed; otherwise only asks for the redraw a preview needs.
    fn hover_action(&self, state: &mut State, text: Option<String>) -> canvas::Action<Message> {
        if state.hover == text {
            return canvas::Action::request_redraw();
        }
        state.hover = text.clone();
        canvas::Action::publish(Message::Timeline(Action::Hover(text)))
    }
}

#[cfg(test)]
mod tests;
