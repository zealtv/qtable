//! The one mapper between time and x on a timeline canvas: a window of ticks laid across a run
//! of pixels. The clip editor's canvases, ruler, loop brace and point gestures all ask it, so
//! none of them keeps its own pixel-to-tick formula. It knows no clip, so an Arrangement
//! ruler can use it later with a window of its own.
//!
//! A *fitted* axis shows the whole extent across the span, as every canvas did before
//! zooming existed; a *windowed* one shows `Window::visible` ticks from `Window::start`.
//! The axis only converts. Each caller still decides how to clamp and snap what it gets
//! back: a hit press snaps inside a half-open clip, a point inside a closed one, a drag to
//! the extent.
use qtable_engine::model::BeatTime;

/// The narrowest a zoomed window may get: a sixteenth note at 960 ticks a quarter.
pub(crate) const MIN_VISIBLE: f64 = 240.0;

/// A clip rectangle's height when only its columns matter.
const CLIP_TALL: f32 = 16_384.0;

/// A ruler drag of this many px zooms by one doubling (or halving).
pub(crate) const ZOOM_OCTAVE_PX: f64 = 50.0;

/// Which way a vertical ruler drag zooms. Live zooms in as the pointer moves down and out as
/// it moves up (observed 2026-11-17, Live 12.1); flip this one sign if that ever changes.
pub(crate) const DRAG_DOWN_ZOOMS_IN: f64 = 1.0;

/// A wheel notch counts as this many px of travel, for a device that reports lines.
pub(crate) const WHEEL_LINE_PX: f64 = 40.0;

/// A held-⌘ wheel travel of this many px zooms by one doubling.
pub(crate) const WHEEL_OCTAVE_PX: f64 = 200.0;

/// A zoomed view of content: `visible` ticks from `start`. A clip with no window is fitted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Window {
    pub start: f64,
    pub visible: f64,
}

impl Window {
    /// `window`, or the whole extent when there is none.
    pub(crate) fn resolve(window: Option<Self>, extent: BeatTime) -> Self {
        window.map_or_else(
            || Self {
                start: 0.0,
                visible: extent.ticks() as f64,
            },
            |w| w.clamped(extent),
        )
    }

    /// This window held to `extent`, or `None` once it shows the whole of it: zooming all the
    /// way out returns to the fitted view, which follows the content as it grows.
    pub(crate) fn normalised(self, extent: BeatTime) -> Option<Self> {
        let held = self.clamped(extent);
        (held.start > 0.0 || held.visible < extent.ticks() as f64).then_some(held)
    }

    /// Zoomed by `factor` (above 1 narrows it) about the tick `fraction` of the way across,
    /// which keeps its place on screen.
    pub(crate) fn zoom_about(
        window: Option<Self>,
        extent: BeatTime,
        fraction: f64,
        factor: f64,
    ) -> Option<Self> {
        let now = Self::resolve(window, extent);
        let fraction = fraction.clamp(0.0, 1.0);
        let anchor = now.start + fraction * now.visible;
        let visible = now.visible / factor;
        Self {
            start: anchor - fraction * visible,
            visible,
        }
        .normalised(extent)
    }

    /// Moved by `ticks` (positive moves later), held inside the content.
    pub(crate) fn scroll(window: Option<Self>, extent: BeatTime, ticks: f64) -> Option<Self> {
        let now = Self::resolve(window, extent);
        Self {
            start: now.start + ticks,
            ..now
        }
        .normalised(extent)
    }

    /// The window that shows exactly `range`, within the limits. A range narrower than the
    /// limit is centred in a window of that width. An empty range gives `None`.
    pub(crate) fn to_range(extent: BeatTime, range: &std::ops::Range<BeatTime>) -> Option<Self> {
        if range.is_empty() {
            return None;
        }
        let (start, end) = (range.start.ticks() as f64, range.end.ticks() as f64);
        let visible = (end - start).max(MIN_VISIBLE);
        Self {
            start: (start + end) / 2.0 - visible / 2.0,
            visible,
        }
        .normalised(extent)
    }

    /// The window held to `extent`: never wider than the whole content, never narrower than
    /// `MIN_VISIBLE` ticks (or the extent, if that is shorter), and wholly inside it.
    pub(crate) fn clamped(self, extent: BeatTime) -> Self {
        let whole = extent.ticks() as f64;
        let visible = if self.visible.is_finite() {
            self.visible.clamp(MIN_VISIBLE.min(whole), whole)
        } else {
            whole
        };
        let start = if self.start.is_finite() {
            self.start.clamp(0.0, whole - visible)
        } else {
            0.0
        };
        Self { start, visible }
    }
}

/// A ruler drag in progress: the window it began on and the time under the press.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Grab {
    window: Window,
    /// The tick under the press.
    anchor: f64,
    /// Where across the window the press fell, from 0 (left edge) to 1.
    fraction: f64,
}

impl Grab {
    /// A press at `fraction` of the way across `window` (`None` is fitted).
    pub(crate) fn new(window: Option<Window>, extent: BeatTime, fraction: f64) -> Self {
        let window = Window::resolve(window, extent);
        let fraction = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self {
            window,
            anchor: window.start + fraction * window.visible,
            fraction,
        }
    }

