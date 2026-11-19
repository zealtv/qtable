//! The ruler, grid, playhead and marker gestures, which the All lanes view and the one-lane view
//! draw the same way: x comes from the canvas's `TimeAxis`.
//! The brace and its flags are the control layer's `timeline_brace` element.
use crate::style;
use iced::widget::canvas::{Frame, Path, Stroke, Text};
use iced::{Point, Size};
use qtable_engine::model::{BeatTime, TimeSignature};
use std::ops::RangeInclusive;

use super::super::event_timeline_math::RULER_HEIGHT;
use crate::control::time_axis::{TimeAxis, Window};
use crate::control::timeline_brace::Brace;
pub(crate) use crate::control::timeline_brace::Drag;
use crate::style::detail::{INSERT_MARKER_HALF, INSERT_MARKER_INSET, INSERT_STRIP_TOP};

/// What the time axis shows: the window of a clip's content that the canvas lays across its
/// pixels.
pub(crate) struct Axis {
    pub time: TimeAxis,
    pub signature: TimeSignature,
    /// Grid lines at this many ticks (thinned when they would crowd).
    pub grid: u64,
    pub markers: qtable_engine::model::region::Region,
    pub colour: iced::Color,
    /// A marker drag is previewing `markers`: the brace shows Live's drag fill.
    pub dragging: bool,
}

impl Axis {
    pub(crate) fn x(&self, position: BeatTime) -> f32 {
        self.time.x(position)
    }
    /// The brace and flags of `markers`, the timeline element both canvases share.
    pub(crate) fn brace(&self, flag_ink: iced::Color) -> Brace {
        Brace {
            axis: self.time,
            region: self.markers,
            ink: self.colour,
            flag_ink,
        }
    }
}

fn line(frame: &mut Frame, a: Point, b: Point, colour: iced::Color) {
    frame.stroke(
        &Path::line(a, b),
        Stroke::default().with_color(colour).with_width(1.0),
    )
}

/// The multiples of `step` ticks that fall in the axis's window, as indices.
fn lines(time: &TimeAxis, step: u64) -> RangeInclusive<u64> {
    let (first, last) = time.visible_range();
    (first.ceil() as u64).div_ceil(step)..=(last.floor() as u64) / step
}

/// Grid lines, bar lines and numbers, the loop brace, and the rule beside the header column,
/// down to the frame's height.
///
/// On a zoomed axis the time area is clipped, and iced puts a clipped region under every shape
/// the same frame drew itself, whatever the order of the calls. So this belongs in a frame of
/// its own, after the frames that fill the background.
pub(crate) fn paint(frame: &mut Frame, axis: &Axis, size: Size) {
    paint_with_flags(frame, axis, size, style::TEXT);
}

pub(crate) fn paint_with_flags(frame: &mut Frame, axis: &Axis, size: Size, flag_ink: iced::Color) {
    let bar = axis.signature.bar_length().ticks();
    let time = &axis.time;
    time.within(frame, |frame| {
        // Bound line count even for very long clips; grid choice still sets precise snapping.
        // The count thins by the ticks the window shows, so a zoomed window gets its own lines.
        let step = axis.grid;
        let stride = ((time.visible / step as f64) / (time.span / 8.0).max(1.0) as f64)
            .ceil()
            .max(1.0) as u64;
        let visible_step = step.saturating_mul(stride).max(1);
        for i in lines(time, visible_step) {
            let x = axis.x(BeatTime(i * visible_step));
            line(
                frame,
                Point::new(x, RULER_HEIGHT),
                Point::new(x, size.height),
                style::HIGHLIGHT,
            );
        }
        let bar_stride = ((time.visible / bar as f64) / (time.span / 48.0).max(1.0) as f64)
            .ceil()
            .max(1.0) as u64;
        let ruler_step = bar.saturating_mul(bar_stride).max(1);
        for i in lines(time, ruler_step) {
            let x = axis.x(BeatTime(i * ruler_step));
            line(
                frame,
                Point::new(x, 18.0),
                Point::new(x, size.height),
                style::CONTROL,
            );
        }
        // Labels: bar numbers, and once the window is zoomed far enough, the beats and
        // sixteenths between them, whichever is the finest that keeps 48 px apart.
        for (ticks, content) in labels(axis) {
            let x = axis.x(BeatTime(ticks));
            if ticks % bar != 0 {
                line(
                    frame,
                    Point::new(x, 18.0),
                    Point::new(x, 28.0),
                    style::CONTROL,
                );
            }
            if x + 20.0 <= size.width {
                frame.fill_text(Text {
                    content,
                    position: Point::new(x + 3.0, 1.0),
                    color: style::TEXT,
                    size: iced::Pixels(crate::style::detail::TEXT_SIZE as f32),
                    ..Default::default()
                });
            }
        }
        frame.fill_rectangle(
            Point::new(time.left, INSERT_STRIP_TOP),
            Size::new(time.span, RULER_HEIGHT - INSERT_STRIP_TOP),
            style::CONTROL,
        );
        axis.brace(flag_ink).draw(frame, axis.dragging);
    });
    line(
        frame,
        Point::new(time.left, 0.0),
        Point::new(time.left, size.height),
        style::CONTROL,
    );
}

