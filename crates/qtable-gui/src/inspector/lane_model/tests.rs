use super::*;
use qtable_engine::model::{
    AutomationEnvelope, ClipId, DeviceId, EntryKey, EnumOption, FieldDef, FieldKey, FieldType,
    Lane, OscAddress, OscArgument, ParameterDef, ParameterKey, ParameterRef, SceneId, SendMode,
    Track,
};
use qtable_engine::scheduler::{AutomationState, AutomationValue};

fn argument(device: DeviceRef, parameter: &str, field: &str) -> ArgumentRef {
    ArgumentRef {
        device,
        parameter: ParameterKey(parameter.into()),
        field: FieldKey(field.into()),
    }
}

fn parameter(key: &str, name: &str, fields: Vec<FieldDef>) -> ParameterDef {
    ParameterDef {
        key: EntryKey(key.into()),
        name: name.into(),
        path: OscAddress::Relative(format!("/{key}")),
        fields,
        send_mode: SendMode::Continuous,
    }
}

fn ramp(target: ArgumentRef, values: &[(u64, f64, f32)]) -> Lane {
    Lane::Envelope(AutomationEnvelope {
        region: None,
        target,
        points: values
            .iter()
            .map(|&(time, value, curve)| Breakpoint {
                time: BeatTime(time),
                value,
                curve,
            })
            .collect(),
    })
}

/// The clip the lane-overview captures show: an event lane, then envelopes over Target
/// arguments of Float, Int, Toggle and Enum kinds and Mixer Volume, plus a second argument
/// of a two-argument parameter with no points yet.
pub(crate) fn mixed_project() -> Project {
    let mut p = Project::default();
    p.scenes.push(qtable_engine::model::Scene {
        id: SceneId(1),
        name: "Scene 1".into(),
        follow_actions: Default::default(),
    });
    p.tracks.push(Track::new(TrackId(1), "Visuals", 1));
    let kick = p
        .hit(
            TrackId(1),
            BeatTime(0),
            OscAddress::Relative("/kick".into()),
            vec![],
        )
        .unwrap();
    let hits = [960, 1920, 2880].map(|t| qtable_engine::model::Event {
        position: BeatTime(t),
        ..kick.clone()
    });
    let track = &mut p.tracks[0];
    track.devices[0].name = "BopOS".into();
    let device_id = track.devices[0].id;
    let device = track.devices[0].target_mut().unwrap();
    let float = {
        let mut f = FieldDef::new(FieldKey("value".into()), FieldType::Float, true);
        f.range = Some([0.0, 1.0]);
        f
    };
    let int = {
        let mut f = FieldDef::new(FieldKey("value".into()), FieldType::Int, true);
        f.range = Some([0.0, 8.0]);
        f
    };
    let toggle = FieldDef::new(FieldKey("value".into()), FieldType::Bool, true);
    let mode = {
        let mut f = FieldDef::new(FieldKey("value".into()), FieldType::String, true);
        f.options = ["Off", "Strobe", "Chase", "Wash"]
            .into_iter()
            .map(|label| EnumOption {
                label: label.into(),
                value: OscArgument::String(label.into()),
            })
            .collect();
        f.default = f.options[0].value.clone();
        f
    };
    let xy = ["x", "y"].map(|k| {
        let mut f = FieldDef::new(FieldKey(k.into()), FieldType::Float, true);
        f.range = Some([0.0, 100.0]);
        f.name = k.into();
        f
    });
    device.parameters = vec![
        parameter("opacity", "Opacity", vec![float]),
        parameter("steps", "Steps", vec![int]),
        parameter("strobe", "Strobe", vec![toggle]),
        parameter("mode", "Mode", vec![mode]),
        parameter("position", "Position", xy.to_vec()),
    ];
    let device_ref = DeviceRef::Chain(device_id);
    let mut clip = Clip::new(
        ClipId(1),
        "Mixed",
        BeatTime(3840),
        qtable_engine::model::LIVE_CLIP_PALETTE[0],
    )
    .unwrap();
    clip.events = hits.to_vec();
    clip.lanes = vec![
        Lane::Events(qtable_engine::model::EventLane {
            definition: kick.definition,
        }),
        ramp(
            argument(device_ref, "opacity", "value"),
            &[
                (0, 0.2, 0.0),
                (1200, 0.9, 0.5),
                (2400, 0.3, 0.0),
                (3840, 0.6, 0.0),
            ],
        ),
        ramp(
            argument(device_ref, "steps", "value"),
            &[(0, 1.0, 0.0), (1920, 6.0, 0.0), (3840, 2.0, 0.0)],
        ),
        ramp(
            argument(device_ref, "strobe", "value"),
            &[(0, 0.0, 0.0), (960, 1.0, 0.0), (2880, 0.0, 0.0)],
        ),
        ramp(
            argument(device_ref, "mode", "value"),
            &[
                (0, 0.0, 0.0),
                (1920, 2.0, 0.0),
                (1920, 3.0, 0.0),
                (3840, 1.0, 0.0),
            ],
        ),
        ramp(argument(device_ref, "position", "y"), &[]),
        ramp(
            argument(DeviceRef::Mixer, "volume", "value"),
            &[(0, 0.7, 0.0), (1920, 0.7, 0.0), (3840, 0.9, 0.0)],
        ),
    ];
    p.tracks[0].slots[0].clip = Some(clip);
    p.validate().unwrap();
    p
}

