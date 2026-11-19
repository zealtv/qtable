//! Columns and hit testing. As in Live 12, a group has a column of its own (its group track)
//! left of its members; folding the group hides the members and leaves that column
//! (screenshots of Live 12, 2026-10-12).
use super::{CELL_H, GAP, HEADER_H, Hit, MASTER_W, TRACK_W, scene_area};
use iced::{Color, Point, Rectangle, Size};
use qtable_engine::model::Track;

/// Stable identity independent of a column's position or region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColumnKey {
    Track(qtable_engine::model::TrackId),
}

#[derive(Clone, Debug)]
pub(crate) struct Column {
    pub key: ColumnKey,
    pub caps: qtable_engine::model::Capabilities,
    pub width: f32,
    pub index: Option<usize>,
    pub number: Option<usize>,
    /// Ordered descendant clip tracks, independent of visibility and header stop flags.
    pub leaves: Vec<usize>,
    /// Immediate model parent, distinct from the column's own stable identity.
    pub parent: Option<qtable_engine::model::TrackId>,
    /// Outer-first ancestor identities and colours, including on Group columns.
    pub ancestors: Vec<(qtable_engine::model::TrackId, Color)>,
    pub header_h: f32,
}

impl Column {
    pub fn title(&self, x: f32) -> Rectangle {
        let top = self.ancestors.len() as f32 * crate::style::detail::GROUP_HEADER_STEP;
        Rectangle::new(
            Point::new(x, top),
            Size::new(self.width, self.header_h - GAP - top),
        )
    }

    /// The aligned title text/rename/map row, excluding the Fold control.
    pub fn name(&self, x: f32) -> Rectangle {
        Rectangle::new(
            Point::new(x, self.header_h - HEADER_H),
            Size::new(
                self.width
                    - if self.caps.foldable {
                        super::FOLD_W
                    } else {
                        0.0
                    },
                super::TRACK_H,
            ),
        )
    }

    pub fn bands(&self, x: f32) -> impl Iterator<Item = (Rectangle, Color)> + '_ {
        self.ancestors
            .iter()
            .enumerate()
            .map(move |(level, (_, colour))| {
                (
                    Rectangle::new(
                        Point::new(
                            x - GAP,
                            level as f32 * crate::style::detail::GROUP_HEADER_STEP,
                        ),
                        Size::new(
                            self.width + GAP,
                            crate::style::detail::GROUP_HEADER_STEP - GAP,
                        ),
                    ),
                    *colour,
                )
            })
    }

    pub fn label_y(&self) -> f32 {
        self.header_h - GAP - super::TRACK_H / 2.0
    }

    pub fn fold(&self, x: f32) -> Rectangle {
        Rectangle::new(
            Point::new(x + self.width - super::FOLD_W, self.header_h - HEADER_H),
            Size::new(super::FOLD_W, super::TRACK_H),
        )
    }

    pub fn group_track(&self) -> Option<usize> {
        self.index.filter(|_| self.caps.foldable)
    }

    pub fn track(&self) -> Option<usize> {
        self.index.filter(|_| self.caps.holds_clips)
    }

    pub fn owner(&self) -> crate::selection::ColumnOwner {
        use crate::selection::ColumnOwner;
        match self.key {
            ColumnKey::Track(id) if self.caps.foldable => ColumnOwner::Group(id),
            ColumnKey::Track(id) => ColumnOwner::Track(id),
        }
    }

    pub fn main(id: qtable_engine::model::TrackId) -> Self {
        Self {
            key: ColumnKey::Track(id),
            caps: qtable_engine::model::TrackKind::Main.capabilities(),
            width: MASTER_W - GAP,
            index: None,
            number: None,
            leaves: Vec::new(),
            parent: None,
            ancestors: Vec::new(),
            header_h: HEADER_H,
        }
    }
}

pub(crate) fn columns(
    project: &qtable_engine::model::Project,
    region: qtable_engine::model::Region,
) -> Vec<Column> {
    use qtable_engine::model::Region;
    let tracks = track_columns(&project.tracks);
    let header_h = header_height(&tracks);
    let mut result = match region {
        Region::Tracks => tracks,
        Region::Outputs => project
            .outputs
            .iter()
            .enumerate()
            .map(|(index, track)| Column {
                key: ColumnKey::Track(track.id),
                caps: track.capabilities(),
                width: TRACK_W,
                index: Some(index),
                number: None,
                leaves: Vec::new(),
                parent: None,
                ancestors: Vec::new(),
                header_h: HEADER_H,
            })
            .collect(),
        Region::Main => vec![Column::main(project.main.id)],
    };
    for col in &mut result {
        col.header_h = header_h;
    }
    result
}

