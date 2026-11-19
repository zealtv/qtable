use super::*;
use crate::model::{
    BeatTime, FollowAction, FollowActions, LIVE_CLIP_PALETTE, Scene, SceneFollowActions,
};

fn project() -> Project {
    Project {
        scenes: (1..=3)
            .map(|id| Scene {
                id: SceneId(id),
                name: format!("S{id}"),
                follow_actions: SceneFollowActions::default(),
            })
            .collect(),
        tracks: (1..=2)
            .map(|id| Track::new(TrackId(id), format!("T{id}"), 3))
            .collect(),
        ..Project::default()
    }
}

#[test]
fn grid_edits_allocate_ids_and_keep_rows_aligned() {
    let mut p = project();
    apply(&mut p, ProjectEdit::AddTrack { name: "T3".into() }).unwrap();
    apply(
        &mut p,
        ProjectEdit::AddScene {
            at: 1,
            name: "inserted".into(),
        },
    )
    .unwrap();
    assert_eq!(p.tracks[2].id, TrackId(3));
    assert_eq!(p.scenes[1].id, SceneId(4));
    assert!(p.tracks.iter().all(|t| t.slots.len() == p.scenes.len()));
    apply(&mut p, ProjectEdit::MoveScene { from: 1, to: 3 }).unwrap();
    assert!(p.tracks.iter().all(|t| t.slots.len() == p.scenes.len()));
    apply(&mut p, ProjectEdit::RemoveScene { scene: SceneId(4) }).unwrap();
    apply(
        &mut p,
        ProjectEdit::RemoveTracks {
            tracks: vec![TrackId(3)],
        },
    )
    .unwrap();
    assert_eq!(p.tracks.len(), 2);
}

