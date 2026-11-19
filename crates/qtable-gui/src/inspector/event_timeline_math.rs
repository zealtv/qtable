//! Geometry is independent of drawing and of the engine command layer.
use super::lane_model::{LaneModel, RowKind};
use crate::control::time_axis::TimeAxis;
use iced::Point;
use qtable_engine::model::{BeatTime, Event};

pub(super) const LABEL_WIDTH: f32 = 148.0;
/// The insert strip occupies the space immediately below the loop brace.
pub(super) use crate::style::detail::INSERT_STRIP_TOP;
pub(super) const RULER_HEIGHT: f32 = 38.0;
/// Event lanes keep today's 24 px; envelope lanes open at 48 px (design section 8).
pub(super) const EVENT_LANE_HEIGHT: f32 = 24.0;
pub(super) const ENVELOPE_LANE_HEIGHT: f32 = 48.0;
pub(super) const MARKER_HALF: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Grid {
    Off,
    Quarter,
    Eighth,
    #[default]
    Sixteenth,
    ThirtySecond,
    EighthTriplet,
    SixteenthTriplet,
}
impl Grid {
    pub(super) const ALL: [Self; 7] = [
        Self::Off,
        Self::Quarter,
        Self::Eighth,
        Self::Sixteenth,
        Self::ThirtySecond,
        Self::EighthTriplet,
        Self::SixteenthTriplet,
    ];
    pub(super) fn ticks(self) -> u64 {
        match self {
            Self::Off => 1,
            Self::Quarter => 960,
            Self::Eighth => 480,
            Self::Sixteenth => 240,
            Self::ThirtySecond => 120,
            Self::EighthTriplet => 320,
            Self::SixteenthTriplet => 160,
        }
    }
}
impl std::fmt::Display for Grid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Off => "Off",
            Self::Quarter => "1/4",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
            Self::ThirtySecond => "1/32",
            Self::EighthTriplet => "1/8T",
            Self::SixteenthTriplet => "1/16T",
        })
    }
}

