//! Group tracks: create, ungroup, remove and move.
//!
//! A group track precedes its members in `Project.tracks`, as in Live's track list.
use crate::model::{ModelError, Project, Route, Track, TrackId};

fn err(message: &str) -> ModelError {
    ModelError(message.to_owned())
}

pub(super) fn get_group(project: &mut Project, id: TrackId) -> Result<&mut Track, ModelError> {
    project
        .tracks
        .iter_mut()
        .find(|t| t.id == id && t.is_group())
        .ok_or_else(|| err("unknown group"))
}

/// The grouping planner shared by command validation and client eligibility.
// Decided 2026-11-06: group only consecutive sibling
// subtree roots; selected ancestors subsume descendants, with no widening.
pub fn grouping_plan(
    project: &Project,
    tracks: &[TrackId],
) -> Result<crate::model::SiblingRun, ModelError> {
    project.topology()?.sibling_run(tracks)
}

pub(super) fn create_group(project: &mut Project, tracks: Vec<TrackId>) -> Result<(), ModelError> {
    let plan = grouping_plan(project, &tracks)?;
    let main = project.main.id;
    let id = TrackId(project.next_strip_id());
    let mut group = Track::new_group(
        id,
        format!("Group {}", project.group_tracks().count() + 1),
        project.scenes.len(),
    );
    group.colour = project.take_next_colour();
    group.group = plan.parent;
    group.route = if plan.parent.is_some() {
        Route::Group
    } else {
        Route::single(project.main.id)
    };
    for root in plan.roots {
        super::membership::reparent(&mut project[root], Some(id), plan.parent, main, false);
    }
    project.tracks.insert(plan.span.start, group);
    Ok(())
}

pub(super) fn ungroup(project: &mut Project, group: TrackId) -> Result<(), ModelError> {
    ungroup_tracks(project, vec![group])
}

/// Validate all headers before promoting; inner-first makes overlapping selections
/// independent of the caller's selection order.
// Decided 2026-11-06: ungroup each selected Group one level,
// preserving surviving descendants and fold flags, in one command/revision.
pub(super) fn ungroup_tracks(
    project: &mut Project,
    groups: Vec<TrackId>,
) -> Result<(), ModelError> {
    let topology = project.topology()?;
    for id in &groups {
        get_group(project, *id)?;
    }
    let mut groups = groups;
    groups.sort_by_key(|id| std::cmp::Reverse(topology.index_of(*id)));
    groups.dedup();
    let main = project.main.id;
    for id in groups {
        let parent = project[id].group;
        for child in project.tracks.iter_mut().filter(|t| t.group == Some(id)) {
            super::membership::reparent(child, parent, Some(id), main, true);
        }
        project.tracks.retain(|t| t.id != id);
    }
    Ok(())
}

/// A deletion selection expands to complete subtrees, independent of folds.
// Decided 2026-11-06: deleting a Group deletes its entire
// subtree; the last clip track must survive.
fn deletion_ids(
    project: &Project,
    tracks: &[TrackId],
) -> Result<std::collections::HashSet<TrackId>, ModelError> {
    let topology = project.topology()?;
    let mut doomed = std::collections::HashSet::new();
    for id in tracks {
        let track = project.track(*id).ok_or_else(|| err("unknown track"))?;
        if !track.capabilities().removable {
            return Err(err("this track cannot be removed"));
        }
        if let Some(ids) = topology.subtree(*id) {
            doomed.extend(ids);
        } else {
            doomed.insert(*id);
        }
    }
    // Only a deletion that removes a clip track can strand the set; removing an
    // Output (or nothing) from a project without clip tracks must not trip the
    // vacuous `all` over an empty iterator.
    let removes_clip_track = project.clip_tracks().any(|t| doomed.contains(&t.id));
    if removes_clip_track && project.clip_tracks().all(|t| doomed.contains(&t.id)) {
        return Err(err("a set needs at least one track"));
    }
    Ok(doomed)
}

pub(super) fn remove_tracks(project: &mut Project, tracks: Vec<TrackId>) -> Result<(), ModelError> {
    let doomed = deletion_ids(project, &tracks)?;
    let outputs: Vec<_> = project
        .outputs
        .iter()
        .filter(|t| doomed.contains(&t.id))
        .map(|t| t.id)
        .collect();
    for id in outputs {
        super::routes::remove_output(project, id)?;
    }
    project.tracks.retain(|t| !doomed.contains(&t.id));
    remove_empty_ancestors(project)
}

// Decided 2026-11-06: remove groups emptied by a move or
// deletion, cascading until every surviving group has a direct child.
pub(super) fn remove_empty_ancestors(project: &mut Project) -> Result<(), ModelError> {
    loop {
        let empty: std::collections::HashSet<_> = project.topology()?.childless_groups().collect();
        if empty.is_empty() {
            return Ok(());
        }
        project.tracks.retain(|t| !empty.contains(&t.id));
    }
}

