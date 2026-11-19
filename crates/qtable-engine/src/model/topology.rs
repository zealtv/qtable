//! Nested-group topology (2026-11-06): subtree spans, direct
//! children and descendant clip tracks over the one ordered `Project.tracks` list.
//!
//! `Track.group` names the immediate parent, and the list holds every group directly before
//! its descendants (depth-first preorder), so a subtree is one contiguous range. This module
//! is the single home of that vocabulary and of the iterative preorder check behind it. It is
//! pure; current validation and edits share it. `members` / `group_member_ids`
//! keep meaning direct children only.
use super::*;
use std::collections::HashMap;
use std::ops::Range;

/// Per-track facts the preorder pass derives, parallel to the track list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Node {
    /// Index of the immediate parent group.
    parent: Option<usize>,
    /// Number of ancestor groups; roots are zero.
    depth: usize,
    /// One past the last index of this track's subtree.
    end: usize,
    group: bool,
    children: usize,
}

/// A checked view of a track list's nesting. Build it with [`Topology::build`] or
/// [`Project::topology`]; every query then reads stable facts, with no further walking
/// of malformed data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Topology {
    ids: Vec<TrackId>,
    nodes: Vec<Node>,
    index: HashMap<TrackId, usize>,
}

/// One or more consecutive sibling subtrees under one parent, as a structural edit would
/// take them (see [`Topology::sibling_run`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiblingRun {
    /// The common parent group; None for roots.
    pub parent: Option<TrackId>,
    /// The subtree roots in list order.
    pub roots: Vec<TrackId>,
    /// The contiguous list range the roots' subtrees cover.
    pub span: Range<usize>,
}

impl Topology {
    /// Check the nesting of `tracks` in one iterative preorder pass, keeping a stack of the
    /// open ancestor groups. Rejects duplicate ids, a missing or non-Group parent, a track
    /// that is its own parent or part of a parent cycle, a parent after its child, and a
    /// subtree that continues after it closed. It does not check that groups are nonempty or
    /// hold no clips; see [`Topology::childless_groups`].
    pub fn build(tracks: &[Track]) -> ModelResult<Self> {
        let mut index = HashMap::with_capacity(tracks.len());
        for (i, track) in tracks.iter().enumerate() {
            require(index.insert(track.id, i).is_none(), "duplicate track ID")?;
        }
        let mut nodes: Vec<Node> = Vec::with_capacity(tracks.len());
        // Indices of the groups still open, outermost first.
        let mut open: Vec<usize> = Vec::new();
        for (i, track) in tracks.iter().enumerate() {
            let parent = match track.group {
                None => None,
                Some(id) => Some(Self::parent_index(tracks, &index, i, id)?),
            };
            match parent {
                None => Self::close(&mut nodes, &mut open, 0, i),
                Some(p) => {
                    let depth = open.iter().position(|&o| o == p).ok_or_else(|| {
                        err("a group's subtree must not continue after it closed")
                    })? + 1;
                    Self::close(&mut nodes, &mut open, depth, i);
                }
            }
            nodes.push(Node {
                parent,
                depth: open.len(),
                // A plain track ends itself; a group's end waits for the close that follows.
                end: if track.is_group() {
                    tracks.len()
                } else {
                    i + 1
                },
                group: track.is_group(),
                children: 0,
            });
            if let Some(p) = parent {
                nodes[p].children += 1;
            }
            if track.is_group() {
                open.push(i);
            }
        }
        Ok(Self {
            ids: tracks.iter().map(|t| t.id).collect(),
            nodes,
            index,
        })
    }