    /// The window after the pointer has moved `dx` px sideways and `dy` px down since the
    /// press, across a time area `span` px wide. The pressed time stays under the pointer, so
    /// moving down zooms in around it (a doubling per `ZOOM_OCTAVE_PX`) and moving sideways
    /// scrolls. `None` is the whole clip.
    pub(crate) fn at(&self, extent: BeatTime, span: f32, dx: f32, dy: f32) -> Option<Window> {
        let octaves = DRAG_DOWN_ZOOMS_IN * f64::from(dy) / ZOOM_OCTAVE_PX;
        let visible = self.window.visible / octaves.clamp(-60.0, 60.0).exp2();
        let fraction = self.fraction + f64::from(dx) / f64::from(span.max(1.0));
        Window {
            start: self.anchor - fraction * visible,
            visible,
        }
        .normalised(extent)
    }
}

/// Where time sits across a run of pixels: `visible` ticks from `start` over `span` px from
/// `left`, within content that runs `extent` ticks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TimeAxis {
    /// x of the window's left edge.
    pub left: f32,
    /// Its width in px.
    pub span: f32,
    /// Content ticks at `left`; 0 when fitted.
    pub start: f64,
    /// Ticks across `span`; the extent when fitted.
    pub visible: f64,
    /// The content's extent: what snapping and drags clamp to.
    pub extent: BeatTime,
}

impl TimeAxis {
    /// The whole extent across `span` px from `left`.
    pub(crate) fn fitted(left: f32, span: f32, extent: BeatTime) -> Self {
        Self {
            left,
            span,
            start: 0.0,
            visible: extent.ticks() as f64,
            extent,
        }
    }

    /// `window` (held to `extent`) across `span` px from `left`.
    pub(crate) fn windowed(left: f32, span: f32, extent: BeatTime, window: Window) -> Self {
        let window = window.clamped(extent);
        Self {
            left,
            span,
            start: window.start,
            visible: window.visible,
            extent,
        }
    }

    /// `window` where there is one, else fitted: the axis a canvas builds from its clip's zoom.
    pub(crate) fn zoomed(left: f32, span: f32, extent: BeatTime, window: Option<Window>) -> Self {
        window.map_or_else(
            || Self::fitted(left, span, extent),
            |window| Self::windowed(left, span, extent, window),
        )
    }

    /// Paints `draw` clipped to the time area's columns, from the window's left edge to its
    /// right, so content left of a zoomed window cannot cover the header column or the value
    /// scale. A fitted axis is not clipped: its marks overhang the area's edge on purpose (a
    /// hit at tick 0, the Start flag), and its pixels stay as they were.
    ///
    /// Use it only at the top level of a frame that has drawn nothing of its own, and never
    /// nest it: iced lays a clipped region under everything its frame drew directly, in any
    /// order of calls, and above everything drawn in an earlier frame.
    pub(crate) fn within(
        &self,
        frame: &mut iced::widget::canvas::Frame,
        draw: impl FnOnce(&mut iced::widget::canvas::Frame),
    ) {
        if self.is_windowed() {
            frame.with_clip(
                iced::Rectangle::new(
                    iced::Point::new(self.left - 1.0, 0.0),
                    iced::Size::new(self.span + 2.0, CLIP_TALL),
                ),
                draw,
            );
        } else {
            draw(frame);
        }
    }

    /// Whether the window shows less than the whole extent.
    pub(crate) fn is_windowed(&self) -> bool {
        self.start > 0.0 || self.visible < self.extent.ticks() as f64
    }

    /// Whether a mark at `position` with `reach` px either side of its x could show: always on
    /// a fitted axis (which draws everything, as it always has), and only inside the window
    /// plus `reach` on a zoomed one, so drawing follows what the window shows.
    pub(crate) fn shows(&self, position: BeatTime, reach: f32) -> bool {
        !self.is_windowed() || {
            let x = self.x(position);
            x >= self.left - reach && x <= self.left + self.span + reach
        }
    }

    /// x of a time, unclamped: left of `left` before the window, right of its end after.
    pub(crate) fn x(&self, position: BeatTime) -> f32 {
        self.x_at(position.ticks() as f64)
    }

    /// x of a time given in (possibly fractional) ticks.
    pub(crate) fn x_at(&self, ticks: f64) -> f32 {
        self.left + ((ticks - self.start) / self.visible.max(1.0) * f64::from(self.span)) as f32
    }

    /// The tick under `x`, not clamped to anything: negative before the content, past the
    /// extent after it. Callers apply their own clamp and snap rule to this.
    pub(crate) fn unclamped_ticks_at(&self, x: f32) -> f64 {
        self.start + f64::from(x - self.left) / f64::from(self.span) * self.visible
    }

    /// The tick under `x`, held to the content `[0, extent]`.
    pub(crate) fn ticks_at(&self, x: f32) -> f64 {
        self.unclamped_ticks_at(x)
            .clamp(0.0, self.extent.ticks() as f64)
    }

    /// Ticks across one pixel.
    pub(crate) fn ticks_per_px(&self) -> f64 {
        self.visible / f64::from(self.span.max(1.0))
    }

    /// The ticks a pointer travelling `dx` px covers (the same ratio as `ticks_per_px`,
    /// multiplied last so a fitted drag rounds as it always did).
    pub(crate) fn delta_ticks(&self, dx: f32) -> f64 {
        f64::from(dx) / f64::from(self.span) * self.visible
    }

    /// The window's ticks, `[start, end]`: what drawing and sampling keep to.
    pub(crate) fn visible_range(&self) -> (f64, f64) {
        (self.start, self.start + self.visible)
    }
}

#[cfg(test)]
mod tests;
