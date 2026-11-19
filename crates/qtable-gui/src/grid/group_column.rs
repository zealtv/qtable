//! A group track's column, after Live 12 (screenshots, 2026-10-12): a title in the group's colour with a fold
//! button, and group slots that summarise the members' clips in each row.
use super::draw::{
    BUTTON, DISABLED, GREEN, LAUNCH_ACTIVE, ROW_TINT, SLOT_BODY, colour, outline, rect,
    stop_button, triangle,
};
use super::{CELL_H, COLUMN_TINT, CellStatus, Column, Grid, SELECTION_OUTLINE, THIN_OUTLINE};
use crate::control::theme::DARK_INK;
use iced::widget::canvas::{Frame, Path, Stroke};
use iced::{Color, Point, Rectangle, Size};

/// The expanded group slot's striped tile, at the slot's right end.
const TILE_W: f32 = 14.0;
const STRIPE_PITCH: f32 = 5.0;
const STRIPE_W: f32 = 2.5;

/// What one group slot summarises from its members' slots in that row.
pub(super) struct Summary {
    /// The colour the stripes take: a playing member clip's, else the first member clip's.
    pub stripes: Option<Color>,
    pub playing: bool,
    pub triggered: bool,
    pub stop_button: bool,
    pub follow_pulse: Option<Color>,
    pub selected: bool,
}

pub(super) fn summary(g: &Grid<'_>, col: &Column, row: usize) -> Summary {
    let cells = || {
        col.leaves
            .iter()
            .filter_map(|&ti| g.cells.get(ti)?.get(row))
    };
    let playing_clip = cells()
        .find(|c| matches!(c.status, CellStatus::Playing(_)))
        .and_then(|c| c.clip);
    let first_clip = cells().find_map(|c| c.clip);
    Summary {
        stripes: playing_clip.or(first_clip).map(|clip| colour(clip.colour)),
        playing: playing_clip.is_some(),
        triggered: cells().any(|c| c.status == CellStatus::Triggered),
        stop_button: cells().any(|c| c.stop_button),
        follow_pulse: cells().find_map(|c| super::draw::pulsing_icon(c, g.blink_on)),
        selected: cells().any(|c| c.selected),
    }
}

/// The group slots, redrawn with playback: their launch icons and stripes follow the members.
pub(super) fn slots(frame: &mut Frame, g: &Grid<'_>, col: &Column, x: f32, folded: bool) {
    let selected_row =
        super::scene_area::selected_row(g.selected, g.selected_scene, g.cell, g.scenes);
    let column_selected = g.column_selected(col);
    for row in 0..g.scenes.len() {
        let area = Rectangle::new(
            Point::new(x, g.layout.header_h + row as f32 * (CELL_H + super::GAP)),
            Size::new(col.width, CELL_H),
        );
        let launch = Rectangle::new(area.position(), Size::new(CELL_H, CELL_H));
        let s = summary(g, col, row);
        rect(frame, area, SLOT_BODY);
        if let Some(stripe) = s.stripes {
            let tile = if folded {
                Rectangle::new(
                    Point::new(x + CELL_H + 2.0, area.y + 2.0),
                    Size::new(col.width - CELL_H - 4.0, CELL_H - 4.0),
                )
            } else {
                Rectangle::new(
                    Point::new(x + col.width - TILE_W - 2.0, area.y + 2.0),
                    Size::new(TILE_W, CELL_H - 4.0),
                )
            };
            stripes(frame, tile, stripe);
            let icon = match (s.follow_pulse, s.playing, s.triggered) {
                (Some(pulse), _, _) => Some(pulse),
                (None, true, _) => Some(GREEN),
                (None, false, true) => Some(if g.blink_on { GREEN } else { DARK_INK }),
                (None, false, false) => None,
            };
            if let Some(icon) = icon {
                rect(frame, launch, LAUNCH_ACTIVE);
                triangle(frame, launch, icon);
            } else {
                triangle(frame, launch, DARK_INK);
            }
        } else if s.stop_button {
            stop_button(frame, launch, BUTTON, DISABLED);
        }
        if selected_row == Some(row) {
            rect(frame, area, ROW_TINT);
        }
        if column_selected {
            rect(frame, area, COLUMN_TINT);
        }
        if let Some(cell) = g.cell
            && cell.owner == col.owner()
            && cell.row == row
        {
            let colour = if cell.active {
                crate::control::LIVE_DARK.selection_background
            } else {
                super::draw::SELECTION_INACTIVE
            };
            outline(
                frame,
                area,
                colour,
                if cell.active {
                    SELECTION_OUTLINE
                } else {
                    THIN_OUTLINE
                },
            );
        }
        if s.stripes.is_none()
            && s.stop_button
            && let Some(on) = g.stop_feedback(&col.leaves, row)
        {
            stop_button(frame, launch, BUTTON, if on { GREEN } else { DARK_INK });
        }
        // Folded groups also show selected member clips.
        if s.selected && folded {
            outline(
                frame,
                area,
                crate::control::LIVE_DARK.selection_background,
                SELECTION_OUTLINE,
            );
        }
    }
}

/// Diagonal stripes in `color` filling `area`, as Live marks a group slot over member clips.
/// Each 45° stripe is cut to the area by hand: a canvas `with_clip` draft is not moved with
/// a scrolled canvas, so its clip lands in the wrong place.
fn stripes(frame: &mut Frame, area: Rectangle, color: Color) {
    let bottom = area.y + area.height;
    let path = Path::new(|b| {
        let mut offset = -area.height;
        while offset < area.width {
            // The stripe rises from (area.x + offset, bottom) by t for t in 0..height.
            let from = (-offset).max(0.0);
            let to = area.height.min(area.width - offset);
            if from < to {
                b.move_to(Point::new(area.x + offset + from, bottom - from));
                b.line_to(Point::new(area.x + offset + to, bottom - to));
            }
            offset += STRIPE_PITCH;
        }
    });
    frame.stroke(
        &path,
        Stroke::default().with_color(color).with_width(STRIPE_W),
    );
}
