//! Parameter controllers share clip address and route resolution, and the track valve: a muted
//! or soloed-away source sends no parameter OSC (2026-10-22).
use super::*;

impl Project {
    /// Where an enabled parameter is configured to send, whatever its source's Activator and
    /// Solo: validation and notices read this, so muting never changes what a set may hold.
    pub fn parameter_recipients(&self, target: &ParameterRef) -> Vec<&Track> {
        if !self.parameter(target).is_some_and(|p| p.enabled) {
            return Vec::new();
        }
        self.parameter_outputs(target.track)
    }

    /// Where the parameter sends now: its recipients while its source's valve is open.
    pub fn sending_parameter_recipients(&self, target: &ParameterRef) -> Vec<&Track> {
        if !self.source_sends(target.track) {
            return Vec::new();
        }
        self.parameter_recipients(target)
    }

    /// The destination rule shared by parameter slots and exact envelope jumps. Device
    /// enablement and the source valve belong to slot eligibility, so a voice's jump cursors
    /// keep advancing while either is off, and reopening resumes without a backlog.
    pub(crate) fn parameter_outputs(&self, source: TrackId) -> Vec<&Track> {
        let Some(track) = self.track(source) else {
            return Vec::new();
        };
        if track.capabilities().destination {
            return if track.accepts_sends() {
                vec![track]
            } else {
                Vec::new()
            };
        }
        self.routed_outputs(source)
            .filter(|output| output.mixer.active)
            .collect()
    }
    pub fn parameter_address(&self, target: &ParameterRef, output: TrackId) -> Option<String> {
        let parameter = self.parameter(target)?;
        let piece = match target.device {
            DeviceRef::Mixer => OscAddress::Relative(parameter.output.address.clone()),
            DeviceRef::Chain(id) => {
                let device = self.track(target.track)?.device(id)?;
                let definition = device.target()?;
                let entry = definition.parameter(&target.key)?;
                match &entry.path {
                    OscAddress::Absolute(_) => entry.path.clone(),
                    OscAddress::Relative(_) => {
                        OscAddress::Relative(resolve_address(definition.path.as_str(), &entry.path))
                    }
                }
            }
        };
        self.resolve_event_address(target.track, output, &piece)
    }
    pub fn parameter_delay_ms(&self, target: &ParameterRef, output: TrackId) -> f64 {
        self.send_delay_ms(target.track, output)
    }
}