/// Where the ruler is labelled across the window, and with what.
fn labels(axis: &Axis) -> Vec<(u64, String)> {
    let unit = label_unit(axis);
    lines(&axis.time, unit)
        .map(|i| (i * unit, label(i * unit, axis.signature)))
        .collect()
}

/// The text of each ruler label, for tests.
#[cfg(test)]
pub(super) fn label_texts(axis: &Axis) -> Vec<String> {
    labels(axis).into_iter().map(|(_, text)| text).collect()
}

/// The least spacing between ruler labels, in px.
const LABEL_GAP: f32 = 48.0;

/// The ticks between ruler labels. A fitted axis labels bars only, thinned when they crowd,
/// as it always has. A zoomed one steps down to beats and then sixteenths as the window
/// narrows: the finest of the three that still leaves `LABEL_GAP` px between labels.
fn label_unit(axis: &Axis) -> u64 {
    let time = &axis.time;
    let bar = axis.signature.bar_length().ticks();
    let thinned = || {
        let stride = ((time.visible / bar as f64) / (time.span / LABEL_GAP).max(1.0) as f64)
            .ceil()
            .max(1.0) as u64;
        bar.saturating_mul(stride).max(1)
    };
    if !time.is_windowed() {
        return thinned();
    }
    let (beat, sixteenth) = (beat_length(axis.signature), SIXTEENTH);
    [sixteenth, beat, bar]
        .into_iter()
        .filter(|unit| *unit <= bar)
        .find(|unit| *unit as f64 / time.visible * f64::from(time.span) >= f64::from(LABEL_GAP))
        .unwrap_or_else(thinned)
}

/// A sixteenth note, the finest label.
const SIXTEENTH: u64 = qtable_engine::model::TICKS_PER_BEAT / 4;

/// The ticks in one beat of `signature`: a quarter in x/4, an eighth in x/8.
fn beat_length(signature: TimeSignature) -> u64 {
    4 * qtable_engine::model::TICKS_PER_BEAT / u64::from(signature.denominator())
}

/// A ruler label for `ticks`: the bar alone ("3") on a bar line, "3.2" on a beat, "3.2.3" on
/// a sixteenth.
fn label(ticks: u64, signature: TimeSignature) -> String {
    let (bar, beat) = (signature.bar_length().ticks(), beat_length(signature));
    let (bars, rest) = (ticks / bar + 1, ticks % bar);
    if rest == 0 {
        bars.to_string()
    } else if rest % beat == 0 {
        format!("{bars}.{}", rest / beat + 1)
    } else {
        let sixteenths = (rest % beat) / SIXTEENTH + 1;
        format!("{bars}.{}.{sixteenths}", rest / beat + 1)
    }
}

/// Live's zero-width time selection: triangle in its own strip, with a line while
/// the editor owns focus. ChosenAlternative is the retained theme's cyan role;
/// that .ask has no separately named insert-marker colour.
pub(crate) fn insert(
    frame: &mut Frame,
    axis: &Axis,
    position: BeatTime,
    height: f32,
    focused: bool,
) {
    let x = axis.x(position);
    let ink = crate::control::LIVE_DARK.chosen_alternative;
    let triangle = Path::new(|p| {
        p.move_to(Point::new(
            x - INSERT_MARKER_HALF,
            INSERT_STRIP_TOP + INSERT_MARKER_INSET,
        ));
        p.line_to(Point::new(
            x + INSERT_MARKER_HALF,
            INSERT_STRIP_TOP + INSERT_MARKER_INSET,
        ));
        p.line_to(Point::new(x, RULER_HEIGHT));
        p.close();
    });
    frame.fill(&triangle, ink);
    if focused {
        line(
            frame,
            Point::new(x, RULER_HEIGHT),
            Point::new(x, height),
            ink,
        );
    }
}

/// The playhead: a line down from its marker on the ruler.
pub(crate) fn playhead(frame: &mut Frame, axis: &Axis, position: BeatTime, height: f32) {
    playhead_line(frame, axis, position, RULER_HEIGHT, height);
    playhead_marker(frame, axis, position);
}

