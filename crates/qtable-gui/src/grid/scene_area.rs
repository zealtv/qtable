//! Return-style slots and the space between tracks and outputs share scene selection.
use super::{Action, CELL_H, GAP, Selection};
use super::{
    Layout, cache, header,
    layout::{Column, column_at},
};
use crate::info::{Id, region_declaring};
use crate::selection::SelectionView;
use iced::widget::canvas;
use iced::widget::canvas::{Frame, Geometry, Program};
use iced::{Element, Length, Point, Rectangle, Size, Theme, mouse};
use qtable_engine::model::{Scene, SceneId, Track};
use std::rc::Rc;

pub(super) fn row_at(y: f32, count: usize, header_h: f32) -> Option<usize> {
    let y = y - header_h;
    if y < 0.0 {
        return None;
    }
    let row = (y / (CELL_H + GAP)).floor() as usize;
    (row < count && y % (CELL_H + GAP) < CELL_H).then_some(row)
}

pub(crate) fn selected_row(
    clip: Option<Selection>,
    scene: Option<SceneId>,
    cell: Option<crate::selection::ColumnCell>,
    scenes: &[Scene],
) -> Option<usize> {
    clip.map(|s| s.slot)
        .or_else(|| scene.and_then(|id| scenes.iter().position(|s| s.id == id)))
        .or_else(|| cell.filter(|cell| cell.active).map(|cell| cell.row))
        .filter(|&row| row < scenes.len())
}

#[derive(Clone)]
pub(super) struct OutputArea<'a> {
    pub scenes: &'a [Scene],
    pub selected_row: Option<usize>,
    pub outputs: &'a [Track],
    pub selected_tracks: &'a [qtable_engine::model::TrackId],
    pub owner: Option<crate::selection::ColumnOwner>,
    pub cell: Option<crate::selection::ColumnCell>,
    pub layout: Rc<Layout>,
    pub visible: bool,
    pub revision: u64,
    pub drag: header::Gesture,
    pub drag_viewport: crate::control::viewport::Viewport,
    pub drag_h: f32,
    pub shift_down: bool,
}

#[derive(Default)]
pub(super) struct HeaderState {
    clicks: cache::Clicks,
    names: cache::Names,
}

impl<'a> OutputArea<'a> {
    pub(super) fn selection(&self) -> SelectionView<'a> {
        SelectionView {
            models: self.outputs,
            tracks: self.selected_tracks,
            owner: self.owner,
            cell: self.cell,
            clip: None,
            scene: None,
        }
    }
    pub fn with_help(mut self, width: f32, height: f32) -> Element<'a, Action> {
        self.drag_viewport = Default::default();
        let visible = self.drag_viewport.clone();
        let info = self.clone();
        let declared = self.clone();
        crate::control::viewport::report(
            region_declaring(
                canvas(self)
                    .width(Length::Fixed(width))
                    .height(Length::Fixed(height)),
                move |p, _| {
                    if p.y < info.layout.header_h && info.column_at(p).is_some() {
                        Some(Id::OutputTitle)
                    } else {
                        row_at(p.y, info.scenes.len(), info.layout.header_h)
                            .map(|_| Id::SceneSelect)
                    }
                },
                move || {
                    declared
                        .layout
                        .columns
                        .iter()
                        .enumerate()
                        .filter_map(|(ci, col)| {
                            let super::layout::ColumnKey::Track(id) = col.key;
                            declared.visible.then_some((
                                col.name(declared.layout.xs[ci]),
                                qtable_engine::model::ControlRef::TrackSelect(id),
                            ))
                        })
                        .collect()
                },
                Action::Info,
            ),
            visible,
        )
    }
    fn column_at(&self, p: Point) -> Option<(usize, &Column)> {
        self.visible
            .then(|| column_at(&self.layout.columns, &self.layout.xs, p.x))
            .flatten()
    }
}

