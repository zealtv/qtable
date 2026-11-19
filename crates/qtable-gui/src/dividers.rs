//! Draggable dividers between window regions, as in Live: the Browser's width and the
//! Detail View's height. Sizes belong to the GUI session, not the set.
use crate::messages::Message;
use crate::style::detail::DEFAULT_BROWSER_WIDTH;
use iced::widget::{container, mouse_area};
use iced::{Element, Length, Point, Size, mouse};

pub(crate) const THICKNESS: f32 = 4.0;
const MIN_BROWSER_WIDTH: f32 = 120.0;
/// The Detail View's default share of the column below the Control Bar: grid 3, detail 2.
const DEFAULT_DETAIL_SHARE: f32 = 0.4;
const MIN_DETAIL_HEIGHT: f32 = 120.0;
/// Room the grid keeps beside the Browser and above the Detail View.
const MIN_GRID_WIDTH: f32 = 360.0;
/// Header row, three slot rows, status row and Mixer (`grid::band::least_height`).
const MIN_GRID_HEIGHT: f32 = crate::grid::band::least_height();
/// Height taken by the Control Bar, Status bar and the gaps between regions.
const WINDOW_CHROME_HEIGHT: f32 = 90.0;
/// The shortest window that still shows the grid's minimum above the Detail View's, so nothing
/// overlaps the track header row or the first slot rows.
pub(crate) const MIN_WINDOW_HEIGHT: f32 =
    WINDOW_CHROME_HEIGHT + MIN_GRID_HEIGHT + MIN_DETAIL_HEIGHT;
