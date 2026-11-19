use qtable_engine::edit::{self, ProjectEdit};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use qtable_test_support::TestReceiver;
use std::time::Duration;

fn project() -> Project {
    let mut p = Project::default();
    let mut output = Track::new_output(TrackId(201), "Stage", "127.0.0.1", 9000);
    output = output.with_path("/stage");
    p.outputs.push(output);
    let mut group = Track::new_group(TrackId(10), "Group", 0);
    group.route = Route::single(TrackId(201));
    group = group.with_path("/ensemble");
    let mut member = Track::new(TrackId(1), "Member", 0);
    member.group = Some(group.id);
    member.route = Route::Group;
    p.tracks = vec![group, member, Track::new(TrackId(2), "Outside", 0)];
    p
}

fn groups(p: &Project, subject: TrackId) -> Vec<Option<TrackId>> {
    p.ancestors(subject)
        .map(|a| a.group().map(|g| g.id))
        .collect()
}

fn nested() -> Project {
    let mut p = project();
    let mut inner = Track::new_group(TrackId(11), "Inner", 0);
    inner.group = Some(TrackId(10));
    inner.route = Route::Group;
    inner = inner.with_path("/inner");
    p[TrackId(1)].group = Some(inner.id);
    p.tracks.insert(1, inner);
    p
}

#[test]
fn walk_excludes_subject_and_finishes_with_main_once() {
    let p = project();
    assert_eq!(groups(&p, TrackId(1)), [Some(TrackId(10)), None]);
    for id in [10, 2, 201] {
        assert_eq!(groups(&p, TrackId(id)), [None]);
    }
    assert!(groups(&p, TrackId(999)).is_empty());
    assert!(groups(&p, p.main.id).is_empty());
    let mut p = nested();
    assert_eq!(
        groups(&p, TrackId(1)),
        [Some(TrackId(11)), Some(TrackId(10)), None]
    );
    p[TrackId(10)].group = Some(TrackId(11));
    let walk = groups(&p, TrackId(1));
    assert!(walk.len() <= p.tracks.len() + 1);
    assert_eq!(walk.last(), Some(&None));
    p[TrackId(11)].group = Some(TrackId(999));
    assert_eq!(groups(&p, TrackId(1)), [Some(TrackId(11)), None]);
    p[TrackId(11)].group = Some(TrackId(2));
    assert_eq!(groups(&p, TrackId(1)), [Some(TrackId(11)), None]);
    p[TrackId(1)].group = Some(TrackId(1));
    assert_eq!(groups(&p, TrackId(1)), [None]);
}

#[test]
fn nested_folds_inherit_nearest_route_and_sum_delays() {
    let mut p = nested();
    p[TrackId(1)].delay_ms = 1.0;
    p[TrackId(11)].delay_ms = 2.0;
    p[TrackId(10)].delay_ms = 3.0;
    p.main.delay_ms = 4.0;
    p.outputs[0].delay_ms = 5.0;
    assert_eq!(p.effective_delay_ms(TrackId(1)), 10.0);
    assert_eq!(p.effective_delay_ms(TrackId(201)), 9.0);
    assert_eq!(p.effective_delay_ms(TrackId(999)), 0.0);
    assert_eq!(
        p.effective_delay_ms(TrackId(1)) + p.outputs[0].delay_ms,
        15.0
    );
    assert_eq!(p.resolved_route(TrackId(1)), [TrackId(201)]);
    p[TrackId(11)].route = Route::empty();
    assert!(p.resolved_route(TrackId(1)).is_empty());
    p[TrackId(1)].route = Route::single(TrackId(201));
    assert_eq!(p.resolved_route(TrackId(1)), [TrackId(201)]);
    assert!(p.resolved_route(TrackId(201)).is_empty());
    p.validate().unwrap();
}

