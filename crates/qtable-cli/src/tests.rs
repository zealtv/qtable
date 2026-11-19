//! CLI parsing and selection regression tests.
use super::*;
use qtable_engine::model::*;

#[test]
fn parsed_line_keeps_its_receipt_through_resolution_and_execution() {
    use qtable_engine::{engine::Receipt, scheduler::SchedulerEvent};
    use std::time::{Duration, Instant};
    let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut track = Track::new(TrackId(1), "One", 1);
    track.route = Route::single(TrackId(1001));
    let mut clip = Clip::new(ClipId(1), "Cue", BeatTime(960), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.looping = false;
    clip.events.push(track.hit(
        BeatTime::ZERO,
        OscAddress::Absolute("/receipt/cli".into()),
        vec![],
        || DeviceId(1),
    ));
    track.slots[0].clip = Some(clip);
    let project = Project {
        tracks: vec![track],
        outputs: vec![Track::new_output(
            TrackId(1001),
            "Receiver",
            "127.0.0.1",
            receiver.local_addr().unwrap().port(),
        )],
        scenes: vec![Scene {
            id: SceneId(7),
            name: "Cue".into(),
            follow_actions: Default::default(),
        }],
        ..Project::default()
    };
    let mut engine = Engine::with_grid_lead(project.clone(), Duration::ZERO).unwrap();
    // IP literals resolve synchronously when the worker is created.
    assert!(engine.snapshot().unresolved_outputs.is_empty());
    // Same boundary as run(): receipt belongs to the complete line, before parse/resolve.
    let received_at = Instant::now();
    let context = engine.command_context(Receipt {
        received_at,
        sequence: None,
    });
    let ParsedCommand::Scene(selector) = parse_command("scene Cue").unwrap() else {
        unreachable!()
    };
    let scene = resolve_scene(&selector, &project.scenes).unwrap();
    execute(&mut engine, context, Command::LaunchScene { scene }).unwrap();
    loop {
        let event = engine
            .scheduler_events()
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        assert!(!matches!(event, SchedulerEvent::Error(_)), "{event:?}");
        if let SchedulerEvent::LaunchAdmission {
            received,
            sequence,
            submission,
            ..
        } = event
        {
            assert_eq!(received, received_at);
            assert_eq!(sequence, None);
            assert_eq!(submission, Some(1));
            break;
        }
    }
    let mut packet = [0; 1024];
    let length = receiver.recv(&mut packet).unwrap();
    assert!(packet[..length].starts_with(b"/receipt/cli\0"));
}

#[test]
fn follow_switch_parses() {
    assert_eq!(parse_command("follow on"), Ok(ParsedCommand::Follow(true)));
    assert_eq!(
        parse_command("follow off"),
        Ok(ParsedCommand::Follow(false))
    );
    assert!(parse_command("follow").is_err());
    assert!(parse_command("follow yes").is_err());
}

#[test]
fn group_commands_parse_and_resolve() {
    assert_eq!(
        parse_command("sg 2"),
        Ok(ParsedCommand::StopGroup(Selector::Index(2)))
    );
    assert_eq!(
        parse_command("STOPGROUP Big Group"),
        Ok(ParsedCommand::StopGroup(Selector::Name("Big Group".into())))
    );
    assert!(parse_command("sg").is_err());
    assert!(parse_command("sg 0").is_err());
    let group = Track::new_group(TrackId(1077), "Big Group", 0);
    let groups = vec![&group];
    assert_eq!(
        resolve_group(&Selector::Index(1), &groups),
        Ok(TrackId(1077))
    );
    assert_eq!(
        resolve_group(&Selector::Name("bIG gROUP".into()), &groups),
        Ok(TrackId(1077))
    );
    assert!(resolve_group(&Selector::Index(2), &groups).is_err());
    assert!(resolve_group(&Selector::Name("missing".into()), &groups).is_err());
    assert_eq!(
        parse_command("launch 1 2"),
        Ok(ParsedCommand::Launch(Selector::Index(1), 2))
    );
}

#[test]
fn scene_names_and_indices_parse_and_resolve_in_project_order() {
    assert_eq!(
        parse_command("SCENE Big Chorus"),
        Ok(ParsedCommand::Scene(Selector::Name("Big Chorus".into())))
    );
    assert_eq!(
        parse_command("scene 2"),
        Ok(ParsedCommand::Scene(Selector::Index(2)))
    );
    assert!(parse_command("scene").is_err());
    assert!(parse_command("scene 0").is_err());
    let scenes = vec![
        Scene {
            id: SceneId(90),
            name: "Big Chorus".into(),
            follow_actions: Default::default(),
        },
        Scene {
            id: SceneId(12),
            name: "Outro".into(),
            follow_actions: Default::default(),
        },
    ];
    assert_eq!(resolve_scene(&Selector::Index(2), &scenes), Ok(SceneId(12)));
    assert_eq!(
        resolve_scene(&Selector::Name("bIG cHORUS".into()), &scenes),
        Ok(SceneId(90))
    );
    assert!(resolve_scene(&Selector::Index(3), &scenes).is_err());
    assert!(resolve_scene(&Selector::Name("missing".into()), &scenes).is_err());
}

#[test]
fn quantisation_menu_parses_and_round_trips() {
    for value in [
        "none", "1bar", "2bar", "4bar", "8bar", "1/2", "1/2T", "1/4", "1/4T", "1/8", "1/8T",
        "1/16", "1/16T", "1/32",
    ] {
        let grid = parse_quantisation(value).unwrap();
        assert_eq!(
            parse_command(&format!("quant {value}")),
            Ok(ParsedCommand::Quantisation(grid))
        );
        assert_eq!(quantisation_label(grid), value);
    }
    for value in [
        "0bar", "3bar", "1/1", "1/32T", "1/64", "1/64T", "global", "garbage",
    ] {
        assert!(parse_command(&format!("quant {value}")).is_err(), "{value}");
    }
    assert!(parse_command("quant").is_err());
    assert!(parse_command("quant none extra").is_err());
}

#[test]
fn parses_commands_and_track_selectors() {
    assert_eq!(
        parse_command("l Music 2"),
        Ok(ParsedCommand::Launch(Selector::Name("Music".into()), 2))
    );
    assert_eq!(
        parse_command("launch 3 1"),
        Ok(ParsedCommand::Launch(Selector::Index(3), 1))
    );
    assert_eq!(
        parse_command("s Lighting cue"),
        Ok(ParsedCommand::StopTrack(Selector::Name(
            "Lighting cue".into()
        )))
    );
    assert_eq!(parse_command("sa"), Ok(ParsedCommand::StopAll));
    assert_eq!(
        parse_command("stop-transport"),
        Ok(ParsedCommand::StopTransport)
    );
    assert_eq!(parse_command("tempo 98.5"), Ok(ParsedCommand::Tempo(98.5)));
    assert_eq!(parse_command("status"), Ok(ParsedCommand::Status));
    assert_eq!(parse_command("\n"), Ok(ParsedCommand::Status));
}

#[test]
fn rejects_bad_command_input_and_resolves_case_insensitively() {
    assert!(parse_command("launch Music 0").is_err());
    assert!(parse_command("launch Music nope").is_err());
    assert!(parse_command("launch").is_err());
    assert!(parse_command("tempo fast").is_err());
    assert!(parse_command("stop").is_err());
    assert!(parse_command("nonsense").is_err());
    assert_eq!(
        resolve_track(
            &Selector::Name("mUsIc".into()),
            &[&Track::new(TrackId(4), "Music", 1)]
        ),
        Ok(TrackId(4))
    );
    assert!(resolve_track(&Selector::Index(2), &[&Track::new(TrackId(4), "Music", 1)]).is_err());
}

#[test]
fn transport_words_and_their_aliases_parse() {
    for word in ["start-transport", "START-TRANSPORT", "play"] {
        assert_eq!(parse_command(word), Ok(ParsedCommand::StartTransport));
    }
    for word in ["stop-transport", "halt"] {
        assert_eq!(parse_command(word), Ok(ParsedCommand::StopTransport));
    }
    for word in ["stop-all", "stopall", "sa"] {
        assert_eq!(parse_command(word), Ok(ParsedCommand::StopAll));
    }
    assert!(parse_command("start-transport now").is_err());
    // A bare `stop` stays a usage error rather than stopping the transport.
    assert!(parse_command("stop").is_err());
    assert!(parse_command("s").is_err());
}

#[test]
fn canonical_and_short_settings_words_agree() {
    assert_eq!(
        parse_command("quantisation 1/8T"),
        parse_command("quant 1/8T")
    );
    assert!(parse_command("quantisation 1/8T").is_ok());
    assert!(parse_command("quantisation").is_err());
    assert_eq!(
        parse_command("follow-actions off"),
        Ok(ParsedCommand::Follow(false))
    );
    assert_eq!(
        parse_command("follow-actions on"),
        parse_command("follow on")
    );
    assert!(parse_command("follow-actions maybe").is_err());
}

#[test]
fn selector_rules_stand_as_documented() {
    // A name made of digits reads as a position.
    assert_eq!(
        parse_command("stop 2"),
        Ok(ParsedCommand::StopTrack(Selector::Index(2)))
    );
    // The last word of `launch` is the slot, so a multi-word name keeps the rest.
    assert_eq!(
        parse_command("launch Lighting cue 3"),
        Ok(ParsedCommand::Launch(
            Selector::Name("Lighting cue".into()),
            3
        ))
    );
    // Names match case-insensitively and the first duplicate wins.
    let first = Track::new(TrackId(1), "Dup", 1);
    let second = Track::new(TrackId(2), "dup", 1);
    assert_eq!(
        resolve_track(&Selector::Name("DUP".into()), &[&first, &second]),
        Ok(TrackId(1))
    );
}

#[test]
fn command_line_forms() {
    let args = |words: &[&str]| parse_args(words.iter().map(|word| word.to_string()));
    assert_eq!(args(&["--version"]), Invocation::Version);
    assert_eq!(args(&["-V"]), Invocation::Version);
    assert_eq!(args(&["--help"]), Invocation::Help);
    assert_eq!(args(&["-h"]), Invocation::Help);
    assert_eq!(args(&["play", "--help"]), Invocation::Help);
    assert_eq!(
        args(&["play", "set.ron"]),
        Invocation::Play("set.ron".into())
    );
    assert_eq!(args(&[]), Invocation::Usage);
    assert_eq!(args(&["play"]), Invocation::Usage);
    assert_eq!(args(&["play", "-x"]), Invocation::Usage);
    assert_eq!(args(&["play", "a.ron", "b.ron"]), Invocation::Usage);
    assert_eq!(args(&["--version", "extra"]), Invocation::Usage);
    assert_eq!(args(&["stop"]), Invocation::Usage);
}
