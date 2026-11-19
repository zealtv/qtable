//! Admission through the command application seam, including resulting planner state.
use super::*;
use crate::model::*;
use commands::{EffectiveFloor, admission::AdmissionOutcome};

impl Rig {
    fn receive_time(
        &mut self,
        floor: &mut EffectiveFloor,
        command: SchedulerCommand,
        receipt: Instant,
        dequeued: Instant,
    ) -> AdmissionOutcome {
        // Direct clients use P as R; engine-owned receipt propagation is covered over UDP.
        let mut envelope = command.enqueued(receipt);
        floor.admit(&mut envelope, dequeued, &self.planner, &self.clock);
        let outcome = envelope.admitted.unwrap().outcome;
        let effective = floor.advance(&envelope, dequeued);
        self.apply(envelope.command, effective);
        assert!(
            self._events
                .try_iter()
                .all(|e| !matches!(e, SchedulerEvent::Error(_)))
        );
        outcome
    }

    fn grouped() -> Self {
        let mut rig = Self::new();
        Arc::make_mut(&mut rig.planner.project).insert_group(
            Track::new_group(TrackId(101), "group", 2),
            &[TrackId(1), TrackId(2)],
        );
        // The second member is below a folded inner group. Its grid must take
        // part in the same admission decision as the outer group's direct leaf.
        let project = Arc::make_mut(&mut rig.planner.project);
        let mut inner = Track::new_group(TrackId(102), "inner", 2);
        inner.group = Some(TrackId(101));
        inner.fold_state = true;
        project[TrackId(2)].group = Some(inner.id);
        project.tracks.insert(2, inner);
        project.validate().unwrap();
        rig.planner
            .launch_at(TrackId(2), 0, BeatTime(2880))
            .unwrap();
        rig
    }

    fn incoming(&self, track: u64) -> &Voice {
        self.planner
            .voices
            .iter()
            .find(|v| v.track == TrackId(track) && v.slot == 1)
            .unwrap()
    }

    fn scene_launch(
        &mut self,
        floor: &mut EffectiveFloor,
        scene: u64,
        at: Instant,
        admitted: bool,
    ) {
        let command = SchedulerCommand::LaunchScene(SceneId(scene));
        if admitted {
            assert_eq!(
                self.receive_time(floor, command, at, at + Duration::from_millis(1)),
                AdmissionOutcome::Admitted
            );
        } else {
            self.apply(command, at);
        }
    }

    fn voice_state(&self) -> Vec<(u64, TrackId, usize, BeatTime, Option<BeatTime>)> {
        self.planner
            .voices
            .iter()
            .map(|v| (v.id, v.track, v.slot, v.launch, v.cutoff))
            .collect()
    }
}

#[test]
fn admitted_track_group_all_and_empty_stops_apply_on_the_recovered_boundary() {
    for command in [
        SchedulerCommand::StopTrack(TrackId(1)),
        SchedulerCommand::StopGroup(TrackId(101)),
        SchedulerCommand::StopAll,
        SchedulerCommand::Launch(TrackId(1), 1),
        SchedulerCommand::LaunchGroupSlot(TrackId(101), 1),
        SchedulerCommand::LaunchScene(SceneId(2)),
    ] {
        let mut rig = Rig::grouped();
        let project = Arc::make_mut(&mut rig.planner.project);
        for track in [TrackId(1), TrackId(2)] {
            project[track].slots[1].clip = None;
        }
        let one_track = matches!(
            command,
            SchedulerCommand::StopTrack(_) | SchedulerCommand::Launch(..)
        );
        let r = rig.boundary - Duration::from_micros(500);
        let d = rig.boundary + Duration::from_micros(500);
        let mut floor = EffectiveFloor::default();
        assert_eq!(
            rig.receive_time(&mut floor, command.clone(), r, d),
            AdmissionOutcome::Admitted
        );
        let expected = if one_track {
            vec![TrackId(1)]
        } else {
            vec![TrackId(1), TrackId(2)]
        };
        assert_eq!(rig.planner.stops.len(), expected.len(), "{command:?}");
        for track in &expected {
            assert_eq!(rig.planner.stops[track].beat, BeatTime(3840));
        }
        // A repeated request after B must retain its already-pending stop at B.
        assert_eq!(
            rig.receive_time(&mut floor, command, d, d),
            AdmissionOutcome::Admitted
        );
        for track in &expected {
            assert_eq!(rig.planner.stops[track].beat, BeatTime(3840));
        }
        rig.dispatch_through(d);
        for track in expected {
            assert!(!rig.planner.voices.iter().any(|v| v.track == track));
        }
        if one_track {
            assert!(rig.planner.voices.iter().any(|v| v.track == TrackId(2)));
        }
    }
}

