//! Copying and pasting nested group forests (2026-11-06):
//! complete subtrees, fresh ids at every level, preserved slots, devices, folds and routes.
use qtable_engine::{
    edit::{self, ProjectEdit},
    model::*,
};
use std::collections::HashSet;

fn t(id: u64) -> TrackId {
    TrackId(id)
}

/// Outer(Inner(Deep(a=1, b=2)), c=3), Other(d=4), e=5. Everything but Inner is folded, so
/// copies must reach hidden descendants. Tracks 1 and 2 hold clips with hits on their own
/// Target and gaps between them; Outer's descendants route to their groups.
fn project() -> Project {
    let mut p = Project::default();
    for at in 0..3 {
        edit::apply(
            &mut p,
            ProjectEdit::AddScene {
                at,
                name: format!("S{at}"),
            },
        )
        .unwrap();
    }
    let slots = p.scenes.len();
    p.tracks = [
        (10, true, None),
        (20, true, Some(10)),
        (30, true, Some(20)),
        (1, false, Some(30)),
        (2, false, Some(30)),
        (3, false, Some(10)),
        (40, true, None),
        (4, false, Some(40)),
        (5, false, None),
    ]
    .into_iter()
    .map(|(id, group, parent)| {
        let mut track = if group {
            Track::new_group(t(id), format!("G{id}"), slots)
        } else {
            Track::new(t(id), format!("T{id}"), slots)
        };
        track.group = parent.map(t);
        track.route = if parent.is_some() {
            Route::Group
        } else {
            Route::single(p.main.id)
        };
        track.fold_state = group && id != 20;
        track
    })
    .collect();
    p[t(30)].path = Path::new("/deep").unwrap();
    for (track, slot, clip_id) in [(1, 0, 1), (1, 2, 2), (2, 1, 3)] {
        let mut clip = Clip::new(
            ClipId(clip_id),
            format!("c{clip_id}"),
            BeatTime(960),
            LIVE_CLIP_PALETTE[2],
        )
        .unwrap();
        let hit = p
            .hit(
                t(track),
                BeatTime(0),
                OscAddress::Relative("/x".into()),
                vec![OscArgument::Int(clip_id as i32)],
            )
            .unwrap();
        clip.events.push(hit);
        p[t(track)].slots[slot].clip = Some(clip);
    }
    p.validate().unwrap();
    p
}

fn order(p: &Project) -> Vec<u64> {
    p.tracks.iter().map(|track| track.id.0).collect()
}

fn paste(
    p: &mut Project,
    tracks: Vec<Track>,
    before: Option<u64>,
    group: Option<u64>,
) -> Result<edit::EditReport, ModelError> {
    edit::apply(
        p,
        ProjectEdit::PasteTracks {
            tracks,
            before: before.map(t),
            group: group.map(t),
        },
    )
}

fn copy(p: &Project, selected: &[u64]) -> Vec<Track> {
    let ids: Vec<_> = selected.iter().copied().map(t).collect();
    p.closure_tracks(&ids).unwrap()
}

/// Copy `selected` from the project itself and paste it back.
fn paste_copy(
    p: &mut Project,
    selected: &[u64],
    before: Option<u64>,
    group: Option<u64>,
) -> Result<edit::EditReport, ModelError> {
    let tracks = copy(p, selected);
    paste(p, tracks, before, group)
}

/// The tracks the project gained, in list order.
fn fresh(before: &Project, after: &Project) -> Vec<Track> {
    after
        .tracks
        .iter()
        .filter(|track| before.track(track.id).is_none())
        .cloned()
        .collect()
}

