//! Point gestures on one envelope lane, as a pure function from the lane's geometry and
//! pointer events to a preview list and, on release, a commit. Nothing here draws, publishes
//! or edits: the overview timeline drives it today and the one-lane view will drive it
//! unchanged, each giving its own `Plot`.
//!
//! Click or drag is decided on release (as checked in Live 12.1, 2026-10-17): a press that never moves past `DRAG_THRESHOLD` is a click, and a press
//! that does is a drag even if it comes back, so a drag never deletes and a click never moves.
//! Unlike Live, a click on a point selects it for the point fields, and only a click on the
//! point already selected deletes it (choice A: a click that only selects first).
//! `Gesture::preview` is always exactly the list `Gesture::release` would commit.
use crate::control::time_axis::TimeAxis;
use iced::Point;
use qtable_engine::model::{ArgumentRef, AutomationEnvelope, Axis, BeatTime, Breakpoint};

/// Pixels a press must travel to count as a drag; the hit timeline's threshold.
pub(crate) const DRAG_THRESHOLD: f32 = 3.0;
/// How near a press must land to a point's centre, and to a drawn line, to grab it.
const POINT_REACH: f32 = 6.0;
const LINE_REACH: f32 = 5.0;
/// How near a press must land to a segment to move it: Live 12's band is about 15-20 pt either
/// side (recording). A short lane takes a
/// quarter of its height, so All lanes rows keep room for background clicks.
const SEGMENT_REACH: f32 = 16.0;
/// Pixels of vertical travel for a bend of 1.0 (the full curvature range is ±1).
const BEND_PIXELS: f32 = 40.0;
/// Shift's share of the usual value change per pixel, on Continuous arguments.
const FINE: f64 = 0.1;

/// The modifier keys a gesture reads: Alt bends, Shift refines, ⌘ leaves the grid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Keys {
    pub alt: bool,
    pub shift: bool,
    pub command: bool,
}

/// Where the lane's values and time sit on the canvas: x runs from time 0 to the clip length,
/// y from the maximum (top) to the minimum.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Plot {
    pub left: f32,
    pub width: f32,
    pub top: f32,
    pub height: f32,
}

/// One envelope lane as the gestures see it.
#[derive(Clone, Debug)]
pub(crate) struct Lane {
    pub plot: Plot,
    /// Where time sits across the plot; the extent is the clip's closed timeline.
    pub time: TimeAxis,
    pub axis: Axis,
    pub min: f64,
    pub max: f64,
    /// Snap step in ticks; 1 is Off.
    pub grid: u64,
    envelope: AutomationEnvelope,
    default_value: Option<f64>,
}

/// What lies under a pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    Point(usize),
    /// The vertical step of a jump pair; it takes no gesture.
    Step,
    /// The line between `points[i]` and `points[i + 1]`.
    Segment(usize),
    /// The line before the first point or after the last, which has no segment to move.
    Line,
    Empty,
}

impl Lane {
    pub(crate) fn new(
        plot: Plot,
        (length, axis, min, max, grid): (BeatTime, Axis, f64, f64, u64),
        target: &ArgumentRef,
        points: &[Breakpoint],
    ) -> Self {
        Self {
            plot,
            default_value: None,
            time: TimeAxis::fitted(plot.left, plot.width, length),
            axis,
            min,
            max,
            grid: grid.max(1),
            envelope: AutomationEnvelope {
                region: None,
                target: target.clone(),
                points: points.to_vec(),
            },
        }
    }

    /// This lane laid out on `time` (a zoomed window), in place of the fitted axis it starts with.
    pub(crate) fn with_time(mut self, time: TimeAxis) -> Self {
        self.time = time;
        self
    }

    pub(crate) fn with_default(mut self, value: Option<f64>) -> Self {
        self.default_value = value;
        self
    }

    pub(crate) fn points(&self) -> &[Breakpoint] {
        &self.envelope.points
    }

    /// The clip's extent: what a point's time snaps and clamps to.
    pub(crate) fn length(&self) -> BeatTime {
        self.time.extent
    }

    pub(crate) fn x(&self, time: BeatTime) -> f32 {
        self.time.x(time)
    }

    pub(crate) fn y(&self, value: f64) -> f32 {
        let span = self.max - self.min;
        let fraction = if span.abs() < f64::EPSILON {
            0.5
        } else {
            ((value - self.min) / span).clamp(0.0, 1.0)
        };
        self.plot.top + self.plot.height * (1.0 - fraction as f32)
    }

