//! Stored device chains, separate from each track's Mixer (LOM `Track.devices`).
use super::*;

pub const DEVICE_ON: &str = "device_on";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Device {
    pub id: DeviceId,
    pub name: String,
    /// LOM "Device On". An inactive device sends nothing.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub active: bool,
    /// Saved view state; older sets open on the generated controls.
    #[serde(default, skip_serializing_if = "is_false")]
    pub expanded: bool,
    pub kind: DeviceKind,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn yes() -> bool {
    true
}
fn is_true(value: &bool) -> bool {
    *value
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum DeviceKind {
    Target(TargetDevice),
}

/// Left-to-right value-flow order; the Mixer follows the chain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeviceCategory {
    Control,
    Generator,
    Modulator,
    Target,
    Processor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceCapabilities {
    pub category: DeviceCategory,
    pub has_events: bool,
    pub has_parameters: bool,
    pub user_defined: bool,
}

impl Device {
    /// A fresh, active, expanded device of `kind`, named as the kind names itself.
    pub fn new(id: DeviceId, kind: DeviceKind) -> Self {
        Self {
            id,
            name: kind.default_name().into(),
            active: true,
            expanded: true,
            kind,
        }
    }

    /// The Target definition, or None for any other kind.
    pub fn target(&self) -> Option<&TargetDevice> {
        self.kind.target()
    }

    pub fn target_mut(&mut self) -> Option<&mut TargetDevice> {
        self.kind.target_mut()
    }
}

impl DeviceKind {
    /// The Target definition, or None for any other kind. The match stays exhaustive, so a
    /// new kind fails to compile here until it says what it answers.
    pub fn target(&self) -> Option<&TargetDevice> {
        match self {
            Self::Target(target) => Some(target),
        }
    }

    pub fn target_mut(&mut self) -> Option<&mut TargetDevice> {
        match self {
            Self::Target(target) => Some(target),
        }
    }

    pub fn capabilities(&self) -> DeviceCapabilities {
        match self {
            Self::Target(_) => DeviceCapabilities {
                category: DeviceCategory::Target,
                has_events: true,
                has_parameters: true,
                user_defined: true,
            },
        }
    }

    pub fn default_name(&self) -> &'static str {
        match self {
            Self::Target(_) => "Target",
        }
    }
}

/// The browser's catalogue key for a device kind: a data-free, `Copy` name for "add one of
/// these", carried by `ProjectEdit::AddDevice` and the browser's selection and drag state.
/// `DeviceKind` holds a definition, so it can't serve as that key; `kind()` is the one place
/// that turns a key into a kind, and `Device::new` the one that builds the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewDevice {
    Target,
}
impl NewDevice {
    pub fn kind(self) -> DeviceKind {
        match self {
            Self::Target => DeviceKind::Target(TargetDevice::default()),
        }
    }

    pub fn build(self, id: DeviceId) -> Device {
        Device::new(id, self.kind())
    }
}

impl Project {
    pub fn next_device_id(&self) -> DeviceId {
        DeviceId(
            self.tracks
                .iter()
                .chain(&self.outputs)
                .chain([&self.main])
                .flat_map(|track| &track.devices)
                .map(|device| device.id.0)
                .max()
                .unwrap_or(0)
                + 1,
        )
    }

    pub fn device(&self, id: DeviceId) -> Option<(&Track, &Device)> {
        self.tracks
            .iter()
            .chain(&self.outputs)
            .chain([&self.main])
            .find_map(|track| track.device(id).map(|device| (track, device)))
    }

    pub fn device_mut(&mut self, id: DeviceId) -> Option<&mut Device> {
        self.tracks
            .iter_mut()
            .chain(&mut self.outputs)
            .chain([&mut self.main])
            .flat_map(|track| &mut track.devices)
            .find(|device| device.id == id)
    }
}

impl Track {
    pub fn device(&self, id: DeviceId) -> Option<&Device> {
        self.devices.iter().find(|device| device.id == id)
    }

    pub fn device_mut(&mut self, id: DeviceId) -> Option<&mut Device> {
        self.devices.iter_mut().find(|device| device.id == id)
    }

    /// The Target device, parameter and field an argument names. None for Mixer Volume (the
    /// Mixer holds no Target) and for anything that doesn't resolve on this track.
    pub fn argument(&self, reference: &ArgumentRef) -> Option<(&Device, &ParameterDef, &FieldDef)> {
        let (device, parameter) = self.argument_parameter(reference)?;
        Some((device, parameter, parameter.field(&reference.field)?))
    }

    /// `argument` without the field: the Target device and parameter the reference names, for
    /// callers that carry a parameter whose field may be gone.
    pub fn argument_parameter(&self, reference: &ArgumentRef) -> Option<(&Device, &ParameterDef)> {
        let DeviceRef::Chain(id) = reference.device else {
            return None;
        };
        let device = self.device(id)?;
        Some((device, device.target()?.parameter(&reference.parameter)?))
    }

    pub fn device_index(&self, id: DeviceId) -> Option<usize> {
        self.devices.iter().position(|device| device.id == id)
    }

    /// Clamp a requested position into the category's run. None appends to the run.
    pub fn insert_index(&self, category: DeviceCategory, before: Option<DeviceId>) -> usize {
        let start = self
            .devices
            .partition_point(|d| d.kind.capabilities().category < category);
        let end = self
            .devices
            .partition_point(|d| d.kind.capabilities().category <= category);
        before
            .and_then(|id| self.device_index(id))
            .unwrap_or(end)
            .clamp(start, end)
    }

    /// The chain's Target devices with their definitions, in chain order.
    pub fn targets(&self) -> impl Iterator<Item = (&Device, &TargetDevice)> {
        self.devices
            .iter()
            .filter_map(|device| Some((device, device.target()?)))
    }

    pub(super) fn validate_devices_with(&self, address: fn(&str) -> ModelResult) -> ModelResult {
        require(
            self.devices.windows(2).all(|pair| {
                pair[0].kind.capabilities().category <= pair[1].kind.capabilities().category
            }),
            "device chain must be in category order",
        )?;
        for device in &self.devices {
            require(
                self.capabilities()
                    .accepts_device(device.kind.capabilities().category),
                "Targets require a Track with clips; Groups, Outputs and Main cannot host them",
            )?;
            if let Some(target) = device.target() {
                target.validate_with(address)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
