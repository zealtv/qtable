//! `Project::track_facts` (2026-10-12) and the LOM-name aliases.
use super::*;

const GROUP: TrackId = TrackId(101);

/// Tracks 1–3; tracks 2 and 3 form group 1; one output.
fn project() -> Project {
    let mut output = Track::new_output(TrackId(201), "Synth", "127.0.0.1", 9000);
    output.mixer.active = false;
    let mut p = Project {
        outputs: vec![output],
        tracks: (1..=3)
            .map(|i| Track::new(TrackId(i), format!("T{i}"), 0))
            .collect(),
        ..Project::default()
    };
    let mut group = Track::new_group(TrackId(101), "G", 0);
    group.colour = LIVE_CLIP_PALETTE[1];
    group.route = Route::Group;
    p.insert_group(group, &[TrackId(2), TrackId(3)]);
    p
}

fn facts(p: &Project, strip: TrackId) -> TrackFacts<'_> {
    p.track_facts(strip).unwrap()
}

#[test]
fn facts_cover_a_track_a_group_and_an_output() {
    let p = project();
    let track = facts(&p, TrackId(1));
    assert_eq!((track.kind, track.name), (TrackKind::Track, "T1"));
    assert_eq!(track.route, Some(&Route::empty()));
    assert!(!track.capabilities.foldable && !track.is_grouped && track.group_track.is_none());

    let group = facts(&p, GROUP);
    assert_eq!((group.kind, group.name), (TrackKind::Group, "G"));
    assert_eq!(group.colour, LIVE_CLIP_PALETTE[1]);
    assert!(group.capabilities.foldable && !group.is_grouped);

    let output = facts(&p, TrackId(201));
    assert_eq!((output.kind, output.name), (TrackKind::Output, "Synth"));
    assert_eq!((output.route, output.number), (None, None));
    // `enabled` reads as the Activator.
    assert!(!output.mixer.active && !output.mixer.solo);
    assert!(p.track_facts(TrackId(209)).is_none());
}

#[test]
fn group_solo_look_is_partial_with_only_a_member_soloed() {
    let mut p = project();
    assert_eq!(facts(&p, GROUP).solo_look, SoloLook::Off);
    p[TrackId(2)].mixer.solo = true;
    assert_eq!(facts(&p, GROUP).solo_look, SoloLook::Partial);
    assert_eq!(facts(&p, TrackId(2)).solo_look, SoloLook::On);
    p[TrackId(101)].mixer.solo = true;
    assert_eq!(facts(&p, GROUP).solo_look, SoloLook::On);
    p.outputs[0].mixer.solo = true;
    assert_eq!(facts(&p, TrackId(201)).solo_look, SoloLook::On);
}

#[test]
fn members_name_their_group_and_hide_while_it_is_folded() {
    let mut p = project();
    let member = TrackId(2);
    let facts_of = |p: &Project, s| {
        let f = facts(p, s);
        (f.is_grouped, f.group_track, f.is_visible, f.fold_state)
    };
    assert_eq!(
        facts_of(&p, member),
        (true, Some(TrackId(101)), true, false)
    );
    p[TrackId(101)].fold_state = true;
    assert_eq!(
        facts_of(&p, member),
        (true, Some(TrackId(101)), false, false)
    );
    assert_eq!(facts_of(&p, GROUP), (false, None, true, true));
    assert_eq!(facts_of(&p, TrackId(1)), (false, None, true, false));
}

#[test]
fn numbers_put_a_group_before_its_first_member() {
    let p = project();
    let numbers: Vec<_> = [TrackId(1), GROUP, TrackId(2), TrackId(3)]
        .map(|s| facts(&p, s).number)
        .to_vec();
    assert_eq!(numbers, [Some(1), Some(2), Some(3), Some(4)]);
}

#[test]
fn old_field_names_still_load_and_new_ones_are_written() {
    let slot: Slot = ron::from_str("(clip: None, stop_button: false)").unwrap();
    assert!(!slot.has_stop_button);
    let group: Track =
        ron::from_str("(id: (1), kind: Group, name: \"G\", fold_state: true, slots: [])").unwrap();
    assert!(group.fold_state);
    let written = ron::to_string(&group).unwrap();
    assert!(written.contains("fold_state") && !written.contains("folded"));
    let written = ron::to_string(&slot).unwrap();
    assert!(written.contains("has_stop_button"));
}
