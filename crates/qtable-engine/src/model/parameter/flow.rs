use super::*;

/// The parameter pipeline. Runtime modulation and processors acquire implementations later.
pub fn parameter_value_flow(
    spec: &ParameterSpec,
    base: f64,
    mapping: Mapping,
) -> Result<f32, MappingError> {
    let modulation = 0.0;
    let effective = (base + modulation).clamp(spec.min, spec.max);
    let mapped = mapping.map(effective)?;
    let processed = processors(mapped);
    let send = processed as f32;
    Ok(send)
}
fn processors(value: f64) -> f64 {
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clamps_before_mapping_and_sends_a_float() {
        let mut stored = StoredParameter {
            value: -1.0,
            output: ParameterOutput {
                mapping: Mapping::new([0.0, 1.0], [10.0, 20.0], 2.0, 3.0).unwrap(),
                ..ParameterOutput::default()
            },
        };
        assert_eq!(
            parameter_value_flow(&VOLUME, stored.value, stored.output.mapping).unwrap(),
            13.0_f32
        );
        stored.value = 2.0;
        assert_eq!(
            parameter_value_flow(&VOLUME, stored.value, stored.output.mapping).unwrap(),
            23.0_f32
        );
        stored.value = 0.5;
        assert_eq!(
            parameter_value_flow(&VOLUME, stored.value, stored.output.mapping).unwrap(),
            15.5_f32
        );
    }
}
