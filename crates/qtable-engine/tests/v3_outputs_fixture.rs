//! A project saved by the v3 (pre-outputs-as-tracks) build (2026-10-13): four outputs (one broadcast with a prefix, one disabled, one soloed, two with
//! latency, one default for new tracks, two saved with the tag O5), a group routed to two of them,
//! a member on `Route::Group` and a track routed to three. The expectations were recorded
//! against the build that wrote the file and are stated by output *name*, so they hold after
//! outputs become tracks and take new ids.
use qtable_engine::{file, model::*, sender::resolve_address};
use std::path::Path;

fn fixture() -> Project {
    file::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v3-outputs.ron")).unwrap()
}

fn track_id(p: &Project, name: &str) -> TrackId {
    p.tracks.iter().find(|t| t.name == name).unwrap().id
}

fn names<'a>(outputs: impl Iterator<Item = &'a Track>) -> Vec<String> {
    outputs.map(|o| o.name.clone()).collect()
}

#[test]
fn routes_resolve_through_the_group_default() {
    let p = fixture();
    let route = |t: &str| names(p.routed_outputs(track_id(&p, t)));
    assert_eq!(route("T1"), ["Lead", "Lights"]);
    assert_eq!(route("T2"), ["Pads"]);
    assert_eq!(route("T3"), ["Lights", "Visuals"]); // Route::Group, inherited
    assert_eq!(route("Group"), ["Lights", "Visuals"]);
    assert_eq!(route("T4"), ["Lead", "Pads", "Visuals"]);
    assert!(route("T5").is_empty());
}

#[test]
fn a_soloed_output_plays_only_its_tracks_and_a_disabled_one_drops_out() {
    let p = fixture();
    let sends = |t: &str| p.track_sends(track_id(&p, t));
    let sending = |t: &str| names(p.sending_outputs(track_id(&p, t)));
    let all = ["T1", "T2", "Group", "T3", "T4", "T5"].map(sends);
    assert_eq!(all, [true, false, true, true, false, false]);
    assert_eq!(sending("T1"), ["Lead", "Lights"]);
    assert_eq!(sending("T3"), ["Lights", "Visuals"]);
    assert!(sending("T4").is_empty());
}

#[test]
fn track_delay_plus_output_latency_per_pair() {
    let p = fixture();
    let pairs = |t: &str| -> Vec<(String, f64)> {
        let id = track_id(&p, t);
        p.routed_outputs(id)
            .map(|o| (o.name.clone(), p.effective_delay_ms(id) + o.delay_ms))
            .collect()
    };
    let pair = |n: &str, d: f64| (n.to_string(), d);
    assert_eq!(pairs("T1"), [pair("Lead", 9.0), pair("Lights", 6.0)]);
    assert_eq!(pairs("T2"), [pair("Pads", 2.0)]);
    assert_eq!(pairs("T3"), [pair("Lights", 8.0), pair("Visuals", 6.0)]);
    assert_eq!(
        pairs("T4"),
        [pair("Lead", 10.0), pair("Pads", 7.0), pair("Visuals", 5.0)]
    );
}

#[test]
fn a_relative_address_takes_each_outputs_path() {
    let p = fixture();
    let play = OscAddress::Relative("/play".into());
    let address = |name: &str| {
        let o = p.outputs.iter().find(|o| o.name == name).unwrap();
        resolve_address(o.path.as_str(), &play)
    };
    assert_eq!(
        ["Lead", "Pads", "Lights", "Visuals"].map(address),
        ["/lead/play", "/pads/play", "/play", "/play"]
    );
    let pads = p.outputs.iter().find(|o| o.name == "Pads").unwrap();
    assert_eq!(
        (
            pads.destination().unwrap().mode,
            pads.mixer.active,
            pads.path.as_str()
        ),
        (UdpMode::Broadcast, false, "/pads")
    );
}

#[test]
fn strip_order_is_tracks_in_live_order_without_outputs() {
    let p = fixture();
    let ids: Vec<_> = p.strips();
    let tracks: Vec<_> = p.tracks.iter().map(|t| t.id).collect();
    assert_eq!(ids, tracks);
    let order: Vec<_> = p.tracks.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(order, ["T1", "T2", "Group", "T3", "T4", "T5"]);
}

#[test]
fn output_flags_survive_ignored_legacy_tags() {
    let p = fixture();
    let o = |n: &str| p.outputs.iter().find(|o| o.name == n).unwrap();
    assert_eq!(o("Lead").delay_ms, 3.0);
    assert!(o("Lights").mixer.solo && !o("Lead").mixer.solo);
    assert_eq!(o("Visuals").delay_ms, -2.0);
    // Duplicate old tags are ignored; receiver and strip settings survive migration.
}

#[test]
fn saving_reloads_to_the_same_project() {
    let p = fixture();
    let path = std::env::temp_dir().join(format!("qtable-v3-outputs-{}.ron", std::process::id()));
    file::save(&path, &p).unwrap();
    assert_eq!(file::load(&path).unwrap(), p);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn colliding_output_ids_are_renumbered_above_every_id() {
    // The file numbers outputs 1–4 and tracks 1–6 apart; loading gives the outputs 7–10
    // in file order, then Main takes 11 and a new output follows it.
    let mut p = fixture();
    let ids: Vec<_> = p
        .outputs
        .iter()
        .map(|o| (o.name.as_str(), o.id.0))
        .collect();
    assert_eq!(
        ids,
        [("Lead", 7), ("Pads", 8), ("Lights", 9), ("Visuals", 10)]
    );
    qtable_engine::edit::apply(
        &mut p,
        qtable_engine::edit::ProjectEdit::AddOutput {
            name: "New".into(),
            host: "127.0.0.1".into(),
            port: 9100,
        },
    )
    .unwrap();
    assert_eq!(p.main.id, TrackId(11));
    assert_eq!(p.outputs.last().unwrap().id, TrackId(12));
}

#[test]
fn outputs_become_output_tracks_that_save_without_empty_fields() {
    let p = fixture();
    let lead = p.outputs.iter().find(|o| o.name == "Lead").unwrap();
    assert!(lead.is_output() && lead.capabilities().destination);
    assert_eq!((lead.delay_ms, lead.mixer.active), (3.0, true));
    let destination = lead.destination().unwrap();
    assert_eq!(
        (
            destination.host.as_str(),
            destination.port,
            lead.path.as_str()
        ),
        ("127.0.0.1", 9000, "/lead")
    );
    let pads = p.outputs.iter().find(|o| o.name == "Pads").unwrap();
    assert!(!pads.mixer.active);
    assert!(
        p.outputs
            .iter()
            .find(|o| o.name == "Lights")
            .unwrap()
            .mixer
            .solo
    );
    assert!(
        p.tracks
            .iter()
            .all(|t| !t.is_output() && t.output.is_none())
    );
    let path = std::env::temp_dir().join(format!("qtable-v4-outputs-{}.ron", std::process::id()));
    file::save(&path, &p).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(text.contains("version: 12") && text.contains("kind: Output"));
    assert!(!text.contains("latency_ms") && !text.contains("short_tag"));
}