#[test]
fn scene_and_group_apply_one_effective_time_to_mixed_grids_and_empty_members() {
    for command in [
        SchedulerCommand::LaunchScene(SceneId(2)),
        SchedulerCommand::LaunchGroupSlot(TrackId(101), 1),
    ] {
        for kind in 0..5 {
            let mut rig = Rig::grouped();
            let slot = &mut Arc::make_mut(&mut rig.planner.project)[TrackId(2)].slots[1];
            match kind {
                0 => {
                    slot.clip.as_mut().unwrap().quantisation =
                        LaunchQuantisation::Value(QuantisationValue::Triplet(NoteValue::Half))
                }
                1 => slot.clip.as_mut().unwrap().quantisation = LaunchQuantisation::None,
                2 | 3 => {
                    slot.clip = None;
                    slot.has_stop_button = kind == 2;
                }
                _ => {
                    slot.clip.as_mut().unwrap().quantisation =
                        LaunchQuantisation::Value(QuantisationValue::Note(NoteValue::ThirtySecond))
                }
            }
            let r = rig.boundary - Duration::from_millis(if kind == 4 { 200 } else { 1 });
            let d = rig.boundary + Duration::from_millis(1);
            let outcome = rig.receive_time(&mut EffectiveFloor::default(), command.clone(), r, d);
            assert_eq!(
                outcome,
                match kind {
                    1 => AdmissionOutcome::Unquantised,
                    4 => AdmissionOutcome::Expired,
                    _ => AdmissionOutcome::Admitted,
                }
            );
            let first = if kind == 1 || kind == 4 { 7680 } else { 3840 };
            assert_eq!(rig.incoming(1).launch, BeatTime(first));
            assert_eq!(
                rig.planner
                    .voices
                    .iter()
                    .find(|v| v.track == TrackId(1) && v.slot == 0)
                    .unwrap()
                    .cutoff,
                Some(BeatTime(first))
            );
            match kind {
                0 => assert_eq!(rig.incoming(2).launch, BeatTime(3840)),
                1 => assert_eq!(rig.incoming(2).launch, rig.clock.beat_at(d).unwrap()),
                2 => assert_eq!(rig.planner.stops[&TrackId(2)].beat, BeatTime(3840)),
                3 => {
                    assert!(!rig.planner.stops.contains_key(&TrackId(2)));
                    assert_eq!(
                        rig.planner
                            .voices
                            .iter()
                            .find(|v| v.track == TrackId(2))
                            .unwrap()
                            .cutoff,
                        None
                    );
                }
                _ => assert_eq!(rig.incoming(2).launch, BeatTime(3960)),
            }
        }
    }
}

#[test]
fn admitted_repeated_launch_retains_pending_voice() {
    let mut rig = Rig::grouped();
    let mut floor = EffectiveFloor::default();
    let r = rig.boundary - Duration::from_millis(1);
    let d = rig.boundary + Duration::from_millis(1);
    for _ in 0..2 {
        let serial = rig.planner.serial;
        assert_eq!(
            rig.receive_time(&mut floor, SchedulerCommand::Launch(TrackId(1), 1), r, d),
            AdmissionOutcome::Admitted
        );
        assert_eq!(rig.incoming(1).launch, BeatTime(3840));
        if serial > 2 {
            assert_eq!(
                rig.planner.serial, serial,
                "repeated launch must not replace the pending voice"
            );
        }
    }
}