fn model() -> (Project, LaneModel) {
    let p = mixed_project();
    let clip = p.clip(TrackId(1), 0).unwrap();
    let model = LaneModel::build(&p, TrackId(1), clip);
    (p, model)
}

#[test]
fn rows_follow_lane_order_with_event_rows_at_24_and_envelopes_at_48() {
    let (_, model) = model();
    let heights: Vec<f32> = model.rows.iter().map(|r| r.height).collect();
    assert_eq!(heights, [24.0, 48.0, 48.0, 48.0, 48.0, 48.0, 48.0]);
    let tops: Vec<f32> = model.rows.iter().map(|r| r.top).collect();
    assert_eq!(tops, [0.0, 24.0, 72.0, 120.0, 168.0, 216.0, 264.0]);
    assert_eq!(model.height(), 312.0);
    assert!(matches!(
        model.rows[0].kind,
        RowKind::Event { index: Some(0), .. }
    ));
    assert!(matches!(model.rows[6].kind, RowKind::Envelope(_)));
}

#[test]
fn row_at_finds_the_row_under_a_y_and_nothing_outside() {
    let (_, model) = model();
    assert_eq!(model.row_at(-1.0), None);
    assert_eq!(model.row_at(0.0), Some(0));
    assert_eq!(model.row_at(23.9), Some(0));
    assert_eq!(model.row_at(24.0), Some(1));
    assert_eq!(model.row_at(311.9), Some(6));
    assert_eq!(model.row_at(312.0), None);
}

#[test]
fn device_events_precede_envelopes_independently_of_stored_lanes() {
    let (mut p, _) = model();
    let extra = p
        .hit(
            TrackId(1),
            BeatTime(0),
            OscAddress::Relative("/snare".into()),
            vec![],
        )
        .unwrap();
    p.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .events
        .push(extra.clone());
    let clip = p.clip(TrackId(1), 0).unwrap();
    let model = LaneModel::build(&p, TrackId(1), clip);
    assert_eq!(model.rows.len(), 8);
    let RowKind::Event { index, reference } = &model.rows[1].kind else {
        panic!("second device event");
    };
    assert_eq!((*index, reference), (Some(1), &extra.definition));
    assert_eq!(model.event_row(&extra.definition), Some(1));
    let events = model.events();
    assert_eq!(events.rows.len(), 2);
    assert_eq!(events.rows[1].top, EVENT_LANE_HEIGHT);
    assert_eq!(events.rows[1].kind, model.rows[1].kind);
    assert_eq!(events.event_row(&extra.definition), Some(1));
    assert_eq!(events.row_at(EVENT_LANE_HEIGHT + 12.0), Some(1));
}

