use super::*;
use crate::model::{
    Device, DeviceId, DeviceKind, EntryKey, FieldDef, FieldKey, FieldType, OscAddress,
    ParameterDef, TargetDevice,
};

fn point(mode: SendMode) -> Project {
    let mut p = at_50_hz(project(mode, 0.0));
    p.tracks[0].mixer_device.volume.output.enabled = false;
    let fields = [0.1, 0.2, 10.0]
        .into_iter()
        .enumerate()
        .map(|(i, value)| {
            let mut field = FieldDef::new(FieldKey(format!("f{}", i + 1)), FieldType::Float, true);
            field.range = Some([0.0, 100.0]);
            field.value = Some(value);
            field.threshold = if i == 2 { 0.5 } else { 0.0 };
            field
        })
        .collect();
    p.tracks[0].devices.push(Device {
        id: DeviceId(1),
        name: "Target".into(),
        active: true,
        expanded: false,
        kind: DeviceKind::Target(TargetDevice {
            parameters: vec![ParameterDef {
                key: EntryKey("point".into()),
                name: "Point".into(),
                path: OscAddress::Relative("/point".into()),
                fields,
                send_mode: mode,
            }],
            ..TargetDevice::default()
        }),
    });
    p.validate().unwrap();
    p
}
fn field(p: &mut Project, index: usize, value: f64) {
    let t = p.tracks[0].devices[0].target_mut().unwrap();
    t.parameters[0].fields[index].value = Some(value);
}
fn args(rig: &mut Rig, p: &Project, at: u64) -> Vec<Vec<OscType>> {
    rig.datagrams(p, at)
        .into_iter()
        .map(|packet| {
            let OscPacket::Message(message) = packet else {
                panic!("expected one plain message");
            };
            assert_eq!(message.addr, "/point");
            message.args
        })
        .collect()
}
fn point_args(x: f32, y: f32, radius: f32) -> Vec<OscType> {
    vec![OscType::Float(x), OscType::Float(y), OscType::Float(radius)]
}

#[test]
fn whole_messages_coalesce_and_each_field_has_its_own_threshold() {
    let mut p = point(SendMode::Continuous);
    let mut rig = Rig::new(&p);
    // Fifty edits before the first frame become one three-argument datagram.
    for edit in 1..=50 {
        field(&mut p, 0, f64::from(edit) / 100.0);
        rig.update(&p, 1);
    }
    assert_eq!(args(&mut rig, &p, 20), [point_args(0.5, 0.2, 10.0)]);
    assert_eq!(rig.stages.counts(TrackId(2)).unwrap().merged, 49);
    field(&mut p, 2, 10.2);
    rig.update(&p, 25);
    assert!(args(&mut rig, &p, 40).is_empty());
    assert_eq!(args(&mut rig, &p, 60), [point_args(0.5, 0.2, 10.2)]);
    field(&mut p, 2, 10.3);
    field(&mut p, 0, 0.7);
    rig.update(&p, 65);
    assert_eq!(args(&mut rig, &p, 80), [point_args(0.7, 0.2, 10.3)]);
    assert!(args(&mut rig, &p, 100).is_empty());
}

#[test]
fn whole_messages_keep_delay_snapshots_and_every_frame_tail() {
    let mut p = point(SendMode::EveryFrame);
    p.tracks[0].delay_ms = 100.0;
    let mut rig = Rig::new(&p);
    rig.stages.start(rig.anchor);
    field(&mut p, 0, 0.8);
    field(&mut p, 1, 0.9);
    rig.update(&p, 25);
    rig.stages.stop(ms(rig.anchor, 55));
    assert!(args(&mut rig, &p, 80).is_empty());
    assert_eq!(args(&mut rig, &p, 100), [point_args(0.1, 0.2, 10.0)]);
    assert_eq!(args(&mut rig, &p, 120), [point_args(0.1, 0.2, 10.0)]);
    assert_eq!(args(&mut rig, &p, 140), [point_args(0.8, 0.9, 10.0)]);
    assert!(args(&mut rig, &p, 160).is_empty());
}

