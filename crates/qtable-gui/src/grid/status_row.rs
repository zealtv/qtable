//! Fixed status canvases share their local geometry with drawing and hit testing.
use super::draw::{DISABLED, INK, LIT, colour, rect, stop_button};
use super::{
    CELL_H, CellStatus, Column, FOOTER_H, GAP, Grid, Hit, Layout, MASTER_W, Master, x_positions,
};
use crate::control::{self, ControlKind, FollowToggle, LIVE_DARK};
use crate::selection::SelectionView;
use iced::widget::canvas::{Frame, Geometry, Program};
use iced::{Color, Point, Rectangle, Renderer, Size, mouse};
use std::rc::Rc;

const BACKGROUND: Color = LIVE_DARK.status_row_background;
const BORDER: Color = LIVE_DARK.status_row_border;

pub(super) fn area(x: f32, width: f32) -> Rectangle {
    Rectangle::new(Point::new(x, GAP), Size::new(width, FOOTER_H - GAP))
}

pub(super) fn cell_background(selection: &SelectionView<'_>, col: &Column) -> Color {
    if selection.column_selected(col) {
        crate::style::HIGHLIGHT
    } else {
        BACKGROUND
    }
}

fn background(frame: &mut Frame, x: f32, width: f32, fill: Color) {
    rect(frame, area(x, width), fill);
    rect(
        frame,
        Rectangle::new(Point::new(x, 0.0), Size::new(width, GAP)),
        BORDER,
    );
}

pub(super) fn track_hit(point: Point, cols: &[Column]) -> Option<Hit> {
    let xs = x_positions(cols);
    cols.iter().enumerate().find_map(|(i, col)| {
        // Only the stop square is a control; the position display is not a slot.
        area(xs[i], CELL_H)
            .contains(point)
            .then(|| match col.group_track() {
                Some(gi) => Some(Hit::GroupStop(gi)),
                None => col.track().map(Hit::TrackStop),
            })?
    })
}

/// Live 12 puts the global Follow Actions button at the right end of Main's status row
/// (screenshot, 2026-10-11).
pub(super) fn follow_area() -> Rectangle {
    area(MASTER_W - GAP - CELL_H, CELL_H)
}

pub(super) fn master_hit(point: Point) -> Option<Hit> {
    if area(0.0, CELL_H).contains(point) {
        Some(Hit::StopAll)
    } else {
        follow_area().contains(point).then_some(Hit::FollowToggle)
    }
}

pub(super) fn master(frame: &mut Frame, master: &Master<'_>) {
    let selection = SelectionView {
        models: &[],
        tracks: &[],
        owner: master.owner,
        cell: master.cell,
        clip: master.clip,
        scene: master.selected_scene,
    };
    let fill = cell_background(&selection, &Column::main(master.main.id));
    background(frame, 0.0, MASTER_W - GAP, fill);
    stop_button(frame, area(0.0, CELL_H), fill, DISABLED);
    let state = control::State {
        lit: if master.follow_enabled {
            control::Lit::On
        } else {
            control::Lit::Off
        },
        ..control::State::default()
    };
    let look = FollowToggle.look(&LIVE_DARK, state, control::Interaction::Idle);
    FollowToggle.paint(frame, follow_area(), &look);
}

pub(super) fn tracks(grid: &Grid<'_>, frame: &mut Frame) {
    for (ci, col) in grid.layout.columns.iter().enumerate() {
        let x = grid.layout.xs[ci];
        let idle = cell_background(&grid.selection_view(), col);
        background(frame, x, col.width, idle);
        let playing = col.leaves.iter().find_map(|ti| {
            grid.cells[*ti]
                .iter()
                .find_map(|cell| match (cell.status, cell.clip) {
                    (CellStatus::Playing(p), Some(clip)) => Some((p, clip.colour)),
                    _ => None,
                })
        });
        let (bg, fg) = if playing.is_some() {
            (LIT, INK)
        } else {
            (idle, DISABLED)
        };
        stop_button(frame, area(x, CELL_H), bg, fg);
        if let Some((progress, clip_colour)) = playing {
            let bar = col.width - CELL_H - 4.0;
            rect(
                frame,
                Rectangle::new(
                    Point::new(x + CELL_H + 2.0, FOOTER_H / 2.0),
                    Size::new(bar * progress, 2.0),
                ),
                colour(clip_colour),
            );
        }
    }
}

/// Outputs have no stop control, but their pinned status cell follows column selection.
pub(super) struct Outputs<'a> {
    pub layout: Rc<Layout>,
    pub selection: SelectionView<'a>,
}
impl<M> Program<M> for Outputs<'_> {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        for (ci, col) in self.layout.columns.iter().enumerate() {
            background(
                &mut frame,
                self.layout.xs[ci],
                col.width,
                cell_background(&self.selection, col),
            );
        }
        vec![frame.into_geometry()]
    }
}
