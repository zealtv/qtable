//! Nested structural commands preserve subtree identity, slots and mapping ownership.
use qtable_engine::{
    edit::{self, MappingEdit, ProjectEdit},
    engine::{Command, Engine},
    file,
    model::*,
};

fn t(id: u64) -> TrackId {
    TrackId(id)
}

/// Outer(Inner(Deep(a,b)), c), Other(d), e; gaps survive every structural operation.
fn project() -> Project {
    let mut p = file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/v10-groups-mappings.ron"
    ))
    .unwrap();
    p.mappings.clear();
    let devices = p[t(1)].devices.clone();
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
    p[t(1)].devices = devices;
    edit::apply(
        &mut p,
        ProjectEdit::CreateClip {
            track: t(1),
            slot: 0,
            length: BeatTime::from_beats(4).unwrap(),
        },
    )
    .unwrap();
    let last = slots - 1;
    if last > 1 {
        edit::apply(
            &mut p,
            ProjectEdit::CreateClip {
                track: t(1),
                slot: last,
                length: BeatTime::from_beats(4).unwrap(),
            },
        )
        .unwrap();
        p[t(1)].slots[1].has_stop_button = false;
    }
    p.validate().unwrap();
    p
}

fn order(p: &Project) -> Vec<u64> {
    p.tracks.iter().map(|track| track.id.0).collect()
}
fn apply(p: &mut Project, edit: ProjectEdit) {
    edit::apply(p, edit).unwrap();
}
fn place(track: u64, before: Option<u64>, parent: Option<u64>) -> ProjectEdit {
    ProjectEdit::PlaceTrack {
        track: t(track),
        before: before.map(t),
        group: parent.map(t),
    }
}
fn mapping(p: &mut Project, control: ControlRef) {
    apply(
        p,
        ProjectEdit::EditMapping(MappingEdit::Add {
            source: Source::Key(KeySource {
                key: KeyName("a".into()),
            }),
            control,
            min: 0.0,
            max: 1.0,
        }),
    );
}

#[test]
fn group_wraps_groups_and_sibling_subtrees_at_any_level() {
    for selected in [
        vec![20],
        vec![20, 3],
        vec![20, 1, 3],
        vec![1, 2],
        vec![10, 40],
    ] {
        let mut p = project();
        let plan =
            edit::grouping_plan(&p, &selected.into_iter().map(t).collect::<Vec<_>>()).unwrap();
        let before = p.clone();
        let roots = plan.roots.clone();
        apply(
            &mut p,
            ProjectEdit::CreateGroup {
                tracks: roots.clone(),
            },
        );
        let wrapper = &p.tracks[plan.span.start];
        assert!(wrapper.is_group());
        assert_eq!(wrapper.group, plan.parent);
        assert_eq!(
            wrapper.route,
            if plan.parent.is_some() {
                Route::Group
            } else {
                Route::single(p.main.id)
            }
        );
        for old in &before.tracks {
            let mut expected = old.clone();
            if roots.contains(&old.id) {
                expected.group = Some(wrapper.id);
                expected.route = Route::Group;
            }
            assert_eq!(p[old.id], expected);
        }
    }
}

#[test]
fn mixed_partial_duplicate_unknown_and_gapped_grouping_is_atomic() {
    let mut p = project();
    mapping(&mut p, ControlRef::TrackSolo(t(1)));
    let mut engine = Engine::new(p).unwrap();
    for selected in [
        vec![1, 3],
        vec![20, 4],
        vec![1, 1],
        vec![999],
        vec![10, 5],
        vec![],
    ] {
        let before = engine.project().clone();
        let revision = engine.project_revision();
        assert!(
            engine
                .execute(Command::Edit(ProjectEdit::CreateGroup {
                    tracks: selected.into_iter().map(t).collect()
                }))
                .is_err()
        );
        assert_eq!(*engine.project(), before);
        assert_eq!(engine.project_revision(), revision);
    }
}