/// The window size iced opens with, assumed until the first resize event.
const DEFAULT_WINDOW: Size = Size::new(1024.0, 768.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Divider {
    /// Between the Browser and the grid; drags horizontally.
    Browser,
    /// Between the grid and the Detail View; drags vertically.
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Drag {
    divider: Divider,
    start_cursor: Point,
    start_size: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Sizes {
    browser_width: f32,
    /// `None` until dragged: the Detail View then takes its default share of the column.
    detail_height: Option<f32>,
    window: Size,
    drag: Option<Drag>,
}

impl Default for Sizes {
    fn default() -> Self {
        Self {
            browser_width: DEFAULT_BROWSER_WIDTH,
            detail_height: None,
            window: DEFAULT_WINDOW,
            drag: None,
        }
    }
}

impl Sizes {
    /// Startup-only capture layout: reveal the full argument row without desktop input.
    #[cfg(any(test, feature = "dev-hooks"))]
    pub(crate) fn capture_hit_arguments(&mut self) {
        self.detail_height = Some(360.0);
    }

    /// Startup-only capture layout: a Browser `width` px wide (clamped like a drag), to check
    /// how the Browser's contents sit in it.
    #[cfg(any(test, feature = "dev-hooks"))]
    pub(crate) fn capture_browser_width(&mut self, width: f32) {
        self.browser_width = width;
    }

    /// Room for the clip editor's lanes: seven lanes and the strip beneath.
    #[cfg(any(test, feature = "dev-hooks"))]
    pub(crate) fn capture_lanes(&mut self) {
        self.detail_height = Some(560.0);
    }

    /// The Browser width in px, kept clear of the grid's minimum at the current window size.
    pub(crate) fn browser_width(&self) -> f32 {
        self.browser_width
            .clamp(MIN_BROWSER_WIDTH, self.max(Divider::Browser))
    }

    /// The Detail View height in px once dragged, kept clear of the grid's minimum at the
    /// current window size; `None` while it still takes its default share (2 of 5 parts).
    pub(crate) fn detail_height(&self) -> Option<f32> {
        self.detail_height
            .map(|height| height.clamp(MIN_DETAIL_HEIGHT, self.max(Divider::Detail)))
    }

    /// Device mode has one height; the remembered Clip/Scene size is left untouched.
    pub(crate) fn shown_detail_height(&self, devices: bool) -> Option<f32> {
        if devices {
            Some(
                crate::style::detail::DEVICE_DETAIL_HEIGHT
                    .min((self.window.height - WINDOW_CHROME_HEIGHT - MIN_GRID_HEIGHT).max(0.0)),
            )
        } else {
            // Short of room, even the default share must leave the grid its minimum height.
            self.detail_height().or_else(|| {
                let grid_share =
                    (self.window.height - WINDOW_CHROME_HEIGHT) * (1.0 - DEFAULT_DETAIL_SHARE);
                (grid_share < MIN_GRID_HEIGHT).then(|| self.max(Divider::Detail))
            })
        }
    }

    pub(crate) fn lock_detail(&mut self) {
        if self
            .drag
            .is_some_and(|drag| drag.divider == Divider::Detail)
        {
            self.end();
        }
    }

    /// Estimate of the default share in px, where a first drag starts from.
    fn default_detail_height(&self) -> f32 {
        (self.window.height - WINDOW_CHROME_HEIGHT) * DEFAULT_DETAIL_SHARE
    }

    pub(crate) fn resize_window(&mut self, window: Size) {
        self.window = window;
    }

    /// Start dragging from the cursor's current position.
    pub(crate) fn begin(&mut self, divider: Divider, cursor: Point) {
        let start_size = match divider {
            Divider::Browser => self.browser_width(),
            Divider::Detail => self
                .detail_height()
                .unwrap_or_else(|| self.default_detail_height().max(MIN_DETAIL_HEIGHT)),
        };
        self.drag = Some(Drag {
            divider,
            start_cursor: cursor,
            start_size,
        });
    }

    /// Follow the cursor while a drag runs; the Detail View grows as the cursor moves up.
    pub(crate) fn drag_to(&mut self, cursor: Point) {
        let Some(drag) = self.drag else { return };
        match drag.divider {
            Divider::Browser => {
                let width = drag.start_size + cursor.x - drag.start_cursor.x;
                self.browser_width = width.clamp(MIN_BROWSER_WIDTH, self.max(Divider::Browser));
            }
            Divider::Detail => {
                let height = drag.start_size - (cursor.y - drag.start_cursor.y);
                self.detail_height =
                    Some(height.clamp(MIN_DETAIL_HEIGHT, self.max(Divider::Detail)));
            }
        }
    }

    pub(crate) fn end(&mut self) {
        self.drag = None;
    }

    /// Largest size for the divider; never below its minimum, so a tiny window overflows
    /// rather than inverting the clamp range.
    fn max(&self, divider: Divider) -> f32 {
        match divider {
            Divider::Browser => (self.window.width - MIN_GRID_WIDTH).max(MIN_BROWSER_WIDTH),
            Divider::Detail => {
                (self.window.height - WINDOW_CHROME_HEIGHT - MIN_GRID_HEIGHT).max(MIN_DETAIL_HEIGHT)
            }
        }
    }
}

/// The strip between two regions that starts a drag when pressed.
pub(crate) fn handle<'a>(divider: Divider, draggable: bool) -> Element<'a, Message> {
    let (width, height, interaction) = match divider {
        Divider::Browser => (
            Length::Fixed(THICKNESS),
            Length::Fill,
            mouse::Interaction::ResizingHorizontally,
        ),
        Divider::Detail => (
            Length::Fill,
            Length::Fixed(THICKNESS),
            mouse::Interaction::ResizingVertically,
        ),
    };
    let strip = container("")
        .width(width)
        .height(height)
        .style(|_| container::Style {
            background: Some(crate::control::LIVE_DARK.divider.into()),
            ..Default::default()
        });
    if draggable {
        mouse_area(strip)
            .on_press(Message::BeginDrag(divider))
            .interaction(interaction)
            .into()
    } else {
        strip.into()
    }
}

#[cfg(test)]
mod tests;