#[test]
fn event_projection_keeps_declared_empty_lanes_and_can_be_empty() {
    let mut project = mixed_project();
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    clip.events.clear();
    let clip = project.clip(TrackId(1), 0).unwrap();
    assert_eq!(
        LaneModel::build(&project, TrackId(1), clip)
            .events()
            .rows
            .len(),
        1
    );
    let mut clip = clip.clone();
    clip.lanes.retain(|l| l.envelope().is_some());
    assert_eq!(
        LaneModel::build(&project, TrackId(1), &clip)
            .events()
            .rows
            .len(),
        1
    );
    project.tracks[0].devices.clear();
    assert!(
        LaneModel::build(&project, TrackId(1), &clip)
            .events()
            .rows
            .is_empty()
    );
}

#[test]
fn headers_name_the_device_and_the_argument() {
    let (_, model) = model();
    let names: Vec<(&str, &str)> = model
        .rows
        .iter()
        .map(|r| (r.device.as_str(), r.name.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("BopOS", "/kick"),
            ("BopOS", "Opacity"),
            ("BopOS", "Steps"),
            ("BopOS", "Strobe"),
            ("BopOS", "Mode"),
            ("BopOS", "Position · y"),
            ("Mixer", "Volume"),
        ]
    );
}

fn envelope(model: &LaneModel, row: usize) -> &EnvelopeRow {
    let RowKind::Envelope(e) = &model.rows[row].kind else {
        panic!("envelope row");
    };
    e
}

#[test]
fn live_values_show_in_the_arguments_control_form() {
    let (_, model) = model();
    assert_eq!(envelope(&model, 1).value_text(0.62), "0.62");
    assert_eq!(envelope(&model, 2).value_text(3.4), "3");
    assert_eq!(envelope(&model, 3).value_text(0.0), "Off");
    assert_eq!(envelope(&model, 3).value_text(1.0), "On");
    assert_eq!(envelope(&model, 4).value_text(1.0), "Strobe");
    assert_eq!(envelope(&model, 4).value_text(3.0), "Wash");
    assert_eq!(envelope(&model, 6).value_text(0.71), "0.71");
}

#[test]
fn axes_come_from_the_argument_definitions() {
    let (_, model) = model();
    let axes: Vec<Axis> = (1..7).map(|r| envelope(&model, r).axis).collect();
    assert_eq!(
        axes,
        [
            Axis::Continuous,
            Axis::Stepped,
            Axis::Discrete,
            Axis::Discrete,
            Axis::Continuous,
            Axis::Continuous
        ]
    );
    assert_eq!(
        (envelope(&model, 4).min, envelope(&model, 4).max),
        (0.0, 3.0)
    );
}

#[test]
fn live_follows_the_snapshot_and_falls_back_to_the_stored_value() {
    let (p, model) = model();
    let volume = envelope(&model, 6).field.clone();
    let opacity = envelope(&model, 1).field.clone();
    let automation = [
        AutomationValue {
            field: volume.clone(),
            value: 0.71,
            state: AutomationState::Overridden,
        },
        AutomationValue {
            field: opacity.clone(),
            value: 0.62,
            state: AutomationState::Following,
        },
    ];
    let live = model.live(&p, &automation);
    assert_eq!(live[0], None);
    assert_eq!(
        live[1],
        Some(Effective {
            value: 0.62,
            state: crate::parameter_gesture::AutomationState::Following
        })
    );
    assert_eq!(
        live[6],
        Some(Effective {
            value: 0.71,
            state: crate::parameter_gesture::AutomationState::Overridden
        })
    );
    // Nothing drives the Steps argument: the stored value shows, not overridden.
    let stored = p.field(&envelope(&model, 2).field).unwrap().value;
    assert_eq!(
        live[2],
        Some(Effective {
            value: stored,
            state: crate::parameter_gesture::AutomationState::None
        })
    );
    let _ = ParameterRef::volume(TrackId(1));
}