#[test]
fn a_nested_forest_pastes_with_fresh_ids_remapped_at_every_level() {
    let before = project();
    let mut p = before.clone();
    // Selecting the folded outer group alone brings the whole hidden subtree.
    let source = copy(&p, &[10]);
    assert_eq!(
        source.iter().map(|x| x.id.0).collect::<Vec<_>>(),
        [10, 20, 30, 1, 2, 3]
    );
    paste(&mut p, source.clone(), Some(5), None).unwrap();
    let new = fresh(&before, &p);
    assert_eq!(new.len(), 6);
    assert_eq!(order(&p)[..9], [10, 20, 30, 1, 2, 3, 40, 4, new[0].id.0]);
    // Every id is new, and every internal link names the copy of the same source parent.
    let used: HashSet<_> = before.tracks.iter().map(|x| x.id).collect();
    assert!(new.iter().all(|x| !used.contains(&x.id)));
    let parents: Vec<_> = new.iter().map(|x| x.group).collect();
    let [outer, inner, deep, a, b, c] = new.as_slice() else {
        panic!("{new:?}")
    };
    assert_eq!(
        parents,
        [
            None,
            Some(outer.id),
            Some(inner.id),
            Some(deep.id),
            Some(deep.id),
            Some(outer.id)
        ]
    );
    // Folds, names, paths, routes and slot gaps come through; copied roots are top-level.
    assert_eq!(
        new.iter().map(|x| x.fold_state).collect::<Vec<_>>(),
        [true, false, true, false, false, false]
    );
    assert_eq!(deep.path, Path::new("/deep").unwrap());
    assert_eq!(outer.route, Route::single(p.main.id));
    assert!(
        [inner, deep, a, b, c]
            .iter()
            .all(|x| x.route == Route::Group)
    );
    assert_eq!((a.name.as_str(), b.name.as_str()), ("T1", "T2"),);
    let clips = |x: &Track| -> Vec<Option<ClipId>> {
        x.slots
            .iter()
            .map(|s| s.clip.as_ref().map(|c| c.id))
            .collect()
    };
    let clip_ids: Vec<_> = [a, b].iter().flat_map(|x| clips(x)).flatten().collect();
    let source_clips: HashSet<_> = [ClipId(1), ClipId(2), ClipId(3)].into();
    assert_eq!(clip_ids.len(), 3);
    assert!(clip_ids.iter().all(|id| !source_clips.contains(id)));
    assert!(clips(a)[1].is_none() && clips(b)[0].is_none() && clips(b)[2].is_none());
    // Each copied hit plays on its own copy's Target device.
    for (copy, original) in [(a, t(1)), (b, t(2))] {
        let device = copy.devices[0].id;
        assert!(
            copy.devices
                .iter()
                .all(|d| before[original].devices.iter().all(|o| o.id != d.id))
        );
        for clip in copy.slots.iter().filter_map(|s| s.clip.as_ref()) {
            assert!(clip.events.iter().all(|e| e.definition.device == device));
        }
    }
    assert_eq!(p[t(1)], before[t(1)], "the source stays untouched");
    p.validate().unwrap();
}

#[test]
fn a_selected_ancestor_and_its_descendants_copy_each_track_once() {
    let p = project();
    let once = copy(&p, &[10]);
    for selected in [[10, 20, 30, 1, 3], [1, 3, 30, 20, 10], [2, 10, 20, 1, 30]] {
        assert_eq!(copy(&p, &selected), once, "{selected:?}");
    }
    // A leaf selected beside another subtree adds just itself.
    assert_eq!(
        copy(&p, &[1, 40])
            .iter()
            .map(|x| x.id.0)
            .collect::<Vec<_>>(),
        [1, 40, 4]
    );
    let mut q = p.clone();
    paste(&mut q, copy(&p, &[2, 10, 20, 1, 30]), None, None).unwrap();
    assert_eq!(q.tracks.len(), p.tracks.len() + 6);
    q.validate().unwrap();
}