#[test]
fn invalid_cycle_and_split_subtree_loads_leave_project_revision_and_mappings_unchanged() {
    let mut p = project();
    mapping(&mut p, ControlRef::TrackSelect(t(1)));
    let mut cycle = p.clone();
    cycle[t(10)].group = Some(t(30));
    let mut split = p.clone();
    split.tracks.swap(4, 5); // a, c (Outer), b reopens Deep's subtree.
    let mut engine = Engine::new(p.clone()).unwrap();
    let revision = engine.project_revision();
    for invalid in [cycle, split] {
        assert!(engine.execute(Command::LoadProject(invalid)).is_err());
        assert_eq!(engine.project(), &p);
        assert_eq!(engine.project_revision(), revision);
    }
}

#[test]
fn whole_subtree_reparent_changes_only_root_and_cascades_old_ancestors() {
    let mut p = project();
    let before = p.clone();
    apply(&mut p, place(30, Some(4), Some(40)));
    assert_eq!(order(&p), [10, 3, 40, 30, 1, 2, 4, 5]);
    assert!(p.track(t(20)).is_none());
    assert_eq!(p[t(30)].group, Some(t(40)));
    assert_eq!(p[t(30)].route, Route::Group);
    assert_eq!(p[t(1)], before[t(1)]);
    assert_eq!(p[t(2)], before[t(2)]);
    assert_eq!(p[t(30)].fold_state, before[t(30)].fold_state);
    // Leave to root; the inherited route becomes Main, deeper links stay unchanged.
    apply(&mut p, place(30, None, None));
    assert_eq!(p[t(30)].group, None);
    assert_eq!(p[t(30)].route, Route::single(p.main.id));
    assert_eq!(p[t(1)], before[t(1)]);
}

#[test]
fn explicit_end_boundaries_and_both_directions_keep_subtrees_whole() {
    let mut p = project();
    apply(&mut p, place(40, Some(3), Some(10)));
    assert_eq!(order(&p), [10, 20, 30, 1, 2, 40, 4, 3, 5]);
    apply(&mut p, place(40, Some(5), None));
    assert_eq!(order(&p), [10, 20, 30, 1, 2, 3, 40, 4, 5]);
    // Before 5 is also the end of Outer: the parent determines the level.
    apply(&mut p, place(40, Some(5), Some(10)));
    assert_eq!(p[t(40)].group, Some(t(10)));
    apply(&mut p, ProjectEdit::MoveTrack { from: 6, to: 1 });
    assert_eq!(order(&p), [10, 40, 4, 20, 30, 1, 2, 3, 5]);
    apply(&mut p, ProjectEdit::MoveTrack { from: 1, to: 6 });
    assert_eq!(order(&p), [10, 20, 30, 1, 2, 3, 40, 4, 5]);
}

#[test]
fn invalid_self_descendant_split_stale_and_delay_moves_preserve_revision_and_mappings() {
    let mut p = project();
    mapping(&mut p, ControlRef::TrackSolo(t(30)));
    p[t(30)].delay_ms = 600.0;
    p[t(40)].delay_ms = 500.0;
    p.validate().unwrap();
    let mut engine = Engine::new(p).unwrap();
    for edit in [
        place(20, Some(1), Some(30)),
        place(20, Some(20), None),
        place(20, None, Some(20)),
        place(20, Some(2), Some(10)),
        place(20, Some(999), None),
        place(999, None, None),
        place(20, None, Some(999)),
        place(20, Some(4), Some(40)),
        ProjectEdit::MoveTrack { from: 1, to: 3 },
    ] {
        let before = engine.project().clone();
        let revision = engine.project_revision();
        assert!(
            engine.execute(Command::Edit(edit.clone())).is_err(),
            "{edit:?}"
        );
        assert_eq!(*engine.project(), before, "{edit:?}");
        assert_eq!(engine.project_revision(), revision);
    }
}