/// Sample values by tick, for comparing a drawn curve with the engine's evaluation.
fn at(samples: &[(f64, f64)], tick: f64) -> Vec<f64> {
    samples
        .iter()
        .filter(|(t, _)| (*t - tick).abs() < 0.0001)
        .map(|(_, v)| *v)
        .collect()
}

#[test]
fn samples_are_the_engines_evaluation() {
    let (_, model) = model();
    let opacity = envelope(&model, 1);
    let samples = opacity.samples(BeatTime(3840), 16.0);
    let engine = AutomationEnvelope {
        region: None,
        target: opacity.target.clone(),
        points: opacity.points.clone(),
    };
    for (tick, value) in &samples {
        assert_eq!(engine.value_at(*tick, Axis::Continuous), Some(*value));
    }
    // Samples cover both ends and every breakpoint.
    assert_eq!(samples.first().unwrap().0, 0.0);
    assert_eq!(samples.last().unwrap().0, 3840.0);
    for p in &opacity.points {
        assert!(!at(&samples, p.time.ticks() as f64).is_empty());
    }
}

#[test]
fn a_stepped_argument_draws_unrounded_and_a_discrete_one_holds() {
    let (_, model) = model();
    // Steps rises 1 → 6 over 1920 ticks: halfway it reads 3.5, not a rounded 4.
    let steps = envelope(&model, 2).samples(BeatTime(3840), 480.0);
    assert_eq!(at(&steps, 960.0), [3.5]);
    // Strobe holds its left value until the next point.
    let strobe = envelope(&model, 3).samples(BeatTime(3840), 480.0);
    assert_eq!(at(&strobe, 480.0), [0.0]);
    assert_eq!(at(&strobe, 1440.0), [1.0]);
}

#[test]
fn two_points_at_one_time_draw_a_vertical_jump() {
    let (_, model) = model();
    let mode = envelope(&model, 4).samples(BeatTime(3840), 960.0);
    // Just before 1920 the left value holds; at 1920 the second point's value applies.
    assert_eq!(at(&mode, 1920.0 - 0.001), [0.0]);
    assert_eq!(at(&mode, 1920.0), [3.0]);
}

#[test]
fn an_empty_envelope_has_no_samples_and_fractions_clamp_to_the_range() {
    let (_, model) = model();
    let empty = envelope(&model, 5);
    assert!(empty.samples(BeatTime(3840), 16.0).is_empty());
    // The lane maps value to y: the ceiling at the plot's top, the floor at its bottom.
    let plot = Plot {
        left: 0.0,
        width: 100.0,
        top: 0.0,
        height: 1.0,
    };
    let y = |row: &EnvelopeRow, value| row.lane(plot, BeatTime(3840), 1).y(value);
    assert_eq!(y(empty, 50.0), 0.5);
    assert_eq!(y(empty, -5.0), 1.0);
    assert_eq!(y(empty, 500.0), 0.0);
    let enum_row = envelope(&model, 4);
    assert!((y(enum_row, 1.0) - 2.0 / 3.0).abs() < 1e-6);
}

#[test]
fn a_clip_that_no_longer_resolves_its_device_still_builds_a_row() {
    let (mut p, _) = model();
    p.tracks[0].devices.clear();
    let clip = p.clip(TrackId(1), 0).unwrap();
    let model = LaneModel::build(&p, TrackId(1), clip);
    let row = &model.rows[0];
    assert_eq!((row.device.as_str(), row.name.as_str()), ("", ""));
    assert_eq!(row.height, 48.0);
    let _ = DeviceId(1);
}