    pub(crate) fn ticks_at(&self, x: f32) -> f64 {
        self.time.ticks_at(x)
    }

    /// A time on the grid, or on the tick with ⌘, inside the clip's closed timeline.
    fn snap(&self, ticks: f64, keys: Keys) -> BeatTime {
        let step = if keys.command { 1 } else { self.grid };
        let ticks = if ticks.is_finite() {
            ticks.max(0.0)
        } else {
            0.0
        };
        let snapped = ((ticks / step as f64).round() as u64).saturating_mul(step);
        BeatTime(snapped.min(self.length().ticks()))
    }

    /// The time under `x`, snapped.
    pub(crate) fn time_at(&self, x: f32, keys: Keys) -> BeatTime {
        self.snap(self.ticks_at(x), keys)
    }

    /// A value held to the argument's range, and to whole steps off the Continuous axis.
    pub(crate) fn quantise(&self, value: f64) -> f64 {
        let value = if value.is_finite() { value } else { self.min };
        let value = value.clamp(self.min, self.max);
        if self.axis == Axis::Continuous {
            value
        } else {
            (self.min + (value - self.min).round()).min(self.max)
        }
    }

    /// The value under pixel row `y`, held to the argument's range and steps.
    pub(crate) fn value_at_y(&self, y: f32) -> f64 {
        let fraction = 1.0 - f64::from(y - self.plot.top) / f64::from(self.plot.height.max(1.0));
        self.quantise(self.min + fraction * (self.max - self.min))
    }

    /// The value change for `dy` pixels downward.
    pub(crate) fn delta(&self, dy: f32, keys: Keys) -> f64 {
        let span = self.max - self.min;
        let fine = if keys.shift && self.axis == Axis::Continuous {
            FINE
        } else {
            1.0
        };
        -f64::from(dy) / f64::from(self.plot.height.max(1.0)) * span * fine
    }

    /// The curve as drawn: Stepped arguments draw unrounded in the overview.
    fn line_axis(&self) -> Axis {
        match self.axis {
            Axis::Stepped => Axis::Continuous,
            axis => axis,
        }
    }

    pub(crate) fn line_at(&self, ticks: f64) -> Option<f64> {
        self.envelope
            .value_at(ticks, self.line_axis())
            .or(self.default_value)
    }

    fn centre(&self, point: &Breakpoint) -> Point {
        Point::new(self.x(point.time), self.y(point.value))
    }