/// Move to a post-removal list index, retaining the immediate parent and routes.
/// Final validation rejects boundaries that split a subtree or leave the parent.
pub(super) fn move_track(project: &mut Project, from: usize, to: usize) -> Result<(), ModelError> {
    let id = project
        .tracks
        .get(from)
        .ok_or_else(|| err("move index out of range"))?
        .id;
    let span = project.topology()?.span(id).unwrap();
    super::move_range(&mut project.tracks, span, to)
}

#[cfg(test)]
mod tests {
    use crate::edit::{ProjectEdit, apply};
    use crate::model::{Project, Route, Scene, SceneId, Track, TrackId};

    /// Tracks 1–5; 2 and 3 are in group 20 (placed before 2), 4 and 5 stay out.
    fn project() -> Project {
        let mut p = Project {
            tracks: (1..=5)
                .map(|i| Track::new(TrackId(i), format!("T{i}"), 1))
                .collect(),
            scenes: vec![Scene {
                id: SceneId(1),
                name: "S".into(),
                follow_actions: Default::default(),
            }],
            ..Project::default()
        };
        p.insert_group(
            Track::new_group(TrackId(20), "G", 1),
            &[TrackId(2), TrackId(3)],
        );
        p
    }

    fn order(p: &Project) -> Vec<u64> {
        p.tracks.iter().map(|t| t.id.0).collect()
    }

    #[test]
    fn batch_removal_handles_tracks_groups_outputs_overlap_and_routes_in_one_revision() {
        use crate::engine::{Command, Engine};
        for tracks in [vec![1, 4], vec![20], vec![2, 20, 3, 20], vec![4, 20]] {
            let mut p = project();
            apply(
                &mut p,
                ProjectEdit::AddOutput {
                    name: "Out".into(),
                    host: "127.0.0.1".into(),
                    port: 9000,
                },
            )
            .unwrap();
            let output = p.outputs[0].id;
            p[TrackId(5)].route = Route::single(output);
            let mut engine = Engine::new(p).unwrap();
            let revision = engine.project_revision();
            let mut ids: Vec<_> = tracks.iter().map(|id| TrackId(*id)).collect();
            ids.push(output);
            engine
                .execute(Command::Edit(ProjectEdit::RemoveTracks { tracks: ids }))
                .unwrap();
            assert_eq!(engine.project_revision(), revision + 1);
            let p = engine.project();
            assert!(p.outputs.is_empty());
            assert_eq!(p[TrackId(5)].route, Route::empty());
            for id in tracks {
                assert!(p.track(TrackId(id)).is_none());
            }
            if p.track(TrackId(20)).is_none() {
                assert!(p.track(TrackId(2)).is_none());
                assert!(p.track(TrackId(3)).is_none());
            }
        }
    }

    #[test]
    fn batch_removal_rejects_main_unknown_and_last_track_without_partial_changes() {
        let mut p = project();
        apply(
            &mut p,
            ProjectEdit::AddOutput {
                name: "Out".into(),
                host: "127.0.0.1".into(),
                port: 9000,
            },
        )
        .unwrap();
        let output = p.outputs[0].id;
        let before = p.clone();
        for tracks in [
            vec![TrackId(1), p.main.id],
            vec![TrackId(1), TrackId(999)],
            vec![output, TrackId(1), TrackId(20), TrackId(4), TrackId(5)],
        ] {
            assert!(apply(&mut p, ProjectEdit::RemoveTracks { tracks }).is_err());
            assert_eq!(p, before);
        }
    }

    #[test]
    fn removing_an_output_needs_no_clip_track_but_the_last_clip_track_stays_protected() {
        let mut p = Project::default();
        p.outputs
            .push(Track::new_output(TrackId(201), "Out", "127.0.0.1", 9000));
        apply(
            &mut p,
            ProjectEdit::RemoveTracks {
                tracks: vec![TrackId(201)],
            },
        )
        .unwrap();
        assert!(p.outputs.is_empty());

        let mut p = project();
        let all: Vec<_> = p.clip_tracks().map(|t| t.id).collect();
        let before = p.clone();
        assert!(apply(&mut p, ProjectEdit::RemoveTracks { tracks: all }).is_err());
        assert_eq!(p, before);
    }

    #[test]
    fn creating_a_group_puts_its_track_before_the_first_member_and_rejects_mixed_depth() {
        let mut p = project();
        apply(
            &mut p,
            ProjectEdit::CreateGroup {
                tracks: vec![TrackId(4), TrackId(5)],
            },
        )
        .unwrap();
        assert_eq!(order(&p), vec![1, 20, 2, 3, 21, 4, 5]);
        assert_eq!(p[TrackId(4)].group, Some(TrackId(21)));
        assert_eq!(p[TrackId(4)].route, Route::Group);
        assert_eq!(p[TrackId(5)].route, Route::Group);
        assert_eq!(p[TrackId(21)].slots.len(), 1);
        // The default name counts group tracks, not ids: this is the project's second group.
        assert_eq!(p[TrackId(21)].name, "Group 2");
        let before = p.clone();
        for tracks in [vec![TrackId(1), TrackId(2)], vec![TrackId(1), TrackId(9)]] {
            assert!(apply(&mut p, ProjectEdit::CreateGroup { tracks }).is_err());
        }
        assert_eq!(p, before);
    }