/// The playhead's line between `top` and `bottom` only: an unlinked row's own playhead, or
/// the clip's playhead around unlinked rows.
pub(crate) fn playhead_line(
    frame: &mut Frame,
    axis: &Axis,
    position: BeatTime,
    top: f32,
    bottom: f32,
) {
    let x = axis.x(position);
    line(
        frame,
        Point::new(x, top),
        Point::new(x, bottom),
        style::TEXT,
    );
}

/// The playhead's marker on the ruler.
pub(crate) fn playhead_marker(frame: &mut Frame, axis: &Axis, position: BeatTime) {
    let x = axis.x(position);
    let marker = Path::new(|p| {
        p.move_to(Point::new(x - 4.0, RULER_HEIGHT - 5.0));
        p.line_to(Point::new(x + 4.0, RULER_HEIGHT - 5.0));
        p.line_to(Point::new(x, RULER_HEIGHT));
        p.close();
    });
    frame.fill(&marker, style::TEXT);
}

/// The x extent a time selection's shading covers. A zoomed window shows part of the range: the
/// shading stays in the time area, off the header column and the gutter. A fitted axis already
/// lies inside it.
pub(crate) fn selection_span(axis: &Axis, range: &std::ops::Range<BeatTime>) -> (f32, f32) {
    let (left, right) = (
        axis.x(range.start.min(axis.time.extent)),
        axis.x(range.end.min(axis.time.extent)),
    );
    if axis.time.is_windowed() {
        let (lo, hi) = (axis.time.left, axis.time.left + axis.time.span);
        (left.clamp(lo, hi), right.clamp(lo, hi))
    } else {
        (left, right)
    }
}

/// A content selection uses Live's selection ink in the ruler and each selected lane.
/// Both canvases call this with their own axis and vertical extent.
pub(crate) fn selection(
    frame: &mut Frame,
    axis: &Axis,
    range: &std::ops::Range<BeatTime>,
    top: f32,
    height: f32,
) {
    if range.is_empty() {
        return;
    }
    let (left, right) = selection_span(axis, range);
    frame.fill_rectangle(
        Point::new(left, top),
        Size::new((right - left).max(0.0), height),
        iced::Color {
            a: crate::style::detail::DIM_ALPHA,
            ..crate::control::LIVE_DARK.selection_background
        },
    );
}

/// Content outside the played interval stays editable under a subdued layer.
/// Called after the content but before the independent insert marker and playhead.
pub(crate) fn dim(frame: &mut Frame, axis: &Axis, height: f32) {
    let end = if axis.markers.looping {
        axis.markers.loop_end
    } else {
        axis.markers.end
    };
    // With Start inside the loop, the earlier part still plays on later passes.
    let start = if axis.markers.looping {
        axis.markers.start.min(axis.markers.loop_start)
    } else {
        axis.markers.start
    };
    let ink = iced::Color {
        a: crate::style::detail::DIM_ALPHA,
        ..crate::control::LIVE_DARK.surface_background
    };
    for (a, b) in [
        (axis.time.left, axis.x(start)),
        (axis.x(end), axis.time.left + axis.time.span),
    ] {
        frame.fill_rectangle(
            Point::new(a, RULER_HEIGHT),
            Size::new((b - a).max(0.0), (height - RULER_HEIGHT).max(0.0)),
            ink,
        );
    }
}

/// Shared marker gesture, run before Draw Mode and content selection in both views.
/// The host resets this state on identity/revision changes. Only release publishes an edit.
pub(crate) fn interact(
    drag: &mut Option<Drag>,
    axis: &Axis,
    (key, revision, lane): (
        super::ClipKey,
        u64,
        Option<super::super::event_editor::PointTarget>,
    ),
    event: &iced::widget::canvas::Event,
    bounds: iced::Rectangle,
    cursor: iced::mouse::Cursor,
    off_grid: bool,
) -> Option<iced::widget::canvas::Action<super::Message>> {
    use iced::{
        keyboard::{Key, key::Named},
        mouse,
        widget::canvas::{Action, Event},
    };
    let redraw = || Action::request_redraw().and_capture();
    match event {
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            let at = cursor.position_in(bounds)?;
            let part = axis.brace(style::TEXT).part_at(at)?;
            *drag = Some(Drag::new(part, at.x, axis.markers));
            Some(Action::publish(super::Message::Timeline(super::Action::Focus(key))).and_capture())
        }
        Event::Mouse(mouse::Event::CursorMoved { position }) if drag.is_some() => {
            drag.as_mut()?
                .follow(&axis.time, position.x - bounds.x, step(axis, off_grid));
            Some(redraw())
        }
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if drag.is_some() => {
            let mut d = drag.take()?;
            if let Some(p) = cursor.position() {
                d.follow(&axis.time, p.x - bounds.x, step(axis, off_grid));
            }
            Some(if let Some(region) = d.changed() {
                Action::publish(super::Message::Timeline(match lane {
                    Some(target) => super::Action::LaneRegion(target, Some(region)),
                    None => super::Action::Markers(key, revision, region),
                }))
                .and_capture()
            } else {
                redraw()
            })
        }
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(Named::Escape),
            ..
        })
        | Event::Window(iced::window::Event::Unfocused)
        | Event::Mouse(mouse::Event::CursorLeft)
            if drag.is_some() =>
        {
            *drag = None;
            Some(redraw())
        }
        _ => None,
    }
}

