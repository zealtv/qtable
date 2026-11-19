//! A visible insertion boundary selects a model boundary and an explicit parent.
use super::*;
use qtable_engine::model::Topology;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::grid) enum Marker {
    Before(usize),
    Into(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::grid) struct Drop {
    pub source: Vec<TrackId>,
    /// Boundary in the original model list, including hidden descendants.
    pub boundary: usize,
    pub before: Option<TrackId>,
    pub group: Option<TrackId>,
    pub marker: Marker,
    pub depth: usize,
}

#[cfg(test)]
pub(in crate::grid) fn drop_at(
    tracks: &[Track],
    cols: &[Column],
    xs: &[f32],
    key: ColumnKey,
    at: Point,
) -> Option<Drop> {
    drop_selected_at(
        tracks,
        cols,
        xs,
        &[match key {
            ColumnKey::Track(id) => id,
        }],
        key,
        at,
    )
}

pub(in crate::grid) fn drop_selected_at(
    tracks: &[Track],
    cols: &[Column],
    xs: &[f32],
    selected: &[TrackId],
    key: ColumnKey,
    at: Point,
) -> Option<Drop> {
    let source = cols.iter().find(|c| c.key == key)?;
    if !source.caps.movable || !at.x.is_finite() || !at.y.is_finite() {
        return None;
    }
    let topology = Topology::build(tracks).ok()?;
    let hit = column_at(cols, xs, at.x);
    // Bands do not create an alternative parent-level target. The title/slot
    // surface owns its column; folded titles select outside/first-member halves.
    if hit.is_some_and(|(ci, col)| at.y < col.title(xs[ci]).y) {
        return None;
    }
    let folded = hit.filter(|(_, col)| {
        col.caps.foldable && at.y < col.header_h && tracks[col.index.unwrap()].fold_state
    });
    let (boundary, group, marker) = if let Some((ci, col)) = folded {
        let start = topology.index_of(id(col))?;
        if at.x < xs[ci] + col.width / 2.0 {
            (start, col.parent, Marker::Before(ci))
        } else {
            (start + 1, Some(id(col)), Marker::Into(ci))
        }
    } else {
        let boundary = match hit {
            Some((ci, col)) => crate::selection::header_boundary(ci, at.x, xs[ci], col.width),
            None => xs
                .iter()
                .position(|left| at.x < *left)
                .unwrap_or(cols.len()),
        };
        // Before a displayed column means before that subtree at its own parent
        // level. At a shared end this exits every group that has just ended.
        // A folded column's right edge therefore skips its hidden descendants.
        let right = cols.get(boundary);
        (
            right.map_or(tracks.len(), |c| topology.index_of(id(c)).unwrap()),
            right.and_then(|c| c.parent),
            Marker::Before(boundary),
        )
    };
    let before = tracks.get(boundary).map(|t| t.id);
    let plan = qtable_engine::edit::placement_plan(tracks, selected, before, group).ok()?;
    if !plan.changed {
        return None;
    }
    Some(Drop {
        source: plan.source,
        boundary,
        before,
        group,
        marker,
        depth: group.map_or(0, |g| topology.depth(g).unwrap() + 1),
    })
}

/// Live's insertion line starts at the destination title level, leaving ancestor
/// bands intact. Folded first-member drops use the folded column's right edge.
pub(super) fn insertion_line(cols: &[Column], xs: &[f32], drop: &Drop, height: f32) -> Rectangle {
    let boundary = match drop.marker {
        Marker::Before(i) => i,
        Marker::Into(i) => i + 1,
    };
    let end = xs
        .last()
        .zip(cols.last())
        .map_or(GAP, |(x, c)| x + c.width + GAP);
    let x = xs.get(boundary).copied().unwrap_or(end) - GAP / 2.0;
    let top = drop.depth as f32 * crate::style::detail::GROUP_HEADER_STEP;
    let width = crate::style::detail::TRACK_DROP_LINE;
    Rectangle::new(
        Point::new(x - width / 2., top),
        Size::new(width, (height - top).max(0.0)),
    )
}

pub(in crate::grid) fn draw_drop(frame: &mut Frame, cols: &[Column], xs: &[f32], drop: &Drop) {
    draw::rect(
        frame,
        insertion_line(cols, xs, drop, frame.height()),
        crate::control::LIVE_DARK.control_selection_frame,
    );
}

/// Continue the same production preview through the pinned status/strip sections.
/// This transparent canvas has no gesture handling; the header remains the owner.
pub(in crate::grid) struct DropContinuation<'a>(pub Grid<'a>);

impl canvas::Program<Action> for DropContinuation<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        if let Some(drop) = self
            .0
            .drag
            .get()
            .and_then(|drag| self.0.pending_drop(&drag))
        {
            let mut line = insertion_line(
                &self.0.layout.columns,
                &self.0.layout.xs,
                &drop,
                bounds.height,
            );
            line.y = 0.;
            line.height = bounds.height;
            draw::rect(
                &mut frame,
                line,
                crate::control::LIVE_DARK.control_selection_frame,
            );
        }
        vec![frame.into_geometry()]
    }
}