#[test]
fn a_three_field_message_costs_one_budget_token_and_one_refresh_step() {
    let mut p = with_stage(point(SendMode::Continuous), |stage| {
        stage.budget_on = true;
        stage.budget = 10;
        stage.refresh_on = true;
        stage.refresh = 0.1;
    });
    let mut rig = Rig::new(&p);
    field(&mut p, 0, 0.8);
    rig.update(&p, 1);
    assert_eq!(args(&mut rig, &p, 20), [point_args(0.8, 0.2, 10.0)]);
    field(&mut p, 1, 0.9);
    rig.update(&p, 21);
    assert!(args(&mut rig, &p, 40).is_empty());
    assert_eq!(args(&mut rig, &p, 120), [point_args(0.8, 0.9, 10.0)]);
    assert_eq!(args(&mut rig, &p, 220), [point_args(0.8, 0.9, 10.0)]);
    assert_eq!(rig.stages.counts(TrackId(2)).unwrap().messages, 3);
}

#[test]
fn bundles_contain_whole_parameter_messages() {
    let mut p = with_stage(point(SendMode::Continuous), |stage| stage.bundle = true);
    let mut rig = Rig::new(&p);
    field(&mut p, 0, 0.8);
    rig.update(&p, 1);
    let packets = rig.datagrams(&p, 20);
    let [OscPacket::Bundle(bundle)] = packets.as_slice() else {
        panic!("expected a bundle");
    };
    assert_eq!(
        bundle.content,
        [OscPacket::Message(OscMessage {
            addr: "/point".into(),
            args: point_args(0.8, 0.2, 10.0)
        })]
    );
    let counts = rig.stages.counts(TrackId(2)).unwrap();
    assert_eq!((counts.messages, counts.packets), (1, 1));
}

/// A one-argument parameter `/point` whose argument has `threshold` stored, whatever its control.
fn one_argument(mut field: FieldDef, threshold: f64) -> Project {
    let mut p = at_50_hz(project(SendMode::Continuous, 0.0));
    p.tracks[0].mixer_device.volume.output.enabled = false;
    field.threshold = threshold;
    p.tracks[0].devices.push(Device {
        id: DeviceId(1),
        name: "Target".into(),
        active: true,
        expanded: false,
        kind: DeviceKind::Target(TargetDevice {
            parameters: vec![ParameterDef {
                key: EntryKey("point".into()),
                name: "Point".into(),
                path: OscAddress::Relative("/point".into()),
                fields: vec![field],
                send_mode: SendMode::Continuous,
            }],
            ..TargetDevice::default()
        }),
    });
    p.validate().unwrap();
    p
}

/// Decided 2026-11-02: Threshold applies only to a continuous numeric range. A stored Threshold left
/// over from an earlier Slider must not delay a Toggle or an Enum, whose adjacent values are
/// 1 apart (below the 5 stored here), and a Slider keeps holding back a change under it.
#[test]
fn a_stored_threshold_holds_back_only_a_continuous_numeric_argument() {
    use crate::model::{Control, EnumOption};
    let float = || {
        let mut field = FieldDef::new(FieldKey("f".into()), FieldType::Float, true);
        field.range = Some([0.0, 100.0]);
        field.value = Some(0.0);
        field
    };
    let toggle = || {
        let mut field = float();
        field.control = Control::Toggle;
        field.range = Some([0.0, 1.0]);
        field
    };
    let enumerated = || {
        let mut field = float();
        field.control = Control::Enum;
        field.options = (1..=2)
            .map(|n| EnumOption::numbered(FieldType::Float, n).unwrap())
            .collect();
        field.default = field.options[0].value.clone();
        field
    };
    for (name, argument, step, sends_at_once) in [
        ("slider", float(), 3.0, false),
        ("toggle", toggle(), 0.0, true),
        ("enum", enumerated(), 0.0, true),
    ] {
        let mut p = one_argument(argument, 5.0);
        let mut rig = Rig::new(&p);
        // Send 1 first, so the next change has a last-sent value to be measured from.
        field(&mut p, 0, 1.0);
        rig.update(&p, 1);
        assert_eq!(args(&mut rig, &p, 20).len(), 1, "{name}");
        // Then a change of 2 (slider) or 1 (toggle, enum), under the stored Threshold of 5.
        field(&mut p, 0, step);
        rig.update(&p, 25);
        assert_eq!(
            args(&mut rig, &p, 40).len(),
            usize::from(sends_at_once),
            "{name}"
        );
    }
}
