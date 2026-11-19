//! Live-shaped normalized volume fader; its well also shows OSC activity.
//! Neutral colours and dimensions: a read-only capture of Live 12, 2026-10-13.
pub(super) mod host;

use super::{ControlKind, Interaction, Look, SizeRole, State, Theme};
use iced::widget::canvas::{Frame, Path, Stroke};
use iced::{Point, Rectangle, Size};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Fader {
    pub value: f64,
    pub level: f32,
}
/// Actual bounds can be shorter than the nominal 96 px (the mixer row's CONTROLS_H) inside the existing mixer row.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geometry {
    pub well: Rectangle,
    pub top: f32,
    pub bottom: f32,
}
impl Geometry {
    pub fn new(bounds: Rectangle) -> Self {
        let margin = (bounds.height / 2.0).min(5.5);
        let top = bounds.y + margin;
        let bottom = (bounds.y + bounds.height - margin).max(top);
        Self {
            well: Rectangle::new(
                // The handle extends 10 px left of the 13 px well. Centre that
                // whole footprint in the canvas that also owns the focus border.
                Point::new(bounds.center_x() - 1.5, top),
                Size::new(13.0, (bottom - top).max(0.0)),
            ),
            top,
            bottom,
        }
    }
    pub fn handle_y(self, value: f64) -> f32 {
        self.bottom - unit(value) as f32 * (self.bottom - self.top)
    }
    pub fn tick(self, index: usize) -> (f32, f32) {
        (
            self.handle_y(index as f64 / 10.0),
            if index.is_multiple_of(5) { 4.0 } else { 2.0 },
        )
    }
}
/// Centre a 1 pt line on a pixel row, so ticks and the level mark stay crisp.
fn snap(y: f32) -> f32 {
    y.floor() + 0.5
}
fn unit(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}
impl ControlKind for Fader {
    fn look(&self, theme: &Theme, state: State, _: Interaction) -> Look {
        Look {
            fill: theme.fader_well,
            ink: if state.enabled {
                theme.fader_handle
            } else {
                theme.text_disabled
            },
            frame: theme.fader_edge,
            accent: theme.orange_vu_meter,
        }
    }
    fn role(&self) -> SizeRole {
        SizeRole::Fixed {
            width: 27.0,
            height: 96.0,
        }
    }
    fn paint(&self, frame: &mut Frame, bounds: Rectangle, look: &Look) {
        self.paint_with(frame, bounds, look, &super::LIVE_DARK);
    }
}
impl Fader {
    /// Paint with `theme`'s tick and mark colours; the trait's `paint` has no theme to read.
    pub(crate) fn paint_with(
        &self,
        frame: &mut Frame,
        bounds: Rectangle,
        look: &Look,
        theme: &Theme,
    ) {
        let geometry = Geometry::new(bounds);
        let well = geometry.well;
        let path = Path::rounded_rectangle(well.position(), well.size(), theme.corner_cell.into());
        frame.fill(&path, look.fill);
        frame.stroke(
            &path,
            Stroke::default().with_color(look.frame).with_width(1.0),
        );
        let level = if self.level.is_finite() {
            self.level.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let inner = Rectangle::new(
            Point::new(well.x + 1.0, well.y + 1.0),
            Size::new((well.width - 2.0).max(0.0), (well.height - 2.0).max(0.0)),
        );
        let height = inner.height * level;
        if height > 0.0 {
            frame.fill_rectangle(
                Point::new(inner.x, inner.y + inner.height - height),
                Size::new(inner.width, height),
                look.accent,
            );
        }
        let line = |frame: &mut Frame, a, b, colour, width| {
            frame.stroke(
                &Path::line(a, b),
                Stroke::default().with_color(colour).with_width(width),
            )
        };
        let centre = well.center_x();
        line(
            frame,
            Point::new(centre, well.y),
            Point::new(centre, well.y + well.height),
            theme.fader_centre,
            0.5,
        );
        for i in 0..=10 {
            let (y, length) = geometry.tick(i);
            let y = snap(y);
            let colour = if i.is_multiple_of(5) {
                theme.fader_tick_major
            } else {
                theme.fader_tick_minor
            };
            line(
                frame,
                Point::new(well.x - 1.0 - length, y),
                Point::new(well.x - 1.0, y),
                colour,
                1.0,
            );
        }
        let y = geometry.handle_y(self.value);
        let mark_y = snap(y);
        let mark = theme.fader_mark;
        for (left, right) in [
            (well.x + 1.0, centre - 0.5),
            (centre + 0.5, well.x + well.width - 1.0),
        ] {
            line(
                frame,
                Point::new(left, mark_y),
                Point::new(right, mark_y),
                mark,
                1.0,
            );
        }
        let handle = Path::new(|p| {
            p.move_to(Point::new(well.x - 10.0, y - 5.5));
            p.line_to(Point::new(well.x, y));
            p.line_to(Point::new(well.x - 10.0, y + 5.5));
            p.close();
        });
        frame.fill(&handle, look.ink);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use iced::Color;
    #[test]
    fn focus_bounds_centre_the_handle_and_well_at_every_volume() {
        for bounds in [
            Rectangle::with_size(Size::new(27., 96.)),
            Rectangle::new(Point::new(50., 20.), Size::new(40., 104.)),
        ] {
            let g = Geometry::new(bounds);
            let left_gap = g.well.x - 10.0 - bounds.x;
            let right_gap = bounds.x + bounds.width - (g.well.x + g.well.width);
            assert_eq!(left_gap, right_gap);
            assert!(left_gap > 0.0);
            for value in [0., 0.5, 1.] {
                let y = g.handle_y(value);
                assert!(y - 5.5 >= bounds.y);
                assert!(y + 5.5 <= bounds.y + bounds.height);
            }
            assert_eq!(g.top - bounds.y, bounds.y + bounds.height - g.bottom);
        }
    }
    #[test]
    fn handle_and_ticks_cover_normalized_range_without_db_curve() {
        let g = Geometry::new(Rectangle::with_size(Size::new(27., 104.)));
        assert_eq!(g.handle_y(1.), g.top);
        assert_eq!(g.handle_y(0.), g.bottom);
        assert_eq!(g.handle_y(0.5), 52.);
        for i in 0..=10 {
            let (y, length) = g.tick(i);
            assert!((y - (g.bottom - i as f32 * 9.3)).abs() < 0.001);
            assert_eq!(length, if i % 5 == 0 { 4. } else { 2. });
        }
        assert_eq!(g.handle_y(2.), g.top);
        assert_eq!(g.handle_y(-1.), g.bottom);
        assert!(g.handle_y(f64::NAN).is_finite());
    }
    #[test]
    fn painting_uses_the_measured_well_handle_edge_and_activity_colours() {
        let f = Fader {
            value: 0.5,
            level: 0.4,
        };
        let look = f.look(
            &super::super::LIVE_DARK,
            State::default(),
            Interaction::Idle,
        );
        assert_eq!(look.fill, Color::from_rgb8(15, 15, 15));
        assert_eq!(look.ink, Color::from_rgb8(216, 216, 216));
        assert_eq!(look.frame, Color::from_rgb8(34, 34, 34));
        assert_eq!(look.accent, super::super::LIVE_DARK.orange_vu_meter);
        let renderer = crate::grid::test_renderer();
        for size in [Size::new(27., 104.), Size::new(27., 96.)] {
            let mut frame = Frame::new(&renderer, size);
            f.paint(&mut frame, Rectangle::with_size(size), &look);
            let _ = frame.into_geometry();
            let g = Geometry::new(Rectangle::with_size(size));
            assert!(g.top >= 0. && g.bottom <= size.height);
        }
    }
}
