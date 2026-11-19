use super::*;
use std::borrow::Cow;

/// Stable device-local key; never an index into a reordered parameter list.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParameterKey(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceRef {
    Mixer,
    Chain(DeviceId),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterRef {
    pub track: TrackId,
    pub device: DeviceRef,
    pub key: ParameterKey,
}
impl ParameterRef {
    pub fn volume(track: TrackId) -> Self {
        Self {
            track,
            device: DeviceRef::Mixer,
            key: ParameterKey(VOLUME_PARAMETER.into()),
        }
    }

    /// Whether this is Mixer Volume.
    pub fn is_volume(&self) -> bool {
        self.device == DeviceRef::Mixer && self.key.0 == VOLUME_PARAMETER
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldRef {
    pub parameter: ParameterRef,
    pub field: FieldKey,
}
impl FieldRef {
    /// Mixer Volume's one argument on `track`.
    pub fn volume(track: TrackId) -> Self {
        Self {
            parameter: ParameterRef::volume(track),
            field: FieldKey(VOLUME_FIELD.into()),
        }
    }

    /// Whether this is Mixer Volume's argument.
    pub fn is_volume(&self) -> bool {
        self.parameter.is_volume() && self.field.0 == VOLUME_FIELD
    }
}

pub struct ParameterView<'a> {
    pub fields: Vec<FieldView<'a>>,
    pub output: Cow<'a, ParameterOutput>,
    pub enabled: bool,
}
pub struct FieldView<'a> {
    pub key: Cow<'a, FieldKey>,
    pub spec: Cow<'a, ParameterSpec>,
    pub value: f64,
    pub value_type: FieldType,
    pub control: Control,
    pub options: &'a [EnumOption],
    pub threshold: f64,
}
impl Track {
    /// All parameters declared by this track's devices, in display order.
    pub fn parameters(&self) -> impl Iterator<Item = ParameterRef> {
        self.targets()
            .flat_map(move |(device, target)| {
                target.parameters.iter().map(move |parameter| ParameterRef {
                    track: self.id,
                    device: DeviceRef::Chain(device.id),
                    key: ParameterKey(parameter.key.0.clone()),
                })
            })
            .chain(std::iter::once(ParameterRef::volume(self.id)))
    }
    pub fn fields(&self) -> impl Iterator<Item = FieldRef> + '_ {
        self.targets()
            .flat_map(move |(device, target)| {
                target.parameters.iter().flat_map(move |p| {
                    p.fields.iter().map(move |f| FieldRef {
                        parameter: ParameterRef {
                            track: self.id,
                            device: DeviceRef::Chain(device.id),
                            key: ParameterKey(p.key.0.clone()),
                        },
                        field: f.key.clone(),
                    })
                })
            })
            .chain(std::iter::once(FieldRef::volume(self.id)))
    }
}
impl Project {
    pub fn parameter(&self, target: &ParameterRef) -> Option<ParameterView<'_>> {
        self.parameter_near(target, None)
    }
    /// Where a Target parameter sits in its device's definition, for `parameter_near`.
    pub fn parameter_position(&self, target: &ParameterRef) -> Option<usize> {
        let DeviceRef::Chain(id) = target.device else {
            return None;
        };
        self.track(target.track)?
            .device(id)?
            .target()?
            .parameters
            .iter()
            .position(|p| p.key.0 == target.key.0)
    }
    /// `parameter`, trying the definition at `position` before scanning every key.
    pub fn parameter_near(
        &self,
        target: &ParameterRef,
        position: Option<usize>,
    ) -> Option<ParameterView<'_>> {
        if let DeviceRef::Chain(id) = target.device {
            let device = self.track(target.track)?.device(id)?;
            let definition = device.target()?;
            let parameter = position
                .and_then(|position| definition.parameters.get(position))
                .filter(|p| p.key.0 == target.key.0)
                .or_else(|| definition.parameter(&target.key))?;
            let fields = parameter
                .fields
                .iter()
                .enumerate()
                .map(|(position, f)| {
                    let spec = parameter.field_spec(position).expect("position of a field");
                    FieldView {
                        key: Cow::Borrowed(&f.key),
                        spec: Cow::Owned(spec),
                        value: f.value.unwrap(),
                        value_type: f.value_type,
                        control: f.control,
                        options: &f.options,
                        threshold: f.effective_threshold(),
                    }
                })
                .collect();
            return Some(ParameterView {
                fields,
                output: Cow::Owned(ParameterOutput {
                    enabled: device.active,
                    send_mode: parameter.send_mode,
                    address: resolve_address(definition.path.as_str(), &parameter.path),
                    ..ParameterOutput::default()
                }),
                enabled: device.active,
            });
        }
        let (spec, stored) = self.resolve_parameter(target)?;
        Some(ParameterView {
            fields: vec![FieldView {
                key: Cow::Owned(FieldKey(VOLUME_FIELD.into())),
                spec: Cow::Borrowed(spec),
                value: stored.value,
                value_type: FieldType::Float,
                control: Control::Slider,
                options: &[],
                threshold: stored.output.threshold,
            }],
            output: Cow::Borrowed(&stored.output),
            enabled: stored.output.enabled,
        })
    }
    /// Whether the argument exists, without constructing its parameter's display/output view.
    pub fn contains_field(&self, target: &FieldRef) -> bool {
        let Some(track) = self.track(target.parameter.track) else {
            return false;
        };
        match target.parameter.device {
            DeviceRef::Mixer => target.is_volume(),
            DeviceRef::Chain(id) => track
                .device(id)
                .and_then(Device::target)
                .and_then(|definition| definition.parameter(&target.parameter.key))
                .is_some_and(|p| p.fields.iter().any(|f| f.key == target.field)),
        }
    }
    pub fn field(&self, target: &FieldRef) -> Option<FieldView<'_>> {
        self.parameter(&target.parameter)?
            .fields
            .into_iter()
            .find(|f| *f.key == target.field)
    }
    fn resolve_parameter(
        &self,
        target: &ParameterRef,
    ) -> Option<(&ParameterSpec, &StoredParameter)> {
        match target.device {
            DeviceRef::Mixer if target.is_volume() => {
                Some((&VOLUME, &self.track(target.track)?.mixer_device.volume))
            }
            _ => None,
        }
    }
    pub(crate) fn parameter_mut(&mut self, target: &ParameterRef) -> Option<&mut StoredParameter> {
        match target.device {
            DeviceRef::Mixer if target.is_volume() => {
                Some(&mut self.track_mut(target.track)?.mixer_device.volume)
            }
            _ => None,
        }
    }
    pub(crate) fn set_parameter_values(
        &mut self,
        target: &ParameterRef,
        values: Vec<(FieldKey, f64)>,
    ) -> ModelResult {
        self.set_parameters_values(std::iter::once((target.clone(), values)))
    }
    /// Check and snap the complete batch before changing any stored value.
    pub(crate) fn set_parameters_values(
        &mut self,
        parameters: impl IntoIterator<Item = (ParameterRef, Vec<(FieldKey, f64)>)>,
    ) -> ModelResult {
        let mut keys = HashSet::new();
        let mut checked_parameters = Vec::new();
        for (target, values) in parameters {
            let view = self
                .parameter(&target)
                .ok_or_else(|| ModelError("parameter not found".into()))?;
            require(!values.is_empty(), "no arguments supplied")?;
            let mut checked = Vec::with_capacity(values.len());
            for (key, value) in values {
                require(
                    keys.insert(FieldRef {
                        parameter: target.clone(),
                        field: key.clone(),
                    }),
                    "duplicate argument in SetParameter",
                )?;
                let field = view
                    .fields
                    .iter()
                    .find(|f| *f.key == key)
                    .ok_or_else(|| ModelError("argument not found".into()))?;
                checked.push((key, field.spec.check(value).map_err(ModelError)?));
            }
            checked_parameters.push((target, checked));
        }
        require(!checked_parameters.is_empty(), "no parameters supplied")?;
        for (target, checked) in checked_parameters {
            self.write_parameter_values(&target, checked);
        }
        Ok(())
    }
    /// Only called after every identity and value in a batch has been checked.
    fn write_parameter_values(&mut self, target: &ParameterRef, checked: Vec<(FieldKey, f64)>) {
        match target.device {
            DeviceRef::Mixer => self.parameter_mut(target).unwrap().value = checked[0].1,
            DeviceRef::Chain(id) => {
                let parameter = self
                    .track_mut(target.track)
                    .and_then(|track| track.device_mut(id))
                    .and_then(Device::target_mut)
                    .and_then(|definition| definition.parameter_mut(&target.key))
                    .unwrap();
                for (key, value) in checked {
                    parameter
                        .fields
                        .iter_mut()
                        .find(|f| f.key == key)
                        .unwrap()
                        .value = Some(value);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
