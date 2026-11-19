//! Grid buttons take the window's click focus like iced controls. Clicks and the outline use
//! one rectangle per button, from the same geometry as hit-testing; the outline is drawn in
//! the overlay layer so a focus change leaves the cached base drawing alone.
use super::{CELL_H, FOOTER_H, GAP, Grid, Hit, Master, column_at};
use crate::control::{LIVE_DARK, outline};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::ControlId;
use iced::widget::canvas::Frame;
use iced::{Point, Rectangle, Size};

fn id(name: &'static str, owner: ControlOwner) -> ControlId {
    ControlId::action(View::Grid, name, owner)
}

fn slot_area(x: f32, slot: usize, header_h: f32) -> Rectangle {
    Rectangle::new(
        Point::new(x, header_h + slot as f32 * (CELL_H + GAP)),
        Size::new(CELL_H, CELL_H),
    )
}

/// The status row's stop squares sit below its top gap.
fn status_area(x: f32) -> Rectangle {
    Rectangle::new(Point::new(x, GAP), Size::new(CELL_H, FOOTER_H - GAP))
}

impl Grid<'_> {
    /// The button under a canvas-local point, exactly where a click would act on it.
    pub(super) fn control_at(&self, point: Point) -> Option<ControlId> {
        match self.button_hit(point) {
            Some(hit) => self.control_of(hit),
            None => self.fold_at(point),
        }
    }

    fn fold_at(&self, point: Point) -> Option<ControlId> {
        if self.status_row || point.y >= self.layout.header_h {
            return None;
        }
        let (ci, col) = column_at(&self.layout.columns, &self.layout.xs, point.x)?;
        let group = self.tracks.get(col.group_track()?)?.id;
        (col.fold(self.layout.xs[ci]).contains(point))
            .then_some(id("fold", ControlOwner::Track(group)))
    }

    fn control_of(&self, hit: Hit) -> Option<ControlId> {
        Some(match hit {
            Hit::ClipLaunch(t, s) => {
                id("launch", ControlOwner::ClipSlot(self.tracks.get(t)?.id, s))
            }
            Hit::GroupLaunch(gi, s) => {
                id("launch", ControlOwner::ClipSlot(self.tracks.get(gi)?.id, s))
            }
            Hit::TrackStop(t) => id("stop", ControlOwner::Track(self.tracks.get(t)?.id)),
            Hit::GroupStop(gi) => id("stop", ControlOwner::Track(self.tracks.get(gi)?.id)),
            _ => return None,
        })
    }

    /// Every focusable button this canvas draws, with its rectangle.
    fn buttons(&self) -> impl Iterator<Item = (ControlId, Rectangle)> + '_ {
        let cols = &self.layout.columns;
        cols.iter().enumerate().flat_map(move |(ci, col)| {
            let x = self.layout.xs[ci];
            let strip = match (col.group_track(), col.track()) {
                (Some(gi), _) => self.tracks.get(gi).map(|g| g.id),
                (None, Some(t)) => self.tracks.get(t).map(|t| t.id),
                (None, None) => None,
            };
            let status = self.status_row;
            let slots = if status { 0 } else { self.scenes.len() };
            let fold = col.caps.foldable && !status;
            strip.into_iter().flat_map(move |strip| {
                let stop = status.then(|| (id("stop", ControlOwner::Track(strip)), status_area(x)));
                let fold = fold.then(|| {
                    let area = col.fold(x);
                    (id("fold", ControlOwner::Track(strip)), area)
                });
                let launches = (0..slots).map(move |s| {
                    (
                        id("launch", ControlOwner::ClipSlot(strip, s)),
                        slot_area(x, s, self.layout.header_h),
                    )
                });
                stop.into_iter().chain(fold).chain(launches)
            })
        })
    }

    /// Outline the focused button, if it is one of ours.
    pub(super) fn draw_focus(&self, frame: &mut Frame) {
        let Some(focused) = self.focused else {
            return;
        };
        if let Some((_, area)) = self.buttons().find(|(id, _)| *id == focused) {
            outline(frame, area, &LIVE_DARK);
        }
    }
}

impl Master<'_> {
    pub(super) fn control_at(&self, point: Point) -> Option<ControlId> {
        self.button_hit(point).and_then(|hit| self.control_of(hit))
    }

    fn control_of(&self, hit: Hit) -> Option<ControlId> {
        Some(match hit {
            Hit::SceneLaunch(s) => id("launch", ControlOwner::Scene(self.scenes.get(s)?.id)),
            Hit::StopAll => id("stop-all", ControlOwner::Song),
            Hit::FollowToggle => id("follow-actions", ControlOwner::Song),
            _ => return None,
        })
    }

    pub(super) fn draw_focus(&self, frame: &mut Frame) {
        let Some(focused) = self.focused else {
            return;
        };
        let area = if self.status_row {
            [
                (id("stop-all", ControlOwner::Song), status_area(0.0)),
                (
                    id("follow-actions", ControlOwner::Song),
                    super::status_row::follow_area(),
                ),
            ]
            .into_iter()
            .find(|(id, _)| *id == focused)
            .map(|(_, area)| area)
        } else {
            self.scenes
                .iter()
                .position(|scene| id("launch", ControlOwner::Scene(scene.id)) == focused)
                .map(|s| slot_area(0.0, s, self.header_h))
        };
        if let Some(area) = area {
            outline(frame, area, &LIVE_DARK);
        }
    }
}

#[cfg(test)]
mod tests;
