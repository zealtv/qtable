//! Moving a track into, out of or within a group, as dragging a track does in Live.
use crate::model::{ModelError, Project, Route, Track, TrackId};

/// Put `track` just before `before` (or last when None) and make it a member of `group`
/// (or of no group). Groups stay contiguous; `apply` validates the result.
///
/// - Joining a group routes the track to it, as Live sets a grouped track's output to its
///   group track.
/// - Leaving a group returns a `Route::Group` track to Main, as `Ungroup` does;
///   explicit output routes survive.
/// - A group left with no members is removed. Inferred from Live, where a group track
///   cannot be empty; not observed (2026-10-12).
pub(super) fn place_track(
    project: &mut Project,
    track: TrackId,
    before: Option<TrackId>,
    group: Option<TrackId>,
) -> Result<(), ModelError> {
    place_tracks(project, &[track], before, group)
}

pub(super) fn place_tracks(
    project: &mut Project,
    selected: &[TrackId],
    before: Option<TrackId>,
    group: Option<TrackId>,
) -> Result<(), ModelError> {
    let outputs = selected
        .first()
        .is_some_and(|id| project.outputs.iter().any(|t| t.id == *id));
    let list = if outputs {
        &mut project.outputs
    } else {
        &mut project.tracks
    };
    if outputs && group.is_some() {
        return Err(ModelError("an output joins no group".into()));
    }
    let plan = super::placement_plan(list, selected, before, group)?;
    let mut subtrees = Vec::new();
    for span in plan.spans.into_iter().rev() {
        subtrees.push(list.drain(span).collect::<Vec<_>>());
    }
    let block = subtrees.into_iter().rev().flat_map(|mut subtree| {
        let old_group = subtree[0].group;
        reparent(&mut subtree[0], group, old_group, project.main.id, false);
        subtree
    });
    list.splice(plan.destination..plan.destination, block);
    super::groups::remove_empty_ancestors(project)
}

// Root-only joins and promotion routing were accepted on 2026-11-07.
/// The one route policy for parent changes. Joining changes only the subtree root;
/// promotion preserves explicit routes, and inherited routes follow a surviving parent.
pub(super) fn reparent(
    track: &mut Track,
    group: Option<TrackId>,
    old_group: Option<TrackId>,
    main: TrackId,
    promoting: bool,
) {
    if !promoting && group.is_some() && group != old_group {
        track.route = Route::Group;
    } else if group.is_none() && track.route == Route::Group && old_group.is_some() {
        track.route = Route::single(main);
    }
    track.group = group;
}