/// The fixture the capture opens is this project; regenerate it with
/// `QTABLE_REGENERATE_FIXTURES=1 cargo test -p qtable-gui lane_fixture`.
#[test]
fn lane_fixture_matches_the_builder() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lanes.ron");
    let project = mixed_project();
    if std::env::var_os("QTABLE_REGENERATE_FIXTURES").is_some() {
        qtable_engine::file::save(&path, &project).unwrap();
    }
    assert_eq!(qtable_engine::file::load(&path).unwrap(), project);
}

#[test]
fn an_argument_without_a_lane_has_a_row_with_no_points_and_its_own_axis_and_range() {
    let (p, _) = model();
    let x = EnvelopeRow::of_argument(
        &p,
        TrackId(1),
        &argument(DeviceRef::Chain(DeviceId(1)), "position", "x"),
    );
    assert!(x.points.is_empty());
    assert_eq!((x.axis, x.min, x.max), (Axis::Continuous, 0.0, 100.0));
    assert_eq!(x.field.parameter.key.0, "position");
}

#[test]
fn stepped_samples_round_where_the_line_does_not() {
    let (_, model) = model();
    let RowKind::Envelope(steps) = &model.rows[2].kind else {
        panic!("Steps is an envelope");
    };
    assert_eq!(steps.axis, Axis::Stepped);
    let line = steps.samples(BeatTime(3840), 240.0);
    let staircase = steps.samples_as(BeatTime(3840), 240.0, Axis::Stepped);
    assert!(line.iter().any(|(_, v)| v.fract() != 0.0));
    assert!(staircase.iter().all(|(_, v)| v.fract() == 0.0));
    assert_eq!(line.len(), staircase.len());
}

#[test]
fn live_value_prefers_the_engine_and_falls_back_to_the_stored_value_following() {
    let (p, model) = model();
    let RowKind::Envelope(volume) = &model.rows[6].kind else {
        panic!("Volume is last");
    };
    let engine = [AutomationValue {
        field: volume.field.clone(),
        value: 0.33,
        state: AutomationState::Overridden,
    }];
    let from_engine = live_value(&p, &engine, &volume.field).unwrap();
    assert_eq!((from_engine.value, from_engine.overridden()), (0.33, true));
    let stored = live_value(&p, &[], &volume.field).unwrap();
    assert!(!stored.overridden());
}

#[test]
fn dense_lane_views_share_one_build_until_clip_or_revision_changes() {
    use super::super::event_editor::ClipKey;
    use std::rc::Rc;
    let mut project = mixed_project();
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    let Lane::Envelope(envelope) = &mut clip.lanes[1] else {
        panic!()
    };
    envelope.points = (0..10_000)
        .map(|i| Breakpoint {
            time: BeatTime(i),
            value: 0.5,
            curve: 0.0,
        })
        .collect();
    clip.end = BeatTime(10_000);
    clip.loop_end = BeatTime(10_000);
    let clip = project.clip(TrackId(1), 0).unwrap();
    let key = ClipKey {
        track: TrackId(1),
        slot: 0,
        id: clip.id,
        epoch: 0,
    };
    let cache = ModelCache::default();
    let (cached_clip, model) = cache.get(&project, key, 0, clip);
    let events = cache.events();
    assert_eq!(*events, model.events());
    let RowKind::Envelope(envelope) = &model.rows[1].kind else {
        panic!()
    };
    // Sixty view constructions (two seconds at 30 Hz): no model/clip/point copies.
    for _ in 0..60 {
        let (next_clip, next_model) = cache.get(&project, key, 0, clip);
        assert!(Rc::ptr_eq(&events, &cache.events()));
        assert!(Rc::ptr_eq(&cached_clip, &next_clip));
        assert!(Rc::ptr_eq(&model, &next_model));
        let chosen =
            super::super::lane_view::chosen_row(&project, key.track, &next_model, &envelope.target);
        assert!(Rc::ptr_eq(envelope, &chosen));
        assert_eq!(chosen.points.as_ptr(), envelope.points.as_ptr());
    }
    let mut edited = project.clone();
    edited.tracks[0].devices[0].name = "Renamed".into();
    let (new_clip, new_model) = cache.get(&edited, key, 1, edited.clip(key.track, 0).unwrap());
    assert!(!Rc::ptr_eq(&events, &cache.events()));
    assert!(!Rc::ptr_eq(&model, &new_model));
    assert!(!Rc::ptr_eq(&cached_clip, &new_clip));
    assert_eq!(new_model.rows[1].device, "Renamed");
    assert_eq!(
        *new_model,
        LaneModel::build(&edited, key.track, edited.clip(key.track, 0).unwrap())
    );
    for changed in [
        ClipKey { slot: 1, ..key },
        ClipKey {
            id: ClipId(99),
            ..key
        },
        ClipKey {
            track: TrackId(99),
            ..key
        },
        ClipKey { epoch: 1, ..key },
    ] {
        let (_, changed_model) = cache.get(&project, changed, 1, clip);
        assert!(!Rc::ptr_eq(&new_model, &changed_model));
    }
}

