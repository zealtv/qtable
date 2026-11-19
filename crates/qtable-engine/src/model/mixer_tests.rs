//! Who sends under Activator and Solo: one test per row of of the Live 12 solo and mute rules
//! (checked 2026-10-12).
use super::*;
use crate::edit::{ProjectEdit, apply};

/// Tracks 1–4; tracks 2 and 3 form group 1.
fn project() -> Project {
    let mut p = Project {
        tracks: (1..=4)
            .map(|i| Track::new(TrackId(i), format!("{i}"), 0))
            .collect(),
        ..Project::default()
    };
    p.insert_group(
        Track::new_group(TrackId(101), "G", 0),
        &[TrackId(2), TrackId(3)],
    );
    p
}

fn t(id: u64) -> TrackId {
    TrackId(id)
}
const GROUP: TrackId = TrackId(101);

fn active(p: &mut Project, strips: &[TrackId], active: bool) {
    let strips = strips.to_vec();
    apply(p, ProjectEdit::SetActive { strips, active }).unwrap();
}
fn solo(p: &mut Project, strips: &[TrackId], solo: bool, exclusive: bool) {
    let strips = strips.to_vec();
    apply(
        p,
        ProjectEdit::SetSolo {
            strips,
            solo,
            exclusive,
        },
    )
    .unwrap();
}
fn sending(p: &Project) -> Vec<u64> {
    p.clip_tracks()
        .filter(|t| p.track_sends(t.id))
        .map(|t| t.id.0)
        .collect()
}

#[test]
fn every_track_sends_by_default() {
    assert_eq!(sending(&project()), vec![1, 2, 3, 4]);
}

#[test]
fn rule_1_an_inactive_track_is_silent() {
    let mut p = project();
    active(&mut p, &[t(1)], false);
    assert_eq!(sending(&p), vec![2, 3, 4]);
}

#[test]
fn rule_2_solo_silences_every_other_track() {
    let mut p = project();
    solo(&mut p, &[t(4)], true, true);
    assert_eq!(sending(&p), vec![4]);
}

#[test]
fn rule_3_exclusive_solo_replaces_and_a_modifier_click_adds() {
    let mut p = project();
    solo(&mut p, &[t(1)], true, true);
    solo(&mut p, &[t(4)], true, true);
    assert_eq!(sending(&p), vec![4]);
    solo(&mut p, &[t(1)], true, false);
    assert_eq!(sending(&p), vec![1, 4]);
    // Plain click on a soloed track unsolos just that one (open question 2).
    solo(&mut p, &[t(1)], false, true);
    assert_eq!(sending(&p), vec![4]);
}

#[test]
fn rule_4_switches_apply_to_every_selected_strip() {
    let mut p = project();
    solo(&mut p, &[t(1), t(4)], true, true);
    assert_eq!(sending(&p), vec![1, 4]);
    active(&mut p, &[t(1), t(4)], false);
    assert!(sending(&p).is_empty());
}

#[test]
fn rule_6_solo_and_activator_are_independent_flags() {
    let mut p = project();
    active(&mut p, &[t(2)], false);
    solo(&mut p, &[t(1)], true, true);
    solo(&mut p, &[t(1)], false, true);
    assert_eq!(sending(&p), vec![1, 3, 4]);
    assert_eq!(
        p.track_facts(t(2)).map(|f| f.mixer),
        Some(MixerState {
            active: false,
            solo: false
        })
    );
}

#[test]
fn rule_7_an_inactive_track_stays_silent_under_another_solo() {
    let mut p = project();
    active(&mut p, &[t(1)], false);
    solo(&mut p, &[t(4)], true, true);
    assert_eq!(sending(&p), vec![4]);
}

#[test]
fn rule_8_an_inactive_soloed_track_stays_silent() {
    // Manual reading; a MIDI observation in Live conflicts with it.
    let mut p = project();
    active(&mut p, &[t(1)], false);
    solo(&mut p, &[t(1)], true, true);
    assert!(sending(&p).is_empty());
}

#[test]
fn rule_9_a_group_with_some_soloed_members_looks_partial() {
    let mut p = project();
    solo(&mut p, &[t(2)], true, true);
    assert_eq!(p.track_facts(GROUP).unwrap().solo_look, SoloLook::Partial);
    assert_eq!(p.track_facts(GROUP).map(|f| f.mixer.solo), Some(false));
    solo(&mut p, &[GROUP], true, true);
    assert_eq!(p.track_facts(GROUP).unwrap().solo_look, SoloLook::On);
    assert_eq!(p.track_facts(t(1)).unwrap().solo_look, SoloLook::Off);
}

#[test]
fn rule_10_soloing_a_group_plays_its_members() {
    let mut p = project();
    solo(&mut p, &[GROUP], true, true);
    assert_eq!(sending(&p), vec![2, 3]);
}

#[test]
fn rule_11_a_soloed_group_with_soloed_members_plays_only_those() {
    let mut p = project();
    solo(&mut p, &[t(2)], true, true);
    // Exclusive group click keeps its own members' solos (open question 3).
    solo(&mut p, &[GROUP], true, true);
    assert_eq!(sending(&p), vec![2]);
}

#[test]
fn rule_12_group_solo_does_not_reactivate_a_member() {
    let mut p = project();
    active(&mut p, &[t(3)], false);
    solo(&mut p, &[GROUP], true, true);
    assert_eq!(sending(&p), vec![2]);
}

#[test]
fn rule_13_an_inactive_group_silences_its_members_and_keeps_their_flags() {
    let mut p = project();
    active(&mut p, &[GROUP], false);
    assert_eq!(sending(&p), vec![1, 4]);
    solo(&mut p, &[t(2)], true, true);
    assert!(sending(&p).is_empty());
    active(&mut p, &[GROUP], true);
    assert_eq!(sending(&p), vec![2]);
}