    pub(crate) fn hit(&self, at: Point) -> Hit {
        let points = self.points();
        if let Some((index, _)) = points
            .iter()
            .enumerate()
            .map(|(i, p)| (i, self.centre(p).distance(at)))
            .filter(|(_, d)| *d <= POINT_REACH)
            // A zoomed window clips its marks to the plot: a point whose centre is outside
            // it is not drawn there and cannot be grabbed by its sliver.
            .filter(|(i, _)| self.time.shows(points[*i].time, 0.0))
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            return Hit::Point(index);
        }
        for pair in points.windows(2) {
            if pair[0].time == pair[1].time {
                let x = self.x(pair[0].time);
                let (a, b) = (self.y(pair[0].value), self.y(pair[1].value));
                if (at.x - x).abs() <= LINE_REACH
                    && at.y >= a.min(b) - LINE_REACH
                    && at.y <= a.max(b) + LINE_REACH
                {
                    return Hit::Step;
                }
            }
        }
        // The nearest point of the drawn line within reach, scanned a pixel at a time so a
        // steep segment still reads as near.
        let reach = self.segment_reach();
        let mut best: Option<(f32, f64)> = None;
        for offset in -(reach as i32)..=(reach as i32) {
            let x = at.x + offset as f32;
            if x < self.plot.left || x > self.plot.left + self.plot.width {
                continue;
            }
            let ticks = self.ticks_at(x);
            let Some(value) = self.line_at(ticks) else {
                return Hit::Empty;
            };
            let distance = (offset as f32).hypot(self.y(value) - at.y);
            if best.is_none_or(|(d, _)| distance < d) {
                best = Some((distance, ticks));
            }
        }
        let Some((_, ticks)) = best.filter(|(d, _)| *d <= reach) else {
            return Hit::Empty;
        };
        let right = points.partition_point(|p| p.time.ticks() as f64 <= ticks);
        if right == 0 || right == points.len() {
            if self.on_line(at) {
                Hit::Line
            } else {
                Hit::Empty
            }
        } else {
            Hit::Segment(right - 1)
        }
    }

    /// Capture-only (`QTABLE_CAPTURE_HOVER=segment`): where a pointer hovering the second
    /// segment (else the first) from three quarters of the band below would be (12 pt on a
    /// tall lane), in the lane's own coordinates.
    pub(crate) fn capture_hover(&self) -> Option<Point> {
        if !crate::capture::hover_segment() {
            return None;
        }
        let points = self.points();
        let pair = points.windows(2).nth(1).or(points.windows(2).next())?;
        let ticks = (pair[0].time.ticks() + pair[1].time.ticks()) as f64 / 2.0;
        Some(Point::new(
            self.x(BeatTime(ticks as u64)),
            self.y(self.line_at(ticks)?) + self.segment_reach() * 0.75,
        ))
    }

    /// The segment band either side of the line, in pixels.
    pub(crate) fn segment_reach(&self) -> f32 {
        SEGMENT_REACH.min(self.plot.height / 4.0).max(LINE_REACH)
    }

    /// The value a hover over `at` reads: a point's own, or the line's under the pointer.
    pub(crate) fn hover(&self, at: Point) -> Option<f64> {
        match self.hit(at) {
            Hit::Point(i) => Some(self.points()[i].value),
            Hit::Segment(_) | Hit::Line => self.line_at(self.ticks_at(at.x)),
            Hit::Step | Hit::Empty => None,
        }
    }

    /// Alt-double-click: straighten the segment under `at`. `None` when it is already
    /// straight, the argument is Discrete, or no segment lies there.
    pub(crate) fn straighten(&self, at: Point) -> Option<Commit> {
        let Hit::Segment(i) = self.hit(at) else {
            return None;
        };
        if self.axis == Axis::Discrete || self.points()[i].curve == 0.0 {
            return None;
        }
        let mut points = self.points().to_vec();
        points[i].curve = 0.0;
        Some(Commit {
            points,
            select: None,
        })
    }

    /// A new point at `time`, after any point already there. `None` where two points already
    /// share the time, so no time ever holds three.
    fn insert(&self, time: BeatTime, value: f64) -> Option<(Vec<Breakpoint>, usize)> {
        let mut points = self.points().to_vec();
        if points.iter().filter(|p| p.time == time).count() >= 2 {
            return None;
        }
        let at = points.partition_point(|p| p.time <= time);
        points.insert(
            at,
            Breakpoint {
                time,
                value,
                curve: 0.0,
            },
        );
        Some((points, at))
    }
}

/// What a release publishes: the whole list (one `SetPoints`) and the point to select.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Commit {
    pub points: Vec<Breakpoint>,
    pub select: Option<usize>,
}

/// What a click does when it never moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Click {
    /// A point not yet selected: select it, so the fields show it.
    Select(usize),
    Delete(usize),
    AddOnLine,
    Nothing,
}

/// What a moving press does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Point { index: usize },
    Segment { left: usize },
    Bend { left: usize, up: f32 },
    Nothing,
}

/// What the readout beside a gesture shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Readout {
    Value(f64),
    Curve(f32),
}

/// One press-to-release gesture.
#[derive(Clone, Debug)]
pub(crate) struct Gesture {
    click: Click,
    drag: Drag,
    origin: Point,
    moved: bool,
    base: Vec<Breakpoint>,
    now: Vec<Breakpoint>,
    grabbed: Option<usize>,
    pointer: Point,
}

impl Gesture {
    /// A press at `at`, with `selected` the lane's selected point. `None` for what takes no
    /// gesture (a jump pair's step).
    pub(crate) fn press(
        lane: &Lane,
        at: Point,
        keys: Keys,
        selected: Option<usize>,
    ) -> Option<Self> {
        let base = lane.points().to_vec();
        let (click, drag) = match lane.hit(at) {
            Hit::Step => return None,
            Hit::Point(index) if selected == Some(index) => {
                (Click::Delete(index), Drag::Point { index })
            }
            Hit::Point(index) => (Click::Select(index), Drag::Point { index }),
            Hit::Segment(left) if keys.alt => (
                Click::Nothing,
                // Alt-drag up bends upward whichever way the segment runs: the sign picks the
                // curve value that raises its middle (positive starts slowly, so it lowers a
                // rising segment and raises a falling one). A flat segment bends nothing.
                match base[left + 1].value.total_cmp(&base[left].value) {
                    _ if lane.axis == Axis::Discrete => Drag::Nothing,
                    std::cmp::Ordering::Greater => Drag::Bend { left, up: -1.0 },
                    std::cmp::Ordering::Less => Drag::Bend { left, up: 1.0 },
                    std::cmp::Ordering::Equal => Drag::Nothing,
                },
            ),
            Hit::Segment(_) if lane.on_line(at) => return Self::new_point(lane, at, keys),
            Hit::Segment(left) => (Click::Nothing, Drag::Segment { left }),
            Hit::Line if keys.alt => (Click::Nothing, Drag::Nothing),
            Hit::Line => return Self::new_point(lane, at, keys),
            Hit::Empty => return None,
        };
        Some(Self {
            click,
            drag,
            origin: at,
            moved: false,
            now: base.clone(),
            base,
            grabbed: None,
            pointer: at,
        })
    }