/// Where the Events and Overview canvas puts time: its time axis, from `LABEL_WIDTH` to the
/// right gutter.
pub(super) struct Geometry {
    pub(super) axis: TimeAxis,
}
impl Geometry {
    /// The whole of `length` across a canvas `width` px wide.
    #[cfg(test)]
    pub(super) fn fitted(length: BeatTime, width: f32) -> Self {
        Self {
            axis: TimeAxis::fitted(LABEL_WIDTH, (width - LABEL_WIDTH - 10.0).max(1.0), length),
        }
    }
    /// `length` across a canvas `width` px wide as the clip's zoom window shows it (`None` is
    /// the whole of it).
    pub(super) fn new(
        length: BeatTime,
        width: f32,
        window: Option<crate::control::time_axis::Window>,
    ) -> Self {
        Self {
            axis: TimeAxis::zoomed(
                LABEL_WIDTH,
                (width - LABEL_WIDTH - 10.0).max(1.0),
                length,
                window,
            ),
        }
    }
    /// The content's extent: what snapping and drags clamp to.
    pub(super) fn length(&self) -> BeatTime {
        self.axis.extent
    }
    pub(super) fn span(&self) -> f32 {
        self.axis.span
    }
    pub(super) fn x(&self, position: BeatTime) -> f32 {
        self.axis.x(position)
    }
    /// The row whose header (the address column) is under `point`.
    pub(super) fn header_at(&self, point: Point, model: &LaneModel) -> Option<usize> {
        if point.x < 0.0 || point.x >= LABEL_WIDTH || point.y < RULER_HEIGHT {
            return None;
        }
        model.row_at(point.y - RULER_HEIGHT)
    }
    pub(super) fn lane_at(&self, point: Point, model: &LaneModel) -> Option<usize> {
        if point.x < LABEL_WIDTH || point.x > LABEL_WIDTH + self.span() || point.y < RULER_HEIGHT {
            return None;
        }
        model.row_at(point.y - RULER_HEIGHT)
    }
    /// The unsnapped tick under `x`, held to the clip.
    pub(super) fn ticks_at(&self, x: f32) -> f64 {
        self.axis.ticks_at(x)
    }
    /// The time under `x`, snapped to the grid inside a half-open clip.
    pub(super) fn position_at(&self, x: f32, grid: Grid) -> BeatTime {
        snap(
            self.axis.unclamped_ticks_at(x).max(0.0),
            self.length(),
            grid,
        )
    }
    pub(super) fn dragged(&self, initial: BeatTime, delta_x: f32, grid: Grid) -> BeatTime {
        let ticks = initial.ticks() as f64 + self.axis.delta_ticks(delta_x);
        snap(ticks, self.length(), grid)
    }
    /// The index of the hit under `point`, among `events`, in an event row of `model`.
    pub(super) fn hit(&self, point: Point, events: &[Event], model: &LaneModel) -> Option<usize> {
        // A zoomed window clips its marks to the time area, so nothing outside it is hit.
        let reach = if self.axis.is_windowed() {
            0.0
        } else {
            MARKER_HALF
        };
        if point.x < LABEL_WIDTH - reach
            || point.x > LABEL_WIDTH + self.span() + reach
            || point.y < RULER_HEIGHT
        {
            return None;
        }
        let row = model.row_at(point.y - RULER_HEIGHT)?;
        let RowKind::Event { reference, .. } = &model.rows[row].kind else {
            return None;
        };
        // Last-drawn event wins ties; equal-time events remain distinct in the model.
        events
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, e)| &e.definition == reference)
            // A zoomed window clips its marks to the time area: one whose centre is outside
            // it is not drawn there and cannot be hit by its sliver.
            .filter(|(_, e)| self.axis.shows(e.position, 0.0))
            .filter_map(|(i, e)| {
                let distance = (point.x - self.x(e.position)).abs();
                (distance <= MARKER_HALF + 2.0).then_some((i, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }
}

pub(super) fn snap(ticks: f64, length: BeatTime, grid: Grid) -> BeatTime {
    let step = grid.ticks();
    let last = length.ticks().saturating_sub(1) / step * step;
    let ticks = if ticks.is_finite() {
        ticks.max(0.0)
    } else {
        0.0
    };
    let snapped = (ticks / step as f64).round() as u64;
    BeatTime(snapped.saturating_mul(step).min(last))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn geometry() -> Geometry {
        Geometry::fitted(BeatTime(3840), 558.0)
    }
    #[test]
    fn snapping_uses_nearest_grid_and_half_open_clip_end() {
        assert_eq!(snap(119.0, BeatTime(3840), Grid::Sixteenth), BeatTime(0));
        assert_eq!(snap(120.0, BeatTime(3840), Grid::Sixteenth), BeatTime(240));
        assert_eq!(
            snap(4000.0, BeatTime(3840), Grid::Sixteenth),
            BeatTime(3600)
        );
        assert_eq!(snap(3840.0, BeatTime(3840), Grid::Off), BeatTime(3839));
        assert_eq!(
            snap(330.0, BeatTime(3840), Grid::EighthTriplet),
            BeatTime(320)
        );
        assert_eq!(snap(-30.0, BeatTime(20), Grid::Quarter), BeatTime(0));
        assert_eq!(snap(f64::NAN, BeatTime(20), Grid::Off), BeatTime(0));
        assert!(snap(u64::MAX as f64, BeatTime(u64::MAX), Grid::Sixteenth).ticks() < u64::MAX);
    }
    #[test]
    fn dragging_keeps_the_grab_offset_and_clamps_both_ends() {
        let g = geometry();
        assert_eq!(
            g.dragged(BeatTime(960), 50.0, Grid::Sixteenth),
            BeatTime(1440)
        );
        assert_eq!(
            g.dragged(BeatTime(960), -500.0, Grid::Sixteenth),
            BeatTime(0)
        );
        assert_eq!(
            g.dragged(BeatTime(960), 500.0, Grid::Sixteenth),
            BeatTime(3600)
        );
        assert_eq!(
            g.position_at(g.x(BeatTime(1440)), Grid::Sixteenth),
            BeatTime(1440)
        );
    }
    #[test]
    fn headers_and_hits_distinguish_absolute_from_relative_and_ignore_ruler_labels_and_gaps() {
        use qtable_engine::model::{DeviceId, OscAddress, Track, TrackId};
        let mut track = Track::new(TrackId(1), "t", 0);
        let mut hit = |ticks, path| track.hit(BeatTime(ticks), path, vec![], || DeviceId(1));
        let events = vec![
            hit(0, OscAddress::Relative("/x".into())),
            hit(960, OscAddress::Absolute("/x".into())),
            hit(0, OscAddress::Relative("/x".into())),
        ];
        let lanes =
            LaneModel::of_events(&[events[0].definition.clone(), events[1].definition.clone()]);
        let g = geometry();
        assert_eq!(
            g.hit(Point::new(LABEL_WIDTH, 50.0), &events, &lanes),
            Some(2)
        );
        assert_eq!(
            g.hit(Point::new(g.x(BeatTime(960)) + 4.0, 74.0), &events, &lanes),
            Some(1)
        );
        for point in [
            Point::new(0.0, 50.0),
            Point::new(150.0, 20.0),
            Point::new(200.0, 50.0),
            Point::new(150.0, 100.0),
        ] {
            assert_eq!(g.hit(point, &events, &lanes), None);
        }
        assert_eq!(g.lane_at(Point::new(200.0, 62.0), &lanes), Some(1));
        assert_eq!(g.lane_at(Point::new(140.0, 62.0), &lanes), None);
        assert_eq!(g.header_at(Point::new(140.0, 62.0), &lanes), Some(1));
        assert_eq!(g.header_at(Point::new(200.0, 62.0), &lanes), None);
        assert_eq!(g.header_at(Point::new(20.0, 20.0), &lanes), None);
        assert_eq!(g.header_at(Point::new(20.0, 90.0), &lanes), None);
    }
}