#[test]
fn admitted_scene_cancel_restores_old_voices() {
    let mut rig = Rig::grouped();
    let mut floor = EffectiveFloor::default();
    let r = rig.boundary;
    let d = r + Duration::from_millis(1);
    assert_eq!(
        rig.receive_time(&mut floor, SchedulerCommand::LaunchScene(SceneId(2)), r, d),
        AdmissionOutcome::Admitted
    );
    let cancel = SchedulerCommand::CancelSceneLaunch.enqueued(r);
    let effective = floor.advance(&cancel, d);
    assert_eq!(effective, d);
    rig.apply(cancel.command, effective);
    assert_eq!(rig.planner.voices.len(), 2);
    assert!(
        rig.planner
            .voices
            .iter()
            .all(|v| v.slot == 0 && v.cutoff.is_none())
    );
    assert!(rig.planner.scene_timer.is_none());
}

#[test]
fn repeated_scene_cancel_restores_playing_and_stopped_tracks() {
    for admitted in [false, true] {
        for second_playing in [false, true] {
            let mut rig = Rig::grouped();
            if !second_playing {
                rig.planner.stop_now(TrackId(2));
            }
            let project = Arc::make_mut(&mut rig.planner.project);
            project.follow_actions_enabled = true;
            project.scenes[1].follow_actions.actions.enabled = true;
            project.scenes[1].follow_actions.actions.action_a = FollowAction::Next;
            let before = rig.voice_state();
            let mut floor = EffectiveFloor::default();
            let at = rig.boundary;
            rig.scene_launch(&mut floor, 2, at, admitted);
            let pending = rig.voice_state();
            let serial = rig.planner.serial;
            for offset in [10, 20] {
                rig.scene_launch(&mut floor, 2, at + Duration::from_millis(offset), admitted);
                assert_eq!(rig.voice_state(), pending);
                assert_eq!(rig.planner.serial, serial);
            }
            assert!(rig.planner.queued_follow_scene().is_some());
            rig.apply(
                SchedulerCommand::CancelSceneLaunch,
                at + Duration::from_millis(30),
            );
            assert_eq!(rig.voice_state(), before);
            assert!(rig.planner.scene_voices.is_empty());
            assert_eq!(rig.planner.last_triggered_scene, None);
            assert!(rig.planner.scene_timer.is_none());
            assert!(rig.planner.queued_follow_scene().is_none());
            assert!(rig.clock.is_playing());
            rig.dispatch_through(at + Duration::from_millis(2100));
            assert_eq!(rig.voice_state(), before);
        }
    }
}

#[test]
fn repeated_different_scene_cancel_only_owns_the_latest_scene() {
    for admitted in [false, true] {
        for empty_no_stop in [false, true] {
            let mut rig = Rig::grouped();
            let before = rig.voice_state();
            let project = Arc::make_mut(&mut rig.planner.project);
            let mut scene = project.scenes[1].clone();
            scene.id = SceneId(3);
            project.scenes.push(scene);
            for track in &mut project.tracks {
                let mut slot = track.slots[1].clone();
                if let Some(clip) = &mut slot.clip {
                    clip.id.0 += 20;
                }
                if empty_no_stop && track.id == TrackId(2) {
                    slot = Slot {
                        clip: None,
                        has_stop_button: false,
                    };
                }
                track.slots.push(slot);
            }
            project.validate().unwrap();
            let mut floor = EffectiveFloor::default();
            let at = rig.boundary;
            rig.scene_launch(&mut floor, 2, at, admitted);
            rig.scene_launch(&mut floor, 2, at + Duration::from_millis(10), admitted);
            let previous_scene = rig.voice_state();
            rig.scene_launch(&mut floor, 3, at + Duration::from_millis(20), admitted);
            let pending = rig.voice_state();
            rig.scene_launch(&mut floor, 3, at + Duration::from_millis(30), admitted);
            assert_eq!(rig.voice_state(), pending);
            rig.apply(
                SchedulerCommand::CancelSceneLaunch,
                at + Duration::from_millis(40),
            );
            let mut expected = before;
            if empty_no_stop {
                // The earlier scene's pending clip on the untouched member stays pending.
                expected.retain(|v| v.1 != TrackId(2));
                expected.extend(previous_scene.into_iter().filter(|v| v.1 == TrackId(2)));
                expected.sort_by_key(|v| v.0);
            }
            assert_eq!(rig.voice_state(), expected);
        }
    }
}