impl Program<Action> for OutputArea<'_> {
    type State = HeaderState;
    fn update(
        &self,
        state: &mut HeaderState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Action>> {
        if self.visible
            && let Some(action) = header::shared_drag_update(
                (self.outputs, self.revision),
                &self.layout.columns,
                &self.layout.xs,
                &self.drag,
                event,
                cursor,
                header::Surface {
                    bounds: Rectangle {
                        height: self.drag_h.max(bounds.height),
                        ..bounds
                    },
                    visible: self.drag_viewport.get(),
                    pinned: bounds.height <= self.layout.header_h,
                },
            )
        {
            return Some(action);
        }
        let p = cursor.position_in(bounds)?;
        if let canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) = event {
            return Some(canvas::Action::publish(self.column_at(p).map_or(
                Action::RegionMenu(qtable_engine::model::Region::Outputs),
                |(_, col)| header::menu(col),
            )));
        }
        if !matches!(
            event,
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) {
            return None;
        }
        let double_click = state.clicks.register(p, crate::gesture_clock::now());
        if let Some((ci, col)) = self.column_at(p) {
            if p.y < self.layout.header_h {
                let (action, drag) =
                    header::press(col, p, self.layout.xs[ci], self.shift_down, self.revision);
                self.drag.set(drag);
                return Some(canvas::Action::publish(super::detail_open::click(
                    Action::PressHeader(Box::new(action)),
                    crate::inspector_panel::Tab::Output,
                    double_click,
                )));
            }
            let action = row_at(p.y, self.scenes.len(), self.layout.header_h)
                .map_or(Action::SelectBlankColumn(col.owner()), |row| {
                    Action::SelectColumn(col.owner(), Some(row))
                });
            return Some(canvas::Action::publish(action));
        }
        row_at(p.y, self.scenes.len(), self.layout.header_h).map(|row| {
            canvas::Action::publish(super::detail_open::click(
                Action::SelectScene(self.scenes[row].id),
                crate::inspector_panel::Tab::Scene,
                double_click,
            ))
        })
    }
    fn mouse_interaction(
        &self,
        _state: &HeaderState,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if self.drag.get().is_some_and(|drag| drag.dragging()) {
            mouse::Interaction::Grabbing
        } else if cursor
            .position_in(bounds)
            .is_some_and(|p| p.y < self.layout.header_h && self.column_at(p).is_some())
        {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
    fn draw(
        &self,
        state: &HeaderState,
        renderer: &iced::Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        state.names.begin_frame();
        if self.visible {
            super::draw::rect(
                &mut frame,
                Rectangle::new(Point::ORIGIN, bounds.size()),
                super::draw::GAP_COLOUR,
            );
            for (ci, col) in self.layout.columns.iter().enumerate() {
                let Some(output) = col.index.and_then(|index| self.outputs.get(index)) else {
                    continue;
                };
                let x = self.layout.xs[ci];
                let body_h = (bounds.height - self.layout.header_h).max(0.0);
                super::draw::rect(
                    &mut frame,
                    Rectangle::new(
                        Point::new(x, self.layout.header_h),
                        Size::new(col.width, body_h),
                    ),
                    super::draw::TRACK_BODY,
                );
                let look =
                    header::look(col, &header::HeaderFacts::track(output), &self.selection());
                header::draw(&mut frame, col, x, &look, &state.names);
                for row in 0..self.scenes.len() {
                    let area = Rectangle::new(
                        Point::new(x, self.layout.header_h + row as f32 * (CELL_H + GAP)),
                        Size::new(col.width, CELL_H),
                    );
                    let fill = self
                        .cell
                        .filter(|cell| cell.owner == col.owner() && cell.row == row)
                        .map_or(super::draw::SLOT_BODY, |cell| {
                            if cell.active {
                                crate::control::LIVE_DARK.selection_background
                            } else {
                                super::draw::SELECTION_INACTIVE
                            }
                        });
                    super::draw::rect(&mut frame, area, fill);
                }
            }
        }
        if self.visible && self.outputs.is_empty() {
            super::draw::label(
                &mut frame,
                "Right-click or ⌘⌥T".into(),
                GAP + 4.0,
                self.layout.header_h + 12.0,
                super::draw::DISABLED,
                false,
            );
            super::draw::label(
                &mut frame,
                "to insert an output".into(),
                GAP + 4.0,
                self.layout.header_h + 28.0,
                super::draw::DISABLED,
                false,
            );
        }
        if let Some(row) = self.selected_row {
            super::draw::rect(
                &mut frame,
                Rectangle::new(
                    Point::new(0.0, self.layout.header_h + row as f32 * (CELL_H + GAP)),
                    Size::new(bounds.width, CELL_H),
                ),
                super::draw::ROW_TINT,
            );
        }
        if self.visible {
            for (ci, col) in self.layout.columns.iter().enumerate() {
                let Some(output) = col.index.and_then(|index| self.outputs.get(index)) else {
                    continue;
                };
                let look =
                    header::look(col, &header::HeaderFacts::track(output), &self.selection());
                header::tint(
                    &mut frame,
                    col,
                    self.layout.xs[ci],
                    (bounds.height - self.layout.header_h).max(0.0),
                    &look,
                );
            }
        }
        if self.visible
            && let Some(drag) = self
                .drag
                .get()
                .filter(|drag| drag.revision == self.revision && drag.inside && drag.dragging())
            && let Some(drop) = header::drop_selected_at(
                self.outputs,
                &self.layout.columns,
                &self.layout.xs,
                &drag.selected,
                drag.key,
                drag.cursor,
            )
        {
            header::draw_drop(&mut frame, &self.layout.columns, &self.layout.xs, &drop);
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests;