#[test]
fn nested_solo_mute_and_fold_see_all_ancestors() {
    let mut p = nested();
    let mut sibling = Track::new(TrackId(3), "Sibling", 0);
    sibling.group = Some(TrackId(10));
    sibling.route = Route::Group;
    p.tracks.insert(3, sibling);
    p[TrackId(10)].mixer.solo = true;
    assert!(p.track_sends(TrackId(1)));
    assert!(p.track_sends(TrackId(3)));
    assert!(!p.track_sends(TrackId(2)));
    p[TrackId(11)].mixer.solo = true;
    assert!(p.track_sends(TrackId(1)));
    assert!(!p.track_sends(TrackId(3)));
    p[TrackId(10)].mixer.solo = false;
    assert_eq!(
        p.track_facts(TrackId(10)).unwrap().solo_look,
        SoloLook::Partial
    );
    p[TrackId(11)].mixer.solo = false;
    p[TrackId(1)].mixer.solo = true;
    for id in [10, 11] {
        assert_eq!(
            p.track_facts(TrackId(id)).unwrap().solo_look,
            SoloLook::Partial
        );
    }
    p[TrackId(10)].mixer.active = false;
    assert!(!p.track_sends(TrackId(1)));
    p[TrackId(10)].fold_state = true;
    assert!(!p.track_facts(TrackId(1)).unwrap().is_visible);
    assert_eq!(
        p.track_facts(TrackId(1)).unwrap().group_track,
        Some(TrackId(11))
    );
    p[TrackId(1)].mixer.solo = false;
    p.outputs[0].mixer.solo = true;
    assert_eq!(p.track_facts(TrackId(10)).unwrap().solo_look, SoloLook::Off);
}

#[test]
fn composition_preserves_boundaries_and_absolute_bypass() {
    let mut p = nested();
    let resolve = |p: &Project, address| {
        p.resolve_event_address(TrackId(1), TrackId(201), &address)
            .unwrap()
    };
    assert_eq!(
        resolve(&p, OscAddress::Relative("/play".into())),
        "/stage/ensemble/inner/play"
    );
    assert_eq!(
        resolve(&p, OscAddress::Relative("/a//b".into())),
        "/stage/ensemble/inner/a//b"
    );
    assert_eq!(
        resolve(&p, OscAddress::Relative("/".into())),
        "/stage/ensemble/inner/"
    );
    assert_eq!(
        resolve(&p, OscAddress::Absolute("/override".into())),
        "/override"
    );
    p[TrackId(11)].path = Path::default();
    p[TrackId(10)].path = Path::default();
    assert_eq!(
        resolve(&p, OscAddress::Relative("/play".into())),
        "/stage/play"
    );
    p.outputs[0].path = Path::default();
    assert_eq!(resolve(&p, OscAddress::Relative("/play".into())), "/play");
    assert!(
        p.resolve_event_address(
            TrackId(999),
            TrackId(201),
            &OscAddress::Relative("/x".into())
        )
        .is_none()
    );
    assert!(
        p.resolve_event_address(TrackId(1), TrackId(999), &OscAddress::Absolute("/x".into()))
            .is_none()
    );
}

#[test]
fn a_tracks_own_path_sits_between_its_groups_and_the_address() {
    let mut p = nested();
    p[TrackId(1)].path = Path::new("/t").unwrap();
    let play = OscAddress::Relative("/play".into());
    let send = |p: &Project, to| {
        p.resolve_event_address(TrackId(1), TrackId(to), &play)
            .unwrap()
    };
    assert_eq!(send(&p, 201), "/stage/ensemble/inner/t/play");
    p[TrackId(11)].path = Path::default();
    assert_eq!(send(&p, 201), "/stage/ensemble/t/play");
    p[TrackId(10)].path = Path::default();
    p.outputs[0].path = Path::default();
    assert_eq!(send(&p, 201), "/t/play");
    p[TrackId(1)].path = Path::default();
    assert_eq!(send(&p, 201), "/play");
}

#[test]
fn main_and_output_paths_enter_only_as_destinations() {
    let mut p = project();
    p.main.output = p.outputs[0].output.clone();
    p.main.path = Path::new("/show").unwrap();
    p.tracks[1].path = Path::new("/t").unwrap();
    let play = OscAddress::Relative("/play".into());
    let to = |p: &Project, id| p.resolve_event_address(TrackId(1), id, &play).unwrap();
    assert_eq!(to(&p, p.main.id), "/show/ensemble/t/play");
    assert_eq!(to(&p, TrackId(201)), "/stage/ensemble/t/play");
}

#[test]
fn a_destination_sending_to_itself_applies_its_path_once() {
    let mut p = project();
    p.main.output = p.outputs[0].output.clone();
    p.main.path = Path::new("/show").unwrap();
    let volume = OscAddress::Relative("/volume".into());
    let resolve = |p: &Project, source, destination| {
        p.resolve_event_address(source, destination, &volume)
            .unwrap()
    };
    assert_eq!(resolve(&p, TrackId(201), TrackId(201)), "/stage/volume");
    assert_eq!(resolve(&p, p.main.id, p.main.id), "/show/volume");
    // A group's own volume goes through the group's path.
    assert_eq!(
        resolve(&p, TrackId(10), TrackId(201)),
        "/stage/ensemble/volume"
    );
}