#[test]
fn same_parent_duplicate_lands_after_the_source_subtree_and_keeps_its_route() {
    let before = project();
    // Duplicate Inner(Deep(a, b)) inside Outer: just before c, parent Outer, route kept.
    let mut p = before.clone();
    paste_copy(&mut p, &[20], Some(3), Some(10)).unwrap();
    let new = fresh(&before, &p);
    assert_eq!(new.len(), 4);
    let ids: Vec<_> = new.iter().map(|x| x.id.0).collect();
    assert_eq!(
        order(&p)[..10],
        [10, 20, 30, 1, 2, ids[0], ids[1], ids[2], ids[3], 3]
    );
    assert_eq!(new[0].group, Some(t(10)));
    assert_eq!(new[0].route, Route::Group);
    assert_eq!(new[1].group, Some(new[0].id));
    p.validate().unwrap();
    // Duplicate Deep(a, b), the last child of Inner, whose next list neighbour belongs to
    // Outer: the boundary at Inner's end still keeps the copy inside Inner.
    let mut p = before.clone();
    paste_copy(&mut p, &[30], Some(3), Some(20)).unwrap();
    let new = fresh(&before, &p);
    assert_eq!(new[0].group, Some(t(20)));
    assert_eq!(new[0].route, Route::Group);
    assert_eq!(p.group_member_ids(t(20)).unwrap(), [t(30), new[0].id]);
    p.validate().unwrap();
    // Duplicating a root group at the end keeps it at top level.
    let mut p = before.clone();
    paste_copy(&mut p, &[40], Some(5), None).unwrap();
    assert_eq!(fresh(&before, &p)[0].group, None);
    p.validate().unwrap();
}

#[test]
fn a_nested_copy_pasted_to_another_level_follows_the_join_and_leave_route_rules() {
    let before = project();
    // Leaving every group for the top level: the root's Group route becomes Main, and its
    // descendants keep theirs.
    let mut p = before.clone();
    paste_copy(&mut p, &[20], None, None).unwrap();
    let new = fresh(&before, &p);
    assert_eq!(new[0].group, None);
    assert_eq!(new[0].route, Route::single(p.main.id));
    assert!(new[1..].iter().all(|x| x.route == Route::Group));
    // Joining another group routes the root to it.
    let mut p = before.clone();
    paste_copy(&mut p, &[30], Some(4), Some(40)).unwrap();
    let new = fresh(&before, &p);
    assert_eq!(new[0].group, Some(t(40)));
    assert_eq!(new[0].route, Route::Group);
    // A root group with an explicit route still takes Group routing when it joins a group,
    // as a plain track does.
    let mut p = before.clone();
    paste_copy(&mut p, &[40], Some(3), Some(10)).unwrap();
    assert_eq!(fresh(&before, &p)[0].route, Route::Group);
    p.validate().unwrap();
}

#[test]
fn a_copy_outlives_its_source_parent() {
    let before = project();
    let source = copy(&before, &[20]);
    let mut p = before.clone();
    edit::apply(
        &mut p,
        ProjectEdit::RemoveTracks {
            tracks: vec![t(10)],
        },
    )
    .unwrap();
    assert_eq!(order(&p), [40, 4, 5]);
    // The copied root still names the deleted Outer; it pastes as a plain subtree.
    assert_eq!(source[0].group, Some(t(10)));
    let mut top = p.clone();
    paste(&mut top, source.clone(), None, None).unwrap();
    let new = fresh(&p, &top);
    assert_eq!(new[0].group, None);
    assert_eq!(new[0].route, Route::single(top.main.id));
    top.validate().unwrap();
    let mut inside = p.clone();
    paste(&mut inside, source, Some(4), Some(40)).unwrap();
    let new = fresh(&p, &inside);
    assert_eq!(
        (new[0].group, new[0].route.clone()),
        (Some(t(40)), Route::Group)
    );
    inside.validate().unwrap();
}

#[test]
fn a_member_copied_alone_pastes_as_a_plain_track_whose_parent_is_unknown() {
    let before = project();
    let mut p = before.clone();
    paste_copy(&mut p, &[1, 3], None, None).unwrap();
    let new = fresh(&before, &p);
    assert_eq!(new.len(), 2);
    assert!(
        new.iter()
            .all(|x| x.group.is_none() && x.route == Route::single(p.main.id))
    );
    p.validate().unwrap();
}

