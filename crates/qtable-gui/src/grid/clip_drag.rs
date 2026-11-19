//! Single-clip dragging. Where Live's drop rules are unknown, qtable's own
//! choices apply (decided 2026-10-12).
use super::{Action, CELL_H, Grid, Hit, Selection, cache::GridState, draw, hit_test_layout};
use iced::widget::canvas::{self, Frame};
use iced::{Point, Rectangle, Size, keyboard, mouse};
use qtable_engine::model::ClipId;

#[derive(Clone, Copy, Debug)]
pub(super) struct ClipDrag {
    pub from: Selection,
    id: ClipId,
    origin: Point,
    pub cursor: Point,
    started: bool,
}

impl ClipDrag {
    pub fn new(from: Selection, id: ClipId, origin: Point) -> Self {
        Self {
            from,
            id,
            origin,
            cursor: origin,
            started: false,
        }
    }

    fn move_to(&mut self, at: Point) {
        self.cursor = at;
        self.started |= self.origin.distance(at) >= 4.0;
    }
}

pub(super) fn copy_modifier(modifiers: keyboard::Modifiers) -> bool {
    if cfg!(target_os = "windows") {
        modifiers.control()
    } else {
        modifiers.alt()
    }
}

impl Grid<'_> {
    pub(super) fn clip_drop(&self, drag: &ClipDrag, at: Point) -> Option<Selection> {
        if !drag.started || self.status_row {
            return None;
        }
        // A source removed/replaced during the gesture must not move a different clip.
        let source = self.tracks.iter().find(|t| t.id == drag.from.track)?;
        if source.slots.get(drag.from.slot)?.clip.as_ref()?.id != drag.id {
            return None;
        }
        let (t, slot) = match hit_test_layout(at, &self.layout.columns, self.scenes.len())? {
            Hit::ClipName(t, s) | Hit::ClipLaunch(t, s) => (t, s),
            _ => return None,
        };
        let track = self.tracks.get(t)?;
        track.slots.get(slot)?;
        let to = Selection {
            track: track.id,
            slot,
        };
        (to != drag.from).then_some(to)
    }

    pub(super) fn clip_drag_update(
        &self,
        state: &mut GridState,
        event: &canvas::Event,
        cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Option<canvas::Action<Action>> {
        if let canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
            state.clear_feedback();
            return state
                .clip_drag
                .as_ref()
                .map(|_| canvas::Action::request_redraw());
        }
        state.clip_drag.as_ref()?;
        match event {
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let at = cursor.position()?;
                state
                    .clip_drag
                    .as_mut()?
                    .move_to(Point::new(at.x - bounds.x, at.y - bounds.y));
                state.clear_feedback();
                Some(canvas::Action::request_redraw())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let mut drag = state.clip_drag.take()?;
                state.clear_feedback();
                // Resolve using the release position, never the last valid hovered slot.
                let to = cursor.position_in(bounds).and_then(|at| {
                    drag.move_to(at);
                    self.clip_drop(&drag, at)
                });
                if drag.started {
                    state.reset_click();
                }
                Some(match to {
                    Some(to) => canvas::Action::publish(Action::DropClip {
                        from: drag.from,
                        to,
                        copy: copy_modifier(state.modifiers),
                    }),
                    None => canvas::Action::request_redraw(),
                })
            }
            canvas::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            })
            | canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                state.clip_drag = None;
                state.reset_click();
                state.clear_feedback();
                Some(canvas::Action::request_redraw())
            }
            _ => None,
        }
    }
}

pub(super) fn draw(frame: &mut Frame, grid: &Grid<'_>, to: Selection, copy: bool) {
    let Some((ci, col)) = grid
        .layout
        .columns
        .iter()
        .enumerate()
        .find(|(_, c)| c.track().is_some_and(|t| grid.tracks[t].id == to.track))
    else {
        return;
    };
    let area = Rectangle::new(
        Point::new(
            grid.layout.xs[ci] + 1.0,
            grid.layout.header_h + to.slot as f32 * (super::CELL_H + super::GAP) + 1.0,
        ),
        Size::new(col.width - 2.0, CELL_H - 2.0),
    );
    draw::outline(
        frame,
        area,
        crate::control::LIVE_DARK.selection_background,
        2.0,
    );
    if copy {
        draw::label(
            frame,
            "+".into(),
            area.x + area.width - 8.0,
            area.y + area.height / 2.0,
            crate::control::LIVE_DARK.selection_background,
            true,
        );
    }
}

#[cfg(test)]
mod tests;
