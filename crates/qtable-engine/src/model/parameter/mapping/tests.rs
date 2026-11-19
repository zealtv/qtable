use super::*;

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn identity_endpoints_saturation_and_nonfinite_samples() {
    let m = Mapping::default();
    for x in [0.0, 0.25, 0.5, 1.0] {
        assert_eq!(m.map(x).unwrap(), x);
    }
    assert_eq!(m.map(-f64::MAX).unwrap(), 0.0);
    assert_eq!(m.map(f64::MAX).unwrap(), 1.0);
    assert_eq!(m.unmap(-100.0).unwrap(), 0.0);
    assert_eq!(m.unmap(100.0).unwrap(), 1.0);
    for x in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(m.map(x).is_err());
        assert!(m.unmap(x).is_err());
    }
}

#[test]
fn power_shapes_and_bias_in_output_units() {
    let m = Mapping::new([0.0, 10.0], [20.0, 40.0], 2.0, -5.0).unwrap();
    assert_eq!(m.map(0.0).unwrap(), 15.0);
    assert_eq!(m.map(10.0).unwrap(), 35.0);
    assert_eq!(m.map(5.0).unwrap(), 20.0);
    let root = Mapping::new([0.0, 1.0], [0.0, 1.0], 0.5, 0.0).unwrap();
    assert_eq!(root.map(0.25).unwrap(), 0.5);
}

#[test]
fn descending_ranges_monotonicity_and_inverse_round_trips() {
    for input in [[-2.0, 8.0], [8.0, -2.0]] {
        for output in [[-10.0, 30.0], [30.0, -10.0]] {
            for exponent in [0.5, 1.0, 2.0, 3.5] {
                let m = Mapping::new(input, output, exponent, 4.0).unwrap();
                let mut prev = m.map(input[0]).unwrap();
                for i in 0..=100 {
                    let x = interpolate(input, f64::from(i) / 100.0);
                    let y = m.map(x).unwrap();
                    assert!((y - prev) * (output[1] - output[0]) >= 0.0);
                    close(m.unmap(y).unwrap(), x);
                    prev = y;
                }
                assert_eq!(m.map(input[0]).unwrap(), output[0] + 4.0);
                assert_eq!(m.map(input[1]).unwrap(), output[1] + 4.0);
                assert_eq!(m.unmap(output[0] + 4.0).unwrap(), input[0]);
                assert_eq!(m.unmap(output[1] + 4.0).unwrap(), input[1]);
            }
        }
    }
}

#[test]
fn constant_output_is_valid_but_has_no_inverse() {
    let m = Mapping::new([0.0, 1.0], [7.0, 7.0], 2.0, 3.0).unwrap();
    assert_eq!(m.map(0.5).unwrap(), 10.0);
    assert!(m.unmap(10.0).is_err());
}

#[test]
fn invalid_settings_are_rejected() {
    for exponent in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::from_bits(1)] {
        assert!(Mapping::new([0.0, 1.0], [0.0, 1.0], exponent, 0.0).is_err());
    }
    for input in [[1.0, 1.0], [-f64::MAX, f64::MAX], [0.0, f64::NAN]] {
        assert!(Mapping::new(input, [0.0, 1.0], 1.0, 0.0).is_err());
    }
    for output in [[-f64::MAX, f64::MAX], [0.0, f64::INFINITY]] {
        assert!(Mapping::new([0.0, 1.0], output, 1.0, 0.0).is_err());
    }
    assert!(Mapping::new([0.0, 1.0], [1e308, 1e308], 1.0, 1e308).is_err());
    assert!(Mapping::new([0.0, 1.0], [0.0, 1.0], 1.0, f64::NAN).is_err());
}

#[test]
fn extreme_finite_samples_and_ranges_stay_finite() {
    let m = Mapping::new([-f64::MAX, -f64::MAX / 2.0], [0.0, f64::MAX], 1.0, 0.0).unwrap();
    assert_eq!(m.map(f64::MAX).unwrap(), f64::MAX);
    assert_eq!(m.map(-f64::MAX).unwrap(), 0.0);
    assert!(m.map(-f64::MAX * 0.75).unwrap().is_finite());
    assert!(m.unmap(f64::MAX / 2.0).unwrap().is_finite());
}

#[test]
fn serde_round_trip_defaults_and_validation() {
    let m = Mapping::new([1.0, -1.0], [-7.0, 8.0], 0.5, 2.0).unwrap();
    let text = ron::to_string(&m).unwrap();
    assert_eq!(ron::from_str::<Mapping>(&text).unwrap(), m);
    assert_eq!(
        ron::from_str::<Mapping>("(input:(0.0,1.0),output:(0.0,1.0))").unwrap(),
        Mapping::default()
    );
    for text in [
        "(input:(0.0,0.0),output:(0.0,1.0))",
        "(input:(0.0,1.0),output:(0.0,1.0),exponent:0.0)",
        "(input:(0.0,1.0),output:(0.0,1.0),bias:inf)",
    ] {
        assert!(ron::from_str::<Mapping>(text).is_err(), "{text}");
    }
}
