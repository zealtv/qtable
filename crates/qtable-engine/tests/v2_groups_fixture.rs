//! A project saved by the v2 (pre-merge) build, with groups. The expectations were recorded
//! against the build that wrote the file and must hold after the group/track merge.
use qtable_engine::{file, model::*};
use std::path::Path;

fn fixture() -> Project {
    file::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v2-groups.ron")).unwrap()
}

fn t(id: u64) -> TrackId {
    TrackId(id)
}

/// The fixture numbers outputs 1 and 2 like tracks 1 and 2; loading renumbers them above
/// every id, so tests find them by name.
fn output(p: &Project, name: &str) -> TrackId {
    p.outputs.iter().find(|o| o.name == name).unwrap().id
}

#[test]
fn routes_resolve_through_group_defaults() {
    let p = fixture();
    let (a, b) = (output(&p, "A"), output(&p, "B"));
    let route = |id| p.resolved_route(t(id)).to_vec();
    assert_eq!(route(1), [b]);
    assert_eq!(route(2), [a]);
    assert_eq!(route(3), [a]);
    assert_eq!([route(4), route(5), route(6)], [[b]; 3]);
}

#[test]
fn only_the_soloed_track_sends_and_an_inactive_group_silences_members() {
    let p = fixture();
    let sends: Vec<_> = (1..=6).map(|id| p.track_sends(t(id))).collect();
    assert_eq!(sends, [false, true, false, false, false, false]);
}

#[test]
fn effective_delay_adds_group_delay() {
    let p = fixture();
    let delay: Vec<_> = (1..=6).map(|id| p.effective_delay_ms(t(id))).collect();
    assert_eq!(delay, [0.0, 5.0, 5.0, 10.0, -10.0, -10.0]);
}

#[test]
fn groups_become_group_tracks_before_their_first_member_with_fresh_ids() {
    let p = fixture();
    let (g1, g2) = (TrackId(7), TrackId(8));
    let ids: Vec<_> = p.tracks.iter().map(|t| t.id.0).collect();
    assert_eq!(ids, [1, 7, 2, 3, 8, 4, 5, 6]);
    let strips = p.strips();
    assert_eq!(strips[1], g1);
    assert_eq!(strips[4], g2);
    let group = &p[g2];
    assert!(group.is_group() && group.fold_state && !group.mixer.active);
    assert_eq!((group.name.as_str(), group.delay_ms), ("Texture", -10.0));
    assert_eq!(group.route, Route::single(output(&p, "B")));
    assert_eq!(group.slots.len(), p.scenes.len());
    assert_eq!(p.members(g1).map(|t| t.id.0).collect::<Vec<_>>(), [2, 3]);
    let facts = p.track_facts(g2).unwrap();
    assert!(facts.fold_state && facts.capabilities.foldable && facts.kind == TrackKind::Group);
    assert!(!p.track_facts(TrackId(5)).unwrap().is_visible);
    assert_eq!(p.track_facts(TrackId(5)).unwrap().group_track, Some(g2));
}

#[test]
fn saving_writes_the_current_version_and_reloads_to_the_same_project() {
    let p = fixture();
    let path = std::env::temp_dir().join(format!("qtable-v2-groups-{}.ron", std::process::id()));
    file::save(&path, &p).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("version: 12") && !text.contains("groups:"));
    assert_eq!(file::load(&path).unwrap(), p);
    std::fs::remove_file(path).unwrap();
}
