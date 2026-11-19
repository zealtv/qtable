//! Activator and Solo edits. Several strips change together, as when Live's
//! switches are pressed with several tracks selected (Live 12 manual 18.1).
use super::get_track;
use crate::model::{ModelError, Project, TrackId};

/// Refuse the whole list when any strip lacks the switches, before changing any of them.
fn require_switched(project: &mut Project, tracks: &[TrackId]) -> Result<(), ModelError> {
    for &id in tracks {
        if !get_track(project, id)?.capabilities().switched {
            return Err(ModelError("this track has no Activator or Solo".into()));
        }
    }
    Ok(())
}

/// An output's Activator is its `enabled` switch: its track's `mixer.active`.
pub(super) fn set_active(
    project: &mut Project,
    tracks: &[TrackId],
    active: bool,
) -> Result<(), ModelError> {
    require_switched(project, tracks)?;
    for &id in tracks {
        get_track(project, id)?.mixer.active = active;
    }
    Ok(())
}

/// `exclusive` is Live's plain click with Exclusive Solo on: soloing clears every
/// other solo. The members of a soloed group keep theirs, so a group can be
/// soloed over a partial member solo (live-solo-mute.md rule 11; inferred).
/// Over AbletonOSC, Live's own solo setter is not exclusive (rule 5).
pub(super) fn set_solo(
    project: &mut Project,
    tracks: &[TrackId],
    solo: bool,
    exclusive: bool,
) -> Result<(), ModelError> {
    require_switched(project, tracks)?;
    for &id in tracks {
        get_track(project, id)?.mixer.solo = solo;
    }
    if solo && exclusive {
        for output in &mut project.outputs {
            output.mixer.solo &= tracks.contains(&output.id);
        }
        let kept: Vec<_> = project
            .tracks
            .iter()
            .filter(|t| {
                tracks
                    .iter()
                    .any(|group| project.is_descendant(t.id, *group))
            })
            .map(|t| t.id)
            .collect();
        for track in &mut project.tracks {
            let kept_member = kept.contains(&track.id);
            if !tracks.contains(&track.id) && !kept_member {
                track.mixer.solo = false;
            }
        }
    }
    Ok(())
}

/// Receiver solo is a virtual group of its resolved routed tracks.
pub(super) fn set_output_solo(
    project: &mut Project,
    output: TrackId,
    solo: bool,
    exclusive: bool,
) -> Result<(), ModelError> {
    let receiver = project
        .outputs
        .iter_mut()
        .find(|o| o.id == output)
        .ok_or_else(|| ModelError("unknown output".into()))?;
    receiver.mixer.solo = solo;
    if solo && exclusive {
        let members: Vec<_> = project
            .clip_tracks()
            .filter(|t| project.resolved_route(t.id).contains(&output))
            .map(|t| t.id)
            .collect();
        for track in &mut project.tracks {
            if !members.contains(&track.id) {
                track.mixer.solo = false;
            }
        }
        for receiver in &mut project.outputs {
            if receiver.id != output {
                receiver.mixer.solo = false;
            }
        }
    }
    Ok(())
}