/// A ruler zoom drag in progress: Live's magnifying glass. The window it began from is kept,
/// so every move works out its window from the press, not from the one before.
pub(crate) struct ZoomDrag {
    grab: crate::control::time_axis::Grab,
    from: Option<Window>,
    origin: Point,
    shown: Option<Window>,
}

/// What the ruler's zoom zone remembers between events: the drag, and the last press for a
/// double-click.
#[derive(Default)]
pub(crate) struct Zoom {
    drag: Option<ZoomDrag>,
    last_press: Option<(std::time::Instant, Point)>,
}

impl Zoom {
    pub(crate) fn dragging(&self) -> bool {
        self.drag.is_some()
    }
}

/// Whether `at` (canvas coordinates) is in the zoom zone: the ruler above the brace row, across
/// the time area, except a Start or End flag, which keeps its own drag.
pub(crate) fn zoom_zone(axis: &Axis, at: Point) -> bool {
    let time = &axis.time;
    at.x >= time.left
        && at.x <= time.left + time.span
        && (0.0..crate::style::detail::CLIP_BRACE_TOP).contains(&at.y)
        && axis.brace(style::TEXT).part_at(at).is_none()
}

/// The ruler's zoom gesture, run after the marker gesture in both canvases. A press in the zone
/// starts a drag that zooms about the pressed time (down zooms in) and scrolls sideways;
/// a second press within 500 ms and 5 px zooms to the time selection, or out to the whole clip.
/// Nothing here reaches the engine: it publishes the clip's window, nothing else.
pub(crate) fn zoom(
    state: &mut Zoom,
    axis: &Axis,
    (key, window): (super::ClipKey, Option<Window>),
    event: &iced::widget::canvas::Event,
    bounds: iced::Rectangle,
    cursor: iced::mouse::Cursor,
) -> Option<iced::widget::canvas::Action<super::Message>> {
    use iced::{
        keyboard::{Key, key::Named},
        mouse,
        widget::canvas::{Action, Event},
    };
    let time = &axis.time;
    let moved = |drag: &mut ZoomDrag, at: Point| {
        let next = drag.grab.at(
            time.extent,
            time.span,
            at.x - drag.origin.x,
            at.y - drag.origin.y,
        );
        let changed = next != drag.shown;
        drag.shown = next;
        changed.then(|| {
            Action::publish(super::Message::Timeline(super::Action::Zoom(key, next))).and_capture()
        })
    };
    match event {
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            let at = cursor.position_in(bounds)?;
            if !zoom_zone(axis, at) {
                return None;
            }
            let now = crate::gesture_clock::now();
            let double = state.last_press.take().is_some_and(|(then, previous)| {
                now.duration_since(then) <= std::time::Duration::from_millis(500)
                    && previous.distance(at) <= 5.0
            });
            if double {
                state.drag = None;
                return Some(
                    Action::publish(super::Message::Timeline(super::Action::ZoomTo(key)))
                        .and_capture(),
                );
            }
            state.last_press = Some((now, at));
            let fraction = f64::from(at.x - time.left) / f64::from(time.span.max(1.0));
            state.drag = Some(ZoomDrag {
                grab: crate::control::time_axis::Grab::new(window, time.extent, fraction),
                from: window,
                origin: at,
                shown: window,
            });
            Some(Action::request_redraw().and_capture())
        }
        Event::Mouse(mouse::Event::CursorMoved { position }) if state.drag.is_some() => {
            let at = Point::new(position.x - bounds.x, position.y - bounds.y);
            let drag = state.drag.as_mut()?;
            // A press that wanders is a drag, not the first half of a double-click.
            if at.distance(drag.origin) > 5.0 {
                state.last_press = None;
            }
            Some(moved(drag, at).unwrap_or_else(|| Action::request_redraw().and_capture()))
        }
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.drag.is_some() => {
            let mut drag = state.drag.take()?;
            let last = cursor
                .position()
                .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y));
            Some(
                last.and_then(|at| moved(&mut drag, at))
                    .unwrap_or_else(|| Action::request_redraw().and_capture()),
            )
        }
        // Escape puts the window back as the press found it.
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: Key::Named(Named::Escape),
            ..
        }) if state.drag.is_some() => {
            let drag = state.drag.take()?;
            state.last_press = None;
            Some(
                Action::publish(super::Message::Timeline(super::Action::Zoom(
                    key, drag.from,
                )))
                .and_capture(),
            )
        }
        // A right-press ends the drag where it is: unlike Escape, the window stays as the last
        // move left it. It is captured, so it opens no menu; a later right-press does.
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) if state.drag.is_some() => {
            state.drag = None;
            state.last_press = None;
            Some(Action::request_redraw().and_capture())
        }
        Event::Window(iced::window::Event::Unfocused) | Event::Mouse(mouse::Event::CursorLeft)
            if state.drag.is_some() =>
        {
            state.drag = None;
            state.last_press = None;
            Some(Action::request_redraw().and_capture())
        }
        _ => None,
    }
}

