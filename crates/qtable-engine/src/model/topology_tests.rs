//! `Topology` and the `Project` subtree vocabulary (2026-11-06).
use super::*;

const OUTER: TrackId = TrackId(10);
const INNER: TrackId = TrackId(11);
const A: TrackId = TrackId(1);
const B: TrackId = TrackId(2);
const C: TrackId = TrackId(3);
const D: TrackId = TrackId(4);

/// One track per row: `(id, is_group, parent)`.
fn list(rows: &[(u64, bool, Option<u64>)]) -> Vec<Track> {
    rows.iter()
        .map(|&(id, group, parent)| {
            let name = format!("T{id}");
            let mut track = if group {
                Track::new_group(TrackId(id), name, 0)
            } else {
                Track::new(TrackId(id), name, 0)
            };
            track.group = parent.map(TrackId);
            track
        })
        .collect()
}

/// `Outer(Inner(a, b), c), d`: three levels of depth-first preorder.
fn nested() -> Vec<Track> {
    list(&[
        (10, true, None),
        (11, true, Some(10)),
        (1, false, Some(11)),
        (2, false, Some(11)),
        (3, false, Some(10)),
        (4, false, None),
    ])
}

fn error(tracks: &[Track]) -> String {
    Topology::build(tracks).unwrap_err().0
}

#[test]
fn three_levels_give_spans_depths_and_parents() {
    let t = Topology::build(&nested()).unwrap();
    assert_eq!(t.len(), 6);
    assert_eq!(t.span(OUTER), Some(0..5));
    assert_eq!(t.span(INNER), Some(1..4));
    assert_eq!(t.span(A), Some(2..3));
    assert_eq!(t.span(C), Some(4..5));
    assert_eq!(t.span(D), Some(5..6));
    assert_eq!(t.span(TrackId(99)), None);
    let depths: Vec<_> = [OUTER, INNER, A, B, C, D]
        .iter()
        .map(|&id| t.depth(id).unwrap())
        .collect();
    assert_eq!(depths, [0, 1, 2, 2, 1, 0]);
    assert_eq!(t.max_depth(), 2);
    assert_eq!(t.parent(A), Some(INNER));
    assert_eq!(t.parent(INNER), Some(OUTER));
    assert_eq!(
        (t.parent(OUTER), t.parent(D), t.parent(TrackId(99))),
        (None, None, None)
    );
    assert_eq!(t.index_of(C), Some(4));
}

#[test]
fn subtrees_hold_every_descendant_in_list_order() {
    let t = Topology::build(&nested()).unwrap();
    assert_eq!(t.subtree(OUTER), Some(&[OUTER, INNER, A, B, C][..]));
    assert_eq!(t.subtree(INNER), Some(&[INNER, A, B][..]));
    assert_eq!(t.subtree(D), Some(&[D][..]));
    assert!(t.is_descendant(A, OUTER) && t.is_descendant(A, INNER));
    assert!(!t.is_descendant(C, INNER) && !t.is_descendant(OUTER, OUTER));
    assert!(!t.is_descendant(D, OUTER) && !t.is_descendant(A, TrackId(99)));
}

#[test]
fn children_are_direct_only_and_leaves_skip_group_headers() {
    let t = Topology::build(&nested()).unwrap();
    assert_eq!(t.children(OUTER), [INNER, C]);
    assert_eq!(t.children(INNER), [A, B]);
    assert!(t.children(A).is_empty() && t.children(TrackId(99)).is_empty());
    assert_eq!(t.descendant_clip_tracks(OUTER), [A, B, C]);
    assert_eq!(t.descendant_clip_tracks(INNER), [A, B]);
    assert!(t.descendant_clip_tracks(D).is_empty());
}

#[test]
fn sibling_subtrees_keep_their_own_spans_and_leaf_order() {
    // P(a, Q(b)), R(c, S(d, e)), f
    let tracks = list(&[
        (20, true, None),
        (1, false, Some(20)),
        (21, true, Some(20)),
        (2, false, Some(21)),
        (22, true, None),
        (3, false, Some(22)),
        (23, true, Some(22)),
        (4, false, Some(23)),
        (5, false, Some(23)),
        (6, false, None),
    ]);
    let t = Topology::build(&tracks).unwrap();
    assert_eq!(t.span(TrackId(20)), Some(0..4));
    assert_eq!(t.span(TrackId(22)), Some(4..9));
    assert_eq!(t.span(TrackId(23)), Some(6..9));
    assert_eq!(t.depth(TrackId(5)), Some(2));
    assert_eq!(
        t.descendant_clip_tracks(TrackId(22)),
        [TrackId(3), TrackId(4), TrackId(5)]
    );
    assert_eq!(t.children(TrackId(22)), [TrackId(3), TrackId(23)]);
    assert_eq!(
        t.descendant_clip_tracks(TrackId(20)),
        [TrackId(1), TrackId(2)]
    );
}

