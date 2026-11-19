use qtable_engine::{file, model::*};

#[test]
fn mixed_lanes_round_trip_and_invalid_domains_fail() {
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v7-targets.ron");
    let mut p = file::load(fixture).unwrap();
    let e = AutomationEnvelope {
        region: None,
        target: ArgumentRef::volume(),
        points: vec![
            Breakpoint {
                time: BeatTime::ZERO,
                value: 0.25,
                curve: 0.5,
            },
            Breakpoint {
                time: p.tracks[1].slots[0].clip.as_ref().unwrap().length(),
                value: 0.75,
                curve: 0.0,
            },
        ],
    };
    p.tracks[1].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .insert(1, Lane::Envelope(e.clone()));
    p.validate().unwrap();
    let path =
        std::env::temp_dir().join(format!("qtable-envelope-format-{}.ron", std::process::id()));
    file::save(&path, &p).unwrap();
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("version: 12")
    );
    assert_eq!(file::load(&path).unwrap(), p);
    std::fs::remove_file(path).unwrap();
    let mut invalid = p.clone();
    invalid.tracks[1].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .lanes
        .push(Lane::Envelope(e));
    assert!(invalid.validate().is_err());
    for (field, value) in [("gone", 0.5), ("value", 2.0)] {
        let mut invalid = p.clone();
        let Lane::Envelope(e) = &mut invalid.tracks[1].slots[0].clip.as_mut().unwrap().lanes[1]
        else {
            unreachable!()
        };
        e.target.field.0 = field.into();
        e.points[0].value = value;
        assert!(invalid.validate().is_err());
    }
}