#[test]
fn path_edits_cover_every_kind_and_refuse_bad_paths() {
    let mut p = project();
    p.validate().unwrap();
    let main = p.main.id;
    for id in [TrackId(1), TrackId(10), TrackId(201), main] {
        edit::apply(
            &mut p,
            ProjectEdit::SetTrackPath {
                track: id,
                path: "/new".into(),
            },
        )
        .unwrap();
        assert_eq!(p.track(id).unwrap().path.as_str(), "/new");
        let text = ron::to_string(&p).unwrap();
        assert_eq!(ron::from_str::<Project>(&text).unwrap(), p);
    }
    for (id, path) in [
        (TrackId(10), "a b"),
        (TrackId(10), "bad/"),
        (TrackId(10), "/"),
        (TrackId(10), "/bad/"),
        (TrackId(10), "/nul\0"),
        (TrackId(999), "/x"),
    ] {
        let before = p.clone();
        assert!(
            edit::apply(
                &mut p,
                ProjectEdit::SetTrackPath {
                    track: id,
                    path: path.into()
                }
            )
            .is_err()
        );
        assert_eq!(p, before);
    }
    edit::apply(
        &mut p,
        ProjectEdit::SetTrackPath {
            track: TrackId(10),
            path: String::new(),
        },
    )
    .unwrap();
    assert!(!ron::to_string(&p).unwrap().contains("ensemble"));
    let old: Track = ron::from_str("(id:(1),name:\"old\",slots:[],path:\"/g\")").unwrap();
    assert_eq!(old.path.as_str(), "/g");
    assert!(ron::from_str::<Track>("(id:(1),name:\"x\",slots:[],path:\"bad\")").is_err());
}

