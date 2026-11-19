use qtable_engine::edit::{self, ProjectEdit};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::*;
use qtable_test_support::TestReceiver;
use std::time::Duration;

#[path = "support/scheduler_project.rs"]
mod fixture;

#[test]
fn defaults_routes_and_output_edits_are_validated_atomically() {
    let mut p = Project::default();
    for name in ["Stage Lights", "Media"] {
        edit::apply(
            &mut p,
            ProjectEdit::AddOutput {
                name: name.into(),
                host: "localhost".into(),
                port: 9000,
            },
        )
        .unwrap();
    }
    assert!(LIVE_CLIP_PALETTE.contains(&p.outputs[0].colour));
    assert_ne!(p.outputs[0].colour, p.outputs[1].colour);
    let output = p.outputs[0].clone();
    edit::apply(
        &mut p,
        ProjectEdit::SetDestination {
            output: output.id,
            destination: output.output,
        },
    )
    .unwrap();
    edit::apply(
        &mut p,
        ProjectEdit::AddTrack {
            name: "Cues".into(),
        },
    )
    .unwrap();
    // Ids share one counter: outputs 1 and 2, then the track.
    let (o1, o2, t1) = (p.outputs[0].id, p.outputs[1].id, p.tracks[0].id);
    assert_eq!(p.tracks[0].route, Route::single(p.main.id));
    edit::apply(
        &mut p,
        ProjectEdit::ToggleOutputInRoute {
            tracks: vec![t1],
            output: o2,
        },
    )
    .unwrap();
    assert_eq!(p.resolved_route(t1), [p.main.id, o2]);
    let output = p.outputs[1].id;
    let active = false;
    edit::apply(
        &mut p,
        ProjectEdit::SetActive {
            strips: vec![output],
            active,
        },
    )
    .unwrap();
    edit::apply(&mut p, ProjectEdit::AddTrack { name: "New".into() }).unwrap();
    assert_eq!(p.tracks[1].route, Route::single(p.main.id));
    let before = p.clone();
    assert!(
        edit::apply(
            &mut p,
            ProjectEdit::SetTrackRoutes {
                routes: vec![(t1, Route::Outputs(vec![o1, o1]))]
            }
        )
        .is_err()
    );
    assert_eq!(p, before);
    edit::apply(
        &mut p,
        ProjectEdit::RenameTrack {
            track: o1,
            name: "Lighting".into(),
        },
    )
    .unwrap();
    edit::apply(&mut p, ProjectEdit::RemoveTracks { tracks: vec![o1] }).unwrap();
    assert_eq!(p.tracks[0].route, Route::Outputs(vec![p.main.id, o2]));
    edit::apply(&mut p, ProjectEdit::RemoveTracks { tracks: vec![o2] }).unwrap();
    assert_eq!(p.resolved_route(t1), [p.main.id]);
}

#[test]
fn group_inheritance_and_toggle_choose_only_the_destination() {
    let mut p = fixture::project(9000, &[0.0, 0.0]);
    p.outputs
        .push(Track::new_output(TrackId(202), "Other", "127.0.0.1", 9001));
    {
        let mut group = Track::new_group(TrackId(101), "Group", p.scenes.len());
        group.colour = qtable_engine::model::LIVE_CLIP_PALETTE[0];
        group.route = Route::single(TrackId(201));
        p.insert_group(group, &[TrackId(1), TrackId(2)]);
    }
    p[TrackId(1)].route = Route::Group;
    assert_eq!(p.resolved_route(TrackId(1)), [TrackId(201)]);
    edit::apply(
        &mut p,
        ProjectEdit::ToggleOutputInRoute {
            tracks: vec![TrackId(1)],
            output: TrackId(202),
        },
    )
    .unwrap();
    assert_eq!(p[TrackId(1)].route, Route::single(TrackId(202)));
    p[TrackId(101)].route = Route::Group;
    assert!(p.validate().is_err());
}

#[test]
fn fanout_delivers_typed_packets_to_both_receivers_and_skips_disabled_output() {
    let a = TestReceiver::bind().unwrap();
    let b = TestReceiver::bind().unwrap();
    let mut p = fixture::project(a.addr().port(), &[0.0]);
    p.global_quantisation = GlobalQuantisation::None;
    p.tracks[0].slots[0].clip.as_mut().unwrap().looping = false;
    let mut output = Track::new_output(TrackId(202), "Media", "127.0.0.1", b.addr().port());
    output = output.with_path("/media");
    output.delay_ms = 25.0;
    p.outputs.push(output);
    p.tracks[0].route = Route::Outputs(vec![TrackId(201), TrackId(202)]);
    let mut engine = Engine::new(p).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let messages_a = a.wait_for_messages(3, Duration::from_secs(2)).unwrap();
    let messages_b = b.wait_for_messages(3, Duration::from_secs(2)).unwrap();
    for ((_, a), (_, b)) in messages_a.iter().zip(&messages_b) {
        assert_eq!(b.addr, format!("/media{}", a.addr));
        assert_eq!(b.args, a.args);
    }
    let output = engine.project().outputs[1].id;
    let active = false;
    engine
        .execute(Command::Edit(ProjectEdit::SetActive {
            strips: vec![output],
            active,
        }))
        .unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    a.wait_for_messages(3, Duration::from_secs(2)).unwrap();
    assert!(b.wait_for_messages(1, Duration::from_millis(200)).is_err());
}

// Output latency is bounded on its own since 2026-10-11.
#[test]
fn combined_offset_bound_allows_cancellation_and_rejects_nonfinite_latency() {
    let mut p = fixture::project(9000, &[-1000.0]);
    p.outputs[0].delay_ms = 1000.0;
    assert!(p.validate().is_ok());
    p.outputs[0].delay_ms = 1000.1;
    assert!(p.validate().is_err());
    p.outputs[0].delay_ms = -1000.1;
    assert!(p.validate().is_err());
    p.outputs[0].delay_ms = f64::NAN;
    assert!(p.validate().is_err());
    p.tracks[0].delay_ms = 600.0;
    p.outputs[0].delay_ms = 500.0;
    assert!(p.validate().is_err());
}

#[test]
fn main_and_output_fanout_sends_to_both_and_unset_main_stays_silent() {
    let output = TestReceiver::bind().unwrap();
    let main = TestReceiver::bind().unwrap();
    let mut p = fixture::project(output.addr().port(), &[0.0]);
    p.main.destination_mut().unwrap().port = main.addr().port();
    p.global_quantisation = GlobalQuantisation::None;
    p.tracks[0].slots[0].clip.as_mut().unwrap().looping = false;
    p.tracks[0].route = Route::Outputs(vec![p.main.id, p.outputs[0].id]);
    let unset = {
        let mut p = p.clone();
        p.main.output = None;
        p
    };
    let mut engine = Engine::new(p).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    let a = output.wait_for_messages(3, Duration::from_secs(2)).unwrap();
    let b = main.wait_for_messages(3, Duration::from_secs(2)).unwrap();
    assert_eq!(
        a.iter().map(|m| &m.1).collect::<Vec<_>>(),
        b.iter().map(|m| &m.1).collect::<Vec<_>>()
    );
    drop(engine);
    let mut engine = Engine::new(unset).unwrap();
    engine
        .execute(Command::LaunchClip {
            track: TrackId(1),
            slot: 0,
        })
        .unwrap();
    output.wait_for_messages(3, Duration::from_secs(2)).unwrap();
    assert!(
        main.wait_for_messages(1, Duration::from_millis(150))
            .is_err()
    );
}