    #[test]
    fn single_track_group_enters_membership_and_ungroups_back_to_main() {
        for route in [Route::empty(), Route::single(project().main.id)] {
            let mut p = project();
            p[TrackId(4)].route = route;
            apply(
                &mut p,
                ProjectEdit::CreateGroup {
                    tracks: vec![TrackId(4)],
                },
            )
            .unwrap();
            assert_eq!(order(&p), vec![1, 20, 2, 3, 21, 4, 5]);
            assert_eq!(p[TrackId(4)].group, Some(TrackId(21)));
            assert_eq!(p[TrackId(4)].route, Route::Group);
            assert_eq!(p[TrackId(21)].route, Route::single(p.main.id));
            apply(&mut p, ProjectEdit::Ungroup { group: TrackId(21) }).unwrap();
            assert_eq!(p[TrackId(4)].group, None);
            assert_eq!(p[TrackId(4)].route, Route::single(p.main.id));
        }
    }

    #[test]
    fn grouping_rejects_empty_main_output_and_nonadjacent_atomically() {
        let mut p = project();
        apply(
            &mut p,
            ProjectEdit::AddOutput {
                name: "Out".into(),
                host: "127.0.0.1".into(),
                port: 9000,
            },
        )
        .unwrap();
        let before = p.clone();
        for tracks in [
            vec![],
            vec![p.main.id],
            vec![p.outputs[0].id],
            vec![TrackId(1), TrackId(4)],
            vec![TrackId(4), TrackId(4)],
        ] {
            assert!(apply(&mut p, ProjectEdit::CreateGroup { tracks }).is_err());
            assert_eq!(p, before);
        }
    }

    #[test]
    fn removing_a_group_track_removes_its_members_and_the_last_member_removes_the_group() {
        let mut p = project();
        apply(
            &mut p,
            ProjectEdit::RemoveTracks {
                tracks: vec![TrackId(3)],
            },
        )
        .unwrap();
        assert_eq!(order(&p), vec![1, 20, 2, 4, 5]);
        apply(
            &mut p,
            ProjectEdit::RemoveTracks {
                tracks: vec![TrackId(2)],
            },
        )
        .unwrap();
        assert_eq!(order(&p), vec![1, 4, 5]);
        let mut p = project();
        apply(
            &mut p,
            ProjectEdit::RemoveTracks {
                tracks: vec![TrackId(20)],
            },
        )
        .unwrap();
        assert_eq!(order(&p), vec![1, 4, 5]);
    }

    #[test]
    fn the_last_clip_track_stays_even_when_only_a_group_would_remain() {
        let mut p = project();
        for id in [1, 4, 5, 2] {
            apply(
                &mut p,
                ProjectEdit::RemoveTracks {
                    tracks: vec![TrackId(id)],
                },
            )
            .unwrap();
        }
        assert_eq!(order(&p), vec![20, 3]);
        for id in [20, 3] {
            assert!(
                apply(
                    &mut p,
                    ProjectEdit::RemoveTracks {
                        tracks: vec![TrackId(id)]
                    }
                )
                .is_err()
            );
        }
        assert_eq!(p.clip_tracks().count(), 1);
    }

    #[test]
    fn moving_a_group_track_moves_its_members_and_a_split_is_refused() {
        let mut p = project();
        apply(&mut p, ProjectEdit::MoveTrack { from: 1, to: 3 }).unwrap();
        assert_eq!(order(&p), vec![1, 4, 5, 20, 2, 3]);
        let before = p.clone();
        // A member alone cannot leave its group's run.
        assert!(apply(&mut p, ProjectEdit::MoveTrack { from: 4, to: 0 }).is_err());
        assert_eq!(p, before);
    }

    #[test]
    fn ungroup_returns_group_routes_to_main_and_preserves_explicit_routes() {
        let mut p = project();
        p[TrackId(20)].route = Route::empty();
        p[TrackId(2)].route = Route::Group;
        p[TrackId(3)].route = Route::empty();
        apply(&mut p, ProjectEdit::Ungroup { group: TrackId(20) }).unwrap();
        assert_eq!(order(&p), vec![1, 2, 3, 4, 5]);
        assert_eq!(p[TrackId(2)].route, Route::single(p.main.id));
        assert_eq!(p[TrackId(3)].route, Route::empty());
        assert!(p.tracks.iter().all(|t| t.group.is_none()));
    }
}
