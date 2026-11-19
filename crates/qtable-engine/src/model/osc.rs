//! Receiver-independent OSC outputs, addresses and events.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UdpMode {
    #[default]
    Unicast,
    Broadcast,
}

/// Where an output's packets go: one receiver.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Destination {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub mode: UdpMode,
}

/// The output component: what makes a track an output. Name, colour, Activator, Solo and latency (`delay_ms`) live on the
/// `Track`. Shared receiver references are deferred to v3.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub destination: Destination,
    /// Omitted at its defaults, so the set still opens in v3.0.
    #[serde(default, skip_serializing_if = "OutputStage::is_default")]
    pub stage: OutputStage,
}

impl Track {
    /// An output track: a track of kind Output with a destination component. It holds no
    /// clips, routes nowhere and never groups.
    pub fn new_output(
        id: TrackId,
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
    ) -> Self {
        let name = name.into();
        Self {
            kind: TrackKind::Output,
            route: Route::Group,
            slots: Vec::new(),
            output: Some(Output {
                destination: Destination {
                    host: host.into(),
                    port,
                    mode: UdpMode::Unicast,
                },
                stage: OutputStage::default(),
            }),
            ..Self::new(id, name, 0)
        }
    }

    /// Builder-style setters for building outputs by hand (tests, tools).
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Path::new(path).expect("valid OSC path");
        self
    }

    pub fn with_mode(mut self, mode: UdpMode) -> Self {
        if let Some(destination) = self.destination_mut() {
            destination.mode = mode;
        }
        self
    }

    pub fn is_output(&self) -> bool {
        self.kind == TrackKind::Output
    }

    /// The destination of an output track; None for any other kind.
    pub fn destination(&self) -> Option<&Destination> {
        self.output.as_ref().map(|o| &o.destination)
    }

    pub fn destination_mut(&mut self) -> Option<&mut Destination> {
        self.output.as_mut().map(|o| &mut o.destination)
    }
}

/// Output IDs retain insertion order; validation rejects duplicates.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Route {
    #[default]
    Group,
    /// Ordered output destinations: Main and/or tracks in Project.outputs.
    Outputs(Vec<TrackId>),
}
impl Route {
    pub fn is_group(&self) -> bool {
        *self == Self::Group
    }
    pub fn single(id: TrackId) -> Self {
        Self::Outputs(vec![id])
    }
    pub fn empty() -> Self {
        Self::Outputs(Vec::new())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OscAddress {
    /// Slash-led suffix appended to the composed path, e.g. `/play`.
    Relative(String),
    /// Full slash-led address, ignoring every path.
    Absolute(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OscArgument {
    Int(i32),
    Float(f32),
    String(String),
    Bool(bool),
}

/// One hit: a timed play of an event defined on the clip's own track (a Target entry).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub position: BeatTime,
    pub definition: EventRef,
    /// Values only; their types must equal the definition's signature.
    #[serde(default)]
    pub arguments: Vec<OscArgument>,
}

/// One row of a clip's event editor: every hit of this event sits on it.
/// A struct, not a bare ref, so later lane kinds (parameter curves) and
/// presentation fields can join without a format break.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventLane {
    pub definition: EventRef,
}

impl OscAddress {
    pub fn text(&self) -> &str {
        let (Self::Relative(text) | Self::Absolute(text)) = self;
        text
    }

    /// Add a missing leading slash and validate, preserving the address kind.
    pub fn normalised(self) -> ModelResult<Self> {
        let path = match self {
            Self::Relative(text) => Self::Relative(lead_with_slash(text)),
            Self::Absolute(text) => Self::Absolute(lead_with_slash(text)),
        };
        path.validate()?;
        Ok(path)
    }

    /// Validate the literal address, whether relative to track paths or absolute.
    pub fn validate(&self) -> ModelResult {
        validate_address(self.text())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisation_preserves_kind_and_literal_slashes() {
        for (text, expected) in [
            ("cue", "/cue"),
            ("/cue", "/cue"),
            ("//cue/", "//cue/"),
            ("/", "/"),
        ] {
            for absolute in [false, true] {
                let address = |text: &str| {
                    if absolute {
                        OscAddress::Absolute(text.into())
                    } else {
                        OscAddress::Relative(text.into())
                    }
                };
                assert_eq!(address(text).text(), text);
                let normalised = address(text).normalised().unwrap();
                assert_eq!(normalised, address(expected));
                assert_eq!(normalised.clone().normalised().unwrap(), normalised);
            }
        }
    }

    #[test]
    fn normalisation_keeps_strict_validation_for_new_paths() {
        for text in ["", "a b", "/a*", "/a\0"] {
            assert!(OscAddress::Relative(text.into()).normalised().is_err());
            assert!(OscAddress::Absolute(text.into()).normalised().is_err());
        }
    }
}
