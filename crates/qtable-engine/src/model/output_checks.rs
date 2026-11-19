//! What an output track must look like: a track of kind Output
//! with a destination, living in `Project.outputs`, and nothing else living there.
use super::*;

impl Project {
    pub(super) fn validate_output_tracks(&self) -> ModelResult<()> {
        for track in &self.tracks {
            require(
                matches!(track.kind, TrackKind::Track | TrackKind::Group) && track.output.is_none(),
                "an output track belongs in the outputs list",
            )?;
        }
        for track in &self.outputs {
            require(
                track.is_output() && track.output.is_some(),
                "the outputs list holds only output tracks",
            )?;
            require(
                track.group.is_none() && !track.fold_state && track.slots.is_empty(),
                "an output holds no clips and joins no group",
            )?;
            require(track.route == Route::Group, "an output has no route")?;
            let Some(output) = &track.output else {
                continue;
            };
            validate_destination(&output.destination)?;
            output.stage.validate()?;
            require(
                track.delay_ms.is_finite() && track.delay_ms.abs() <= MAX_TRACK_DELAY_MS,
                "output latency must be between -1000 and 1000 ms",
            )?;
        }
        Ok(())
    }
}

pub(super) fn validate_destination(destination: &Destination) -> ModelResult {
    require(
        !destination.host.trim().is_empty() && !destination.host.contains('\0'),
        "output host must be non-empty and NUL-free",
    )?;
    require(destination.port > 0, "output port must be non-zero")
}