    /// The index of track `i`'s parent `id`, or why it cannot be the parent.
    fn parent_index(
        tracks: &[Track],
        index: &HashMap<TrackId, usize>,
        i: usize,
        id: TrackId,
    ) -> ModelResult<usize> {
        require(id != tracks[i].id, "a track cannot be its own group")?;
        let p = *index
            .get(&id)
            .ok_or_else(|| err("group references missing track"))?;
        require(
            tracks[p].is_group(),
            "a track's group must be a group track",
        )?;
        if p > i {
            // A parent after its child: name a cycle when walking up from the parent reaches
            // the child again.
            let mut at = Some(p);
            for _ in 0..tracks.len() {
                at = at
                    .and_then(|a| tracks[a].group)
                    .and_then(|g| index.get(&g).copied());
                require(at != Some(i), "groups cannot form a cycle")?;
            }
            return Err(err("a group must precede its members"));
        }
        Ok(p)
    }

    /// Close every open group at `depth` or deeper, ending its subtree at index `at`.
    fn close(nodes: &mut [Node], open: &mut Vec<usize>, depth: usize, at: usize) {
        for closed in open.drain(depth..) {
            nodes[closed].end = at;
        }
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// The list index of `id`; None when it is not in the list.
    pub fn index_of(&self, id: TrackId) -> Option<usize> {
        self.index.get(&id).copied()
    }

    /// The immediate parent group; None for a root or an unknown id.
    pub fn parent(&self, id: TrackId) -> Option<TrackId> {
        self.node(id)?.parent.map(|p| self.ids[p])
    }

    /// The number of ancestor groups: zero for a root; None for an unknown id.
    pub fn depth(&self, id: TrackId) -> Option<usize> {
        self.node(id).map(|n| n.depth)
    }

    /// The largest ancestor count in the list: zero with no groups, one when every group is a
    /// root (flat grouping), more once groups nest.
    pub fn max_depth(&self) -> usize {
        self.nodes.iter().map(|n| n.depth).max().unwrap_or(0)
    }

    /// The list range of a track and all its descendants. A plain track's span has length
    /// one; None for an unknown id.
    pub fn span(&self, id: TrackId) -> Option<Range<usize>> {
        let i = self.index_of(id)?;
        Some(i..self.nodes[i].end)
    }

    /// The track and every descendant in list order, as one slice.
    pub fn subtree(&self, id: TrackId) -> Option<&[TrackId]> {
        Some(&self.ids[self.span(id)?])
    }

    /// Direct children in list order (what `Project::members` yields); empty for a plain
    /// track or an unknown id.
    pub fn children(&self, id: TrackId) -> Vec<TrackId> {
        let Some(span) = self.span(id) else {
            return Vec::new();
        };
        let mut children = Vec::new();
        let mut at = span.start + 1;
        while at < span.end {
            children.push(self.ids[at]);
            at = self.nodes[at].end;
        }
        children
    }

    /// The clip tracks below a group in list order, excluding every group header and
    /// including those hidden by any fold. Empty for a plain track or an unknown id.
    pub fn descendant_clip_tracks(&self, id: TrackId) -> Vec<TrackId> {
        let Some(span) = self.span(id) else {
            return Vec::new();
        };
        (span.start + 1..span.end)
            .filter(|&at| !self.nodes[at].group)
            .map(|at| self.ids[at])
            .collect()
    }

    /// Whether `id` lies strictly inside `ancestor`'s subtree.
    pub fn is_descendant(&self, id: TrackId, ancestor: TrackId) -> bool {
        match (self.index_of(id), self.span(ancestor)) {
            (Some(i), Some(span)) => i > span.start && i < span.end,
            _ => false,
        }
    }

    /// Groups with no direct child, which the nonempty-group policy forbids.
    pub fn childless_groups(&self) -> impl Iterator<Item = TrackId> + '_ {
        self.nodes
            .iter()
            .zip(&self.ids)
            .filter(|(n, _)| n.group && n.children == 0)
            .map(|(_, id)| *id)
    }

    /// Reduce `selected` to subtree roots in list order: a selected group subsumes every
    /// selected descendant, and selecting every child never selects the parent. Rejects an
    /// empty selection and an unknown or repeated id. Shared by structural edits
    /// ([`Topology::sibling_run`]) and by Copy and Duplicate, which each collect a subtree once.
    pub fn subtree_roots(&self, selected: &[TrackId]) -> ModelResult<Vec<TrackId>> {
        Ok(self
            .root_indices(selected)?
            .into_iter()
            .map(|r| self.ids[r])
            .collect())
    }