#[test]
fn unlinked_overview_projects_intro_wraps_and_one_shot_hold_from_launch() {
    let project = mixed_project();
    let model = LaneModel::build(&project, TrackId(1), project.clip(TrackId(1), 0).unwrap());
    let mut row = envelope(&model, 1).clone();
    row.region = Some(qtable_engine::model::region::Region {
        start: BeatTime(240),
        end: BeatTime(2400),
        loop_start: BeatTime(480),
        loop_end: BeatTime(1440),
        looping: true,
    });
    let samples = row.overview_samples(BeatTime::ZERO, BeatTime(3840), 384.0);
    let at = |time| samples.iter().find(|(t, _)| *t == time).unwrap().1;
    let value = |time| {
        row.samples(BeatTime(3840), 240.0)
            .into_iter()
            .find(|(t, _)| *t == time)
            .unwrap()
            .1
    };
    assert!((at(0.0) - value(240.0)).abs() < 1e-6);
    assert!((at(1200.0) - value(480.0)).abs() < 1e-6);
    assert!((at(2160.0) - value(480.0)).abs() < 1e-6);
    let endpoint = value(2400.0);
    row.region.as_mut().unwrap().looping = false;
    let samples = row.overview_samples(BeatTime::ZERO, BeatTime(3840), 384.0);
    assert!((samples.iter().find(|(t, _)| *t == 2160.0).unwrap().1 - endpoint).abs() < 1e-6);
    assert!((samples.last().unwrap().1 - endpoint).abs() < 1e-6);
}

#[test]
fn unlinked_overview_places_the_lane_playhead_on_its_launch_projection() {
    let project = mixed_project();
    let model = LaneModel::build(&project, TrackId(1), project.clip(TrackId(1), 0).unwrap());
    let mut row = envelope(&model, 1).clone();
    // Linked: the clip's own coordinates.
    assert_eq!(
        row.overview_time(BeatTime(960), BeatTime(600)),
        BeatTime(600)
    );
    row.region = Some(qtable_engine::model::region::Region {
        start: BeatTime(240),
        end: BeatTime(2400),
        loop_start: BeatTime(480),
        loop_end: BeatTime(1440),
        looping: true,
    });
    let at = |row: &EnvelopeRow, position| row.overview_time(BeatTime(960), BeatTime(position));
    // The first pass runs from the clip's Start; a repeat lands on the same content's
    // first-pass place, and the loop's part before Start on the first repeat.
    assert_eq!(at(&row, 240), BeatTime(960));
    assert_eq!(at(&row, 1200), BeatTime(1920));
    assert_eq!(at(&row, 1440), BeatTime(2160));
    let mut early = row.clone();
    early.region.as_mut().unwrap().start = BeatTime(720);
    assert_eq!(at(&early, 600), BeatTime(960 + 720 + 120));
    // A one-shot holds at its End.
    row.region.as_mut().unwrap().looping = false;
    assert_eq!(at(&row, 2400), BeatTime(960 + 2160));
}

