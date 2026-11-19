//! Receiver Solo follows the group analogy: a soloed receiver acts as a virtual group of the
//! tracks routed to it. These are qtable's rules, not measured Live return-bus behaviour.
use qtable_engine::edit::{ProjectEdit, apply};
use qtable_engine::model::*;

fn project() -> Project {
    let mut p = Project {
        outputs: (1..=3)
            .map(|i| Track::new_output(TrackId(200 + i), format!("O{i}"), "127.0.0.1", 9000))
            .collect(),
        tracks: (1..=4)
            .map(|i| Track::new(TrackId(i), format!("T{i}"), 0))
            .collect(),
        ..Project::default()
    };
    let mut group = Track::new_group(TrackId(101), "G", 0);
    group.route = Route::single(TrackId(201));
    p.insert_group(group, &[TrackId(2), TrackId(3)]);
    p[TrackId(1)].route = Route::Outputs(vec![TrackId(201), TrackId(202)]);
    p[TrackId(2)].route = Route::Group;
    p[TrackId(3)].route = Route::Group;
    p[TrackId(4)].route = Route::single(TrackId(202));
    p
}

fn solo(p: &mut Project, output: u64, solo: bool, exclusive: bool) {
    apply(
        p,
        ProjectEdit::SetOutputSolo {
            output: TrackId(200 + output),
            solo,
            exclusive,
        },
    )
    .unwrap();
}

fn sends(p: &Project) -> Vec<u64> {
    p.clip_tracks()
        .filter(|t| p.track_sends(t.id))
        .map(|t| t.id.0)
        .collect()
}

#[test]
fn output_solo_selects_inherited_members_and_preserves_fan_out() {
    let mut p = project();
    solo(&mut p, 1, true, true);
    assert_eq!(sends(&p), vec![1, 2, 3]);
    assert_eq!(
        p.sending_outputs(TrackId(1))
            .map(|o| o.id.0 - 200)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(p.tracks.iter().all(|t| !t.mixer.solo));
    assert_eq!(
        p.track_facts(TrackId(101)).unwrap().solo_look,
        SoloLook::Off
    );
}

#[test]
fn activators_and_receiver_enable_are_independent_of_solo() {
    let mut p = project();
    p[TrackId(1)].mixer.active = false;
    p[TrackId(101)].mixer.active = false;
    p.outputs[0].mixer.active = false;
    solo(&mut p, 1, true, true);
    assert!(sends(&p).is_empty());
    p[TrackId(1)].mixer.active = true;
    assert_eq!(
        p.sending_outputs(TrackId(1))
            .map(|o| o.id.0 - 200)
            .collect::<Vec<_>>(),
        vec![2]
    );
    solo(&mut p, 1, false, true);
    assert_eq!(sends(&p), vec![1, 4]);
    assert!(!p[TrackId(101)].mixer.active && !p.outputs[0].mixer.active);
}

#[test]
fn exclusive_output_solo_keeps_only_explicit_routed_member_solos() {
    let mut p = project();
    p[TrackId(2)].mixer.solo = true;
    p[TrackId(4)].mixer.solo = true;
    p[TrackId(101)].mixer.solo = true;
    p.outputs[1].mixer.solo = true;
    solo(&mut p, 1, true, true);
    assert_eq!(sends(&p), vec![2]);
    assert!(p[TrackId(2)].mixer.solo);
    assert!(!p[TrackId(4)].mixer.solo && !p[TrackId(101)].mixer.solo && !p.outputs[1].mixer.solo);
    solo(&mut p, 1, false, true);
    assert_eq!(sends(&p), vec![2], "unsolo preserves member solos");
}

#[test]
fn additive_output_solos_union_routes_and_unsolo_removes_only_one() {
    let mut p = project();
    solo(&mut p, 1, true, true);
    solo(&mut p, 2, true, false);
    assert_eq!(sends(&p), vec![1, 2, 3, 4]);
    solo(&mut p, 1, false, true);
    assert_eq!(sends(&p), vec![1, 4]);
    solo(&mut p, 2, false, false);
    assert!(!p.any_solo());
    assert_eq!(sends(&p), vec![1, 2, 3, 4]);
}

#[test]
fn exclusive_track_and_group_solo_clear_receiver_solos() {
    for strip in [TrackId(4), TrackId(101)] {
        let mut p = project();
        solo(&mut p, 1, true, true);
        apply(
            &mut p,
            ProjectEdit::SetSolo {
                strips: vec![strip],
                solo: true,
                exclusive: true,
            },
        )
        .unwrap();
        assert!(p.outputs.iter().all(|o| !o.mixer.solo));
        assert_eq!(
            sends(&p),
            if strip == TrackId(4) {
                vec![4]
            } else {
                vec![2, 3]
            }
        );
    }
}

#[test]
fn route_edits_and_empty_soloed_outputs_apply_immediately() {
    let mut p = project();
    solo(&mut p, 3, true, true);
    assert!(sends(&p).is_empty());
    apply(
        &mut p,
        ProjectEdit::SetTrackRoutes {
            routes: vec![(TrackId(4), Route::single(TrackId(203)))],
        },
    )
    .unwrap();
    assert_eq!(sends(&p), vec![4]);
    apply(
        &mut p,
        ProjectEdit::RemoveTracks {
            tracks: vec![TrackId(203)],
        },
    )
    .unwrap();
    assert!(!p.any_solo());
}

#[test]
fn solo_round_trips_and_missing_solo_defaults_off() {
    let mut output = Track::new_output(TrackId(201), "O", "127.0.0.1", 9000);
    output.mixer.solo = true;
    let encoded = ron::to_string(&output).unwrap();
    assert_eq!(ron::from_str::<Track>(&encoded).unwrap(), output);
    let old = encoded.replace(",solo:true", "");
    assert!(!ron::from_str::<Track>(&old).unwrap().mixer.solo);
}

#[test]
fn invalid_output_solo_is_atomic() {
    let mut p = project();
    let before = p.clone();
    assert!(
        apply(
            &mut p,
            ProjectEdit::SetOutputSolo {
                output: TrackId(299),
                solo: true,
                exclusive: true
            }
        )
        .is_err()
    );
    assert_eq!(p, before);
}