#[test]
fn udp_path_edit_during_playback_keeps_event_count_and_order() {
    let receiver = TestReceiver::bind().unwrap();
    let mut p = project();
    p.outputs[0].destination_mut().unwrap().port = receiver.addr().port();
    p.scenes.push(Scene {
        id: SceneId(1),
        name: "Scene".into(),
        follow_actions: Default::default(),
    });
    for t in &mut p.tracks {
        t.slots.push(Slot::default());
    }
    let mut clip = Clip::new(ClipId(1), "Events", BeatTime(960), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.looping = false;
    clip.events = (0..3)
        .map(|i| {
            let path = OscAddress::Relative(format!("/{i}"));
            p.hit(TrackId(1), BeatTime(i * 240), path, vec![]).unwrap()
        })
        .collect();
    p[TrackId(1)].slots[0].clip = Some(clip);
    p.main.delay_ms = 20.0;
    p.outputs[0].delay_ms = 10.0;
    p.global_quantisation = GlobalQuantisation::None;
    let mut engine = Engine::new(p).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let first = receiver
        .wait_for_messages(1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(first[0].1.addr, "/stage/ensemble/0");
    engine
        .execute(Command::Edit(ProjectEdit::SetTrackPath {
            track: TrackId(10),
            path: "/edited".into(),
        }))
        .unwrap();
    let messages = receiver
        .wait_for_messages(2, Duration::from_secs(2))
        .unwrap();
    for (i, (_, message)) in messages.iter().enumerate() {
        assert_eq!(message.addr, format!("/stage/edited/{}", i + 1));
    }
    assert!(
        receiver
            .wait_for_messages(1, Duration::from_millis(100))
            .is_err()
    );
}

fn two_outputs() -> Project {
    let mut p = nested();
    p.outputs
        .push(Track::new_output(TrackId(202), "Viz", "127.0.0.1", 9001).with_path("/viz"));
    p[TrackId(10)].route = Route::Outputs(vec![TrackId(202), TrackId(201)]);
    p[TrackId(1)].path = Path::new("/t").unwrap();
    p
}

fn prefixes(p: &Project, source: TrackId, level: PathLevel) -> Vec<(String, String)> {
    p.inherited_paths(source, level)
        .into_iter()
        .map(|(d, path)| (d.name.clone(), path))
        .collect()
}

#[test]
fn a_grouped_track_routed_to_two_outputs_gives_two_prefixes() {
    let p = two_outputs();
    assert_eq!(
        prefixes(&p, TrackId(1), PathLevel::Under),
        [
            ("Stage".into(), "/stage/ensemble/inner/t".into()),
            ("Viz".into(), "/viz/ensemble/inner/t".into())
        ]
    );
}

#[test]
fn track_level_stops_before_the_tracks_own_path() {
    let p = two_outputs();
    assert_eq!(
        prefixes(&p, TrackId(1), PathLevel::Track),
        [
            ("Stage".into(), "/stage/ensemble/inner".into()),
            ("Viz".into(), "/viz/ensemble/inner".into())
        ]
    );
}

#[test]
fn a_destinations_own_prefix_is_empty_at_track_level_and_its_path_under() {
    let mut p = project();
    p.main.output = p.outputs[0].output.clone();
    p.main.path = Path::new("/show").unwrap();
    for (id, own) in [(TrackId(201), "/stage"), (p.main.id, "/show")] {
        assert_eq!(
            p.inherited_path(id, id, PathLevel::Track).as_deref(),
            Some("")
        );
        assert_eq!(
            p.inherited_path(id, id, PathLevel::Under).as_deref(),
            Some(own)
        );
        assert_eq!(p.path_destinations(id).len(), 1);
    }
}

#[test]
fn unset_main_gives_no_destinations_and_inactive_outputs_still_count() {
    let mut p = project();
    p.main.output = None;
    assert!(p.path_destinations(p.main.id).is_empty());
    assert!(p.inherited_paths(p.main.id, PathLevel::Under).is_empty());
    p.tracks[1].route = Route::single(p.main.id);
    assert!(p.path_destinations(TrackId(1)).is_empty());
    p.tracks[1].route = Route::Group;
    p.outputs[0].mixer.active = false;
    assert_eq!(p.path_destinations(TrackId(1)).len(), 1);
    assert!(p.path_destinations(TrackId(999)).is_empty());
}

#[test]
fn main_comes_first_among_destinations() {
    let mut p = two_outputs();
    p.main.output = p.outputs[0].output.clone();
    p.main.path = Path::new("/show").unwrap();
    p[TrackId(10)].route = Route::Outputs(vec![TrackId(202), p.main.id, TrackId(201)]);
    let names: Vec<_> = p
        .path_destinations(TrackId(1))
        .iter()
        .map(|d| d.name.clone())
        .collect();
    assert_eq!(names, ["Main", "Stage", "Viz"]);
}

#[test]
fn prefix_plus_piece_equals_the_resolved_address_everywhere() {
    let mut p = two_outputs();
    p.main.output = p.outputs[0].output.clone();
    p.main.path = Path::new("/show").unwrap();
    p[TrackId(2)].route = Route::single(p.main.id);
    let ids = [
        TrackId(1),
        TrackId(10),
        TrackId(11),
        TrackId(2),
        TrackId(201),
        p.main.id,
    ];
    for id in ids {
        for (d, prefix) in p.inherited_paths(id, PathLevel::Under) {
            let piece = "/piece";
            let full = p
                .resolve_event_address(id, d.id, &OscAddress::Relative(piece.into()))
                .unwrap();
            assert_eq!(format!("{prefix}{piece}"), full, "{id:?} -> {}", d.name);
        }
    }
}

#[test]
fn typed_paths_gain_a_leading_slash() {
    assert_eq!(lead_with_slash("group".into()), "/group");
    assert_eq!(lead_with_slash(String::new()), "");
    assert_eq!(lead_with_slash("/a".into()), "/a");
    let mut p = project();
    edit::apply(
        &mut p,
        ProjectEdit::SetTrackPath {
            track: TrackId(10),
            path: "group".into(),
        },
    )
    .unwrap();
    assert_eq!(p[TrackId(10)].path.as_str(), "/group");
}

#[test]
fn volume_output_addresses_gain_a_leading_slash_and_still_reject_empty() {
    let mut p = project();
    let mut output = p[TrackId(1)].mixer_device.volume.output.clone();
    output.address = "level".into();
    edit::apply(
        &mut p,
        ProjectEdit::SetParameterOutput {
            target: qtable_engine::model::ParameterRef::volume(TrackId(1)),
            output: output.clone(),
        },
    )
    .unwrap();
    assert_eq!(p[TrackId(1)].mixer_device.volume.output.address, "/level");
    output.address.clear();
    let before = p.clone();
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::SetParameterOutput {
                target: qtable_engine::model::ParameterRef::volume(TrackId(1)),
                output
            }
        )
        .is_err()
    );
    assert_eq!(p, before);
}
