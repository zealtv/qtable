//! Duplicate always uses the next row. Scene insertion preserves all existing slots.
use super::{ProjectEdit, apply_inner, get_slot, paste_clip};
use crate::model::{ModelError, Project, TrackId};

pub(super) fn apply(project: &mut Project, track: TrackId, slot: usize) -> Result<(), ModelError> {
    let clip = get_slot(project, track, slot)?
        .clip
        .clone()
        .ok_or_else(|| ModelError("source slot is empty".into()))?;
    let dest = slot + 1;
    if dest == project.scenes.len() || get_slot(project, track, dest)?.clip.is_some() {
        // Bottom-row expansion was observed in Live 12 on 2026-10-11. Insertion
        // for an occupied next slot is inferred; AbletonOSC does not expose
        // Track.duplicate_clip_slot.
        apply_inner(
            project,
            ProjectEdit::AddScene {
                at: dest,
                name: format!("Scene {}", project.scenes.len() + 1),
            },
        )?;
    }
    paste_clip(project, track, dest, clip, " copy")
}

#[cfg(test)]
mod tests {
    use crate::edit::{ProjectEdit, apply};
    use crate::model::{
        BeatTime, Clip, ClipId, FollowAction, LaunchQuantisation, OscAddress, OscArgument, Project,
        SceneId, Slot, Track, TrackId,
    };

    fn project() -> Project {
        let mut p = Project::default();
        p.tracks
            .push(Track::new(TrackId(1), "Source track", p.scenes.len()));
        p.tracks
            .push(Track::new(TrackId(2), "Other track", p.scenes.len()));
        for at in p.scenes.len()..3 {
            apply(
                &mut p,
                ProjectEdit::AddScene {
                    at,
                    name: format!("S{at}"),
                },
            )
            .unwrap();
        }
        let mut source = Clip::new(
            ClipId(1),
            "Source",
            BeatTime(3840),
            crate::model::LIVE_CLIP_PALETTE[0],
        )
        .unwrap();
        source.looping = false;
        source.quantisation = LaunchQuantisation::None;
        source.follow_actions.actions.enabled = true;
        source.follow_actions.actions.action_a = FollowAction::Jump(p.scenes[2].id);
        let arguments = vec![OscArgument::Int(42), OscArgument::String("Intro".into())];
        let cue = OscAddress::Relative("/cue".into());
        source
            .events
            .push(p.hit(TrackId(1), BeatTime(960), cue, arguments).unwrap());
        p.tracks[0].slots[0].clip = Some(source);
        p
    }

    #[test]
    fn empty_next_slot_keeps_scene_structure_and_slot_flags() {
        let mut p = project();
        p.tracks[0].slots[1].has_stop_button = false;
        p.tracks[1].slots[1].clip = Some(
            Clip::new(
                ClipId(2),
                "Other",
                BeatTime(3840),
                crate::model::LIVE_CLIP_PALETTE[0],
            )
            .unwrap(),
        );
        let before = p.clone();
        apply(
            &mut p,
            ProjectEdit::DuplicateClip {
                track: TrackId(1),
                slot: 0,
            },
        )
        .unwrap();
        assert_eq!(p.scenes, before.scenes);
        assert_eq!(p.tracks[1], before.tracks[1]);
        assert_eq!(p.tracks[0].slots[0], before.tracks[0].slots[0]);
        assert_eq!(p.tracks[0].slots[2], before.tracks[0].slots[2]);
        assert!(!p.tracks[0].slots[1].has_stop_button);
        assert_copy(&p, 0, 1);
    }

    #[test]
    fn occupied_next_slot_inserts_a_row_and_preserves_every_existing_slot() {
        let mut p = project();
        p.tracks[0].slots[1].clip = Some(
            Clip::new(
                ClipId(2),
                "Occupied",
                BeatTime(7680),
                crate::model::LIVE_CLIP_PALETTE[0],
            )
            .unwrap(),
        );
        p.tracks[1].slots[1].clip = Some(
            Clip::new(
                ClipId(3),
                "Other",
                BeatTime(3840),
                crate::model::LIVE_CLIP_PALETTE[0],
            )
            .unwrap(),
        );
        p.tracks[0].slots[1].has_stop_button = false;
        p.tracks[1].slots[2].has_stop_button = false;
        let before = p.clone();
        apply(
            &mut p,
            ProjectEdit::DuplicateClip {
                track: TrackId(1),
                slot: 0,
            },
        )
        .unwrap();
        assert_eq!(p.scenes.len(), before.scenes.len() + 1);
        assert!(before.scenes.iter().all(|s| s.id != p.scenes[1].id));
        assert_eq!(p.scenes[0], before.scenes[0]);
        assert_eq!(&p.scenes[2..], &before.scenes[1..]);
        for (track, old) in p.tracks.iter().zip(&before.tracks) {
            assert_eq!(track.slots.len(), p.scenes.len());
            assert_eq!(track.slots[0], old.slots[0]);
            assert_eq!(&track.slots[2..], &old.slots[1..]);
        }
        assert_eq!(p.tracks[1].slots[1], Slot::default());
        assert_copy(&p, 0, 1);
        p.validate().unwrap();
    }

    #[test]
    fn bottom_slot_appends_a_scene_with_aligned_empty_slots_on_other_tracks() {
        let mut p = project();
        p.tracks[0].slots.swap(0, 2);
        let before = p.clone();
        apply(
            &mut p,
            ProjectEdit::DuplicateClip {
                track: TrackId(1),
                slot: 2,
            },
        )
        .unwrap();
        assert_eq!(&p.scenes[..3], &before.scenes);
        assert_eq!(p.scenes[3].id, SceneId(4));
        for (track, old) in p.tracks.iter().zip(&before.tracks) {
            assert_eq!(track.slots.len(), 4);
            assert_eq!(&track.slots[..3], &old.slots);
        }
        assert_eq!(p.tracks[1].slots[3], Slot::default());
        assert_copy(&p, 2, 3);
        p.validate().unwrap();
    }

    #[test]
    fn invalid_source_does_not_add_scenes_or_change_slots() {
        let mut p = project();
        let before = p.clone();
        for (track, slot) in [(TrackId(1), 2), (TrackId(1), usize::MAX), (TrackId(99), 0)] {
            assert!(apply(&mut p, ProjectEdit::DuplicateClip { track, slot }).is_err());
            assert_eq!(p, before);
        }
    }

    fn assert_copy(p: &Project, source: usize, dest: usize) {
        let source = p.tracks[0].slots[source].clip.as_ref().unwrap();
        let copy = p.tracks[0].slots[dest].clip.as_ref().unwrap();
        assert_ne!(copy.id, source.id);
        let mut expected = source.clone();
        expected.id = copy.id;
        expected.name.push_str(" copy");
        assert_eq!(*copy, expected);
    }
}