    fn new_point(lane: &Lane, at: Point, keys: Keys) -> Option<Self> {
        let time = lane.time_at(at.x, keys);
        if lane.points().iter().any(|p| p.time == time) {
            return None;
        }
        let value = lane.quantise(lane.line_at(time.ticks() as f64)?);
        let (points, index) = lane.insert(time, value)?;
        Some(Self {
            click: Click::AddOnLine,
            drag: Drag::Point { index },
            origin: at,
            moved: false,
            base: points.clone(),
            now: points,
            grabbed: Some(index),
            pointer: at,
        })
    }

    /// The pointer moved to `to`, with `keys` down now.
    pub(crate) fn drag(&mut self, lane: &Lane, to: Point, keys: Keys) {
        self.pointer = to;
        let (dx, dy) = (to.x - self.origin.x, to.y - self.origin.y);
        self.moved |= dx.hypot(dy) >= DRAG_THRESHOLD;
        if !self.moved {
            return;
        }
        // Each axis keeps its exact start until it has travelled past the threshold, so a
        // purely vertical drag never snaps an off-grid time, nor a horizontal one a value.
        let (dx, dy) = (
            if dx.abs() < DRAG_THRESHOLD { 0.0 } else { dx },
            if dy.abs() < DRAG_THRESHOLD { 0.0 } else { dy },
        );
        match self.drag {
            Drag::Nothing => {}
            Drag::Point { index } => {
                let from = &self.base[index];
                let time = if dx == 0.0 {
                    from.time
                } else {
                    let ticks = from.time.ticks() as f64 + lane.time.delta_ticks(dx);
                    lane.snap(ticks, keys)
                };
                let value = if dy == 0.0 {
                    from.value
                } else {
                    lane.quantise(from.value + lane.delta(dy, keys))
                };
                let (points, at) = move_point(&self.base, index, time, value);
                self.now = points;
                self.grabbed = Some(at);
            }
            Drag::Segment { left } => {
                let (a, b) = (self.base[left].value, self.base[left + 1].value);
                let mut delta = lane.delta(dy, keys);
                if lane.axis != Axis::Continuous {
                    delta = delta.round();
                }
                // Both ends move together, so the shift stops where the first reaches the range.
                delta = delta.clamp(lane.min - a.min(b), lane.max - a.max(b));
                let mut points = self.base.clone();
                points[left].value = a + delta;
                points[left + 1].value = b + delta;
                self.now = points;
            }
            Drag::Bend { left, up } => {
                let mut points = self.base.clone();
                let curve = f64::from(points[left].curve) + f64::from(up * -dy / BEND_PIXELS);
                points[left].curve = curve.clamp(-1.0, 1.0) as f32;
                self.now = points;
            }
        }
    }

    /// A click in the wider segment band places the insert marker; dragging still offsets.
    pub(crate) fn marker_click(&self, at: Point) -> bool {
        !self.moved
            && self.origin.distance(at) < DRAG_THRESHOLD
            && matches!(self.drag, Drag::Segment { .. })
            && self.click == Click::Nothing
    }

    /// Where the pointer last was.
    pub(crate) fn pointer(&self) -> Point {
        self.pointer
    }

    /// Whether the press has become a drag that edits something; only then is there a preview.
    pub(crate) fn previewing(&self) -> bool {
        (self.moved || self.click == Click::AddOnLine) && self.drag != Drag::Nothing
    }

    /// The list a release would commit, while the gesture is a drag.
    pub(crate) fn preview(&self) -> Option<&[Breakpoint]> {
        self.previewing().then_some(self.now.as_slice())
    }

