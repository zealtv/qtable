use super::*;
use crate::control::{Controls, LIVE_DARK};
use crate::control_focus::ControlFocus;
use crate::control_focus::test_registry::{TestHelp, TestKey, TestMessage};

// Owners supply their own keys; these stand-in keys exercise the editor without one.
fn keys(id: u64) -> [TestKey; 6] {
    std::array::from_fn(|i| TestKey(id + i as u64))
}

#[test]
fn invalid_entries_survive_and_valid_submits_return_complete_replacements() {
    let (mut values, mut errors) = (HashMap::new(), HashMap::new());
    let keys = keys(10);
    let current = Mapping::default();
    for value in ["-", "NaN", "0", "-2"] {
        update(
            &mut values,
            &mut errors,
            current,
            &keys,
            true,
            MappingEditorMessage::Changed(MappingField::Exponent, value.into()),
        )
        .unwrap();
        assert!(
            update(
                &mut values,
                &mut errors,
                current,
                &keys,
                true,
                MappingEditorMessage::Submitted(MappingField::Exponent)
            )
            .is_err()
        );
        assert_eq!(values[&keys[4]], value);
        assert!(errors.contains_key(&keys[4]));
    }
    update(
        &mut values,
        &mut errors,
        current,
        &keys,
        true,
        MappingEditorMessage::Changed(MappingField::Exponent, " 2.5 ".into()),
    )
    .unwrap();
    assert!(!errors.contains_key(&keys[4]));
    let result = update(
        &mut values,
        &mut errors,
        current,
        &keys,
        true,
        MappingEditorMessage::Submitted(MappingField::Exponent),
    )
    .unwrap()
    .unwrap();
    assert_eq!(result.exponent(), 2.5);
    assert_eq!(result.input(), current.input());
    assert_eq!(current, Mapping::default());
    // Keep the draft until the caller confirms the engine edit succeeded.
    assert_eq!(values[&keys[4]], " 2.5 ");
    values.remove(&keys[4]);
    errors.remove(&keys[4]);
    assert!(values.is_empty());
}

#[test]
fn untouched_display_keeps_precision_but_explicit_edit_applies() {
    let current = Mapping::new([0.0, 1.0], [0.0, 1.0], 1.0, 0.12345678).unwrap();
    assert_eq!(ValueFormat::Float.display(current.bias()), "0.1235");
    let keys = keys(10);
    let (mut values, mut errors) = (HashMap::new(), HashMap::new());
    assert!(
        update(
            &mut values,
            &mut errors,
            current,
            &keys,
            true,
            MappingEditorMessage::Submitted(MappingField::Bias)
        )
        .unwrap()
        .is_none()
    );
    update(
        &mut values,
        &mut errors,
        current,
        &keys,
        true,
        MappingEditorMessage::Changed(MappingField::Bias, "0.12".into()),
    )
    .unwrap();
    assert_eq!(
        update(
            &mut values,
            &mut errors,
            current,
            &keys,
            true,
            MappingEditorMessage::Submitted(MappingField::Bias)
        )
        .unwrap()
        .unwrap()
        .bias(),
        0.12
    );
}

#[test]
fn every_setting_uses_its_own_key_and_range_validation() {
    let keys = keys(10);
    for field in MappingField::ALL {
        let (mut values, mut errors) = (HashMap::new(), HashMap::new());
        let message = MappingEditorMessage::Changed(field, "3.0".into());
        assert_eq!(message.key(&keys), keys[field.index()]);
        update(
            &mut values,
            &mut errors,
            Mapping::default(),
            &keys,
            true,
            message,
        )
        .unwrap();
        let result = update(
            &mut values,
            &mut errors,
            Mapping::default(),
            &keys,
            true,
            MappingEditorMessage::Submitted(field),
        )
        .unwrap()
        .unwrap();
        assert_eq!(field.value(result), 3.0);
    }
    let (mut values, mut errors) = (HashMap::new(), HashMap::new());
    update(
        &mut values,
        &mut errors,
        Mapping::default(),
        &keys,
        true,
        MappingEditorMessage::Changed(MappingField::InputEnd, "0.0".into()),
    )
    .unwrap();
    assert!(
        update(
            &mut values,
            &mut errors,
            Mapping::default(),
            &keys,
            true,
            MappingEditorMessage::Submitted(MappingField::InputEnd)
        )
        .is_err()
    );
}

#[test]
fn owners_share_the_store_without_colliding_and_disabled_edits_are_ignored() {
    let (mut values, mut errors) = (HashMap::new(), HashMap::new());
    let a = keys(10);
    let b = keys(20);
    let current = Mapping::default();
    update(
        &mut values,
        &mut errors,
        current,
        &a,
        true,
        MappingEditorMessage::Changed(MappingField::Bias, "1.0".into()),
    )
    .unwrap();
    update(
        &mut values,
        &mut errors,
        current,
        &b,
        true,
        MappingEditorMessage::Changed(MappingField::Bias, "2.0".into()),
    )
    .unwrap();
    update(
        &mut values,
        &mut errors,
        current,
        &a,
        false,
        MappingEditorMessage::Changed(MappingField::Bias, "9.0".into()),
    )
    .unwrap();
    assert!(
        update(
            &mut values,
            &mut errors,
            current,
            &a,
            false,
            MappingEditorMessage::Submitted(MappingField::Bias)
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(values[&a[5]], "1.0");
    assert_eq!(values[&b[5]], "2.0");
}

#[test]
fn editor_builds_with_the_shared_controls_and_the_callers_text() {
    let (values, errors) = (HashMap::new(), HashMap::new());
    let focus = ControlFocus::default();
    let editor = MappingEditor {
        current: Mapping::default(),
        values: &values,
        errors: &errors,
        keys: keys(10),
        location: View::Track,
        enabled: true,
        help: |_| TestHelp::Path,
    };
    let _element = editor.view(Controls::new(LIVE_DARK, &focus), |_| TestMessage::Tick);
}

#[test]
fn compact_mapping_labels_fit_and_keep_full_info_titles() {
    let labels = MappingField::ALL.map(MappingField::label);
    assert_eq!(
        labels,
        [
            "In Start",
            "In End",
            "Out Start",
            "Out End",
            "Exponent",
            "Bias"
        ]
    );
    for label in labels {
        assert!(
            super::super::path_field::measure(label, iced::Pixels(detail::TEXT_SIZE as f32))
                <= detail::DEVICE_NUMBER_WIDTH,
            "{label} clips in its compact column"
        );
    }
}