#[test]
fn partial_or_malformed_copied_groups_reject_without_changing_the_project() {
    let before = project();
    let whole = copy(&before, &[10]);
    let pick = |ids: &[u64]| -> Vec<Track> {
        ids.iter()
            .map(|id| whole.iter().find(|x| x.id == t(*id)).unwrap().clone())
            .collect()
    };
    let mut interleaved = pick(&[10, 20, 30, 1, 3, 2]);
    interleaved[4].group = Some(t(10));
    let mut plain_parent = pick(&[1, 2]);
    plain_parent[1].group = Some(t(1));
    let mut repeated = pick(&[10, 20, 30, 1, 2, 3]);
    repeated.push(repeated[3].clone());
    for (label, tracks) in [
        ("empty", vec![]),
        ("header alone", pick(&[10])),
        ("inner header alone", pick(&[20])),
        ("inner group emptied", pick(&[10, 20, 3])),
        ("header after its child", pick(&[1, 30])),
        ("child before its group", pick(&[10, 30, 20, 1, 2, 3])),
        ("a subtree that reopens", interleaved),
        ("a plain track as a parent", plain_parent),
        ("a repeated track", repeated),
        ("Main", vec![before.main.clone()]),
    ] {
        let mut p = before.clone();
        assert!(paste(&mut p, tracks, None, None).is_err(), "{label}");
        assert_eq!(p, before, "{label}");
    }
}

#[test]
fn boundaries_that_split_a_subtree_or_mismatch_the_parent_reject_unchanged() {
    let before = project();
    let one = copy(&before, &[5]);
    for (label, before_id, group) in [
        ("inside Deep from outside it", Some(2), None),
        ("inside Deep under Outer", Some(2), Some(10)),
        ("a boundary outside the parent", Some(5), Some(10)),
        ("unknown group", None, Some(99)),
        ("unknown boundary", Some(99), None),
        ("a plain track as the parent", None, Some(1)),
    ] {
        let mut p = before.clone();
        assert!(
            paste(&mut p, one.clone(), before_id, group).is_err(),
            "{label}"
        );
        assert_eq!(p, before, "{label}");
    }
    // Before the first child of a nested group, from outside it, means before that group.
    let mut p = before.clone();
    paste(&mut p, one.clone(), Some(1), Some(20)).unwrap();
    assert_eq!(
        p.group_member_ids(t(20)).unwrap()[0],
        fresh(&before, &p)[0].id
    );
    let mut p = before.clone();
    paste(&mut p, one, Some(1), None).unwrap();
    assert_eq!(order(&p)[0], fresh(&before, &p)[0].id.0);
    p.validate().unwrap();
}

#[test]
fn copies_carry_no_mappings_and_removing_none_leaves_the_originals_bound() {
    let mut before = project();
    for control in [ControlRef::TrackSolo(t(30)), ControlRef::TrackSolo(t(1))] {
        edit::apply(
            &mut before,
            ProjectEdit::EditMapping(edit::MappingEdit::Add {
                source: Source::Key(KeySource {
                    key: KeyName("a".into()),
                }),
                control,
                min: 0.0,
                max: 1.0,
            }),
        )
        .unwrap();
    }
    let mut p = before.clone();
    let report = paste_copy(&mut p, &[10], None, None).unwrap();
    assert_eq!(report.removed_mappings, 0);
    assert_eq!(p.mappings, before.mappings);
}

#[test]
fn a_paste_is_one_atomic_edit_with_slots_refit_to_the_scene_list() {
    let before = project();
    let source = copy(&before, &[10]);
    let mut p = before.clone();
    let scene = p.scenes[2].id;
    edit::apply(&mut p, ProjectEdit::RemoveScene { scene }).unwrap();
    // Track 1's clip in the last scene fell off the bottom: scenes return for it, and every
    // copied level, headers included, gets slots for every scene.
    paste(&mut p, source, None, None).unwrap();
    assert_eq!(p.scenes.len(), 3);
    assert!(p.tracks.iter().all(|x| x.slots.len() == 3));
    p.validate().unwrap();
}
