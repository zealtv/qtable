use super::choices::FollowChoice;
use super::{parse_argument, parse_beats};
use qtable_engine::model::{BeatTime, OscArgument, SceneId};

#[test]
fn hit_float_text_keeps_native_precision_and_round_trips() {
    for (value, expected) in [
        (1.0_f32, "1.00"),
        (0.0, "0.00"),
        (-12.5, "-12.50"),
        (0.125, "0.125"),
        (0.123, "0.123"),
    ] {
        let argument = OscArgument::Float(value);
        let text = crate::control::argument_text(&argument);
        assert_eq!(text, expected);
        assert_eq!(parse_argument("float", &text).unwrap(), argument);
    }
    assert_eq!(crate::control::argument_text(&OscArgument::Int(1)), "1");
    assert_eq!(
        crate::control::argument_text(&OscArgument::String("1".into())),
        "1"
    );
}
#[test]
fn scene_follow_choice_mapping_round_trips_live_actions() {
    use qtable_engine::model::FollowAction::*;
    for action in [
        NoAction,
        Stop,
        PlayAgain,
        Previous,
        Next,
        First,
        Last,
        Any,
        Other,
        Jump(SceneId(42)),
    ] {
        assert_eq!(FollowChoice::from_action(action).action(), action);
    }
}
#[test]
fn parses_beats_and_typed_arguments() {
    assert_eq!(parse_beats("1.5"), Ok(BeatTime(1440)));
    assert!(parse_beats("-1").is_err());
    assert_eq!(parse_argument("int", "-3"), Ok(OscArgument::Int(-3)));
    assert_eq!(
        parse_argument("float", "1.25"),
        Ok(OscArgument::Float(1.25))
    );
    assert_eq!(
        parse_argument("string", "x"),
        Ok(OscArgument::String("x".into()))
    );
    assert_eq!(parse_argument("bool", "true"), Ok(OscArgument::Bool(true)));
}

#[test]
fn lane_labels_tell_same_path_events_apart() {
    let p = qtable_engine::file::load("../qtable-engine/tests/fixtures/v6-lanes.ron").unwrap();
    let track = qtable_engine::model::TrackId(1);
    let clip = p[track].slots[0].clip.as_ref().unwrap();
    let model = super::lane_model::LaneModel::build(&p, track, clip);
    assert_eq!(
        model
            .rows
            .iter()
            .map(|r| r.name.as_str())
            .collect::<Vec<_>>(),
        ["/kick (i)", "abs /kick", "/kick (f)", "/snare", "/empty"]
    );
}
