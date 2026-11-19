//! Flat (format 10) and forest (format 11) group rules, side by side.
use super::validate::{CURRENT_GROUP_SHAPE, GroupShape, validate_group_shape};
use super::*;

fn list(rows: &[(u64, bool, Option<u64>)]) -> Vec<Track> {
    rows.iter()
        .map(|&(id, group, parent)| {
            let name = format!("T{id}");
            let mut track = if group {
                Track::new_group(TrackId(id), name, 0)
            } else {
                Track::new(TrackId(id), name, 0)
            };
            track.group = parent.map(TrackId);
            track
        })
        .collect()
}

/// `Outer(Inner(a, b), c), d`.
fn nested() -> Vec<Track> {
    list(&[
        (10, true, None),
        (11, true, Some(10)),
        (1, false, Some(11)),
        (2, false, Some(11)),
        (3, false, Some(10)),
        (4, false, None),
    ])
}

/// `G(a, b), c`, the only shape formats 10 and earlier hold.
fn flat() -> Vec<Track> {
    list(&[
        (10, true, None),
        (1, false, Some(10)),
        (2, false, Some(10)),
        (3, false, None),
    ])
}

fn check(tracks: &[Track], shape: GroupShape) -> Result<(), String> {
    validate_group_shape(tracks, shape).map_err(|error| error.0)
}

#[test]
fn the_current_format_accepts_a_forest() {
    assert_eq!(CURRENT_GROUP_SHAPE, GroupShape::Forest);
    let mut project = Project {
        tracks: nested(),
        ..Project::default()
    };
    project.validate().unwrap();
    project.validate_stored().unwrap();
    project.tracks = flat();
    project.validate().unwrap();
}

#[test]
fn both_shapes_accept_flat_sets_and_only_the_forest_accepts_nesting() {
    for shape in [GroupShape::Flat, GroupShape::Forest] {
        check(&flat(), shape).unwrap();
        check(&[], shape).unwrap();
    }
    check(&nested(), GroupShape::Forest).unwrap();
    assert!(check(&nested(), GroupShape::Flat).is_err());
}

#[test]
fn the_forest_rejects_invalid_ancestry() {
    let reopened = list(&[
        (10, true, None),
        (11, true, Some(10)),
        (1, false, Some(11)),
        (3, false, Some(10)),
        (2, false, Some(11)),
        (4, false, None),
    ]);
    let cycle = list(&[
        (31, true, Some(33)),
        (32, true, Some(31)),
        (33, true, Some(32)),
        (1, false, Some(33)),
    ]);
    let forward = list(&[(1, false, Some(10)), (10, true, None)]);
    let missing = list(&[(10, true, None), (1, false, Some(99))]);
    let plain_parent = list(&[(10, true, None), (1, false, Some(10)), (2, false, Some(1))]);
    let mut duplicate = nested();
    duplicate[5].id = TrackId(1);
    for (tracks, message) in [
        (
            reopened,
            "a group's subtree must not continue after it closed",
        ),
        (cycle, "groups cannot form a cycle"),
        (forward, "a group must precede its members"),
        (missing, "group references missing track"),
        (plain_parent, "a track's group must be a group track"),
        (duplicate, "duplicate track ID"),
    ] {
        assert_eq!(check(&tracks, GroupShape::Forest).unwrap_err(), message);
    }
}

#[test]
fn the_forest_keeps_the_group_content_rules() {
    // An empty group at any depth, including one left behind in a parent.
    let mut tracks = nested();
    tracks.remove(2);
    tracks.remove(2);
    assert_eq!(
        check(&tracks, GroupShape::Forest).unwrap_err(),
        "track group must not be empty"
    );
    // A group holds no clips.
    let mut tracks = nested();
    tracks[1].slots = vec![Slot {
        clip: Some(
            Clip::new(
                ClipId(1),
                "c",
                BeatTime::from_beats(1).unwrap(),
                LIVE_CLIP_PALETTE[0],
            )
            .unwrap(),
        ),
        ..Slot::default()
    }];
    assert_eq!(
        check(&tracks, GroupShape::Forest).unwrap_err(),
        "a group track cannot hold clips"
    );
    // Only a group folds.
    let mut tracks = nested();
    tracks[2].fold_state = true;
    assert_eq!(
        check(&tracks, GroupShape::Forest).unwrap_err(),
        "only a group track can be folded"
    );
    // Folds on any group level are fine, each independent.
    let mut tracks = nested();
    tracks[0].fold_state = true;
    tracks[1].fold_state = true;
    check(&tracks, GroupShape::Forest).unwrap();
}

#[test]
fn the_forest_lets_only_a_nested_group_follow_its_parent_route() {
    let mut tracks = nested();
    tracks[1].route = Route::Group;
    check(&tracks, GroupShape::Forest).unwrap();
    tracks[0].route = Route::Group;
    assert_eq!(
        check(&tracks, GroupShape::Forest).unwrap_err(),
        "group route cannot inherit a group"
    );
    // The flat rule rejects both, at either level.
    let mut tracks = flat();
    tracks[0].route = Route::Group;
    assert_eq!(
        check(&tracks, GroupShape::Flat).unwrap_err(),
        "group route cannot inherit a group"
    );
}

#[test]
fn the_flat_rules_keep_their_messages() {
    // A grouped group.
    let tracks = list(&[(10, true, None), (11, true, Some(10)), (1, false, Some(11))]);
    assert_eq!(
        check(&tracks, GroupShape::Flat).unwrap_err(),
        "groups cannot be nested yet: a group track cannot have a group"
    );
    // A member separated from its group.
    let apart = list(&[
        (10, true, None),
        (1, false, Some(10)),
        (3, false, None),
        (2, false, Some(10)),
    ]);
    assert_eq!(
        check(&apart, GroupShape::Flat).unwrap_err(),
        "group tracks must be adjacent and in project order"
    );
    let mut tracks = flat();
    tracks.remove(2);
    tracks.remove(1);
    assert_eq!(
        check(&tracks, GroupShape::Flat).unwrap_err(),
        "track group must not be empty"
    );
}
