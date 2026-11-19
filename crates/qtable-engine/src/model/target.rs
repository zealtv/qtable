//! Receiver event and parameter definitions, with stable device-local keys.
use super::*;
mod convert;
mod lookup;

pub use convert::signature_text;
pub use field::{Control, EnumOption, FieldDef, FieldKey, FieldType};
mod field;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntryKey(pub String);

impl EntryKey {
    pub fn validate(&self) -> ModelResult {
        require(
            !self.0.is_empty()
                && self.0 != DEVICE_ON
                && self.0.bytes().all(|c| {
                    c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-'
                }),
            "entry key must be lowercase [a-z0-9_-]+ and not device_on",
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TargetDevice {
    #[serde(skip_serializing_if = "Path::is_empty")]
    pub path: Path,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<EventDef>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ParameterDef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventDef {
    pub key: EntryKey,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub path: OscAddress,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldDef>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterDef {
    pub key: EntryKey,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub path: OscAddress,
    pub fields: Vec<FieldDef>,
    #[serde(default)]
    pub send_mode: SendMode,
}

impl ParameterDef {
    pub fn display_name(&self) -> &str {
        display_name(&self.name, &self.path)
    }
    pub fn field(&self, key: &FieldKey) -> Option<&FieldDef> {
        self.fields.iter().find(|f| &f.key == key)
    }
    /// The spec of the field at `position`, named as its face labels it: the parameter's own
    /// name when it has one field, else the field's ("X", or "Arg 2" when unnamed).
    pub fn field_spec(&self, position: usize) -> Option<ParameterSpec> {
        let field = self.fields.get(position)?;
        Some(if self.fields.len() == 1 {
            field.spec_named(self.display_name())
        } else {
            field.spec_named(&field.display_name(position))
        })
    }
    pub fn signature(&self) -> impl Iterator<Item = FieldType> + '_ {
        self.fields.iter().map(|f| f.value_type)
    }
    pub fn fresh_field_key(&self, name: &str) -> FieldKey {
        FieldKey::fresh(&self.fields, name)
    }
    pub fn validate(&self) -> ModelResult {
        self.validate_with(validate_address)
    }
    fn validate_with(&self, address: fn(&str) -> ModelResult) -> ModelResult {
        self.key.validate()?;
        address(self.path.text())?;
        FieldDef::validate_fields(&self.fields, true)
    }
}

fn display_name<'a>(name: &'a str, path: &'a OscAddress) -> &'a str {
    if name.is_empty() { path.text() } else { name }
}

impl EventDef {
    pub fn signature(&self) -> impl Iterator<Item = FieldType> + '_ {
        self.fields.iter().map(|field| field.value_type)
    }

    pub fn display_name(&self) -> &str {
        display_name(&self.name, &self.path)
    }

    /// The spec of the argument at `position`, named as a hit's argument reads.
    pub fn field_spec(&self, position: usize) -> Option<ParameterSpec> {
        let field = self.fields.get(position)?;
        Some(field.spec_named(&field.display_name(position)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventRef {
    pub device: DeviceId,
    pub key: EntryKey,
}

impl TargetDevice {
    /// The parameter a `ParameterKey` names. The key types differ (`EntryKey` is the stored one),
    /// so this is the one place that compares them.
    pub fn parameter(&self, key: &ParameterKey) -> Option<&ParameterDef> {
        self.parameters.iter().find(|p| p.key.0 == key.0)
    }

    pub fn parameter_mut(&mut self, key: &ParameterKey) -> Option<&mut ParameterDef> {
        self.parameters.iter_mut().find(|p| p.key.0 == key.0)
    }

    /// Last path part, normalized into a key unique across both entry lists.
    pub fn fresh_key(&self, path: &OscAddress) -> EntryKey {
        let base: String = path
            .text()
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let base = if base.is_empty() { "event" } else { &base };
        let mut candidate = base.to_owned();
        let mut suffix = 2;
        while candidate == DEVICE_ON
            || self
                .events
                .iter()
                .map(|e| &e.key)
                .chain(self.parameters.iter().map(|p| &p.key))
                .any(|key| key.0 == candidate)
        {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        EntryKey(candidate)
    }

    pub fn validate(&self) -> ModelResult {
        self.validate_with(validate_address)
    }

    pub(super) fn validate_with(&self, address: fn(&str) -> ModelResult) -> ModelResult {
        self.path.validate_with(address)?;
        let mut keys = HashSet::new();
        for key in self
            .events
            .iter()
            .map(|e| &e.key)
            .chain(self.parameters.iter().map(|p| &p.key))
        {
            key.validate()?;
            require(keys.insert(key), "duplicate Target entry key")?;
        }
        for event in &self.events {
            address(event.path.text())?;
            FieldDef::validate_fields(&event.fields, false)?;
        }
        for parameter in &self.parameters {
            parameter.validate_with(address)?;
        }
        Ok(())
    }
}

impl Track {
    /// The clip rules that need its track: every lane and hit plays an event defined on
    /// this track, each lane its own, and every hit matches its event's signature.
    pub(super) fn validate_clip(&self, clip: &Clip) -> ModelResult {
        let mut events = HashSet::new();
        let mut arguments = HashSet::new();
        for lane in &clip.lanes {
            match lane {
                Lane::Events(lane) => {
                    require(
                        self.event_def(&lane.definition).is_some(),
                        "a lane plays an event that isn't on its track",
                    )?;
                    require(
                        events.insert(&lane.definition),
                        "another lane already plays that event",
                    )?;
                }
                Lane::Envelope(envelope) => {
                    require(
                        arguments.insert(&envelope.target),
                        "another lane already automates that argument",
                    )?;
                    self.validate_envelope_values(&envelope.target, &envelope.points)?;
                }
            }
        }
        for event in &clip.events {
            let (_, definition) = self
                .event_def(&event.definition)
                .ok_or_else(|| ModelError("a hit plays an event that isn't on its track".into()))?;
            require(
                definition
                    .signature()
                    .eq(event.arguments.iter().map(FieldType::of)),
                "a hit's arguments must match its event's signature",
            )?;
        }
        Ok(())
    }

    pub(crate) fn validate_envelope_values(
        &self,
        target: &ArgumentRef,
        points: &[Breakpoint],
    ) -> ModelResult {
        let spec = self.argument_spec(target).ok_or_else(|| {
            ModelError("an envelope targets an argument that isn't on its track".into())
        })?;
        for point in points {
            require(
                spec.check(point.value).map_err(ModelError)? == point.value,
                "envelope values must fit the argument's steps",
            )?;
        }
        Ok(())
    }
}

impl Project {
    /// Seed each event field that holds no current value from its default, as a set saved
    /// before event fields held values does.
    pub fn seed_event_values(&mut self) {
        for track in &mut self.tracks {
            for device in &mut track.devices {
                let Some(target) = device.target_mut() else {
                    continue;
                };
                for field in target.events.iter_mut().flat_map(|e| &mut e.fields) {
                    field.seed_current();
                }
            }
        }
    }
}