#[test]
fn repeated_scene_cancel_keeps_later_manual_launches() {
    for admitted in [false, true] {
        let mut rig = Rig::grouped();
        let mut floor = EffectiveFloor::default();
        let at = rig.boundary;
        rig.scene_launch(&mut floor, 2, at, admitted);
        rig.scene_launch(&mut floor, 2, at + Duration::from_millis(10), admitted);
        let waiting = rig.incoming(2).id;
        rig.apply(
            SchedulerCommand::Launch(TrackId(1), 0),
            at + Duration::from_millis(20),
        );
        let manual = rig.planner.serial;
        let mut expected = rig.voice_state();
        expected.retain(|v| v.0 != waiting);
        for voice in &mut expected {
            if voice.1 == TrackId(2) {
                voice.4 = None;
            }
        }
        rig.apply(
            SchedulerCommand::CancelSceneLaunch,
            at + Duration::from_millis(30),
        );
        assert_eq!(rig.voice_state(), expected);
        assert!(
            rig.planner
                .voices
                .iter()
                .any(|v| v.id == manual && v.launch == BeatTime(7680))
        );
    }
}

#[test]
fn repeated_scene_cancel_keeps_started_clips_and_restores_still_waiting_tracks() {
    for admitted in [false, true] {
        let mut rig = Rig::grouped();
        Arc::make_mut(&mut rig.planner.project)[TrackId(1)].slots[1]
            .clip
            .as_mut()
            .unwrap()
            .quantisation = LaunchQuantisation::Value(QuantisationValue::Note(NoteValue::Quarter));
        let old_second = rig
            .planner
            .voices
            .iter()
            .find(|v| v.track == TrackId(2))
            .unwrap()
            .id;
        let mut floor = EffectiveFloor::default();
        let at = rig.boundary;
        rig.scene_launch(&mut floor, 2, at, admitted);
        let started = rig.incoming(1).id;
        let waiting = rig.incoming(2).id;
        assert_eq!(rig.incoming(1).launch, BeatTime(4800));
        assert_eq!(rig.incoming(2).launch, BeatTime(7680));
        rig.dispatch_through(at + Duration::from_millis(550));
        rig.scene_launch(&mut floor, 2, at + Duration::from_millis(600), admitted);
        assert_eq!(rig.incoming(2).id, waiting);
        assert!(
            rig.planner
                .voices
                .iter()
                .any(|v| v.id != started && v.track == TrackId(1) && v.launch == BeatTime(5760))
        );
        rig.apply(
            SchedulerCommand::CancelSceneLaunch,
            at + Duration::from_millis(610),
        );
        assert_eq!(rig.planner.voices.len(), 2);
        assert!(
            rig.planner
                .voices
                .iter()
                .all(|v| [started, old_second].contains(&v.id)
                    && v.cutoff.is_none()
                    && v.cutoff_deadline.is_none())
        );
        rig.dispatch_through(at + Duration::from_millis(2100));
        assert_eq!(rig.planner.voices.len(), 2);
        assert!(
            rig.planner
                .voices
                .iter()
                .all(|v| [started, old_second].contains(&v.id))
        );
    }
}

