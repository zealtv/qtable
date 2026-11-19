//! Hover regions reuse the canvas's click geometry, group tracks included.
use super::{
    Action, CELL_H, GAP, Grid, Hit, MASTER_W, Master, column_at, hit_test_layout, master_hit_test,
    status_row,
};
use crate::control::help::Identity;
use crate::control_focus::region as focus_region;
use crate::info::{Id, region_declaring};
use iced::widget::canvas;
use iced::{Element, Length, Point, Rectangle, Size};
use qtable_engine::model::{ControlRef, TransportControl};

impl<'a> Grid<'a> {
    pub(super) fn with_help(mut self, width: f32, height: f32) -> Element<'a, Action> {
        self.drag_viewport = Default::default();
        let visible = self.drag_viewport.clone();
        let focus = self.clone();
        let declared = self.clone();
        let hover = self.clone();
        crate::control::viewport::report(
            focus_region(
                region_declaring(
                    canvas(self.clone())
                        .width(Length::Fixed(width))
                        .height(Length::Fixed(height)),
                    move |p, _| hover.info_at(p),
                    move || declared.declared_regions(),
                    Action::Info,
                ),
                move |p| focus.control_at(p),
                Action::ControlFocused,
            ),
            visible,
        )
    }

    fn info_at(&self, point: Point) -> Option<Id> {
        self.identity_at(point).map(|identity| identity.help)
    }

    /// Map Mode picks and marks whole slots, including their names; pinned track stops
    /// retain their ordinary square targets.
    fn declared_regions(&self) -> Vec<(Rectangle, ControlRef)> {
        let mut found = Vec::new();
        if !self.status_row {
            for (ci, col) in self.layout.columns.iter().enumerate() {
                let region = col.name(self.layout.xs[ci]);
                if let Some(drives) = self
                    .identity_at(region.center())
                    .and_then(|identity| identity.drives)
                {
                    found.push((region, drives));
                }
            }
        }
        for ci in 0..self.layout.columns.len() {
            let x = self.layout.xs[ci];
            let regions: Vec<Rectangle> = if self.status_row {
                vec![status_row::area(x, CELL_H)]
            } else {
                (0..self.scenes.len())
                    .map(|row| {
                        let y = self.layout.header_h + row as f32 * (CELL_H + GAP);
                        Rectangle::new(
                            Point::new(x, y),
                            Size::new(self.layout.columns[ci].width, CELL_H),
                        )
                    })
                    .collect()
            };
            for region in regions {
                if let Some(drives) = self
                    .identity_at(region.center())
                    .and_then(|identity| identity.drives)
                {
                    found.push((region, drives));
                }
            }
        }
        found
    }

    /// What the pointer rests on: its help entry and what it drives. A canvas cell has no
    /// `Control`, so the grid declares the stable ids here, from the same hit.
    fn identity_at(&self, point: Point) -> Option<Identity<Id>> {
        let cols = &self.layout.columns;
        let slot = |track: usize, row: usize| {
            Some(ControlRef::ClipSlot {
                track: self.tracks.get(track)?.id,
                scene: self.scenes.get(row)?.id,
            })
        };
        let stop = |track: usize| Some(ControlRef::TrackStop(self.tracks.get(track)?.id));
        let with = |help, drives| Identity { help, drives };
        if self.status_row {
            return match status_row::track_hit(point, cols)? {
                Hit::GroupStop(i) => Some(with(Id::GroupStop, stop(i))),
                Hit::TrackStop(i) => Some(with(Id::TrackStop, stop(i))),
                _ => Some(Identity::new(Id::TrackStop)),
            };
        }
        if point.y < self.layout.header_h {
            let (ci, col) = column_at(cols, &self.layout.xs, point.x)?;
            let help = if col.group_track().is_none() {
                Id::TrackHeader
            } else if col.fold(self.layout.xs[ci]).contains(point) {
                Id::GroupFold
            } else {
                Id::GroupHeader
            };
            return Some(Identity {
                help,
                drives: (help != Id::GroupFold).then_some(match col.key {
                    super::layout::ColumnKey::Track(id) => ControlRef::TrackSelect(id),
                }),
            });
        }
        let hit = hit_test_layout(point, cols, self.scenes.len())?;
        Some(match hit {
            Hit::SceneSelect(_) => Identity::new(Id::SceneSelect),
            Hit::GroupLaunch(gi, s) | Hit::GroupName(gi, s) => {
                let members = &self.group_column(gi)?.leaves;
                if members
                    .iter()
                    .any(|&ti| self.cells[ti][s].clip.is_some() || self.cells[ti][s].stop_button)
                {
                    with(
                        if matches!(hit, Hit::GroupLaunch(..)) {
                            Id::GroupLaunch
                        } else {
                            Id::GroupSlot
                        },
                        slot(gi, s),
                    )
                } else {
                    Identity::new(Id::EmptySlot)
                }
            }
            Hit::ClipLaunch(t, s) | Hit::ClipName(t, s) => {
                let cell = self.cells.get(t)?.get(s)?;
                match (hit, cell.clip.is_some(), cell.stop_button) {
                    (Hit::ClipLaunch(..), true, _) => with(Id::ClipLaunch, slot(t, s)),
                    (Hit::ClipLaunch(..), false, true) => with(Id::SlotStop, slot(t, s)),
                    (_, true, _) => with(Id::ClipSlot, slot(t, s)),
                    (_, false, true) => with(Id::EmptySlot, slot(t, s)),
                    _ => Identity::new(Id::EmptySlot),
                }
            }
            _ => return None,
        })
    }
}

