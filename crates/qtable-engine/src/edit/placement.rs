//! One placement plan for engine edits and client previews, over the original list.
use crate::model::{ModelError, Topology, Track, TrackId};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacementPlan {
    pub roots: Vec<TrackId>,
    pub source: Vec<TrackId>,
    pub boundary: usize,
    pub destination: usize,
    pub changed: bool,
    pub(super) spans: Vec<Range<usize>>,
}

/// Reduce selected ancestors/descendants once, preserving project order even for
/// disjoint or mixed-depth roots. The explicit parent selects the boundary level.
pub fn placement_plan(
    tracks: &[Track],
    selected: &[TrackId],
    before: Option<TrackId>,
    group: Option<TrackId>,
) -> Result<PlacementPlan, ModelError> {
    let err = |s: &str| ModelError(s.into());
    let topology = Topology::build(tracks)?;
    let roots = topology.subtree_roots(selected)?;
    if roots.iter().any(|id| {
        !tracks[topology.index_of(*id).unwrap()]
            .capabilities()
            .movable
    }) {
        return Err(err("this track cannot be moved"));
    }
    let spans: Vec<_> = roots.iter().map(|id| topology.span(*id).unwrap()).collect();
    let source: Vec<_> = spans
        .iter()
        .flat_map(|s| tracks[s.clone()].iter().map(|t| t.id))
        .collect();
    if group.is_some_and(|id| source.contains(&id)) || before.is_some_and(|id| source.contains(&id))
    {
        return Err(err("selected subtrees cannot be placed inside themselves"));
    }
    if let Some(id) = group
        && !topology.index_of(id).is_some_and(|i| tracks[i].is_group())
    {
        return Err(err("unknown group"));
    }
    let boundary = match before {
        Some(id) => topology.index_of(id).ok_or_else(|| err("unknown track"))?,
        None => tracks.len(),
    };
    let after_removal = |at: usize| {
        at - spans
            .iter()
            .map(|s| at.min(s.end).saturating_sub(s.start))
            .sum::<usize>()
    };
    let end = group.map_or(tracks.len(), |id| topology.span(id).unwrap().end);
    if after_removal(boundary) != after_removal(end)
        && !before.is_some_and(|id| topology.parent(id) == group)
    {
        return Err(err("placement boundary splits a subtree"));
    }
    let destination = after_removal(boundary);
    let unchanged_order =
        spans.windows(2).all(|s| s[0].end == s[1].start) && destination == spans[0].start;
    let changed = !unchanged_order || roots.iter().any(|id| topology.parent(*id) != group);
    Ok(PlacementPlan {
        roots,
        source,
        boundary,
        destination,
        changed,
        spans,
    })
}