pub(super) fn track_columns(tracks: &[Track]) -> Vec<Column> {
    let mut result = Vec::new();
    let topology = qtable_engine::model::Topology::build(tracks).expect("validated track topology");
    let by_id: std::collections::HashMap<_, _> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id, (i, t)))
        .collect();
    for (index, track) in tracks.iter().enumerate() {
        let caps = track.capabilities();
        let mut ancestors = Vec::new();
        let mut parent = topology.parent(track.id);
        for _ in 0..tracks.len() {
            let Some(group) = parent.and_then(|id| by_id.get(&id).map(|(_, track)| *track)) else {
                break;
            };
            ancestors.push((group.id, super::draw::colour(group.colour)));
            parent = topology.parent(group.id);
        }
        if ancestors
            .iter()
            .any(|(id, _)| by_id.get(id).is_some_and(|(_, t)| t.fold_state))
        {
            continue;
        }
        ancestors.reverse();
        result.push(Column {
            key: ColumnKey::Track(track.id),
            caps,
            width: TRACK_W,
            index: Some(index),
            number: caps.numbered.then_some(index + 1),
            parent: track.group,
            ancestors,
            header_h: HEADER_H,
            leaves: if caps.foldable {
                topology
                    .descendant_clip_tracks(track.id)
                    .into_iter()
                    .map(|id| by_id[&id].0)
                    .collect()
            } else {
                vec![index]
            },
        });
    }
    let height = HEADER_H
        + result.iter().map(|c| c.ancestors.len()).max().unwrap_or(0) as f32
            * crate::style::detail::GROUP_HEADER_STEP;
    for col in &mut result {
        col.header_h = height;
    }
    result
}

pub(crate) fn header_height(cols: &[Column]) -> f32 {
    cols.first().map_or(HEADER_H, |col| col.header_h)
}

pub(super) fn x_positions(columns: &[Column]) -> Vec<f32> {
    let mut x = GAP;
    columns
        .iter()
        .map(|column| {
            let here = x;
            x += column.width + GAP;
            here
        })
        .collect()
}

/// The column under canvas x `x`, if any (gaps between columns belong to none).
pub(super) fn column_at<'c>(cols: &'c [Column], xs: &[f32], x: f32) -> Option<(usize, &'c Column)> {
    cols.iter()
        .enumerate()
        .find(|(i, c)| x >= xs[*i] && x < xs[*i] + c.width)
}

#[cfg(test)]
pub fn hit_test(point: Point, track_count: usize, scene_count: usize) -> Option<Hit> {
    let tracks = (0..track_count)
        .map(|i| {
            Track::new(
                qtable_engine::model::TrackId(i as u64),
                String::new(),
                scene_count,
            )
        })
        .collect::<Vec<_>>();
    let cols = track_columns(&tracks);
    hit_test_layout(point, &cols, scene_count)
}

pub(super) fn hit_test_layout(point: Point, cols: &[Column], scene_count: usize) -> Option<Hit> {
    let header_h = header_height(cols);
    if point.x < 0.0 || point.y < header_h {
        return None;
    }
    let xs = x_positions(cols);
    let row_y = point.y - header_h;
    let slot = (row_y / (CELL_H + GAP)).floor() as usize;
    let local_y = row_y % (CELL_H + GAP);
    // Blank space at a scene Y, column gaps included, selects that scene.
    let Some((ci, col)) = column_at(cols, &xs, point.x) else {
        return scene_area::row_at(point.y, scene_count, header_h).map(Hit::SceneSelect);
    };
    if slot >= scene_count || local_y >= CELL_H {
        return None;
    }
    let launch = point.x - xs[ci] < CELL_H;
    Some(match (col.group_track(), col.track(), launch) {
        (Some(gi), _, true) => Hit::GroupLaunch(gi, slot),
        (Some(gi), _, false) => Hit::GroupName(gi, slot),
        (None, Some(t), true) => Hit::ClipLaunch(t, slot),
        (None, Some(t), false) => Hit::ClipName(t, slot),
        (None, None, _) => return None,
    })
}

/// Hit-testing for the master column, whose canvas starts at its own left edge.
pub(super) fn master_hit_test(point: Point, scene_count: usize, header_h: f32) -> Option<Hit> {
    if point.x < 0.0 || point.x >= MASTER_W || point.y < header_h {
        return None;
    }
    let row_y = point.y - header_h;
    let slot = (row_y / (CELL_H + GAP)).floor() as usize;
    let local_y = row_y % (CELL_H + GAP);
    if slot < scene_count && local_y < CELL_H {
        Some(if point.x < CELL_H {
            Hit::SceneLaunch(slot)
        } else {
            Hit::SceneSelect(slot)
        })
    } else {
        None
    }
}