/// Which wheel gestures reach the window: held ⌘ (Ctrl off the Mac) zooms about the time under
/// the pointer, and a sideways scroll, or Shift with a vertical one, moves the window. A plain
/// vertical scroll is left alone for the Overview's lane list.
#[derive(Clone, Copy)]
pub(crate) struct WheelKeys {
    pub command: bool,
    pub shift: bool,
}

/// A wheel event over the time area. It is captured only when it changes the window, and is
/// ignored while any other gesture runs (`busy`).
pub(crate) fn wheel(
    axis: &Axis,
    (key, window): (super::ClipKey, Option<Window>),
    keys: WheelKeys,
    busy: bool,
    event: &iced::widget::canvas::Event,
    bounds: iced::Rectangle,
    cursor: iced::mouse::Cursor,
) -> Option<iced::widget::canvas::Action<super::Message>> {
    use crate::control::time_axis::{WHEEL_LINE_PX, WHEEL_OCTAVE_PX};
    use iced::{mouse, widget::canvas::Event};
    let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event else {
        return None;
    };
    let time = &axis.time;
    let at = cursor.position_in(bounds)?;
    if busy || at.x < time.left || at.x > time.left + time.span {
        return None;
    }
    let (dx, dy) = match *delta {
        mouse::ScrollDelta::Lines { x, y } => {
            (f64::from(x) * WHEEL_LINE_PX, f64::from(y) * WHEEL_LINE_PX)
        }
        mouse::ScrollDelta::Pixels { x, y } => (f64::from(x), f64::from(y)),
    };
    let next = if keys.command {
        let travel = if dy != 0.0 { dy } else { dx };
        let fraction = f64::from(at.x - time.left) / f64::from(time.span.max(1.0));
        Window::zoom_about(
            window,
            time.extent,
            fraction,
            (travel / WHEEL_OCTAVE_PX).exp2(),
        )
    } else if keys.shift || dx.abs() > dy.abs() {
        let travel = if dx.abs() >= dy.abs() { dx } else { dy };
        // Positive travel moves the content right, the window left.
        Window::scroll(window, time.extent, -travel * time.ticks_per_px())
    } else {
        return None;
    };
    (next != window.and_then(|w| w.normalised(time.extent))).then(|| {
        iced::widget::canvas::Action::publish(super::Message::Timeline(super::Action::Zoom(
            key, next,
        )))
        .and_capture()
    })
}

/// The Info View entry for a brace part.
pub(crate) fn info(part: crate::control::timeline_brace::Part) -> crate::info::Id {
    use crate::control::timeline_brace::Part;
    use crate::info::Id;
    match part {
        Part::Start => Id::StartMarker,
        Part::End => Id::EndMarker,
        Part::LoopStart => Id::LoopStart,
        Part::LoopEnd => Id::LoopEnd,
        Part::Body => Id::LoopBrace,
    }
}

fn step(axis: &Axis, off_grid: bool) -> u64 {
    if off_grid { 1 } else { axis.grid }
}

/// The clip's original brace, above the active envelope brace; it takes no gestures.
pub(crate) fn reference_brace(
    frame: &mut Frame,
    axis: &Axis,
    region: qtable_engine::model::region::Region,
) {
    crate::control::timeline_brace::reference(frame, &axis.time, region);
}