impl<'a> Master<'a> {
    pub(super) fn with_help(self, width: f32, height: f32) -> Element<'a, Action> {
        let focus = self.clone();
        let declared = self.clone();
        let hover = self.clone();
        focus_region(
            region_declaring(
                canvas(self.clone())
                    .width(Length::Fixed(width))
                    .height(Length::Fixed(height)),
                move |p, _| hover.info_at(p),
                move || declared.declared_regions(),
                Action::Info,
            ),
            move |p| focus.control_at(p),
            Action::ControlFocused,
        )
    }

    fn info_at(&self, point: Point) -> Option<Id> {
        self.identity_at(point).map(|identity| identity.help)
    }

    /// Stop All, the Follow Actions button and each scene's launch square.
    fn declared_regions(&self) -> Vec<(Rectangle, ControlRef)> {
        let mut squares: Vec<Rectangle> = if self.status_row {
            vec![status_row::area(0.0, CELL_H), status_row::follow_area()]
        } else {
            (0..self.scenes.len())
                .map(|row| {
                    let y = self.header_h + row as f32 * (CELL_H + GAP);
                    Rectangle::new(Point::new(0.0, y), Size::new(CELL_H.min(MASTER_W), CELL_H))
                })
                .collect()
        };
        if !self.status_row {
            squares.push(Rectangle::new(
                Point::new(0.0, self.header_h - super::HEADER_H),
                Size::new(MASTER_W, super::TRACK_H),
            ));
        }
        squares
            .into_iter()
            .filter_map(|square| {
                let drives = self.identity_at(square.center())?.drives?;
                Some((square, drives))
            })
            .collect()
    }

    fn identity_at(&self, point: Point) -> Option<Identity<Id>> {
        let song = |help, control| Identity {
            help,
            drives: Some(ControlRef::Transport(control)),
        };
        if self.status_row {
            return status_row::master_hit(point).map(|hit| match hit {
                Hit::FollowToggle => song(Id::FollowActionsToggle, TransportControl::FollowActions),
                _ => song(Id::StopAll, TransportControl::StopAll),
            });
        }
        if point.y < self.header_h {
            return Some(Identity {
                help: Id::MainTrack,
                drives: Some(ControlRef::TrackSelect(self.main.id)),
            });
        }
        match master_hit_test(point, self.scenes.len(), self.header_h)? {
            Hit::SceneLaunch(i) => Some(Identity {
                help: Id::SceneLaunch,
                drives: self.scenes.get(i).map(|scene| ControlRef::Scene(scene.id)),
            }),
            Hit::SceneSelect(_) => Some(Identity::new(Id::SceneSelect)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