#[test]
fn a_deeper_chain_closes_every_open_level_at_once() {
    // G1(G2(G3(a))), b: `b` ends all three subtrees.
    let tracks = list(&[
        (31, true, None),
        (32, true, Some(31)),
        (33, true, Some(32)),
        (1, false, Some(33)),
        (2, false, None),
    ]);
    let t = Topology::build(&tracks).unwrap();
    for id in [31, 32, 33] {
        assert_eq!(t.span(TrackId(id)).unwrap().end, 4);
    }
    assert_eq!(t.depth(TrackId(1)), Some(3));
    assert_eq!(t.max_depth(), 3);
}

#[test]
fn a_flat_list_and_an_empty_list_build() {
    let flat = list(&[(1, false, None), (10, true, None), (2, false, Some(10))]);
    let t = Topology::build(&flat).unwrap();
    assert_eq!((t.span(TrackId(10)), t.max_depth()), (Some(1..3), 1));
    let empty = Topology::build(&[]).unwrap();
    assert!(empty.is_empty() && empty.max_depth() == 0);
}

#[test]
fn rejects_duplicate_missing_and_non_group_parents() {
    let mut tracks = nested();
    tracks[5].id = A;
    assert_eq!(error(&tracks), "duplicate track ID");

    let mut tracks = nested();
    tracks[2].group = Some(TrackId(99));
    assert_eq!(error(&tracks), "group references missing track");

    // `c` names plain track `a` as its parent.
    let mut tracks = nested();
    tracks[4].group = Some(A);
    assert_eq!(error(&tracks), "a track's group must be a group track");
}

#[test]
fn rejects_self_parents_cycles_and_forward_parents() {
    let mut tracks = nested();
    tracks[1].group = Some(INNER);
    assert_eq!(error(&tracks), "a track cannot be its own group");

    // Outer under Inner, Inner under Outer.
    let mut tracks = nested();
    tracks[0].group = Some(INNER);
    assert_eq!(error(&tracks), "groups cannot form a cycle");

    // A longer cycle: G1 -> G3 -> G2 -> G1.
    let tracks = list(&[
        (31, true, Some(33)),
        (32, true, Some(31)),
        (33, true, Some(32)),
        (1, false, Some(33)),
    ]);
    assert_eq!(error(&tracks), "groups cannot form a cycle");

    // A member before its group, with no cycle.
    let tracks = list(&[(1, false, Some(10)), (10, true, None)]);
    assert_eq!(error(&tracks), "a group must precede its members");
}

#[test]
fn rejects_a_closed_subtree_that_is_reopened() {
    // Inner closed at `c`; `x` then names Inner again.
    let tracks = list(&[
        (10, true, None),
        (11, true, Some(10)),
        (1, false, Some(11)),
        (3, false, Some(10)),
        (2, false, Some(11)),
    ]);
    assert_eq!(
        error(&tracks),
        "a group's subtree must not continue after it closed"
    );

    // Outer closed by root `d`; `y` names Outer again.
    let tracks = list(&[
        (10, true, None),
        (1, false, Some(10)),
        (4, false, None),
        (2, false, Some(10)),
    ]);
    assert_eq!(
        error(&tracks),
        "a group's subtree must not continue after it closed"
    );
}

#[test]
fn childless_groups_are_reported_not_rejected() {
    let tracks = list(&[(10, true, None), (11, true, None), (1, false, Some(11))]);
    let t = Topology::build(&tracks).unwrap();
    assert_eq!(t.childless_groups().collect::<Vec<_>>(), [OUTER]);
    assert_eq!(t.span(OUTER), Some(0..1));
    assert_eq!(
        Topology::build(&nested())
            .unwrap()
            .childless_groups()
            .count(),
        0
    );
}

fn run(t: &Topology, ids: &[TrackId]) -> Result<SiblingRun, String> {
    t.sibling_run(ids).map_err(|e| e.0)
}

#[test]
fn sibling_runs_reduce_a_selection_to_adjacent_subtree_roots() {
    let t = Topology::build(&nested()).unwrap();
    // Inner and its adjacent sibling `c` wrap together under Outer.
    let both = run(&t, &[INNER, C]).unwrap();
    assert_eq!(
        both,
        SiblingRun {
            parent: Some(OUTER),
            roots: vec![INNER, C],
            span: 1..5,
        }
    );
    // A selected ancestor subsumes its selected descendants.
    let subsumed = run(&t, &[A, INNER, B, C]).unwrap();
    assert_eq!((subsumed.roots, subsumed.span), (vec![INNER, C], 1..5));
    // A group alone, and root siblings Outer + d.
    assert_eq!(run(&t, &[INNER]).unwrap().span, 1..4);
    let root = run(&t, &[D, OUTER]).unwrap();
    assert_eq!(
        (root.parent, root.roots, root.span),
        (None, vec![OUTER, D], 0..6)
    );
    // Selecting every child alone never selects the parent.
    assert_eq!(run(&t, &[A, B]).unwrap().parent, Some(INNER));
    assert_eq!(run(&t, &[INNER, C]).unwrap().parent, Some(OUTER));
}

