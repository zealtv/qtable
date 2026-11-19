//! Literal OSC path validation shared by paths, lanes and volume controllers.
use super::{ModelError, ModelResult, require};

/// Validate one OSC method/container name (not a slash-separated path).
/// OSC 1.0 Address Spaces: https://opensoundcontrol.stanford.edu/spec-1_0.html
pub fn validate_address_part(part: &str) -> ModelResult {
    for character in part.chars() {
        let reason = match character {
            ' ' => Some("space".to_owned()),
            '\0' => Some("NUL".to_owned()),
            '#' | '*' | ',' | '/' | '?' | '[' | ']' | '{' | '}' => Some(format!("'{character}'")),
            _ => None,
        };
        if let Some(reason) = reason {
            return Err(ModelError(format!("path part cannot contain {reason}")));
        }
    }
    Ok(())
}

/// Validate a complete literal address; slashes separate its parts.
pub fn validate_address(address: &str) -> ModelResult {
    validate_stored_address(address)?;
    for part in address.split('/') {
        validate_address_part(part)?;
    }
    Ok(())
}

/// Previous file rules. New character restrictions must not prevent repairing old sets.
pub(crate) fn validate_stored_address(address: &str) -> ModelResult {
    require(
        address.starts_with('/') && !address.contains('\0'),
        "path must start with a slash and contain no NUL",
    )
}
