//! The loop brace and its Start and End flags on a time ruler: Live's clip and envelope brace,
//! and later Arrangement's loop and punch braces (`keep-room-for-arrangement-view-and-richer-
//! clips`). Given a time axis and a region it draws, hit-tests its parts and turns a drag into
//! a new region. It knows nothing about clips or lanes; hosts choose the inks and publish.
use crate::control::time_axis::TimeAxis;
use crate::style;
use crate::style::detail::{
    CLIP_BRACE_TOP, CLIP_FLAG_BOTTOM, CLIP_FLAG_TOP, CLIP_FLAG_WIDTH, CLIP_MARKER_DRAG_THRESHOLD,
    CLIP_MARKER_HIT_SLOP, INSERT_STRIP_TOP,
};
use iced::widget::canvas::{Frame, Path, Stroke};
use iced::{Color, Point, Size, mouse};
use qtable_engine::model::BeatTime;
use qtable_engine::model::region::Region;

/// Live's brace end triangles: this wide, inset this far from the brace's edges.
const END_MARK_WIDTH: f32 = 5.0;
const END_MARK_INSET: f32 = 1.5;

/// A part of the brace or its flags, each with its own gesture, cursor and help.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Start,
    End,
    LoopStart,
    LoopEnd,
    /// The brace between its ends: dragging moves the loop, keeping its length.
    Body,
}

impl Part {
    /// Live's cursors: a resize cursor on an end or a flag, a move cursor on the body.
    pub(crate) fn interaction(self) -> mouse::Interaction {
        match self {
            Part::Body => mouse::Interaction::Move,
            _ => mouse::Interaction::ResizingHorizontally,
        }
    }
}

/// One brace and its flags. `ink` fills the brace while its loop is on; `flag_ink` draws Start
/// and, with the loop off, End.
pub(crate) struct Brace {
    pub axis: TimeAxis,
    pub region: Region,
    pub ink: Color,
    pub flag_ink: Color,
}

impl Brace {
    /// The flags, then the brace in its own row with an inward triangle at each end. While
    /// dragged it fills with Live's selection ink.
    pub(crate) fn draw(&self, frame: &mut Frame, dragged: bool) {
        let r = &self.region;
        let (left, right) = (self.axis.x(r.loop_start), self.axis.x(r.loop_end));
        let fill = if dragged {
            crate::control::LIVE_DARK.selection_background
        } else if r.looping {
            self.ink
        } else {
            style::DISABLED
        };
        frame.fill_rectangle(
            Point::new(left, CLIP_BRACE_TOP),
            Size::new(right - left, INSERT_STRIP_TOP - CLIP_BRACE_TOP),
            fill,
        );
        for x in [left, right] {
            line(
                frame,
                Point::new(x, CLIP_BRACE_TOP),
                Point::new(x, INSERT_STRIP_TOP),
                style::TEXT,
            );
        }
        // ▷ inside the loop start, ◁ inside the loop end; a brace too short for both shows none.
        if right - left >= 2.0 * (END_MARK_WIDTH + END_MARK_INSET) + 2.0 {
            let ink = crate::control::LIVE_DARK.brace_end_mark;
            let (top, bottom) = (
                CLIP_BRACE_TOP + END_MARK_INSET,
                INSERT_STRIP_TOP - END_MARK_INSET,
            );
            for (x, direction) in [(left + END_MARK_INSET, 1.0), (right - END_MARK_INSET, -1.0)] {
                frame.fill(&triangle(x, direction, END_MARK_WIDTH, top, bottom), ink);
            }
        }
        let end_ink = if r.looping {
            style::DISABLED
        } else {
            self.flag_ink
        };
        for (position, direction, ink) in [(r.start, 1.0, self.flag_ink), (r.end, -1.0, end_ink)] {
            frame.fill(
                &triangle(
                    self.axis.x(position),
                    direction,
                    CLIP_FLAG_WIDTH,
                    CLIP_FLAG_TOP,
                    CLIP_FLAG_BOTTOM,
                ),
                ink,
            );
        }
    }

    /// The part under `p`: a flag in the flag row, an end or the body in the brace row.
    pub(crate) fn part_at(&self, p: Point) -> Option<Part> {
        // A zoomed window shows part of the brace: nothing outside the time area is grabbed (the
        // header column, the value scale, the gutter), and nor is an end or flag the window
        // does not show. A fitted axis is unchanged.
        let windowed = self.axis.is_windowed();
        if windowed && (p.x < self.axis.left || p.x > self.axis.left + self.axis.span) {
            return None;
        }
        let near = |position| {
            (!windowed || self.axis.shows(position, 0.0))
                && (p.x - self.axis.x(position)).abs() <= CLIP_MARKER_HIT_SLOP
        };
        let r = &self.region;
        if (CLIP_FLAG_TOP..CLIP_BRACE_TOP).contains(&p.y) {
            if near(r.start) {
                return Some(Part::Start);
            }
            if near(r.end) {
                return Some(Part::End);
            }
        }
        if (CLIP_BRACE_TOP..INSERT_STRIP_TOP).contains(&p.y) {
            if near(r.loop_start) {
                return Some(Part::LoopStart);
            }
            if near(r.loop_end) {
                return Some(Part::LoopEnd);
            }
            if (self.axis.x(r.loop_start)..self.axis.x(r.loop_end)).contains(&p.x) {
                return Some(Part::Body);
            }
        }
        None
    }
}

