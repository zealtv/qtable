//! Parsing and formatting of tempo and port text fields (delay: `qtable_engine::model::parameter`).

pub(crate) fn parse_tempo(input: &str) -> Result<f64, String> {
    let bpm = input
        .trim()
        .parse::<f64>()
        .map_err(|_| "Enter a valid tempo".to_owned())?;
    if bpm.is_finite() && bpm > 0.0 {
        Ok(bpm)
    } else {
        Err("Tempo must be a positive finite number".to_owned())
    }
}

pub(crate) fn parse_port(input: &str) -> Result<u16, String> {
    let port = input
        .trim()
        .parse::<u16>()
        .map_err(|_| "Enter a valid UDP port".to_owned())?;
    if port > 0 {
        Ok(port)
    } else {
        Err("Port must be between 1 and 65535".to_owned())
    }
}

/// Two to four decimals; the stored tempo keeps its full precision.
pub(crate) fn format_tempo(bpm: f64) -> String {
    crate::transport::TEMPO_FORMAT.display(bpm)
}
