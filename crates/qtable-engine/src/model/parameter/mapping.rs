//! Pure range conversion shared by parameters, inputs and OSC scaling.
use serde::{Deserialize, Serialize};
use std::fmt;

/// Ordered ranges, a positive power curve, and an additive bias in output units.
/// Inputs always clamp. Descending endpoints invert a range without another flag.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "MappingData", into = "MappingData")]
pub struct Mapping {
    input: [f64; 2],
    output: [f64; 2],
    exponent: f64,
    bias: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingError(pub &'static str);

impl fmt::Display for MappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for MappingError {}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MappingData {
    input: [f64; 2],
    output: [f64; 2],
    #[serde(default = "one")]
    exponent: f64,
    #[serde(default)]
    bias: f64,
}
fn one() -> f64 {
    1.0
}

impl Default for Mapping {
    fn default() -> Self {
        Self {
            input: [0.0, 1.0],
            output: [0.0, 1.0],
            exponent: 1.0,
            bias: 0.0,
        }
    }
}

impl Mapping {
    pub fn new(
        input: [f64; 2],
        output: [f64; 2],
        exponent: f64,
        bias: f64,
    ) -> Result<Self, MappingError> {
        if !input
            .into_iter()
            .chain(output)
            .chain([exponent, bias])
            .all(f64::is_finite)
        {
            return Err(MappingError("Mapping settings must be finite"));
        }
        let input_span = input[1] - input[0];
        if input_span == 0.0 || !input_span.is_finite() {
            return Err(MappingError("Input endpoints must differ by a finite span"));
        }
        if !(output[1] - output[0]).is_finite() {
            return Err(MappingError("Output span must be finite"));
        }
        if exponent <= 0.0 || !exponent.recip().is_finite() {
            return Err(MappingError(
                "Exponent must be positive with a finite reciprocal",
            ));
        }
        if !output.into_iter().all(|v| (v + bias).is_finite())
            || !((output[1] + bias) - (output[0] + bias)).is_finite()
        {
            return Err(MappingError("Biased output endpoints must be finite"));
        }
        Ok(Self {
            input,
            output,
            exponent,
            bias,
        })
    }

    pub fn input(self) -> [f64; 2] {
        self.input
    }
    pub fn output(self) -> [f64; 2] {
        self.output
    }
    pub fn exponent(self) -> f64 {
        self.exponent
    }
    pub fn bias(self) -> f64 {
        self.bias
    }

    /// Normalize, clamp, exponentiate, scale, then add bias in output units.
    pub fn map(self, x: f64) -> Result<f64, MappingError> {
        let t = normalize(x, self.input)?.powf(self.exponent);
        // Interpolating the biased endpoints is algebraically equivalent to adding
        // bias last, and keeps even extreme finite outputs inside their bounds.
        Ok(interpolate(self.biased_output(), t))
    }

    /// Reverse the curve; saturation recovers the corresponding input endpoint.
    /// Constant outputs (including endpoints collapsed by floating-point bias)
    /// cannot identify an input and return an error.
    pub fn unmap(self, y: f64) -> Result<f64, MappingError> {
        let output = self.biased_output();
        if output[0] == output[1] {
            return Err(MappingError("A constant mapping has no inverse"));
        }
        let t = normalize(y, output)?.powf(self.exponent.recip());
        Ok(interpolate(self.input, t))
    }

    fn biased_output(self) -> [f64; 2] {
        self.output.map(|v| v + self.bias)
    }
}

fn normalize(value: f64, [start, end]: [f64; 2]) -> Result<f64, MappingError> {
    if !value.is_finite() {
        return Err(MappingError("Mapped values must be finite"));
    }
    // Clamp before subtraction: an out-of-range finite value can overflow a
    // subtraction even when the validated range itself has a finite span.
    let value = value.clamp(start.min(end), start.max(end));
    if value == start {
        return Ok(0.0);
    }
    if value == end {
        return Ok(1.0);
    }
    Ok(((value - start) / (end - start)).clamp(0.0, 1.0))
}

fn interpolate([start, end]: [f64; 2], t: f64) -> f64 {
    if t == 0.0 {
        return start;
    }
    if t == 1.0 {
        return end;
    }
    t.mul_add(end - start, start)
        .clamp(start.min(end), start.max(end))
}

impl TryFrom<MappingData> for Mapping {
    type Error = MappingError;
    fn try_from(value: MappingData) -> Result<Self, Self::Error> {
        Self::new(value.input, value.output, value.exponent, value.bias)
    }
}
impl From<Mapping> for MappingData {
    fn from(value: Mapping) -> Self {
        Self {
            input: value.input,
            output: value.output,
            exponent: value.exponent,
            bias: value.bias,
        }
    }
}

#[cfg(test)]
mod tests;
