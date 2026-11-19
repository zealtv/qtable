//! Canvas-local button feedback; commands still fire on the initial left press.
use super::{
    CELL_H, FOOTER_H, GAP, Grid, Hit, Master, hit_test_layout, master_hit_test, status_row,
};
use iced::widget::canvas::{self, Frame};
use iced::{Point, Rectangle, Size, mouse};

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub(super) struct Feedback {
    hover: Option<Hit>,
    pressed: Option<Hit>,
    revision: u64,
}

impl Feedback {
    pub fn update(&mut self, event: &canvas::Event, hit: Option<Hit>, revision: u64) -> bool {
        let before = *self;
        if self.revision != revision {
            *self = Self {
                revision,
                ..Self::default()
            };
        }
        self.hover = hit;
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                self.pressed = hit
            }
            canvas::Event::Mouse(
                mouse::Event::ButtonReleased(mouse::Button::Left) | mouse::Event::CursorLeft,
            )
            | canvas::Event::Window(iced::window::Event::Unfocused) => self.pressed = None,
            _ => {}
        }
        *self != before
    }

    fn visible(self, revision: u64) -> Option<(Hit, bool)> {
        (self.revision == revision).then_some(())?;
        let hit = self.hover?;
        Some((hit, self.pressed == Some(hit)))
    }
}

impl Grid<'_> {
    pub(super) fn button_hit(&self, point: Point) -> Option<Hit> {
        if self.status_row {
            return status_row::track_hit(point, &self.layout.columns);
        }
        let hit = hit_test_layout(point, &self.layout.columns, self.scenes.len())?;
        let (Hit::ClipLaunch(_, slot) | Hit::GroupLaunch(_, slot)) = hit else {
            return None;
        };
        let (_, members) = self.hit_column(hit)?;
        let live = |ti: &usize| {
            self.cells
                .get(*ti)
                .and_then(|cells| cells.get(slot))
                .is_some_and(|cell| cell.clip.is_some() || cell.stop_button)
        };
        members.iter().any(live).then_some(hit)
    }

    /// The column a launch or stop hit's square sits in, and the tracks it summarises.
    fn hit_column(&self, hit: Hit) -> Option<(usize, &[usize])> {
        let cols = &self.layout.columns;
        let ci = match hit {
            Hit::ClipLaunch(t, _) | Hit::TrackStop(t) => {
                cols.iter().position(|c| c.track() == Some(t))
            }
            Hit::GroupLaunch(gi, _) | Hit::GroupStop(gi) => {
                cols.iter().position(|c| c.group_track() == Some(gi))
            }
            _ => None,
        }?;
        Some((ci, cols[ci].leaves.as_slice()))
    }
}

impl Master<'_> {
    pub(super) fn button_hit(&self, point: Point) -> Option<Hit> {
        if self.status_row {
            return status_row::master_hit(point);
        }
        master_hit_test(point, self.scenes.len(), self.header_h)
            .filter(|hit| matches!(hit, Hit::SceneLaunch(_)))
    }
}

pub(super) fn interaction(hit: Option<Hit>) -> mouse::Interaction {
    if hit.is_some() {
        mouse::Interaction::Pointer
    } else {
        mouse::Interaction::default()
    }
}

fn highlight(frame: &mut Frame, area: Rectangle, triangle: bool, pressed: bool) {
    // Preserve the underlying clip/playback colour while lifting the button's contrast.
    let wash = if pressed {
        crate::control::LIVE_DARK.press_wash_pressed
    } else {
        crate::control::LIVE_DARK.press_wash
    };
    super::draw::rect(
        frame,
        Rectangle::new(
            Point::new(area.x + 1.0, area.y + 1.0),
            Size::new(area.width - 2.0, area.height - 2.0),
        ),
        wash,
    );
    let ink = if pressed {
        crate::control::LIVE_DARK.selection_background
    } else {
        crate::control::LIVE_DARK.clip_text_light
    };
    if triangle {
        super::draw::triangle(frame, area, ink);
    } else {
        super::draw::rect(
            frame,
            Rectangle::new(
                Point::new(area.center_x() - 3.5, area.center_y() - 3.5),
                Size::new(7.0, 7.0),
            ),
            ink,
        );
    }
}

pub(super) fn grid(grid: &Grid<'_>, feedback: Feedback, frame: &mut Frame) {
    let Some((hit, pressed)) = feedback.visible(grid.revision) else {
        return;
    };
    let slot = match hit {
        Hit::ClipLaunch(_, s) | Hit::GroupLaunch(_, s) => Some(s),
        Hit::TrackStop(_) | Hit::GroupStop(_) => None,
        _ => return,
    };
    let Some((ci, members)) = grid.hit_column(hit) else {
        return;
    };
    let triangle = slot.is_some_and(|s| members.iter().any(|&t| grid.cells[t][s].clip.is_some()));
    let y = slot.map_or(GAP, |s| grid.layout.header_h + s as f32 * (CELL_H + GAP));
    let height = if slot.is_some() {
        CELL_H
    } else {
        FOOTER_H - GAP
    };
    highlight(
        frame,
        Rectangle::new(Point::new(grid.layout.xs[ci], y), Size::new(CELL_H, height)),
        triangle,
        pressed,
    );
}

pub(super) fn master(master: &Master<'_>, feedback: Feedback, frame: &mut Frame) {
    let Some((hit, pressed)) = feedback.visible(master.revision) else {
        return;
    };
    let (y, height, triangle) = match hit {
        Hit::SceneLaunch(s) => (master.header_h + s as f32 * (CELL_H + GAP), CELL_H, true),
        Hit::StopAll => (GAP, FOOTER_H - GAP, false),
        _ => return,
    };
    // Follow Actions is a latched on/off control with no hover/press overlay.
    // Its striped playhead icon remains visible; focus is handled by the shared UI language.
    highlight(
        frame,
        Rectangle::new(Point::new(0.0, y), Size::new(CELL_H, height)),
        triangle,
        pressed,
    );
}

#[cfg(test)]
mod tests;
