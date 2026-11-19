//! Range refusals quote the argument by the name its face and lane header show.
use qtable_engine::model::*;

fn parameter(name: &str, fields: Vec<FieldDef>) -> ParameterDef {
    ParameterDef {
        key: EntryKey("level".into()),
        name: name.into(),
        path: OscAddress::Relative("/level".into()),
        fields,
        send_mode: SendMode::Continuous,
    }
}
fn float(key: &str, name: &str) -> FieldDef {
    let mut f = FieldDef::new(FieldKey(key.into()), FieldType::Float, true);
    f.range = Some([0.0, 1.0]);
    f.name = name.into();
    f
}

#[test]
fn a_single_argument_parameter_is_quoted_by_the_parameters_name_or_its_path() {
    let named = parameter("Opacity", vec![float("value", "")]);
    let spec = named.field_spec(0).unwrap();
    assert_eq!(
        spec.check(1.5).unwrap_err(),
        "Opacity must be between 0.00 and 1.00"
    );
    assert_eq!(spec.parse("loud").unwrap_err(), "Enter a valid opacity");
    let unnamed = parameter("", vec![float("value", "")]);
    assert_eq!(
        unnamed.field_spec(0).unwrap().check(-1.0).unwrap_err(),
        "/level must be between 0.00 and 1.00"
    );
}

#[test]
fn a_multi_argument_field_is_quoted_by_its_own_name_or_its_position() {
    let p = parameter("Position", vec![float("x", "X"), float("y", "")]);
    assert!(
        p.field_spec(0)
            .unwrap()
            .check(2.0)
            .unwrap_err()
            .starts_with("X must")
    );
    assert!(
        p.field_spec(1)
            .unwrap()
            .check(2.0)
            .unwrap_err()
            .starts_with("Arg 2 must")
    );
    assert!(p.field_spec(2).is_none());
}

#[test]
fn a_spec_nobody_named_still_has_a_subject() {
    let unnamed = float("value", "").spec();
    assert_eq!(
        unnamed.check(2.0).unwrap_err(),
        "Value must be between 0.00 and 1.00"
    );
    assert_eq!(unnamed.parse("x").unwrap_err(), "Enter a valid value");
}
