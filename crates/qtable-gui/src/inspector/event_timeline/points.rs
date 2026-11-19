//! The glue between the canvas and `point_gesture`: an envelope row's plot, the keys held,
//! and the readout text. The gestures themselves live in the pure module so the one-lane
//! view reuses them unchanged.
use super::*;
use crate::inspector::point_gesture::{Keys, Lane, Plot, Readout};

/// Space above and below the curve's extremes within a lane.
pub(super) const CURVE_PAD: f32 = 6.0;

impl Timeline {
    pub(super) fn keys(&self) -> Keys {
        Keys {
            alt: self.context.alt_down,
            shift: self.context.shift_down,
            command: self.context.command_down,
        }
    }

    /// The plot of an envelope row, in canvas coordinates.
    pub(super) fn plot(&self, row: usize, layout: &Layout) -> Plot {
        let lane = &self.model.rows[row];
        Plot {
            left: LABEL_WIDTH,
            width: layout.span(),
            top: RULER_HEIGHT + lane.top + CURVE_PAD,
            height: lane.height - 2.0 * CURVE_PAD,
        }
    }

    /// An envelope row as the gestures see it; `None` for an event row.
    pub(super) fn lane(&self, row: usize, layout: &Layout) -> Option<Lane> {
        let envelope = self.envelope(row)?;
        Some(
            envelope
                .lane(
                    self.plot(row, layout),
                    crate::inspector::clip_markers::extent(&self.clip, self.signature),
                    self.grid.ticks(),
                )
                .with_time(layout.axis)
                .with_default(self.live.get(row).copied().flatten().map(|l| l.value)),
        )
    }

    /// What a gesture's start is checked against when it releases.
    pub(super) fn point_target(&self, row: usize) -> Option<PointTarget> {
        Some(PointTarget {
            key: self.key,
            revision: self.context.revision,
            target: self.envelope(row)?.target.clone(),
        })
    }

    pub(super) fn readout(&self, row: usize, readout: Readout) -> Option<String> {
        Some(match readout {
            Readout::Value(value) => self.envelope(row)?.value_text(value),
            Readout::Curve(curve) => format!("Curve {curve:.2}"),
        })
    }
}