#[test]
fn inner_ungroup_promotes_only_direct_children_and_preserves_explicit_routes_and_folds() {
    let mut p = project();
    p[t(30)].route = Route::empty();
    let child = p[t(1)].clone();
    let folded = p[t(30)].fold_state;
    apply(&mut p, ProjectEdit::Ungroup { group: t(20) });
    assert_eq!(p[t(30)].group, Some(t(10)));
    assert_eq!(p[t(30)].route, Route::empty());
    assert_eq!(p[t(30)].fold_state, folded);
    assert_eq!(p[t(1)], child);
    apply(&mut p, ProjectEdit::Ungroup { group: t(30) });
    assert_eq!(p[t(1)].group, Some(t(10)));
    assert_eq!(p[t(1)].route, Route::Group);
}

#[test]
fn batch_ungroup_handles_overlapping_and_disjoint_headers_in_one_revision() {
    for groups in [vec![10, 20], vec![20, 10, 20], vec![10, 40]] {
        let mut p = project();
        for id in [10, 20, 30, 1, 40] {
            mapping(&mut p, ControlRef::TrackSolo(t(id)));
        }
        let mut engine = Engine::new(p).unwrap();
        let revision = engine.project_revision();
        let report = engine
            .execute(Command::Edit(ProjectEdit::UngroupTracks {
                groups: groups.iter().copied().map(t).collect(),
            }))
            .unwrap();
        assert_eq!(engine.project_revision(), revision + 1);
        assert_eq!(report.removed_mappings, 2);
        let p = engine.project();
        for id in &groups {
            assert!(p.track(t(*id)).is_none());
        }
        assert_eq!(
            p[t(30)].group,
            if groups.contains(&20) {
                None
            } else {
                Some(t(20))
            }
        );
        assert_eq!(p[t(1)].group, Some(t(30)));
        assert_eq!(p.mappings.len(), 3);
    }
}

#[test]
fn folded_delete_closes_over_overlap_and_cleans_all_empty_ancestors_and_mappings() {
    for selected in [vec![10], vec![1, 20, 10, 2, 10], vec![1, 2, 3]] {
        let mut p = project();
        for id in [10, 20, 30, 1, 2, 3, 4] {
            mapping(&mut p, ControlRef::TrackSolo(t(id)));
        }
        let survivor = p.mappings.last().unwrap().clone();
        let mut engine = Engine::new(p).unwrap();
        let revision = engine.project_revision();
        let report = engine
            .execute(Command::Edit(ProjectEdit::RemoveTracks {
                tracks: selected.into_iter().map(t).collect(),
            }))
            .unwrap();
        assert_eq!(report.removed_mappings, 6);
        assert_eq!(engine.project_revision(), revision + 1);
        assert_eq!(order(engine.project()), [40, 4, 5]);
        assert_eq!(engine.project().mappings, vec![survivor]);
    }
}

#[test]
fn deleting_the_last_nested_leaf_and_invalid_batch_ungroup_are_atomic() {
    let mut p = project();
    for edit in [
        ProjectEdit::RemoveTracks {
            tracks: vec![t(10), t(40), t(5)],
        },
        ProjectEdit::UngroupTracks {
            groups: vec![t(20), t(999)],
        },
        ProjectEdit::UngroupTracks {
            groups: vec![t(20), t(1)],
        },
    ] {
        let before = p.clone();
        assert!(edit::apply(&mut p, edit).is_err());
        assert_eq!(p, before);
    }
    apply(
        &mut p,
        ProjectEdit::RemoveTracks {
            tracks: vec![t(2), t(3), t(40), t(5)],
        },
    );
    let before = p.clone();
    for id in [1, 30, 20, 10] {
        assert!(
            edit::apply(
                &mut p,
                ProjectEdit::RemoveTracks {
                    tracks: vec![t(id)]
                }
            )
            .is_err()
        );
        assert_eq!(p, before);
    }
}

