//! Draw Mode (Live's B), as pure functions from pointer samples to the content one gesture
//! replaces. Nothing here draws, publishes or edits: both canvases feed it pointer positions and
//! publish what it returns on release as one `ReplaceRange` (design section 5, D9).
//!
//! Envelope lanes paint flat steps, one per grid cell the pointer crosses, each at the pointer's
//! value (Shift finer). With ⌘ held, or the grid off, they draw freehand instead: the path is
//! sampled, then thinned with Ramer–Douglas–Peucker so a gesture leaves few points. Event lanes
//! place one hit per grid cell crossed and skip cells that already hold one.
use super::point_gesture::{Keys, Lane};
use iced::Point;
use qtable_engine::model::{Axis, BeatTime, Breakpoint, thin};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::time::{Duration, Instant};

/// Holding B this long makes Draw Mode momentary: releasing the key switches it back.
const HOLD: Duration = Duration::from_millis(300);
/// The cell hits are placed in when the grid is Off (1/16): one per tick would flood the lane.
pub(crate) const OFF_CELL: u64 = 240;

/// Whether Draw Mode is on. The switch, the B key and the clip editor read the one flag.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Mode {
    pub(crate) on: bool,
    pressed: Option<Instant>,
}

impl Mode {
    /// The switch: a plain toggle.
    pub(crate) fn toggle(&mut self) {
        self.on = !self.on;
        self.pressed = None;
    }

    /// B goes down: toggles at once, and notes when, so a long hold can undo it on release.
    pub(crate) fn key_down(&mut self, now: Instant) {
        self.on = !self.on;
        self.pressed = Some(now);
    }

    /// B comes up: a tap leaves the toggle, a hold (Live's temporary toggle) reverses it.
    pub(crate) fn key_up(&mut self, now: Instant) {
        if let Some(since) = self.pressed.take()
            && now.duration_since(since) >= HOLD
        {
            self.on = !self.on;
        }
    }
}

/// What one envelope gesture replaces: the half-open range and the points that fill it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Replacement {
    pub(crate) range: Range<BeatTime>,
    pub(crate) points: Vec<Breakpoint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Brush {
    Steps,
    Freehand,
}

/// One press-to-release paint stroke on an envelope lane.
#[derive(Clone, Debug)]
pub(crate) struct EnvelopeStroke {
    brush: Brush,
    length: u64,
    cell: u64,
    axis: Axis,
    /// How far from the chord a freehand point may sit and still be dropped.
    tolerance: f64,
    existing: Vec<Breakpoint>,
    /// Step brush: the value of each cell crossed, by cell index.
    cells: BTreeMap<u64, f64>,
    last_cell: Option<(u64, f64)>,
    /// Freehand brush: the sampled path in (tick, value).
    path: Vec<(u64, f64)>,
    /// Shift's anchor: the pointer row and the value there when it went down.
    fine: Option<(f32, f64)>,
    last_value: Option<f64>,
}

impl EnvelopeStroke {
    /// A stroke on `lane`. ⌘ at the press, or a grid that is Off, draws freehand.
    pub(crate) fn new(lane: &Lane, threshold: f64, keys: Keys) -> Self {
        let freehand = keys.command || lane.grid <= 1;
        let span = lane.max - lane.min;
        // Never finer than a pixel of the lane, or a threshold of 0 would keep every sample.
        let pixel = span / f64::from(lane.plot.height.max(1.0));
        let tolerance = match lane.axis {
            Axis::Continuous => threshold.max(pixel),
            Axis::Stepped | Axis::Discrete => threshold.max(pixel).min(0.5),
        };
        Self {
            brush: if freehand {
                Brush::Freehand
            } else {
                Brush::Steps
            },
            length: lane.length().ticks(),
            cell: lane.grid.max(1),
            axis: lane.axis,
            tolerance,
            existing: lane.points().to_vec(),
            cells: BTreeMap::new(),
            last_cell: None,
            path: vec![],
            fine: None,
            last_value: None,
        }
    }

    /// The value under the pointer; Shift scales the change since it went down by a tenth.
    fn value(&mut self, lane: &Lane, at: Point, keys: Keys) -> f64 {
        if !keys.shift {
            self.fine = None;
            return lane.value_at_y(at.y);
        }
        let (y0, v0) = *self
            .fine
            .get_or_insert_with(|| (at.y, self.last_value.unwrap_or(lane.value_at_y(at.y))));
        lane.quantise(v0 + lane.delta(at.y - y0, keys))
    }

