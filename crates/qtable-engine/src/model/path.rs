//! A track's OSC path: the namespace it adds to its descendants' relative addresses.
use super::*;

/// Empty, or slash-led with no trailing slash (`/kite/3`). Validated on construction and
/// on edit. Loading preserves newly invalid characters for repair.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(into = "String")]
pub struct Path(String);

impl Path {
    pub fn new(path: impl Into<String>) -> ModelResult<Self> {
        let path = Self(path.into());
        path.validate()?;
        Ok(path)
    }

    /// A path taken from an older file shape; `Project::validate` still checks it.
    pub(crate) fn unchecked(path: String) -> Self {
        Self(path)
    }

    pub fn validate(&self) -> ModelResult {
        self.validate_with(validate_address)
    }

    pub(super) fn validate_with(&self, address: fn(&str) -> ModelResult) -> ModelResult {
        if self.0.is_empty() {
            return Ok(());
        }
        address(&self.0)?;
        require(!self.0.ends_with('/'), "OSC path must not end with slash")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Typed pieces lead with a slash: empty stays empty, and anything else gains a `/` in
/// front when it lacks one. Validation of the rest stays with the caller.
pub fn lead_with_slash(input: String) -> String {
    if input.is_empty() || input.starts_with('/') {
        input
    } else {
        format!("/{input}")
    }
}

impl TryFrom<String> for Path {
    type Error = ModelError;
    fn try_from(path: String) -> ModelResult<Self> {
        Self::new(path)
    }
}

// Deserialization retains characters accepted by previous releases, while constructors
// and TryFrom keep enforcing the current rules for new values.
impl<'de> Deserialize<'de> for Path {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let path = Self(String::deserialize(deserializer)?);
        path.validate_with(validate_stored_address)
            .map_err(serde::de::Error::custom)?;
        Ok(path)
    }
}

impl From<Path> for String {
    fn from(path: Path) -> Self {
        path.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_empty_and_slash_led_paths() {
        for ok in ["", "/a", "/a/b"] {
            assert_eq!(Path::new(ok).unwrap().as_str(), ok);
        }
    }

    #[test]
    fn rejects_unled_trailing_bare_slash_and_nul() {
        for bad in ["a", "/", "/a/", "/a\0"] {
            assert!(Path::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_hand_edited_file_with_a_bad_path_fails_on_load() {
        let error = ron::from_str::<Path>("\"a\"").unwrap_err().to_string();
        assert!(error.contains("path must start with a slash"), "{error}");
    }
}
