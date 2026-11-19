//! Group membership and its route change are one engine edit/revision.
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{Project, Route, Track, TrackId};

fn project() -> Project {
    Project {
        outputs: vec![
            Track::new_output(TrackId(201), "Stage", "127.0.0.1", 9000),
            Track::new_output(TrackId(202), "Video", "127.0.0.1", 9001),
        ],
        tracks: (1..=6)
            .map(|id| Track::new(TrackId(id), format!("Cue {id}"), 0))
            .collect(),
        ..Project::default()
    }
}

fn edit(engine: &mut Engine, edit: ProjectEdit) {
    let revision = engine.project_revision();
    engine.execute(Command::Edit(edit)).unwrap();
    assert_eq!(engine.project_revision(), revision + 1);
    engine.project().validate().unwrap();
}

fn group(engine: &mut Engine, ids: &[u64]) -> TrackId {
    edit(
        engine,
        ProjectEdit::CreateGroup {
            tracks: ids.iter().copied().map(TrackId).collect(),
        },
    );
    engine.project()[TrackId(ids[0])].group.unwrap()
}

fn place(engine: &mut Engine, id: u64, before: Option<u64>, group: Option<TrackId>) {
    edit(
        engine,
        ProjectEdit::PlaceTrack {
            track: TrackId(id),
            before: before.map(TrackId),
            group,
        },
    );
}

#[test]
fn creation_overrides_all_routes_at_every_position_in_one_revision() {
    for ids in [[1, 2, 3, 4], [2, 3, 4, 5], [3, 4, 5, 6]] {
        let mut p = project();
        let routes = [
            Route::single(p.main.id),
            Route::empty(),
            Route::single(TrackId(201)),
            Route::Outputs(vec![p.main.id, TrackId(201), TrackId(202)]),
        ];
        for (&id, route) in ids.iter().zip(routes) {
            p[TrackId(id)].route = route;
        }
        let original = p.clone();
        let mut engine = Engine::new(p).unwrap();
        let parent = group(&mut engine, &ids);
        assert_eq!(engine.project_revision(), 1);
        assert_eq!(
            engine.project()[parent].route,
            Route::single(original.main.id)
        );
        for id in ids {
            let mut expected = original[TrackId(id)].clone();
            expected.group = Some(parent);
            expected.route = Route::Group;
            assert_eq!(engine.project()[TrackId(id)], expected);
        }
        assert_eq!(
            engine
                .project()
                .clip_tracks()
                .map(|t| t.id)
                .collect::<Vec<_>>(),
            original.clip_tracks().map(|t| t.id).collect::<Vec<_>>()
        );
    }
}

#[test]
fn placement_at_start_middle_and_end_overrides_explicit_routes_once() {
    for before in [Some(2), Some(3), Some(4)] {
        for route in [Route::empty(), Route::single(TrackId(201))] {
            let mut p = project();
            p[TrackId(5)].route = route;
            let mut engine = Engine::new(p).unwrap();
            let parent = group(&mut engine, &[2, 3]);
            place(&mut engine, 5, before, Some(parent));
            assert_eq!(engine.project()[TrackId(5)].group, Some(parent));
            assert_eq!(engine.project()[TrackId(5)].route, Route::Group);
        }
    }
    let mut engine = Engine::new(project()).unwrap();
    let parent = group(&mut engine, &[5, 6]);
    place(&mut engine, 1, None, Some(parent));
    assert_eq!(engine.project().tracks.last().unwrap().id, TrackId(1));
    assert_eq!(engine.project()[TrackId(1)].route, Route::Group);
}

#[test]
fn transfer_between_groups_routes_to_the_new_group_but_reorder_preserves_override() {
    let mut engine = Engine::new(project()).unwrap();
    let first = group(&mut engine, &[1, 2]);
    let second = group(&mut engine, &[4, 5]);
    edit(
        &mut engine,
        ProjectEdit::SetTrackRoutes {
            routes: vec![(TrackId(1), Route::single(TrackId(201)))],
        },
    );
    place(&mut engine, 1, Some(2), Some(first));
    assert_eq!(
        engine.project()[TrackId(1)].route,
        Route::single(TrackId(201))
    );
    place(&mut engine, 1, Some(5), Some(second));
    assert_eq!(engine.project()[TrackId(1)].group, Some(second));
    assert_eq!(engine.project()[TrackId(1)].route, Route::Group);
    place(&mut engine, 2, Some(5), Some(second));
    assert!(engine.project().track(first).is_none());
    assert_eq!(engine.project()[TrackId(2)].route, Route::Group);
}

#[test]
fn leaving_and_ungrouping_return_group_routes_to_main_preserving_overrides() {
    for ungroup in [false, true] {
        let mut engine = Engine::new(project()).unwrap();
        let parent = group(&mut engine, &[2, 3]);
        edit(
            &mut engine,
            ProjectEdit::SetTrackRoutes {
                routes: vec![(parent, Route::single(TrackId(202)))],
            },
        );
        edit(
            &mut engine,
            ProjectEdit::SetTrackRoutes {
                routes: vec![(TrackId(3), Route::single(TrackId(201)))],
            },
        );
        if ungroup {
            edit(&mut engine, ProjectEdit::Ungroup { group: parent });
        } else {
            place(&mut engine, 2, None, None);
            place(&mut engine, 3, None, None);
        }
        let p = engine.project();
        assert_eq!(p[TrackId(2)].group, None);
        assert_eq!(p[TrackId(2)].route, Route::single(p.main.id));
        assert_eq!(p[TrackId(3)].group, None);
        assert_eq!(p[TrackId(3)].route, Route::single(TrackId(201)));
        assert!(p.track(parent).is_none());
    }
}

#[test]
fn group_deletion_removes_members_once_and_invalid_grouping_is_atomic() {
    let mut engine = Engine::new(project()).unwrap();
    let parent = group(&mut engine, &[2, 3]);
    let original = engine.project().clone();
    let revision = engine.project_revision();
    for invalid in [
        ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1), TrackId(3)],
        },
        ProjectEdit::PlaceTrack {
            track: TrackId(4),
            before: Some(TrackId(3)),
            group: None,
        },
        ProjectEdit::Ungroup {
            group: TrackId(999),
        },
    ] {
        assert!(engine.execute(Command::Edit(invalid)).is_err());
        assert_eq!(engine.project(), &original);
        assert_eq!(engine.project_revision(), revision);
    }
    edit(
        &mut engine,
        ProjectEdit::RemoveTracks {
            tracks: vec![parent],
        },
    );
    for id in [parent, TrackId(2), TrackId(3)] {
        assert!(engine.project().track(id).is_none());
    }
    for id in [1, 4, 5, 6] {
        assert_eq!(engine.project()[TrackId(id)], original[TrackId(id)]);
    }
}