#[test]
fn group_edits_validate_membership_and_keep_groups_contiguous() {
    let mut p = project();
    apply(&mut p, ProjectEdit::AddTrack { name: "T3".into() }).unwrap();
    apply(
        &mut p,
        ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1), TrackId(2)],
        },
    )
    .unwrap();
    let group = p.group_tracks().next().unwrap().id;
    apply(
        &mut p,
        ProjectEdit::RenameTrack {
            track: group,
            name: "Keys".into(),
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::RecolourTrack {
            track: group,
            colour: Colour {
                red: 1,
                green: 2,
                blue: 3,
            },
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::SetGroupFolded {
            group,
            folded: true,
        },
    )
    .unwrap();
    assert!(p[group].fold_state);
    let before_move = p.clone();
    assert!(apply(&mut p, ProjectEdit::MoveTrack { from: 0, to: 2 }).is_err());
    assert_eq!(p, before_move);
    assert!(
        apply(
            &mut p,
            ProjectEdit::CreateGroup {
                tracks: vec![TrackId(2), TrackId(3)]
            }
        )
        .is_err()
    );
    apply(&mut p, ProjectEdit::Ungroup { group }).unwrap();
    assert_eq!(p.group_tracks().count(), 0);
}

#[test]
fn clip_edits_preserve_gaps_and_stop_flags() {
    let mut p = project();
    let tid = TrackId(1);
    apply(
        &mut p,
        ProjectEdit::CreateClip {
            track: tid,
            slot: 0,
            length: BeatTime::from_beats(4).unwrap(),
        },
    )
    .unwrap();
    let id = p.tracks[0].slots[0].clip.as_ref().unwrap().id;
    apply(
        &mut p,
        ProjectEdit::RenameClip {
            track: tid,
            slot: 0,
            name: "renamed".into(),
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::RecolourClip {
            track: tid,
            slot: 0,
            colour: Colour {
                red: 1,
                green: 2,
                blue: 3,
            },
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::DuplicateClip {
            track: tid,
            slot: 0,
        },
    )
    .unwrap();
    assert_ne!(p.tracks[0].slots[1].clip.as_ref().unwrap().id, id);
    apply(
        &mut p,
        ProjectEdit::MoveClip {
            from: (tid, 1),
            to: (tid, 2),
        },
    )
    .unwrap();
    assert!(p.tracks[0].slots[1].clip.is_none());
    assert!(p.tracks[0].slots[1].has_stop_button);
    let moved = p.tracks[0].slots[2].clip.clone();
    apply(
        &mut p,
        ProjectEdit::MoveClip {
            from: (tid, 2),
            to: (tid, 0),
        },
    )
    .unwrap();
    assert_eq!(p.tracks[0].slots[0].clip, moved);
    assert!(p.tracks[0].slots[2].clip.is_none());
    apply(
        &mut p,
        ProjectEdit::MoveClip {
            from: (tid, 0),
            to: (tid, 0),
        },
    )
    .unwrap();
    assert_eq!(p.tracks[0].slots[0].clip, moved);
    apply(
        &mut p,
        ProjectEdit::SetSlotStopButton {
            track: tid,
            slot: 1,
            enabled: false,
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::DeleteClip {
            track: tid,
            slot: 2,
        },
    )
    .unwrap();
    assert!(!p.tracks[0].slots[1].has_stop_button);
    assert_eq!(p.tracks[0].slots.len(), 3);
}

#[test]
fn track_removal_preserves_the_last_track_and_its_group() {
    let mut p = project();
    apply(
        &mut p,
        ProjectEdit::RemoveTracks {
            tracks: vec![TrackId(2)],
        },
    )
    .unwrap();
    {
        let mut group = Track::new_group(TrackId(101), "Group", p.scenes.len());
        group.colour = crate::model::LIVE_CLIP_PALETTE[0];
        p.insert_group(group, &[TrackId(1)]);
    }
    let before = p.clone();
    let error = apply(
        &mut p,
        ProjectEdit::RemoveTracks {
            tracks: vec![TrackId(1)],
        },
    )
    .unwrap_err();
    assert_eq!(error.0, "a set needs at least one track");
    assert_eq!(p, before);
    let error = apply(
        &mut p,
        ProjectEdit::RemoveTracks {
            tracks: vec![TrackId(99)],
        },
    )
    .unwrap_err();
    assert_eq!(error.0, "unknown track");
    assert_eq!(p, before);
}

#[test]
fn scene_removal_preserves_the_last_scene_and_its_slots() {
    for with_tracks in [false, true] {
        let mut p = project();
        if !with_tracks {
            p.tracks.clear();
        } else {
            apply(
                &mut p,
                ProjectEdit::CreateClip {
                    track: TrackId(1),
                    slot: 0,
                    length: BeatTime(3840),
                },
            )
            .unwrap();
        }
        apply(&mut p, ProjectEdit::RemoveScene { scene: SceneId(3) }).unwrap();
        apply(&mut p, ProjectEdit::RemoveScene { scene: SceneId(2) }).unwrap();
        let before = p.clone();
        let error = apply(&mut p, ProjectEdit::RemoveScene { scene: SceneId(1) }).unwrap_err();
        assert_eq!(error.0, "a set needs at least one scene");
        assert_eq!(p, before);
    }
}

#[test]
fn scene_removal_rejects_jump_references_and_other_edits_work() {
    let mut p = project();
    p.scenes[0].follow_actions.actions = FollowActions {
        action_a: FollowAction::Jump(SceneId(2)),
        ..Default::default()
    };
    assert!(apply(&mut p, ProjectEdit::RemoveScene { scene: SceneId(2) }).is_err());
    p.scenes[0].follow_actions.actions.action_a = FollowAction::NoAction;
    apply(
        &mut p,
        ProjectEdit::RenameTrack {
            track: TrackId(1),
            name: "renamed".into(),
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::RecolourTrack {
            track: TrackId(1),
            colour: Colour {
                red: 4,
                green: 5,
                blue: 6,
            },
        },
    )
    .unwrap();
    apply(&mut p, ProjectEdit::MoveTrack { from: 0, to: 1 }).unwrap();
    apply(
        &mut p,
        ProjectEdit::RenameScene {
            scene: SceneId(1),
            name: "renamed".into(),
        },
    )
    .unwrap();
    assert_eq!(p.tracks[1].name, "renamed");
    assert_eq!(p.tracks[1].colour.red, 4);
    assert_eq!(p.scenes[0].name, "renamed");
}

#[test]
fn clip_inspector_edits_cover_properties_follow_actions_and_events() {
    use crate::model::{LaunchQuantisation, OscAddress, OscArgument};
    let mut p = project();
    let track = TrackId(1);
    apply(
        &mut p,
        ProjectEdit::CreateClip {
            track,
            slot: 0,
            length: BeatTime(3840),
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::RenameClip {
            track,
            slot: 0,
            name: "Cue".into(),
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::RecolourClip {
            track,
            slot: 0,
            colour: Colour {
                red: 10,
                green: 20,
                blue: 30,
            },
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::SetClipLength {
            track,
            slot: 0,
            length: BeatTime(7680),
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::SetClipLooping {
            track,
            slot: 0,
            looping: false,
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::SetClipLegato {
            track,
            slot: 0,
            legato: true,
        },
    )
    .unwrap();
    assert!(p.tracks[0].slots[0].clip.as_ref().unwrap().legato);
    apply(
        &mut p,
        ProjectEdit::SetClipQuantisation {
            track,
            slot: 0,
            quantisation: LaunchQuantisation::None,
        },
    )
    .unwrap();
    let mut follow = p.tracks[0].slots[0]
        .clip
        .as_ref()
        .unwrap()
        .follow_actions
        .clone();
    follow.actions.enabled = true;
    follow.actions.action_a = FollowAction::Next;
    follow.actions.chance_a = 3;
    follow.linked = false;
    follow.time = BeatTime(1920);
    apply(
        &mut p,
        ProjectEdit::SetClipFollowActions {
            track,
            slot: 0,
            follow_actions: follow,
        },
    )
    .unwrap();
    let go = OscAddress::Relative("/go".into());
    let event = p
        .hit(track, BeatTime(960), go, vec![OscArgument::Int(5)])
        .unwrap();
    apply(
        &mut p,
        ProjectEdit::AddEvent {
            track,
            slot: 0,
            event,
            carried: None,
        },
    )
    .unwrap();
    let absolute = OscAddress::Absolute("/absolute".into());
    let arguments = vec![OscArgument::Int(5), OscArgument::Bool(true)];
    let updated = p.hit(track, BeatTime(960), absolute, arguments).unwrap();
    apply(
        &mut p,
        ProjectEdit::UpdateEvent {
            track,
            slot: 0,
            event: 0,
            value: updated,
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::RemoveEvent {
            track,
            slot: 0,
            event: 0,
        },
    )
    .unwrap();
    let clip = p.tracks[0].slots[0].clip.as_ref().unwrap();
    assert_eq!(clip.name, "Cue");
    assert_eq!(
        clip.colour,
        Colour {
            red: 10,
            green: 20,
            blue: 30
        }
    );
    assert_eq!(clip.length(), BeatTime(7680));
    assert!(!clip.looping);
    assert_eq!(clip.quantisation, LaunchQuantisation::None);
    assert!(clip.follow_actions.actions.enabled && !clip.follow_actions.linked);
    assert!(clip.events.is_empty());
    assert!(
        apply(
            &mut p,
            ProjectEdit::RemoveEvent {
                track,
                slot: 0,
                event: 0
            }
        )
        .is_err()
    );
}

fn clip_at(p: &Project, track: usize, slot: usize) -> Option<&Clip> {
    p.tracks[track].slots[slot].clip.as_ref()
}

#[test]
fn paste_gives_a_fresh_id_and_replaces_an_occupied_slot() {
    let mut p = project();
    let t = TrackId(1);
    let length = BeatTime(7680);
    apply(
        &mut p,
        ProjectEdit::CreateClip {
            track: t,
            slot: 0,
            length,
        },
    )
    .unwrap();
    let mut clip = clip_at(&p, 0, 0).unwrap().clone();
    clip.name = "Pasted".into();
    for slot in [1, 1] {
        apply(
            &mut p,
            ProjectEdit::PasteClip {
                track: t,
                slot,
                clip: Box::new(clip.clone()),
                carried: Vec::new(),
            },
        )
        .unwrap();
    }
    let pasted = clip_at(&p, 0, 1).unwrap();
    assert_eq!(pasted.name, "Pasted");
    assert_ne!(pasted.id, clip.id);
    assert!(
        apply(
            &mut p,
            ProjectEdit::PasteClip {
                track: t,
                slot: 9,
                clip: Box::new(clip),
                carried: Vec::new(),
            },
        )
        .is_err()
    );
}

fn add_output(p: &mut Project, name: &str) -> TrackId {
    apply(
        p,
        ProjectEdit::AddOutput {
            name: name.into(),
            host: "127.0.0.1".into(),
            port: 9000,
        },
    )
    .unwrap();
    p.outputs.last().unwrap().id
}

// 2026-10-14: ungrouping returns Group routes to Main, preserving explicit routes.
#[test]
fn ungroup_returns_following_members_to_main() {
    let mut p = project();
    let out = add_output(&mut p, "LX");
    apply(
        &mut p,
        ProjectEdit::CreateGroup {
            tracks: vec![TrackId(1), TrackId(2)],
        },
    )
    .unwrap();
    let group = p.group_tracks().next().unwrap().id;
    p[group].route = Route::single(out);
    p[TrackId(1)].route = Route::Group;
    p[TrackId(2)].route = Route::empty();
    apply(&mut p, ProjectEdit::Ungroup { group }).unwrap();
    assert_eq!(p.resolved_route(TrackId(1)), [p.main.id]);
    assert!(p.resolved_route(TrackId(2)).is_empty());
    assert_eq!(p[TrackId(1)].route, Route::single(p.main.id));
    assert!(p.validate().is_ok());
    assert!(apply(&mut p, ProjectEdit::Ungroup { group }).is_err());
}

// From 2026-10-11: outputs pick their own palette colour and leave the
// track/group swatch counter alone.
#[test]
fn add_output_keeps_the_track_colour_counter() {
    let mut p = project();
    let before = p.next_colour_index;
    add_output(&mut p, "A");
    add_output(&mut p, "B");
    assert_eq!(p.next_colour_index, before);
    assert_eq!(p.outputs[0].colour, LIVE_CLIP_PALETTE[0]);
    assert_eq!(p.outputs[1].colour, LIVE_CLIP_PALETTE[1]);
}

// From 2026-10-11: output latency is bounded on its own, so an unrouted
// output cannot break later route toggles or AddTrack.
#[test]
fn output_latency_is_bounded_without_a_route() {
    let mut p = project();
    let out = add_output(&mut p, "A");
    let mut output = p.outputs[0].clone();
    output.delay_ms = 5000.0;
    let mut bad = p.clone();
    bad.outputs[0] = output;
    let error = bad.validate().unwrap_err().to_string();
    assert!(error.contains("output latency"), "{error}");
    p.outputs[0].delay_ms = -1000.0;
    assert!(p.validate().is_ok());
    apply(
        &mut p,
        ProjectEdit::ToggleOutputInRoute {
            tracks: vec![TrackId(1)],
            output: out,
        },
    )
    .unwrap();
}

#[test]
fn outputs_can_share_names_and_rename_without_changing_settings() {
    let mut p = project();
    let a = add_output(&mut p, "Lights");
    let b = add_output(&mut p, "Lights");
    assert_ne!(a, b);
    let before = p.output(b).unwrap().clone();
    apply(
        &mut p,
        ProjectEdit::RenameTrack {
            track: b,
            name: "Stage Lights".into(),
        },
    )
    .unwrap();
    let mut expected = before;
    expected.name = "Stage Lights".into();
    assert_eq!(p.output(b).unwrap(), &expected);
    expected.destination_mut().unwrap().port = 9010;
    apply(
        &mut p,
        ProjectEdit::SetDestination {
            output: expected.id,
            destination: expected.output.clone(),
        },
    )
    .unwrap();
    assert_eq!(p.output(b).unwrap(), &expected);
    assert_eq!(p.output(a).unwrap().name, "Lights");
}

#[test]
fn output_stage_edits_reach_outputs_and_main_and_reject_out_of_range_values() {
    let mut p = project();
    let out = add_output(&mut p, "A");
    let stage = crate::model::OutputStage {
        frame_rate: 44.0,
        budget_on: true,
        budget: 600,
        bundle: true,
        refresh_on: true,
        refresh: 0.8,
    };
    for id in [out, p.main.id] {
        apply(
            &mut p,
            ProjectEdit::SetOutputStage {
                output: id,
                stage: stage.clone(),
            },
        )
        .unwrap();
        assert_eq!(p[id].output_stage(), Some(&stage));
    }
    let before = p.clone();
    for (target, frame_rate) in [(out, 61.0), (out, 0.0), (TrackId(1), 30.0)] {
        let candidate = crate::model::OutputStage {
            frame_rate,
            ..Default::default()
        };
        assert!(
            apply(
                &mut p,
                ProjectEdit::SetOutputStage {
                    output: target,
                    stage: candidate,
                },
            )
            .is_err()
        );
    }
    p.main.output = None;
    let error = apply(
        &mut p,
        ProjectEdit::SetOutputStage {
            output: before.main.id,
            stage: stage.clone(),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("host"), "{error}");
    p.main.output = before.main.output.clone();
    assert_eq!(p, before);
}

#[test]
fn threshold_travels_in_parameter_output_edits_and_rejects_negatives() {
    let mut p = project();
    let target = crate::model::ParameterRef::volume(TrackId(1));
    let mut output = p.parameter(&target).unwrap().output.clone().into_owned();
    output.threshold = 0.25;
    apply(
        &mut p,
        ProjectEdit::SetParameterOutput {
            target: target.clone(),
            output: output.clone(),
        },
    )
    .unwrap();
    assert_eq!(p.parameter(&target).unwrap().output.threshold, 0.25);
    for threshold in [-0.01, f64::NAN, f64::INFINITY] {
        output.threshold = threshold;
        assert!(
            apply(
                &mut p,
                ProjectEdit::SetParameterOutput {
                    target: target.clone(),
                    output: output.clone(),
                },
            )
            .is_err()
        );
    }
    assert_eq!(p.parameter(&target).unwrap().output.threshold, 0.25);
}

#[test]
fn clip_markers_are_atomic_normalise_start_and_keep_outside_content() {
    use crate::model::region::Region;
    let mut p = project();
    apply(
        &mut p,
        ProjectEdit::CreateClip {
            track: TrackId(1),
            slot: 0,
            length: BeatTime(12 * 960),
        },
    )
    .unwrap();
    let event = p.tracks[0].hit(
        BeatTime(20 * 960),
        crate::model::OscAddress::Absolute("/outside".into()),
        vec![],
        || crate::model::DeviceId(1),
    );
    p.clip_mut(TrackId(1), 0)
        .unwrap()
        .events
        .push(event.clone());
    let markers = Region {
        start: BeatTime(960),
        end: BeatTime(12 * 960),
        loop_start: BeatTime(4 * 960),
        loop_end: BeatTime(8 * 960),
        looping: true,
    };
    for (start, expected) in [(1, 1), (8, 4), (9, 5), (11, 7)] {
        apply(
            &mut p,
            ProjectEdit::SetClipMarkers {
                track: TrackId(1),
                slot: 0,
                markers: Region {
                    start: BeatTime(start * 960),
                    ..markers
                },
            },
        )
        .unwrap();
        assert_eq!(
            p.clip(TrackId(1), 0).unwrap().start,
            BeatTime(expected * 960)
        );
    }
    // Normalise before validating Start against End: Loop ignores End while playing.
    apply(
        &mut p,
        ProjectEdit::SetClipMarkers {
            track: TrackId(1),
            slot: 0,
            markers: Region {
                start: BeatTime(8 * 960),
                end: BeatTime(6 * 960),
                ..markers
            },
        },
    )
    .unwrap();
    assert_eq!(p.clip(TrackId(1), 0).unwrap().start, BeatTime(4 * 960));
    let before = p.clone();
    for bad in [
        Region {
            start: markers.end,
            looping: false,
            ..markers
        },
        Region {
            end: markers.start,
            ..markers
        },
        Region {
            loop_start: markers.loop_end,
            ..markers
        },
        Region {
            loop_end: markers.loop_start,
            ..markers
        },
    ] {
        assert!(
            apply(
                &mut p,
                ProjectEdit::SetClipMarkers {
                    track: TrackId(1),
                    slot: 0,
                    markers: bad
                }
            )
            .is_err()
        );
        assert_eq!(p, before);
    }
    apply(
        &mut p,
        ProjectEdit::SetClipMarkers {
            track: TrackId(1),
            slot: 0,
            markers: Region {
                start: BeatTime(9 * 960),
                looping: false,
                ..markers
            },
        },
    )
    .unwrap();
    assert_eq!(p.clip(TrackId(1), 0).unwrap().start, BeatTime(9 * 960));
    apply(
        &mut p,
        ProjectEdit::SetClipLooping {
            track: TrackId(1),
            slot: 0,
            looping: true,
        },
    )
    .unwrap();
    assert_eq!(p.clip(TrackId(1), 0).unwrap().start, BeatTime(5 * 960));
    apply(
        &mut p,
        ProjectEdit::SetClipLength {
            track: TrackId(1),
            slot: 0,
            length: BeatTime(2 * 960),
        },
    )
    .unwrap();
    let clip = p.clip(TrackId(1), 0).unwrap();
    assert_eq!(clip.events, [event]);
    assert_eq!(clip.end, BeatTime(7 * 960));
    assert_eq!(clip.loop_end, BeatTime(6 * 960));
    let before = p.clone();
    assert!(
        apply(
            &mut p,
            ProjectEdit::SetClipLength {
                track: TrackId(1),
                slot: 0,
                length: BeatTime::ZERO
            }
        )
        .is_err()
    );
    assert_eq!(p, before);
}

#[test]
fn destination_edits_preserve_the_track_and_only_main_can_be_unset() {
    let mut p = project();
    let output = add_output(&mut p, "Lights");
    for id in [output, p.main.id] {
        let before = p.clone();
        let mut destination = p[id].output.clone().unwrap();
        destination.destination.host = "localhost".into();
        destination.destination.port = 9101;
        destination.destination.mode = crate::model::UdpMode::Broadcast;
        destination.stage.frame_rate = 44.0;
        apply(
            &mut p,
            ProjectEdit::SetDestination {
                output: id,
                destination: Some(destination.clone()),
            },
        )
        .unwrap();
        let mut expected = before;
        expected[id].output = Some(destination.clone());
        assert_eq!(p, expected);
        let before = p.clone();
        let result = apply(
            &mut p,
            ProjectEdit::SetDestination {
                output: id,
                destination: None,
            },
        );
        if id == p.main.id {
            result.unwrap();
            let mut expected = before;
            expected[id].output = None;
            assert_eq!(p, expected);
            apply(
                &mut p,
                ProjectEdit::SetDestination {
                    output: id,
                    destination: Some(destination),
                },
            )
            .unwrap();
        } else {
            assert!(result.is_err());
            assert_eq!(p, before);
        }
    }
}

#[test]
fn destination_edits_reject_invalid_components_and_non_destination_tracks_atomically() {
    let mut p = project();
    let output = add_output(&mut p, "Lights");
    let destination = p[output].output.clone().unwrap();
    let mut bad_port = destination.clone();
    bad_port.destination.port = 0;
    let mut bad_host = destination.clone();
    bad_host.destination.host.clear();
    let mut bad_stage = destination.clone();
    bad_stage.stage.frame_rate = 0.0;
    for (id, value) in [
        (output, bad_port),
        (output, bad_host),
        (output, bad_stage),
        (p.tracks[0].id, destination.clone()),
        (TrackId(9999), destination),
    ] {
        let before = p.clone();
        assert!(
            apply(
                &mut p,
                ProjectEdit::SetDestination {
                    output: id,
                    destination: Some(value)
                }
            )
            .is_err()
        );
        assert_eq!(p, before);
    }
}

#[test]
fn delay_edits_cover_every_track_kind_and_reject_combined_offsets_atomically() {
    let mut p = project();
    let output = add_output(&mut p, "Lights");
    let track = p.tracks[0].id;
    apply(
        &mut p,
        ProjectEdit::CreateGroup {
            tracks: vec![track],
        },
    )
    .unwrap();
    let group = p.group_of(track).unwrap().id;
    for id in [track, group, output, p.main.id] {
        let before = p.clone();
        apply(
            &mut p,
            ProjectEdit::SetTrackDelay {
                track: id,
                delay_ms: -12.5,
            },
        )
        .unwrap();
        let mut expected = before;
        expected[id].delay_ms = -12.5;
        assert_eq!(p, expected);
        for invalid in [f64::NAN, f64::INFINITY, 1000.1, -1000.1] {
            let before = p.clone();
            assert!(
                apply(
                    &mut p,
                    ProjectEdit::SetTrackDelay {
                        track: id,
                        delay_ms: invalid
                    }
                )
                .is_err()
            );
            assert_eq!(p, before);
        }
    }
    apply(
        &mut p,
        ProjectEdit::SetTrackRoutes {
            routes: vec![(track, crate::model::Route::single(output))],
        },
    )
    .unwrap();
    apply(
        &mut p,
        ProjectEdit::SetTrackDelay {
            track,
            delay_ms: 600.0,
        },
    )
    .unwrap();
    let before = p.clone();
    for (id, delay_ms) in [(output, 500.0), (TrackId(9999), 0.0)] {
        assert!(
            apply(
                &mut p,
                ProjectEdit::SetTrackDelay {
                    track: id,
                    delay_ms
                }
            )
            .is_err()
        );
        assert_eq!(p, before);
    }
}

#[test]
fn remove_scenes_is_atomic_and_keeps_surviving_slot_structure() {
    let mut p = project();
    p.tracks[0].slots[1].has_stop_button = false;
    let survivor = p.tracks[0].slots[1].clone();
    apply(
        &mut p,
        ProjectEdit::RemoveScenes {
            scenes: vec![SceneId(3), SceneId(1), SceneId(3)],
        },
    )
    .unwrap();
    assert_eq!(
        p.scenes.iter().map(|s| s.id).collect::<Vec<_>>(),
        vec![SceneId(2)]
    );
    assert_eq!(p.tracks[0].slots, vec![survivor]);
    assert!(p.tracks.iter().all(|t| t.slots.len() == 1));
}

#[test]
fn remove_scenes_rejects_unknown_ids_last_scene_and_jump_references_atomically() {
    for scenes in [
        vec![SceneId(1), SceneId(99)],
        vec![SceneId(1), SceneId(2), SceneId(3)],
    ] {
        let mut p = project();
        let before = p.clone();
        assert!(apply(&mut p, ProjectEdit::RemoveScenes { scenes }).is_err());
        assert_eq!(p, before);
    }
    let mut p = project();
    p.scenes[0].follow_actions.actions.action_a = FollowAction::Jump(SceneId(3));
    let before = p.clone();
    assert!(
        apply(
            &mut p,
            ProjectEdit::RemoveScenes {
                scenes: vec![SceneId(2), SceneId(3)]
            }
        )
        .is_err()
    );
    assert_eq!(p, before);
}

#[test]
fn scene_block_moves_keep_slots_and_project_order_at_original_boundaries() {
    for (selected, boundary, expected) in [
        (vec![SceneId(3)], 0, vec![3, 1, 2]),
        (vec![SceneId(1)], 3, vec![2, 3, 1]),
        (vec![SceneId(2), SceneId(1), SceneId(2)], 3, vec![3, 1, 2]),
        (vec![SceneId(3), SceneId(1)], 2, vec![2, 1, 3]),
        (vec![SceneId(1), SceneId(2)], 1, vec![1, 2, 3]),
        (vec![SceneId(1), SceneId(2), SceneId(3)], 3, vec![1, 2, 3]),
    ] {
        let mut p = project();
        // Include populated/empty slots and non-default stop flags, plus group slots.
        apply(
            &mut p,
            ProjectEdit::CreateClip {
                track: TrackId(1),
                slot: 0,
                length: BeatTime(960),
            },
        )
        .unwrap();
        p.tracks[0].slots[1].has_stop_button = false;
        p.insert_group(Track::new_group(TrackId(101), "Group", 3), &[TrackId(1)]);
        let before = p.clone();
        apply(
            &mut p,
            ProjectEdit::MoveScenes {
                scenes: selected,
                boundary,
            },
        )
        .unwrap();
        assert_eq!(
            p.scenes.iter().map(|s| s.id.0).collect::<Vec<_>>(),
            expected
        );
        for (row, id) in expected.iter().enumerate() {
            let old = before.scenes.iter().position(|s| s.id.0 == *id).unwrap();
            assert_eq!(p.scenes[row], before.scenes[old]);
            for (track, source) in p.tracks.iter().zip(&before.tracks) {
                assert_eq!(track.slots[row], source.slots[old]);
            }
        }
    }
}

#[test]
fn scene_block_move_rejects_stale_ids_and_bad_boundaries_atomically() {
    for (scenes, boundary) in [(vec![SceneId(99)], 1), (vec![SceneId(1)], 4)] {
        let mut p = project();
        let before = p.clone();
        assert!(apply(&mut p, ProjectEdit::MoveScenes { scenes, boundary }).is_err());
        assert_eq!(p, before);
    }
}