#[test]
fn moving_the_only_child_subtree_removes_all_empty_ancestors() {
    let mut p = project();
    apply(&mut p, ProjectEdit::RemoveTracks { tracks: vec![t(3)] });
    mapping(&mut p, ControlRef::TrackSolo(t(10)));
    mapping(&mut p, ControlRef::TrackSolo(t(20)));
    mapping(&mut p, ControlRef::TrackSolo(t(1)));
    let report = edit::apply(&mut p, place(30, Some(4), Some(40))).unwrap();
    assert_eq!(order(&p), [40, 30, 1, 2, 4, 5]);
    assert_eq!(report.removed_mappings, 2);
    assert_eq!(p.mappings[0].control, ControlRef::TrackSolo(t(1)));
}

#[test]
fn inner_group_route_batches_accept_inheritance_but_root_groups_reject_it() {
    let mut p = project();
    p[t(20)].route = Route::empty();
    apply(
        &mut p,
        ProjectEdit::SetTrackRoutes {
            routes: vec![(t(20), Route::Group)],
        },
    );
    let before = p.clone();
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::SetTrackRoutes {
                routes: vec![(t(10), Route::Group), (t(10), Route::empty())]
            }
        )
        .is_err()
    );
    assert_eq!(p, before);
}

#[test]
fn three_level_format_11_round_trip_preserves_folds_routes_mappings_and_slot_gaps() {
    let mut p = project();
    mapping(&mut p, ControlRef::TrackSolo(t(20)));
    mapping(&mut p, ControlRef::TrackSolo(t(1)));
    p[t(20)].route = Route::empty();
    let path = std::env::temp_dir().join(format!(
        "qtable-nested-structural-{}.ron",
        std::process::id()
    ));
    file::save(&path, &p).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("version: 12"));
    let loaded = file::load(&path).unwrap();
    assert_eq!(loaded, p);
    assert_eq!(loaded.topology().unwrap().max_depth(), 3);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    file::save(&path, &loaded).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn device_and_mixer_bindings_follow_survivors_and_disappear_only_with_their_owner() {
    let mut p = project();
    let event = ControlRef::Event {
        track: t(1),
        event: EventRef {
            device: DeviceId(1),
            key: EntryKey("hit".into()),
        },
    };
    let volume = ControlRef::Argument(FieldRef::volume(t(20)));
    for control in [
        event.clone(),
        volume,
        ControlRef::TrackSelect(t(30)),
        ControlRef::TrackSolo(t(4)),
    ] {
        mapping(&mut p, control);
    }
    let leaf_binding = p.mappings[0].clone();
    let report = edit::apply(&mut p, ProjectEdit::Ungroup { group: t(20) }).unwrap();
    assert_eq!(report.removed_mappings, 1);
    assert_eq!(p.mappings[0], leaf_binding);
    let report = edit::apply(&mut p, place(30, Some(4), Some(40))).unwrap();
    assert_eq!(report.removed_mappings, 0);
    assert_eq!(p.mappings[0], leaf_binding);
    let report = edit::apply(
        &mut p,
        ProjectEdit::RemoveTracks {
            tracks: vec![t(30)],
        },
    )
    .unwrap();
    assert_eq!(report.removed_mappings, 2);
    assert_eq!(p.mappings.len(), 1);
    assert_eq!(p.mappings[0].control, ControlRef::TrackSolo(t(4)));
}

#[test]
fn create_and_place_and_move_each_publish_one_revision() {
    let mut engine = Engine::new(project()).unwrap();
    for edit in [
        ProjectEdit::CreateGroup {
            tracks: vec![t(20), t(3)],
        },
        place(20, Some(4), Some(40)),
        ProjectEdit::MoveTrack { from: 0, to: 7 },
    ] {
        let revision = engine.project_revision();
        engine.execute(Command::Edit(edit)).unwrap();
        assert_eq!(engine.project_revision(), revision + 1);
    }
}

fn place_many(tracks: &[u64], before: Option<u64>, group: Option<u64>) -> ProjectEdit {
    ProjectEdit::PlaceTracks {
        tracks: tracks.iter().copied().map(t).collect(),
        before: before.map(t),
        group: group.map(t),
    }
}