    /// The index in `preview` of the point being dragged.
    pub(crate) fn grabbed(&self) -> Option<usize> {
        self.previewing().then_some(self.grabbed).flatten()
    }

    /// What to read out beside the pointer: the dragged point's value, the curve being bent,
    /// or the line's value under a segment drag.
    pub(crate) fn readout(&self, lane: &Lane) -> Option<Readout> {
        if !self.previewing() {
            return None;
        }
        match self.drag {
            Drag::Point { .. } => self.grabbed.map(|i| Readout::Value(self.now[i].value)),
            Drag::Bend { left, .. } => Some(Readout::Curve(self.now[left].curve)),
            Drag::Segment { .. } => AutomationEnvelope {
                region: None,
                target: lane.envelope.target.clone(),
                points: self.now.clone(),
            }
            .value_at(lane.ticks_at(self.pointer.x), lane.line_axis())
            .map(Readout::Value),
            Drag::Nothing => None,
        }
    }

    /// The release at `at`: the final drag, then the click or drag decided. `None` commits
    /// nothing.
    pub(crate) fn release(mut self, lane: &Lane, at: Point, keys: Keys) -> Option<Commit> {
        self.drag(lane, at, keys);
        if self.moved {
            return (self.drag != Drag::Nothing
                && (self.now != self.base || self.click == Click::AddOnLine))
                .then_some(Commit {
                    select: self.grabbed,
                    points: self.now,
                });
        }
        match self.click {
            Click::Nothing => None,
            // The unchanged list: publishing it selects without an edit.
            Click::Select(index) => Some(Commit {
                points: self.base,
                select: Some(index),
            }),
            Click::Delete(index) => {
                let mut points = self.base;
                points.remove(index);
                Some(Commit {
                    points,
                    select: None,
                })
            }
            Click::AddOnLine => Some(Commit {
                points: self.now,
                select: self.grabbed,
            }),
        }
    }
}

impl Lane {
    pub(crate) fn on_line(&self, at: Point) -> bool {
        // A narrow stroke creates points; the wider hit band offsets a segment.
        self.line_at(self.ticks_at(at.x))
            .is_some_and(|v| (self.y(v) - at.y).abs() <= 1.5)
    }
}
/// `points` with `points[index]` moved to `time` and `value`, and the index it ends at.
///
/// Nothing clamps it by its neighbours (Live 12.1 check, 2026-10-21): each neighbour it
/// passes is gone from the result, landing on a neighbour's exact time makes a vertical step
/// with it (the dragged point on the side it came from, as it has not passed it), and where
/// two points already share the landing time the nearer one counts as passed, so no time ever
/// holds three. Typed positions go through the same function.
pub(crate) fn move_point(
    points: &[Breakpoint],
    index: usize,
    time: BeatTime,
    value: f64,
) -> (Vec<Breakpoint>, usize) {
    let from = points[index].time;
    let passed = |p: &Breakpoint| {
        if time > from {
            p.time > from && p.time < time
        } else {
            p.time < from && p.time > time
        }
    };
    let mut kept: Vec<(usize, Breakpoint)> = points
        .iter()
        .cloned()
        .enumerate()
        .filter(|(i, p)| *i != index && !passed(p))
        .collect();
    if time != from {
        let landing: Vec<usize> = kept
            .iter()
            .filter(|(_, p)| p.time == time)
            .map(|(i, _)| *i)
            .collect();
        if landing.len() >= 2 {
            // Moving right meets the earlier of the pair first; moving left, the later.
            let nearer = if time > from {
                landing[0]
            } else {
                landing[landing.len() - 1]
            };
            kept.retain(|(i, _)| *i != nearer);
        }
    }
    let at = match time.cmp(&from) {
        // It has not passed a neighbour it lands on, so it stays on the side it came from:
        // moving right it goes before the points at that time, moving left after them.
        std::cmp::Ordering::Greater => kept.iter().filter(|(_, p)| p.time < time).count(),
        std::cmp::Ordering::Less => kept.iter().filter(|(_, p)| p.time <= time).count(),
        std::cmp::Ordering::Equal => index,
    };
    let mut result: Vec<Breakpoint> = kept.into_iter().map(|(_, p)| p).collect();
    result.insert(
        at,
        Breakpoint {
            time,
            value,
            curve: points[index].curve,
        },
    );
    (result, at)
}

#[cfg(test)]
mod tests;