    /// The pointer is at `at` with `keys` down.
    pub(crate) fn push(&mut self, lane: &Lane, at: Point, keys: Keys) {
        let value = self.value(lane, at, keys);
        self.last_value = Some(value);
        let ticks = lane.ticks_at(at.x);
        match self.brush {
            Brush::Freehand => self.path.push((ticks.round() as u64, value)),
            Brush::Steps => {
                // The far edge belongs to the last cell, not to a cell past the clip.
                let last = self.length.saturating_sub(1) / self.cell;
                let cell = ((ticks as u64) / self.cell).min(last);
                match self.last_cell {
                    Some((from, before)) if from != cell => {
                        // A quick pointer skips cells; each one crossed takes a value on the path.
                        let n = from.abs_diff(cell);
                        for i in 1..=n {
                            let c = if cell > from { from + i } else { from - i };
                            let v = before + (value - before) * i as f64 / n as f64;
                            self.cells.insert(c, lane.quantise(v));
                        }
                    }
                    _ => {
                        self.cells.insert(cell, value);
                    }
                }
                self.last_cell = Some((cell, value));
            }
        }
    }

    /// The replacement the stroke would commit now; `None` before it has painted anything.
    pub(crate) fn replacement(&self) -> Option<Replacement> {
        match self.brush {
            Brush::Steps => self.steps(),
            Brush::Freehand => self.freehand(),
        }
    }

    fn steps(&self) -> Option<Replacement> {
        let first = *self.cells.keys().next()?;
        let last = *self.cells.keys().next_back()?;
        let mut runs: Vec<(u64, u64, f64)> = vec![];
        for (&cell, &value) in &self.cells {
            let (start, end) = (cell * self.cell, ((cell + 1) * self.cell).min(self.length));
            match runs.last_mut() {
                Some(run) if run.1 == start && run.2 == value => run.1 = end,
                _ => runs.push((start, end, value)),
            }
        }
        let (start, end) = (first * self.cell, ((last + 1) * self.cell).min(self.length));
        // Each run is a flat step, two points a cell-run apart; neighbours of different value
        // meet as two points at one time, v3.3's jump form.
        let mut points: Vec<Breakpoint> = runs
            .iter()
            .flat_map(|&(s, e, v)| [point(s, v), point(e, v)])
            .collect();
        // The replaced range is half open and its edges stay, so it reaches one tick past the
        // last step's end. A point already exactly there would be swept up with it: put it back
        // after the step, where the jump to it belongs.
        points.extend(
            self.existing
                .iter()
                .rfind(|p| p.time.ticks() == end)
                .cloned(),
        );
        Some(Replacement {
            range: BeatTime(start)..BeatTime(end + 1),
            points,
        })
    }

    fn freehand(&self) -> Option<Replacement> {
        // One value per tick, the last one drawn there, in time order.
        let mut by_tick: BTreeMap<u64, f64> = BTreeMap::new();
        for &(tick, value) in &self.path {
            by_tick.insert(tick.min(self.length), value);
        }
        let samples: Vec<(u64, f64)> = by_tick.into_iter().collect();
        let (first, last) = (samples.first()?.0, samples.last()?.0);
        let kept = match self.axis {
            Axis::Discrete => held(&samples),
            Axis::Continuous | Axis::Stepped => thin(&samples, self.tolerance),
        };
        Some(Replacement {
            range: BeatTime(first)..BeatTime(last + 1),
            points: kept.into_iter().map(|(t, v)| point(t, v)).collect(),
        })
    }

    /// The lane as it would stand after the stroke: what the canvas draws while painting.
    pub(crate) fn preview(&self) -> Vec<Breakpoint> {
        match self.replacement() {
            Some(r) => replaced(&self.existing, &r),
            None => self.existing.clone(),
        }
    }
}

fn point(ticks: u64, value: f64) -> Breakpoint {
    Breakpoint {
        time: BeatTime(ticks),
        value,
        curve: 0.0,
    }
}

/// `existing` with `replacement` applied, by the engine's `ReplaceRange` rule: points strictly
/// inside the range go; equal-time merges keep the arriving and outgoing values.
pub(crate) fn replaced(existing: &[Breakpoint], replacement: &Replacement) -> Vec<Breakpoint> {
    let mut points = existing.to_vec();
    qtable_engine::edit::PointReplacement::KeepEdges.replace_points(
        &mut points,
        &replacement.range,
        replacement.points.iter().cloned(),
    );
    points
}

/// A Discrete argument holds its value between points, so only the samples where it changes
/// matter (and the last, which ends the drawn span).
fn held(samples: &[(u64, f64)]) -> Vec<(u64, f64)> {
    let mut kept: Vec<(u64, f64)> = vec![];
    for &(t, v) in samples {
        if kept.last().is_none_or(|&(_, before)| before != v) {
            kept.push((t, v));
        }
    }
    if let (Some(&end), Some(&tail)) = (samples.last(), kept.last())
        && end != tail
    {
        kept.push(end);
    }
    kept
}

/// One press-to-release paint stroke on an event lane.
#[derive(Clone, Debug)]
pub(crate) struct HitStroke {
    length: u64,
    cell: u64,
    /// Cells that already hold a hit of this lane.
    occupied: BTreeSet<u64>,
    cells: BTreeSet<u64>,
    last: Option<u64>,
}