    fn root_indices(&self, selected: &[TrackId]) -> ModelResult<Vec<usize>> {
        let mut picked = vec![false; self.ids.len()];
        for id in selected {
            let i = self
                .index_of(*id)
                .ok_or_else(|| err("selection includes an unknown track"))?;
            require(
                !std::mem::replace(&mut picked[i], true),
                "selection repeats a track",
            )?;
        }
        let mut roots: Vec<usize> = Vec::new();
        let mut at = 0;
        while at < self.ids.len() {
            if picked[at] {
                roots.push(at);
                at = self.nodes[at].end;
            } else {
                at += 1;
            }
        }
        require(!roots.is_empty(), "selection is empty")?;
        Ok(roots)
    }

    /// Plan a structural selection: reduce `selected` to subtree roots (see
    /// [`Topology::subtree_roots`]), then require those roots to be consecutive siblings of
    /// one parent. Rejects an empty selection, an unknown or repeated id, roots under
    /// different parents (a mixed-depth partial selection) and roots separated by an
    /// unselected sibling.
    pub fn sibling_run(&self, selected: &[TrackId]) -> ModelResult<SiblingRun> {
        let roots = self.root_indices(selected)?;
        let (first, last) = (roots[0], roots[roots.len() - 1]);
        let parent = self.nodes[first].parent;
        require(
            roots.iter().all(|&r| self.nodes[r].parent == parent),
            "selected tracks must share one parent group",
        )?;
        require(
            roots
                .windows(2)
                .all(|pair| self.nodes[pair[0]].end == pair[1]),
            "selected tracks must be adjacent",
        )?;
        Ok(SiblingRun {
            parent: parent.map(|p| self.ids[p]),
            roots: roots.iter().map(|&r| self.ids[r]).collect(),
            span: first..self.nodes[last].end,
        })
    }

    fn node(&self, id: TrackId) -> Option<&Node> {
        self.index_of(id).map(|i| &self.nodes[i])
    }
}

fn err(message: &str) -> ModelError {
    ModelError(message.to_owned())
}

impl Project {
    /// The checked nesting of `tracks`, or the first structural error.
    pub fn topology(&self) -> ModelResult<Topology> {
        Topology::build(&self.tracks)
    }

    /// The list range of a track and all its descendants (length one for a plain track);
    /// None for an unknown id, an output or Main, or a malformed list.
    pub fn subtree_span(&self, id: TrackId) -> Option<Range<usize>> {
        self.topology().ok()?.span(id)
    }

    /// The tracks a Copy of `selected` carries: each selected subtree root with every
    /// descendant, folded or not, once and in list order. A selected ancestor subsumes its
    /// selected descendants, so overlapping selections never duplicate a track.
    pub fn closure_tracks(&self, selected: &[TrackId]) -> ModelResult<Vec<Track>> {
        let topology = self.topology()?;
        let mut copied = Vec::new();
        for root in topology.subtree_roots(selected)? {
            for id in topology.subtree(root).into_iter().flatten() {
                copied.extend(self.track(*id).cloned());
            }
        }
        Ok(copied)
    }

    /// A track and every descendant, in project order.
    pub fn subtree_ids(&self, id: TrackId) -> Option<Vec<TrackId>> {
        Some(self.topology().ok()?.subtree(id)?.to_vec())
    }

    /// The clip tracks below a group in project order, however deeply nested or folded;
    /// None when `group` is not a group track or the list is malformed. Direct children only
    /// are `group_member_ids`.
    pub fn descendant_clip_track_ids(&self, group: TrackId) -> Option<Vec<TrackId>> {
        self.track(group).filter(|t| t.is_group())?;
        Some(self.topology().ok()?.descendant_clip_tracks(group))
    }
}