#[test]
fn unlinked_overview_offsets_launch_and_wraps_by_clip_start_with_intro() {
    let mut project = mixed_project();
    let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
    clip.start = BeatTime(960);
    clip.loop_start = BeatTime(1920);
    clip.loop_end = BeatTime(3840);
    let model = LaneModel::build(&project, TrackId(1), project.clip(TrackId(1), 0).unwrap());
    let mut row = envelope(&model, 1).clone();
    row.region = Some(qtable_engine::model::region::Region {
        start: BeatTime(240),
        end: BeatTime(2400),
        loop_start: BeatTime(480),
        loop_end: BeatTime(1440),
        looping: true,
    });
    let samples = row.overview_samples(BeatTime(960), BeatTime(4800), 384.0);
    let expected = row.overview_samples(BeatTime::ZERO, BeatTime(3840), 384.0);
    assert_eq!(samples.len(), expected.len());
    for ((time, value), (elapsed, expected_value)) in samples.iter().zip(expected) {
        assert!((time - (960.0 + elapsed)).abs() < 1e-6);
        assert_eq!(*value, expected_value);
    }
    assert_eq!(samples.first().unwrap().0, 960.0);
    assert_eq!(samples.last().unwrap().0, 4800.0);
    // The first wrap is 1200 ticks after launch, on the clip's content axis.
    assert!(samples.iter().any(|(t, _)| *t == 2160.0));
}

/// A multi-device track with used, declared-empty and entirely unstored event rows.
pub(crate) fn track_event_rows_project() -> Project {
    use qtable_engine::model::{EventDef, EventLane, NewDevice};
    let mut project = mixed_project();
    let track = &mut project.tracks[0];
    let target = track.devices[0].target_mut().unwrap();
    target.events.extend(
        [("snare", "Snare"), ("hat", "Hi-hat")].map(|(key, name)| EventDef {
            key: EntryKey(key.into()),
            name: name.into(),
            path: OscAddress::Relative(format!("/{key}")),
            fields: vec![],
        }),
    );
    let mut second = NewDevice::Target.build(DeviceId(2));
    second.name = "Lights".into();
    let target = second.target_mut().unwrap();
    target.events = [("kick", "Flash"), ("blackout", "Blackout")]
        .map(|(key, name)| EventDef {
            key: EntryKey(key.into()),
            name: name.into(),
            path: OscAddress::Relative(format!("/{key}")),
            fields: vec![],
        })
        .to_vec();
    track.devices.push(second);
    let clip = track.slots[0].clip.as_mut().unwrap();
    clip.name = "Device event rows".into();
    clip.events.push(qtable_engine::model::Event {
        definition: EventRef {
            device: DeviceId(2),
            key: EntryKey("kick".into()),
        },
        position: BeatTime(480),
        arguments: vec![],
    });
    // A deliberately different stored order must never change display order.
    clip.lanes.insert(
        0,
        Lane::Events(EventLane {
            definition: EventRef {
                device: DeviceId(1),
                key: EntryKey("hat".into()),
            },
        }),
    );
    project.validate().unwrap();
    project
}

