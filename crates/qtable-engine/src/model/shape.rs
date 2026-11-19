//! Envelope shapes: the points of a waveform or ramp spread over a span of ticks.
//!
//! A shape is pure data. A client places it by writing the returned points into an envelope
//! (one `LaneEdit::ReplaceRange`), so curve maths stays here and clients stay free of it.
use super::{Axis, BeatTime, Breakpoint};

/// The shortest span, in ticks, that takes a shape (a sixty-fourth note).
pub const MIN_SHAPE_SPAN: u64 = 16;

/// One of Live's envelope shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shape {
    /// One cycle: from the middle, up to the top, down to the bottom and back to the middle.
    Sine,
    /// Low, high, low.
    Triangle,
    /// Falls from the top to the bottom.
    SawDown,
    /// Rises from the bottom to the top.
    SawUp,
    /// High for the first half, low for the second.
    Square,
    /// Starts at the bottom, rises fast and meets the value that follows the span.
    RampIn,
    /// Leaves the value that precedes the span and falls to the bottom.
    RampOut,
}

impl Shape {
    /// Every shape, in the order Live lists them.
    pub const ALL: [Shape; 7] = [
        Shape::Sine,
        Shape::Triangle,
        Shape::SawDown,
        Shape::SawUp,
        Shape::Square,
        Shape::RampIn,
        Shape::RampOut,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Shape::Sine => "Sine",
            Shape::Triangle => "Triangle",
            Shape::SawDown => "Saw Down",
            Shape::SawUp => "Saw Up",
            Shape::Square => "Square",
            Shape::RampIn => "Ramp In",
            Shape::RampOut => "Ramp Out",
        }
    }
}

/// The values an envelope holds either side of a span, which the two ramps link up to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    /// What the envelope arrives with at the span's start.
    pub before: f64,
    /// What the envelope leaves with at the span's end.
    pub after: f64,
}

/// A point's place in the span (0 to 1), its level (0 the argument's minimum, 1 its maximum)
/// and the curve of the segment leaving it.
type Spec = (f64, f64, f32);

/// The sine's seven points. A segment leaving an extreme starts flat, as a positive curve does;
/// one reaching an extreme can only be matched in two parts, so each quarter reaching an
/// extreme is split at 0.6 of the way. Fitted over curves in steps of 0.01: within 1% of the
/// range of a true sine.
const SINE: [Spec; 7] = [
    (0.0, 0.5, -0.05),
    (0.15, 0.904_508, -0.20),
    (0.25, 1.0, 0.20),
    (0.5, 0.5, -0.05),
    (0.65, 0.095_492, -0.20),
    (0.75, 0.0, 0.20),
    (1.0, 0.5, 0.0),
];

fn spec(shape: Shape) -> &'static [Spec] {
    match shape {
        Shape::Sine => &SINE,
        Shape::Triangle => &[(0.0, 0.0, 0.0), (0.5, 1.0, 0.0), (1.0, 0.0, 0.0)],
        Shape::SawDown => &[(0.0, 1.0, 0.0), (1.0, 0.0, 0.0)],
        Shape::SawUp => &[(0.0, 0.0, 0.0), (1.0, 1.0, 0.0)],
        Shape::Square => &[
            (0.0, 1.0, 0.0),
            (0.5, 1.0, 0.0),
            (0.5, 0.0, 0.0),
            (1.0, 0.0, 0.0),
        ],
        // The linked end gets its value from `Link` below.
        Shape::RampIn => &[(0.0, 0.0, -0.5), (1.0, 0.0, 0.0)],
        Shape::RampOut => &[(0.0, 0.0, 0.5), (1.0, 0.0, 0.0)],
    }
}

/// The points of `shape` over `span` ticks, from time 0 to `span` inclusive, on an argument
/// running `min` to `max`. Times round to the nearest tick. Whole-step arguments (`axis` not
/// `Continuous`) get whole values and keep the curves. Empty when `span` is under
/// [`MIN_SHAPE_SPAN`] or the range is not finite.
///
/// The ramps' linked ends take `link`; the other shapes ignore it. The shape's edge to the
/// envelope outside the span (a saw's vertical edge, say) is not a point of its own: the
/// client writing it joins it there.
pub fn shape_points(
    shape: Shape,
    span: u64,
    min: f64,
    max: f64,
    axis: Axis,
    link: Link,
) -> Vec<Breakpoint> {
    if span < MIN_SHAPE_SPAN || !min.is_finite() || !max.is_finite() {
        return Vec::new();
    }
    let whole = |value: f64| {
        let value = value.clamp(min.min(max), min.max(max));
        if axis == Axis::Continuous {
            value
        } else {
            value.round()
        }
    };
    let last = spec(shape).len() - 1;
    spec(shape)
        .iter()
        .enumerate()
        .map(|(i, &(phase, level, curve))| {
            let value = match (shape, i) {
                (Shape::RampIn, 1) => link.after,
                (Shape::RampOut, 0) => link.before,
                _ => min + level * (max - min),
            };
            Breakpoint {
                time: BeatTime((phase * span as f64).round() as u64),
                value: whole(value),
                // The span's last point has no segment of its own to curve.
                curve: if i == last { 0.0 } else { curve },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