impl HitStroke {
    /// A stroke on a lane whose hits sit at `existing`, on a grid of `grid` ticks (1 is Off).
    pub(crate) fn new(
        grid: u64,
        length: BeatTime,
        existing: impl Iterator<Item = BeatTime>,
    ) -> Self {
        let cell = if grid <= 1 { OFF_CELL } else { grid };
        Self {
            length: length.ticks(),
            cell,
            occupied: existing.map(|p| p.ticks() / cell).collect(),
            cells: BTreeSet::new(),
            last: None,
        }
    }

    /// The pointer is at `ticks`; every cell between it and the last one is crossed.
    pub(crate) fn push(&mut self, ticks: f64) {
        let last = self.length.saturating_sub(1) / self.cell;
        let cell = ((ticks.max(0.0) as u64) / self.cell).min(last);
        let from = self.last.unwrap_or(cell);
        for c in from.min(cell)..=from.max(cell) {
            self.cells.insert(c);
        }
        self.last = Some(cell);
    }

    /// The range the stroke covers, whole cells from the first to the last.
    pub(crate) fn span(&self) -> Option<Range<BeatTime>> {
        let (first, last) = (*self.cells.first()?, *self.cells.last()?);
        Some(BeatTime(first * self.cell)..BeatTime(((last + 1) * self.cell).min(self.length)))
    }

    /// Where the new hits go: the start of each crossed cell that holds none yet.
    pub(crate) fn positions(&self) -> Vec<BeatTime> {
        self.cells
            .iter()
            .filter(|c| !self.occupied.contains(c))
            .map(|c| BeatTime(c * self.cell))
            .collect()
    }
}

/// A stroke in progress on either kind of lane.
#[derive(Clone, Debug)]
pub(crate) enum Painting {
    Envelope(EnvelopeStroke),
    Hits(HitPainting),
}

/// A hit-lane stroke. A press on an existing hit that never travels is a click and deletes it;
/// one that travels paints from where it began.
#[derive(Clone, Debug)]
pub(crate) struct HitPainting {
    stroke: HitStroke,
    /// The pressed hit (an index into the clip's events), where, and at which tick.
    erase: Option<(usize, Point, f64)>,
}

/// What a finished stroke asks for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Outcome {
    Points(Replacement),
    Hits {
        span: Range<BeatTime>,
        positions: Vec<BeatTime>,
    },
    Erase(usize),
}

impl Painting {
    /// A press on an envelope lane, painting its first sample.
    pub(crate) fn envelope(lane: &Lane, threshold: f64, keys: Keys, at: Point) -> Self {
        let mut stroke = EnvelopeStroke::new(lane, threshold, keys);
        stroke.push(lane, at, keys);
        Self::Envelope(stroke)
    }

    /// A press on an event lane at `ticks`; `over` is the hit under it, if any.
    pub(crate) fn hits(mut stroke: HitStroke, ticks: f64, over: Option<usize>, at: Point) -> Self {
        let erase = over.map(|index| (index, at, ticks));
        if erase.is_none() {
            stroke.push(ticks);
        }
        Self::Hits(HitPainting { stroke, erase })
    }

    /// The pointer moved to `at` (`ticks` along an event lane; envelope lanes use `lane`).
    pub(crate) fn push(&mut self, lane: Option<&Lane>, at: Point, ticks: f64, keys: Keys) {
        match self {
            Self::Envelope(stroke) => {
                if let Some(lane) = lane {
                    stroke.push(lane, at, keys);
                }
            }
            Self::Hits(p) => {
                if let Some((_, origin, began)) = p.erase
                    && origin.distance(at) > super::point_gesture::DRAG_THRESHOLD
                {
                    p.erase = None;
                    p.stroke.push(began);
                }
                if p.erase.is_none() {
                    p.stroke.push(ticks);
                }
            }
        }
    }

    /// The envelope as it would stand after the stroke.
    pub(crate) fn preview(&self) -> Option<Vec<Breakpoint>> {
        match self {
            Self::Envelope(stroke) => Some(stroke.preview()),
            Self::Hits(_) => None,
        }
    }

    /// The hits the stroke would add so far.
    pub(crate) fn pending_hits(&self) -> Vec<BeatTime> {
        match self {
            Self::Hits(p) if p.erase.is_none() => p.stroke.positions(),
            _ => vec![],
        }
    }

    pub(crate) fn finish(self) -> Option<Outcome> {
        match self {
            Self::Envelope(stroke) => stroke.replacement().map(Outcome::Points),
            Self::Hits(p) => {
                match p.erase {
                    Some((index, ..)) => Some(Outcome::Erase(index)),
                    None => {
                        let positions = p.stroke.positions();
                        Some(Outcome::Hits {
                        span: p.stroke.span()?,
                        positions,
                    })
                    .filter(|o| !matches!(o, Outcome::Hits { positions, .. } if positions.is_empty()))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