/// Insert copied subtrees (each a root followed by its descendants, in preorder, none in the
/// list yet) just before `before` (or last) as members of `group` (or of none). Each root's
/// `group` still names where it came from; only the roots change parent, through the shared
/// `reparent` join/leave routing policy, and descendants keep their links and routes.
pub(super) fn enter(
    project: &mut Project,
    subtrees: Vec<Vec<Track>>,
    before: Option<TrackId>,
    group: Option<TrackId>,
) -> Result<(), ModelError> {
    let err = |s: &str| ModelError(s.to_owned());
    let mut to = match before {
        Some(id) => project
            .tracks
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| err("unknown track"))?,
        None => project.tracks.len(),
    };
    // Dropping before a group's first child from outside that group means before the group
    // track, so its run stays whole; the same holds level by level up a chain of first children.
    while to > 0
        && let Some(parent) = project.tracks.get(to).and_then(|t| t.group)
        && group != Some(parent)
        && project.tracks[to - 1].id == parent
    {
        to -= 1;
    }
    // The explicit parent selects the level: the boundary must sit between two of its direct
    // children, or at its subtree's end (top level for none).
    let topology = project.topology()?;
    let between_siblings = match group {
        Some(id) => topology.span(id).is_some_and(|span| {
            to > span.start
                && to <= span.end
                && (to == span.end || topology.parent(project.tracks[to].id) == group)
        }),
        None => to == project.tracks.len() || project.tracks[to].group.is_none(),
    };
    if !between_siblings {
        return Err(err("a paste goes between sibling subtrees of its group"));
    }
    let main = project.main.id;
    let block: Vec<Track> = subtrees
        .into_iter()
        .flat_map(|mut subtree| {
            let old_group = subtree[0].group;
            reparent(&mut subtree[0], group, old_group, main, false);
            subtree
        })
        .collect();
    project.tracks.splice(to..to, block);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::edit::{ProjectEdit, apply};
    use crate::model::{LIVE_CLIP_PALETTE, Project, Route, Track, TrackId};

    /// Tracks 1–4; tracks 2 and 3 form group 1, which sends to output 1.
    fn project() -> Project {
        let mut p = Project {
            outputs: vec![Track::new_output(TrackId(201), "Out", "127.0.0.1", 9000)],
            tracks: (1..=4)
                .map(|i| Track::new(TrackId(i), format!("{i}"), 0))
                .collect(),
            ..Project::default()
        };
        let mut group = Track::new_group(TrackId(101), "G", 0);
        group.colour = LIVE_CLIP_PALETTE[0];
        group.route = Route::single(TrackId(201));
        p.insert_group(group, &[TrackId(2), TrackId(3)]);
        p
    }

    fn place(p: &mut Project, track: u64, before: Option<u64>, group: Option<u64>) {
        apply(
            p,
            ProjectEdit::PlaceTrack {
                track: TrackId(track),
                before: before.map(TrackId),
                group: group.map(|g| TrackId(100 + g)),
            },
        )
        .unwrap();
    }

    fn order(p: &Project) -> Vec<u64> {
        p.clip_tracks().map(|t| t.id.0).collect()
    }

    fn members(p: &Project) -> Vec<Vec<u64>> {
        p.group_tracks()
            .map(|g| p.members(g.id).map(|t| t.id.0).collect())
            .collect()
    }

    #[test]
    fn dropping_a_track_between_members_joins_the_group_and_routes_to_it() {
        let mut p = project();
        place(&mut p, 4, Some(3), Some(1));
        assert_eq!(order(&p), vec![1, 2, 4, 3]);
        assert_eq!(members(&p), vec![vec![2, 4, 3]]);
        assert_eq!(p[TrackId(4)].route, Route::Group);
        // Joining at the group's end: before the next track outside it.
        let mut p = project();
        place(&mut p, 1, Some(4), Some(1));
        assert_eq!(order(&p), vec![2, 3, 1, 4]);
        assert_eq!(members(&p), vec![vec![2, 3, 1]]);
    }

    #[test]
    fn dragging_a_member_out_returns_group_routing_to_main() {
        let mut p = project();
        p[TrackId(2)].route = Route::Group;
        place(&mut p, 2, None, None);
        assert_eq!(order(&p), vec![1, 3, 4, 2]);
        assert_eq!(members(&p), vec![vec![3]]);
        assert_eq!(p[TrackId(2)].route, Route::single(p.main.id));
        // The last member leaving removes the group.
        place(&mut p, 3, Some(1), None);
        assert_eq!(p.group_tracks().count(), 0);
        assert_eq!(order(&p), vec![3, 1, 4, 2]);
    }

    #[test]
    fn moving_within_a_group_keeps_membership_and_route() {
        let mut p = project();
        p[TrackId(3)].route = Route::single(TrackId(201));
        place(&mut p, 3, Some(2), Some(1));
        assert_eq!(members(&p), vec![vec![3, 2]]);
        assert_eq!(p[TrackId(3)].route, Route::single(TrackId(201)));
    }

    #[test]
    fn a_drop_that_would_split_a_group_or_names_unknowns_is_refused_unchanged() {
        let before = project();
        let mut p = before.clone();
        for edit in [
            (TrackId(4), Some(TrackId(3)), None),
            (TrackId(4), Some(TrackId(4)), None),
            (TrackId(9), None, None),
            (TrackId(4), Some(TrackId(9)), None),
            (TrackId(4), None, Some(TrackId(109))),
            (TrackId(1), None, Some(TrackId(101))),
        ] {
            let (track, before, group) = edit;
            let edit = ProjectEdit::PlaceTrack {
                track,
                before,
                group,
            };
            assert!(
                apply(&mut p, edit).is_err(),
                "{track:?} {before:?} {group:?}"
            );
        }
        assert_eq!(p, before);
    }

    #[test]
    fn a_group_moves_as_an_unchanged_block_before_a_track_and_last() {
        let mut p = project();
        let original: Vec<_> = p
            .tracks
            .iter()
            .filter(|t| t.id == TrackId(101) || t.group == Some(TrackId(101)))
            .cloned()
            .collect();
        place(&mut p, 101, Some(1), None);
        assert_eq!(
            p.tracks.iter().map(|t| t.id.0).collect::<Vec<_>>(),
            [101, 2, 3, 1, 4]
        );
        assert_eq!(&p.tracks[..3], original.as_slice());
        place(&mut p, 101, None, None);
        assert_eq!(
            p.tracks.iter().map(|t| t.id.0).collect::<Vec<_>>(),
            [1, 4, 101, 2, 3]
        );
        assert_eq!(&p.tracks[2..], original.as_slice());
    }

    #[test]
    fn group_placement_rejects_split_and_self_boundaries_atomically() {
        let mut p = project();
        let mut other = Track::new_group(TrackId(102), "Other", 0);
        other.colour = LIVE_CLIP_PALETTE[1];
        other.route = Route::single(TrackId(201));
        p.insert_group(other, &[TrackId(4)]);
        let original = p.clone();
        for (before, group) in [
            (Some(TrackId(1)), Some(TrackId(102))),
            (Some(TrackId(4)), None),
            (Some(TrackId(2)), None),
            (Some(TrackId(3)), None),
            (Some(TrackId(101)), None),
            (Some(TrackId(999)), None),
            (Some(TrackId(201)), None),
        ] {
            assert!(
                apply(
                    &mut p,
                    ProjectEdit::PlaceTrack {
                        track: TrackId(101),
                        before,
                        group
                    }
                )
                .is_err()
            );
            assert_eq!(p, original);
        }
        place(&mut p, 101, Some(102), None);
        assert_eq!(
            p.tracks.iter().map(|t| t.id.0).collect::<Vec<_>>(),
            [1, 101, 2, 3, 102, 4]
        );
        assert_eq!(members(&p), [vec![2, 3], vec![4]]);
    }

    #[test]
    fn outputs_reorder_among_outputs_only() {
        let mut p = project();
        for (id, name) in [(202, "B"), (203, "C")] {
            p.outputs.push(Track::new_output(
                TrackId(id),
                name,
                "127.0.0.1",
                9000 + id as u16,
            ));
        }
        let ids = |p: &Project| p.outputs.iter().map(|o| o.id.0).collect::<Vec<_>>();
        let place_out = |p: &mut Project, track: u64, before: Option<u64>, group: Option<u64>| {
            apply(
                p,
                ProjectEdit::PlaceTrack {
                    track: TrackId(track),
                    before: before.map(TrackId),
                    group: group.map(TrackId),
                },
            )
        };
        place_out(&mut p, 203, Some(201), None).unwrap();
        assert_eq!(ids(&p), [203, 201, 202]);
        place_out(&mut p, 203, None, None).unwrap();
        assert_eq!(ids(&p), [201, 202, 203]);
        // Not among tracks, not in a group, not before itself.
        assert!(place_out(&mut p, 201, Some(1), None).is_err());
        assert!(place_out(&mut p, 201, None, Some(101)).is_err());
        assert!(place_out(&mut p, 201, Some(201), None).is_err());
        assert_eq!(ids(&p), [201, 202, 203]);
        // A track cannot be placed before an output.
        assert!(place_out(&mut p, 1, Some(202), None).is_err());
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use crate::edit::{ProjectEdit, apply};

    #[test]
    fn output_batch_uses_the_same_order_plan_and_rejects_cross_region_sources() {
        let mut p = Project {
            tracks: vec![Track::new(TrackId(1), "T", 0)],
            outputs: (201..=204)
                .map(|n| Track::new_output(TrackId(n), "Out", "localhost", 9000))
                .collect(),
            ..Default::default()
        };
        apply(
            &mut p,
            ProjectEdit::PlaceTracks {
                tracks: vec![TrackId(204), TrackId(202)],
                before: Some(TrackId(201)),
                group: None,
            },
        )
        .unwrap();
        assert_eq!(
            p.outputs.iter().map(|t| t.id.0).collect::<Vec<_>>(),
            [202, 204, 201, 203]
        );
        for tracks in [
            vec![TrackId(202), p.tracks[0].id],
            vec![p.tracks[0].id, TrackId(202)],
        ] {
            let before = p.clone();
            assert!(
                apply(
                    &mut p,
                    ProjectEdit::PlaceTracks {
                        tracks,
                        before: None,
                        group: None
                    }
                )
                .is_err()
            );
            assert_eq!(p, before);
        }
    }
}