#[test]
fn sibling_runs_reject_unplannable_selections() {
    let t = Topology::build(&nested()).unwrap();
    assert_eq!(run(&t, &[]), Err("selection is empty".into()));
    assert_eq!(
        run(&t, &[A, TrackId(99)]),
        Err("selection includes an unknown track".into())
    );
    assert_eq!(run(&t, &[A, A]), Err("selection repeats a track".into()));
    // Mixed depth: `b` (inside Inner) with `c` (beside Inner).
    assert_eq!(
        run(&t, &[B, C]),
        Err("selected tracks must share one parent group".into())
    );
    assert_eq!(
        run(&t, &[A, D]),
        Err("selected tracks must share one parent group".into())
    );
    // Same parent but `b` sits between them.
    let flat = list(&[(1, false, None), (2, false, None), (3, false, None)]);
    let t = Topology::build(&flat).unwrap();
    assert_eq!(
        run(&t, &[A, C]),
        Err("selected tracks must be adjacent".into())
    );
}

/// Project holding `nested()` plus an output, with one scene so slots line up.
fn project() -> Project {
    Project {
        tracks: nested(),
        outputs: vec![Track::new_output(TrackId(201), "Synth", "127.0.0.1", 9000)],
        ..Project::default()
    }
}

#[test]
fn project_vocabulary_matches_the_topology_and_leaves_members_direct() {
    let p = project();
    assert!(p.topology().is_ok());
    assert_eq!(p.subtree_span(OUTER), Some(0..5));
    assert_eq!(p.subtree_span(D), Some(5..6));
    assert_eq!(p.subtree_span(TrackId(201)), None);
    assert_eq!(p.subtree_ids(INNER), Some(vec![INNER, A, B]));
    assert_eq!(p.descendant_clip_track_ids(OUTER), Some(vec![A, B, C]));
    assert_eq!(p.descendant_clip_track_ids(INNER), Some(vec![A, B]));
    // Plain tracks, outputs and unknown ids have no descendant clip tracks to list.
    assert_eq!(p.descendant_clip_track_ids(A), None);
    assert_eq!(p.descendant_clip_track_ids(TrackId(201)), None);
    // `members` and `group_member_ids` still mean direct children only.
    assert_eq!(p.group_member_ids(OUTER), Some(vec![INNER, C]));
    assert_eq!(
        p.members(OUTER).map(|t| t.id).collect::<Vec<_>>(),
        [INNER, C]
    );
    assert_eq!(p.group_member_ids(INNER), Some(vec![A, B]));
}

/// What Copy collects: each subtree once, hidden or not.
#[test]
fn a_copy_collects_each_selected_subtree_once_even_with_overlap() {
    let p = project();
    let ids = |selected: &[TrackId]| -> Vec<TrackId> {
        p.closure_tracks(selected)
            .unwrap()
            .iter()
            .map(|t| t.id)
            .collect()
    };
    // A selected ancestor brings every descendant; selected descendants add nothing.
    assert_eq!(ids(&[OUTER]), [OUTER, INNER, A, B, C]);
    assert_eq!(ids(&[A, OUTER, INNER, C]), [OUTER, INNER, A, B, C]);
    assert_eq!(ids(&[D, INNER]), [INNER, A, B, D]);
    // Selecting only the leaves never brings their parents.
    assert_eq!(ids(&[B, A]), [A, B]);
    assert_eq!(ids(&[C, A]), [A, C]);
    for bad in [&[][..], &[A, A], &[TrackId(99)]] {
        assert!(p.closure_tracks(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn a_malformed_project_has_no_subtree_vocabulary() {
    let mut p = project();
    p.tracks[2].group = Some(TrackId(99));
    assert!(p.topology().is_err());
    assert_eq!(p.subtree_span(OUTER), None);
    assert_eq!(p.subtree_ids(OUTER), None);
    assert_eq!(p.descendant_clip_track_ids(OUTER), None);
}

#[test]
fn flat_projects_agree_with_the_existing_member_lookups() {
    let mut p = Project {
        tracks: (1..=4)
            .map(|i| Track::new(TrackId(i), format!("T{i}"), 0))
            .collect(),
        ..Project::default()
    };
    p.insert_group(Track::new_group(TrackId(10), "G", 0), &[B, C]);
    let t = p.topology().unwrap();
    assert_eq!(
        t.children(TrackId(10)),
        p.group_member_ids(TrackId(10)).unwrap()
    );
    assert_eq!(
        t.descendant_clip_tracks(TrackId(10)),
        p.group_member_ids(TrackId(10)).unwrap()
    );
    assert_eq!(p.subtree_span(TrackId(10)), Some(1..4));
}

#[test]
fn nesting_is_accepted_by_project_validation() {
    let mut p = project();
    assert_eq!(p.topology().map(|t| t.max_depth()), Ok(2));
    p.validate().unwrap();
    // A flat arrangement of the same tracks stays valid.
    p.tracks = list(&[(10, true, None), (1, false, Some(10)), (4, false, None)]);
    p.validate().unwrap();
}
