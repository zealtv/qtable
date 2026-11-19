//! Argument parsing. (Formatting an argument is `control::argument_text`.)
pub fn parse_argument(
    kind: &str,
    value: &str,
) -> Result<qtable_engine::model::OscArgument, String> {
    use qtable_engine::model::OscArgument;
    match kind {
        "int" => value
            .parse::<i32>()
            .map(OscArgument::Int)
            .map_err(|_| "Enter a 32-bit integer".into()),
        "float" => value
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .map(OscArgument::Float)
            .ok_or_else(|| "Enter a finite float".into()),
        "string" => Ok(OscArgument::String(value.into())),
        "bool" => match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(OscArgument::Bool(true)),
            "false" | "0" => Ok(OscArgument::Bool(false)),
            _ => Err("Enter true or false".into()),
        },
        _ => Err("Choose int, float, string, or bool".into()),
    }
}