/// Another region's brace as a thin line above the active one, such as the clip's own above
/// an unlinked envelope's; it takes no gestures.
pub(crate) fn reference(frame: &mut Frame, axis: &TimeAxis, region: Region) {
    line(
        frame,
        Point::new(axis.x(region.loop_start), CLIP_BRACE_TOP - 2.0),
        Point::new(axis.x(region.loop_end), CLIP_BRACE_TOP - 2.0),
        style::DISABLED,
    );
}

/// A press on a part, followed until release. Only the host publishes, on release.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Drag {
    part: Part,
    origin: f32,
    initial: Region,
    pub(crate) preview: Region,
    moved: bool,
}

impl Drag {
    pub(crate) fn new(part: Part, origin: f32, region: Region) -> Self {
        Self {
            part,
            origin,
            initial: region,
            preview: region,
            moved: false,
        }
    }

    pub(crate) fn part(&self) -> Part {
        self.part
    }

    /// The new region, once the drag has moved and changed it.
    pub(crate) fn changed(&self) -> Option<Region> {
        (self.moved && self.preview != self.initial).then_some(self.preview)
    }

    /// The pointer is at `x`; positions snap to `step` ticks inside the axis's extent.
    pub(crate) fn follow(&mut self, axis: &TimeAxis, x: f32, step: u64) {
        if (x - self.origin).abs() < CLIP_MARKER_DRAG_THRESHOLD && !self.moved {
            return;
        }
        self.moved = true;
        let initial = match self.part {
            Part::Start => self.initial.start,
            Part::End => self.initial.end,
            Part::LoopStart | Part::Body => self.initial.loop_start,
            Part::LoopEnd => self.initial.loop_end,
        };
        let step = step.max(1);
        let raw = initial.ticks() as f64 + axis.delta_ticks(x - self.origin);
        let at = ((raw.max(0.0) / step as f64).round() as u64)
            .saturating_mul(step)
            .min(axis.extent.ticks());
        let mut r = self.initial;
        match self.part {
            Part::Start => r.start = BeatTime(at.min(r.end.ticks().saturating_sub(1))),
            Part::End => r.end = BeatTime(at.max(r.start.ticks().saturating_add(1))),
            Part::LoopStart => {
                r.loop_start = BeatTime(at.min(r.loop_end.ticks().saturating_sub(1)))
            }
            Part::LoopEnd => r.loop_end = BeatTime(at.max(r.loop_start.ticks().saturating_add(1))),
            Part::Body => {
                let length = r.loop_end.ticks() - r.loop_start.ticks();
                r.loop_start = BeatTime(at.min(axis.extent.ticks().saturating_sub(length)));
                r.loop_end = BeatTime(r.loop_start.ticks() + length);
            }
        }
        self.preview = r.normalised();
    }
}

fn line(frame: &mut Frame, a: Point, b: Point, colour: Color) {
    frame.stroke(
        &Path::line(a, b),
        Stroke::default().with_color(colour).with_width(1.0),
    );
}