#[test]
fn batch_drag_leaves_nested_groups_in_order_and_prunes_only_after_the_batch() {
    let mut p = project();
    let before = p.clone();
    let explicit = Route::single(p.outputs[0].id);
    p[t(2)].route = explicit.clone();
    // Reverse selection order must not reverse the sources. Deep and Inner both empty.
    apply(&mut p, place_many(&[2, 1], Some(5), None));
    assert_eq!(order(&p), [10, 3, 40, 4, 1, 2, 5]);
    assert_eq!(p[t(1)].route, Route::single(p.main.id));
    assert_eq!(p[t(2)].route, explicit);
    assert_eq!(p[t(1)].slots, before[t(1)].slots);
    assert_eq!(p[t(1)].devices, before[t(1)].devices);
    assert_eq!(p[t(10)].fold_state, before[t(10)].fold_state);
}

#[test]
fn batch_drag_normalizes_overlapping_groups_and_routes_only_entering_roots() {
    let mut p = project();
    let before = p.clone();
    // A selected Group plus its explicitly selected descendant and an ordinary track.
    apply(&mut p, place_many(&[5, 1, 20], Some(4), Some(40)));
    assert_eq!(order(&p), [10, 3, 40, 20, 30, 1, 2, 5, 4]);
    for id in [20, 5] {
        assert_eq!(p[t(id)].group, Some(t(40)));
        assert_eq!(p[t(id)].route, Route::Group);
    }
    for id in [30, 1, 2] {
        assert_eq!(p[t(id)], before[t(id)]);
    }
    // Moving all direct children out prunes a source Group once, including cascades.
    apply(&mut p, place_many(&[3, 20, 5, 4], None, None));
    assert!(p.track(t(10)).is_none());
    assert!(p.track(t(40)).is_none());
    p.validate().unwrap();
}

#[test]
fn batch_drag_disjoint_mixed_depth_roots_keep_source_order_and_inherited_children() {
    let mut p = project();
    let before = p.clone();
    apply(&mut p, place_many(&[5, 3, 30], None, None));
    assert_eq!(order(&p), [40, 4, 30, 1, 2, 3, 5]);
    for id in [1, 2] {
        assert_eq!(p[t(id)], before[t(id)]);
    }
    assert_eq!(p[t(30)].group, None);
    assert_eq!(p[t(30)].route, Route::single(p.main.id));
}

#[test]
fn batch_drag_invalid_boundaries_reject_the_whole_transaction() {
    let original = project();
    for edit in [
        place_many(&[], None, None),
        place_many(&[1, 1], None, None),
        place_many(&[1, 999], None, None),
        place_many(&[20, 5], Some(1), Some(30)),
        place_many(&[20, 5], Some(3), Some(30)),
        place_many(&[20, 5], Some(30), None),
        place_many(&[1, 5], Some(4), None),
        place_many(&[1, 5], None, Some(999)),
        place_many(&[1, original.outputs[0].id.0], None, None),
    ] {
        let mut p = original.clone();
        assert!(edit::apply(&mut p, edit.clone()).is_err(), "{edit:?}");
        assert_eq!(p, original);
    }
}

#[test]
fn batch_drag_is_one_engine_revision_and_preserves_surviving_mappings() {
    let mut p = project();
    for id in [1, 20, 30] {
        mapping(&mut p, ControlRef::TrackSelect(t(id)));
    }
    let mut engine = Engine::new(p).unwrap();
    let revision = engine.project_revision();
    let report = engine
        .execute(Command::Edit(place_many(&[2, 1], None, None)))
        .unwrap();
    assert_eq!(engine.project_revision(), revision + 1);
    assert_eq!(report.removed_mappings, 2);
    assert_eq!(engine.project().mappings.len(), 1);
    assert_eq!(
        engine.project().mappings[0].control,
        ControlRef::TrackSelect(t(1))
    );
}