#[test]
fn exclusive_track_solo_clears_group_solos() {
    let mut p = project();
    solo(&mut p, &[GROUP], true, true);
    solo(&mut p, &[t(1)], true, true);
    assert_eq!(p.track_facts(GROUP).map(|f| f.mixer.solo), Some(false));
    assert_eq!(sending(&p), vec![1]);
}

#[test]
fn sending_outputs_is_empty_while_silenced() {
    let mut p = project();
    p.outputs
        .push(Track::new_output(TrackId(201), "o", "127.0.0.1", 9000));
    p.outputs[0].mixer.active = true;
    p.tracks[0].route = Route::single(TrackId(201));
    assert_eq!(p.sending_outputs(TrackId(1)).count(), 1);
    active(&mut p, &[t(1)], false);
    assert_eq!(p.sending_outputs(TrackId(1)).count(), 0);
}

#[test]
fn unknown_strips_are_rejected_and_old_files_default_to_active() {
    let mut p = project();
    let edit = ProjectEdit::SetActive {
        strips: vec![t(99)],
        active: false,
    };
    assert!(apply(&mut p, edit).is_err());
    let track: Track = ron::from_str("(id: (1), name: \"x\", slots: [])").unwrap();
    assert_eq!(
        track.mixer,
        MixerState {
            active: true,
            solo: false
        }
    );
}

#[test]
fn strips_number_group_tracks_before_their_members_as_live_does() {
    let mut p = project();
    assert_eq!(p.strips(), vec![t(1), GROUP, t(2), t(3), t(4)]);
    assert_eq!(p.track_facts(GROUP).and_then(|f| f.number), Some(2));
    assert_eq!(p.track_facts(t(4)).and_then(|f| f.number), Some(5));
    // Folding hides members but keeps their numbers, as Live's do.
    p[TrackId(101)].fold_state = true;
    assert_eq!(p.track_facts(t(4)).and_then(|f| f.number), Some(5));
    assert_eq!(p.track_facts(TrackId(109)).and_then(|f| f.number), None);
}

/// Approved qtable policy (decided 2026-11-06): keep the existing
/// Solo precedence and inactive-ancestor gating at every depth. These cases
/// extend qtable's rules; they are not a new observation of Live combinations.
fn nested_project() -> Project {
    let mut p = project();
    let mut outer = Track::new_group(t(102), "Outer", 0);
    outer.fold_state = true;
    p[GROUP].group = Some(outer.id);
    p[t(1)].group = Some(outer.id);
    p.tracks.insert(0, outer);
    let mut other = Track::new_group(t(103), "Other inner", 0);
    other.group = Some(t(102));
    let at = p.tracks.iter().position(|track| track.id == t(1)).unwrap();
    p.tracks.insert(at, other);
    p[t(1)].group = Some(t(103));
    p.validate().unwrap();
    p
}

#[test]
fn nested_solo_matrix_preserves_existing_precedence_and_partial_ancestors() {
    for (solos, expected) in [
        (vec![102], vec![1, 2, 3]),
        (vec![101], vec![2, 3]),
        (vec![2], vec![2]),
        (vec![102, 101], vec![2, 3]),
        (vec![102, 2], vec![2]),
        (vec![102, 1], vec![1]),
        (vec![101, 103], vec![1, 2, 3]),
    ] {
        let mut p = nested_project();
        let ids: Vec<_> = solos.iter().copied().map(t).collect();
        solo(&mut p, &ids, true, false);
        assert_eq!(sending(&p), expected, "solos={solos:?}");
        assert_eq!(
            p.track_facts(t(102)).unwrap().solo_look,
            if solos.contains(&102) {
                SoloLook::On
            } else {
                SoloLook::Partial
            }
        );
    }
    // Exclusive outer Solo preserves an explicitly soloed nested descendant.
    let mut p = nested_project();
    solo(&mut p, &[t(2)], true, true);
    solo(&mut p, &[t(102)], true, true);
    assert_eq!(sending(&p), [2]);
    assert!(p[t(2)].mixer.solo);
    assert_eq!(p.track_facts(GROUP).unwrap().solo_look, SoloLook::Partial);
}

#[test]
fn nested_mute_and_solo_gate_explicit_routes_without_changing_switches() {
    let mut p = nested_project();
    p.outputs
        .push(Track::new_output(t(201), "out", "127.0.0.1", 9000));
    // Explicit routes bypass group routing, but retain structural mute/Solo.
    for id in [1, 2, 3, 4] {
        p[t(id)].route = Route::single(t(201));
    }
    solo(&mut p, &[t(102)], true, true);
    active(&mut p, &[t(2)], false);
    assert_eq!(sending(&p), [1, 3]);
    active(&mut p, &[GROUP], false);
    assert_eq!(sending(&p), [1]);
    active(&mut p, &[t(102)], false);
    assert!(sending(&p).is_empty());
    solo(&mut p, &[t(3)], true, false);
    assert!(sending(&p).is_empty());
    active(&mut p, &[t(102), GROUP], true);
    assert_eq!(sending(&p), [3]);
    assert!(!p[t(2)].mixer.active);
    solo(&mut p, &[t(102), t(3)], false, false);
    solo(&mut p, &[t(201)], true, false);
    assert_eq!(sending(&p), [1, 3, 4]);
    solo(&mut p, &[t(3)], true, false);
    assert_eq!(sending(&p), [3]);
    p.outputs[0].mixer.active = false;
    assert_eq!(p.sending_outputs(t(3)).count(), 0);
}