#[test]
fn stopped_and_unquantised_launches_use_dequeue_in_application() {
    for stopped in [false, true] {
        let mut rig = Rig::new();
        let r = rig.boundary - Duration::from_millis(1);
        let d = rig.boundary + Duration::from_millis(1);
        if stopped {
            rig.apply(SchedulerCommand::Stop, r);
        } else {
            Arc::make_mut(&mut rig.planner.project)[TrackId(1)].slots[1]
                .clip
                .as_mut()
                .unwrap()
                .quantisation = LaunchQuantisation::None;
        }
        assert_eq!(
            rig.receive_time(
                &mut EffectiveFloor::default(),
                SchedulerCommand::Launch(TrackId(1), 1),
                r,
                d
            ),
            if stopped {
                AdmissionOutcome::Stopped
            } else {
                AdmissionOutcome::Unquantised
            }
        );
        assert_eq!(
            rig.incoming(1).launch,
            if stopped {
                BeatTime::ZERO
            } else {
                rig.clock.beat_at(d).unwrap()
            }
        );
        if stopped {
            assert_eq!(rig.clock.instant_at(BeatTime::ZERO), Some(d));
        }
    }
}

#[test]
fn admitted_legato_uses_boundary_phase_and_clip_follow_uses_its_own_deadline() {
    let mut rig = Rig::new();
    rig.planner.stop_all();
    rig.planner
        .launch_at(TrackId(1), 0, BeatTime(3360))
        .unwrap();
    let project = Arc::make_mut(&mut rig.planner.project);
    project.follow_actions_enabled = true;
    let clip = project[TrackId(1)].slots[1].clip.as_mut().unwrap();
    clip.legato = true;
    clip.follow_actions.actions.enabled = true;
    clip.follow_actions.actions.action_a = FollowAction::First;
    clip.follow_actions.linked = false;
    clip.follow_actions.time = BeatTime(480);
    let r = rig.boundary - Duration::from_millis(1);
    let d = rig.boundary + Duration::from_millis(1);
    assert_eq!(
        rig.receive_time(
            &mut EffectiveFloor::default(),
            SchedulerCommand::Launch(TrackId(1), 1),
            r,
            d
        ),
        AdmissionOutcome::Admitted
    );
    let incoming = rig.incoming(1);
    assert_eq!(incoming.launch, BeatTime(3840));
    assert_eq!(incoming.origin, BeatTime(3360)); // Halfway through the old clip at B.
    assert_eq!(rig.planner.follow_beat(incoming), Some(BeatTime(4320)));
    rig.dispatch_through(rig.boundary + Duration::from_millis(251));
    let followed = rig
        .planner
        .voices
        .iter()
        .find(|v| v.track == TrackId(1) && v.slot == 0 && v.launch == BeatTime(4320))
        .unwrap();
    assert_eq!(followed.origin, BeatTime(4320)); // Follow bypasses the global bar grid.
}

#[test]
fn admitted_scene_arms_follow_from_recovered_boundary() {
    let mut rig = Rig::grouped();
    let project = Arc::make_mut(&mut rig.planner.project);
    project.follow_actions_enabled = true;
    let follow = &mut project.scenes[1].follow_actions;
    follow.actions.enabled = true;
    follow.actions.action_a = FollowAction::First;
    follow.time = BeatTime(960);
    let r = rig.boundary - Duration::from_millis(1);
    let d = rig.boundary + Duration::from_millis(1);
    assert_eq!(
        rig.receive_time(
            &mut EffectiveFloor::default(),
            SchedulerCommand::LaunchScene(SceneId(2)),
            r,
            d
        ),
        AdmissionOutcome::Admitted
    );
    assert_eq!(rig.planner.scene_timer, Some((SceneId(2), BeatTime(4800))));
    rig.dispatch_through(rig.boundary + Duration::from_millis(501));
    assert_eq!(rig.planner.last_triggered_scene, Some(SceneId(1)));
    assert!(
        rig.planner
            .voices
            .iter()
            .filter(|v| v.slot == 0)
            .all(|v| v.launch == BeatTime(4800))
    );
}