/// A triangle with its upright side at `x`, pointing right (1) or left (-1).
fn triangle(x: f32, direction: f32, width: f32, top: f32, bottom: f32) -> Path {
    Path::new(|p| {
        p.move_to(Point::new(x, top));
        p.line_to(Point::new(x + width * direction, (top + bottom) / 2.0));
        p.line_to(Point::new(x, bottom));
        p.close();
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brace() -> Brace {
        Brace {
            axis: TimeAxis::fitted(0.0, 768.0, BeatTime(7680)),
            region: Region {
                start: BeatTime(480),
                end: BeatTime(5760),
                loop_start: BeatTime(960),
                loop_end: BeatTime(2880),
                looping: true,
            },
            ink: style::ACCENT,
            flag_ink: style::TEXT,
        }
    }

    #[test]
    fn parts_are_found_in_their_rows_with_live_s_cursors() {
        let b = brace();
        let flag = (CLIP_FLAG_TOP + CLIP_FLAG_BOTTOM) / 2.0;
        let row = (CLIP_BRACE_TOP + INSERT_STRIP_TOP) / 2.0;
        for (at, part) in [
            ((48.0, flag), Some(Part::Start)),
            ((576.0, flag), Some(Part::End)),
            ((96.0, row), Some(Part::LoopStart)),
            ((288.0, row), Some(Part::LoopEnd)),
            ((180.0, row), Some(Part::Body)),
            ((180.0, flag), None),
            ((400.0, row), None),
            ((48.0, INSERT_STRIP_TOP + 2.0), None),
        ] {
            assert_eq!(b.part_at(Point::new(at.0, at.1)), part, "{at:?}");
        }
        assert_eq!(Part::Body.interaction(), mouse::Interaction::Move);
        for part in [Part::Start, Part::End, Part::LoopStart, Part::LoopEnd] {
            assert_eq!(part.interaction(), mouse::Interaction::ResizingHorizontally);
        }
    }

    #[test]
    fn each_part_drags_only_its_own_markers() {
        let b = brace();
        for (part, from, to, expected) in [
            (Part::Start, 48.0, 121.0, (1200, 5760, 960, 2880)),
            (Part::End, 576.0, 627.0, (480, 6240, 960, 2880)),
            (Part::LoopStart, 96.0, 121.0, (480, 5760, 1200, 2880)),
            (Part::LoopEnd, 288.0, 341.0, (480, 5760, 960, 3360)),
            (Part::Body, 180.0, 229.0, (480, 5760, 1440, 3360)),
        ] {
            let mut d = Drag::new(part, from, b.region);
            d.follow(&b.axis, from + 2.0, 240);
            assert_eq!(d.changed(), None, "under the threshold nothing moves");
            d.follow(&b.axis, to, 240);
            let r = d.changed().unwrap();
            assert_eq!((r.start.0, r.end.0, r.loop_start.0, r.loop_end.0), expected);
        }
    }

    #[test]
    fn a_drag_on_a_windowed_axis_moves_by_its_ticks_per_pixel() {
        use crate::control::time_axis::Window;
        // 960 ticks over 480 px: two ticks a pixel, so 60 px is 120 ticks, one 1/32 step.
        let axis = TimeAxis::windowed(
            0.0,
            480.0,
            BeatTime(7680),
            Window {
                start: 480.0,
                visible: 960.0,
            },
        );
        assert_eq!(axis.ticks_per_px(), 2.0);
        let region = brace().region;
        let mut d = Drag::new(Part::LoopStart, 100.0, region);
        d.follow(&axis, 160.0, 120);
        assert_eq!(d.changed().unwrap().loop_start, BeatTime(960 + 120));
        // Still held to the extent, as on a fitted axis.
        d.follow(&axis, 100_000.0, 120);
        assert_eq!(d.changed().unwrap().loop_start, BeatTime(2879));
        let mut body = Drag::new(Part::Body, 100.0, region);
        body.follow(&axis, 100_000.0, 120);
        let moved = body.changed().unwrap();
        assert_eq!(moved.loop_end, BeatTime(7680));
        assert_eq!(moved.loop_end.0 - moved.loop_start.0, 1920);
    }

    #[test]
    fn a_zoomed_window_grabs_only_what_it_shows() {
        // Ticks 3900..5820 across 400 px from x 148: the loop (960..2880) lies left of it, and
        // the End flag (5830) just right of it.
        let mut b = brace();
        b.region.end = BeatTime(5830);
        b.axis = TimeAxis::windowed(
            148.0,
            400.0,
            BeatTime(7680),
            crate::control::time_axis::Window {
                start: 3900.0,
                visible: 1920.0,
            },
        );
        let (flag, row) = (
            (CLIP_FLAG_TOP + CLIP_FLAG_BOTTOM) / 2.0,
            (CLIP_BRACE_TOP + INSERT_STRIP_TOP) / 2.0,
        );
        // The header column, the value scale and the gutter take no grab, though the loop body's
        // x range (far left of the window) and the End flag's slop would reach them unclipped.
        for at in [
            (70.0, row),
            (147.0, row),
            (550.0, flag),
            (400.0 + 148.0 + 5.0, row),
        ] {
            assert_eq!(b.part_at(Point::new(at.0, at.1)), None, "{at:?}");
        }
        // The End flag centre is outside the window, so its slop inside the edge does not grab it.
        assert_eq!(b.part_at(Point::new(546.0, flag)), None);
        // What the window shows still grabs: the body across it and a flag inside it.
        assert_eq!(
            b.part_at(Point::new(300.0, row)),
            None,
            "no loop here: it ends at 2880"
        );
        b.region.loop_end = BeatTime(7000);
        assert_eq!(b.part_at(Point::new(300.0, row)), Some(Part::Body));
        b.region.end = BeatTime(5700);
        let end_x = b.axis.x(BeatTime(5700));
        assert_eq!(b.part_at(Point::new(end_x + 2.0, flag)), Some(Part::End));
    }
}