#[test]
fn every_device_event_has_a_row_in_definition_order_before_clip_envelopes() {
    let project = track_event_rows_project();
    let clip = project.clip(TrackId(1), 0).unwrap();
    let model = LaneModel::build(&project, TrackId(1), clip);
    let events = model.events();
    let names: Vec<_> = events
        .rows
        .iter()
        .map(|r| (r.device.as_str(), r.name.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("BopOS", "/kick"),
            ("BopOS", "Snare"),
            ("BopOS", "Hi-hat"),
            ("Lights", "Flash"),
            ("Lights", "Blackout")
        ]
    );
    assert_eq!(events.rows.len(), 5);
    assert_eq!(
        events.rows[1].kind,
        RowKind::Event {
            index: None,
            reference: EventRef {
                device: DeviceId(1),
                key: EntryKey("snare".into())
            }
        }
    );
    assert_eq!(
        events.rows[4].kind,
        RowKind::Event {
            index: None,
            reference: EventRef {
                device: DeviceId(2),
                key: EntryKey("blackout".into())
            }
        }
    );
    assert!(
        model.rows[5..]
            .iter()
            .all(|r| matches!(r.kind, RowKind::Envelope(_)))
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/track-event-rows.ron");
    if std::env::var_os("QTABLE_REGENERATE_FIXTURES").is_some() {
        qtable_engine::file::save(&path, &project).unwrap();
    }
    assert_eq!(qtable_engine::file::load(&path).unwrap(), project);
}

#[test]
fn sampling_a_64x_window_costs_its_pixels_and_points_not_the_clip() {
    let (_, model) = model();
    let row = envelope(&model, 1);
    let extent = BeatTime(64 * 3840);
    let plot = crate::inspector::point_gesture::Plot {
        left: 0.0,
        width: 384.0,
        top: 0.0,
        height: 40.0,
    };
    let mut lane = row.lane(plot, extent, 240);
    lane.time = crate::control::time_axis::TimeAxis::windowed(
        0.0,
        384.0,
        extent,
        crate::control::time_axis::Window {
            start: 61_440.0,
            visible: 3840.0,
        },
    );
    let (start, end) = lane.time.visible_range();
    let samples = row.samples_within((start, end), lane.time.ticks_per_px(), Axis::Continuous);
    let inside = row
        .points
        .iter()
        .filter(|p| (start..=end).contains(&(p.time.ticks() as f64)))
        .count();
    assert!(
        samples.len() <= 384 + 2 + 2 * inside,
        "{} samples for 384 px and {inside} points",
        samples.len()
    );
    assert!(samples.iter().all(|(t, _)| (start..=end).contains(t)));
    assert_eq!(samples.first().unwrap().0, start);
    assert_eq!(samples.last().unwrap().0, end);
    // The same row fitted across the clip needs the 64× more samples the window avoids.
    let fitted = row.samples(extent, extent.ticks() as f64 / 384.0);
    assert!(fitted.len() > 300 && samples.len() <= fitted.len() + 2 * inside);
    // A window is sampled as the engine evaluates it, so it agrees with the fitted line there.
    for (t, v) in &samples {
        let engine = AutomationEnvelope {
            region: None,
            target: row.target.clone(),
            points: row.points.clone(),
        };
        assert_eq!(engine.value_at(*t, Axis::Continuous), Some(*v));
    }
}

/// The sample times the fitted lane was drawn from before the window form existed.
fn fitted_sample_times(row: &EnvelopeRow, length: BeatTime, step: f64) -> Vec<f64> {
    let end = length.ticks() as f64;
    let mut at = vec![0.0, end];
    let mut tick = step.max(1.0);
    while tick < end {
        at.push(tick);
        tick += step.max(1.0);
    }
    for p in &row.points {
        let t = p.time.ticks() as f64;
        at.push(t);
        if t > 0.0 {
            at.push(t - 0.001);
        }
    }
    at.sort_by(f64::total_cmp);
    at.dedup();
    at
}

#[test]
fn fitted_sampling_keeps_the_sample_times_it_always_had() {
    let (_, model) = model();
    for index in 1..=4 {
        let row = envelope(&model, index);
        for step in [3840.0 / 384.0, 16.0, 240.0, 0.25] {
            let times: Vec<f64> = row
                .samples(BeatTime(3840), step)
                .into_iter()
                .map(|(t, _)| t)
                .collect();
            let wanted: Vec<f64> = fitted_sample_times(row, BeatTime(3840), step)
                .into_iter()
                .filter(|t| {
                    AutomationEnvelope {
                        region: None,
                        target: row.target.clone(),
                        points: row.points.clone(),
                    }
                    .value_at(*t, row.line_axis())
                    .is_some()
                })
                .collect();
            assert_eq!(times, wanted, "row {index} step {step}");
        }
    }
}
