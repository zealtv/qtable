//! Names for the undo history: what the status bar says an undo or redo took back.
use super::ProjectEdit;

impl ProjectEdit {
    /// A short imperative name for the edit, shown as "Undo: <label>".
    ///
    /// One exhaustive match with no catch-all, so a new edit can't ship without a name.
    pub fn label(&self) -> &'static str {
        match self {
            // Never shown: a mapping edit makes no undo step (`makes_step`).
            ProjectEdit::EditMapping(_) => "Edit Mapping",
            ProjectEdit::EditEnvelope { .. } => "Edit Envelope",
            ProjectEdit::AddDevice { .. } => "Add Device",
            ProjectEdit::PasteDevices { .. } => "Paste Devices",
            ProjectEdit::DuplicateDevices { .. } => "Duplicate Devices",
            ProjectEdit::MoveDevice { .. } => "Move Device",
            ProjectEdit::RemoveDevices { .. } => "Delete Devices",
            ProjectEdit::RenameDevice { .. } => "Rename Device",
            ProjectEdit::SetDeviceExpanded { .. } => "Expand Device",
            ProjectEdit::SetDevicesActive { .. } => "Set Device Activator",
            ProjectEdit::EditTarget { .. } => "Edit Target",
            ProjectEdit::SetParameter { .. } | ProjectEdit::SetParameters(_) => "Set Value",
            ProjectEdit::SetParameterOutput { .. } => "Set Parameter Output",
            ProjectEdit::SetTrackPath { .. } => "Set Track Path",
            ProjectEdit::AddOutput { .. } => "Add Output",
            ProjectEdit::SetDestination { .. } => "Set Destination",
            ProjectEdit::SetTrackDelay { .. } => "Set Track Delay",
            ProjectEdit::SetOutputStage { .. } => "Set Output Stage",
            ProjectEdit::SetTrackRoutes { .. } => "Set Routes",
            ProjectEdit::ToggleOutputInRoute { .. } => "Toggle Output Route",
            ProjectEdit::AddTrack { .. } => "Add Track",
            ProjectEdit::RemoveTracks { .. } => "Delete Tracks",
            ProjectEdit::RenameTrack { .. } => "Rename Track",
            ProjectEdit::RecolourTrack { .. } => "Recolour Track",
            ProjectEdit::MoveTrack { .. }
            | ProjectEdit::PlaceTrack { .. }
            | ProjectEdit::PlaceTracks { .. } => "Move Tracks",
            ProjectEdit::PasteTracks { .. } => "Paste Tracks",
            ProjectEdit::CreateGroup { .. } => "Group Tracks",
            ProjectEdit::Ungroup { .. } | ProjectEdit::UngroupTracks { .. } => "Ungroup Tracks",
            ProjectEdit::SetGroupFolded { .. } => "Fold Group",
            ProjectEdit::AddScene { .. } => "Add Scene",
            ProjectEdit::RemoveScene { .. } | ProjectEdit::RemoveScenes { .. } => "Delete Scenes",
            ProjectEdit::RenameScene { .. } => "Rename Scene",
            ProjectEdit::SetSceneFollowActions { .. } => "Set Scene Follow Actions",
            ProjectEdit::MoveScenes { .. } | ProjectEdit::MoveScene { .. } => "Move Scenes",
            ProjectEdit::CreateClip { .. } => "Create Clip",
            ProjectEdit::DeleteClip { .. } => "Delete Clip",
            ProjectEdit::DuplicateClip { .. } => "Duplicate Clip",
            ProjectEdit::PasteClip { .. } => "Paste Clip",
            ProjectEdit::MoveClip { .. } => "Move Clip",
            ProjectEdit::RecolourClip { .. } => "Recolour Clip",
            ProjectEdit::RenameClip { .. } => "Rename Clip",
            ProjectEdit::SetClipMarkers { .. } => "Set Clip Markers",
            ProjectEdit::SetClipLength { .. } => "Set Clip Length",
            ProjectEdit::SetClipLooping { .. } => "Set Clip Loop",
            ProjectEdit::SetClipLegato { .. } => "Set Clip Legato",
            ProjectEdit::SetClipQuantisation { .. } => "Set Clip Quantisation",
            ProjectEdit::SetClipFollowActions { .. } => "Set Clip Follow Actions",
            ProjectEdit::AddEvent { .. } => "Add Hit",
            ProjectEdit::RemoveEvent { .. } => "Delete Hit",
            ProjectEdit::UpdateEvent { .. } => "Edit Hit",
            ProjectEdit::EditLane { .. } => "Edit Lane",
            ProjectEdit::SetSlotStopButton { .. } => "Set Stop Button",
            ProjectEdit::SetActive { .. } => "Set Activator",
            ProjectEdit::SetOutputSolo { .. } | ProjectEdit::SetSolo { .. } => "Set Solo",
        }
    }

    /// Whether the edit makes an undo step. Mapping edits don't: Live keeps them out of its
    /// history (decided 2026-11-12), and an undo or redo leaves the mappings as they
    /// stand. View-state edits don't either.
    pub fn makes_step(&self) -> bool {
        !self.is_view_state() && !matches!(self, ProjectEdit::EditMapping(_))
    }

    /// Whether the edit changes only what is shown, never what plays: a group's fold or a
    /// device's expanded flag. These make no undo step, and an undo carries them over.
    pub fn is_view_state(&self) -> bool {
        matches!(
            self,
            ProjectEdit::SetGroupFolded { .. } | ProjectEdit::SetDeviceExpanded { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_and_view_state_edits_make_no_step() {
        use crate::edit::MappingEdit;
        use crate::model::{DeviceId, MappingId, TrackId};
        assert!(
            !ProjectEdit::EditMapping(MappingEdit::Remove {
                mapping: MappingId(1)
            })
            .makes_step()
        );
        assert!(
            !ProjectEdit::SetDeviceExpanded {
                device: DeviceId(1),
                expanded: true
            }
            .makes_step()
        );
        assert!(
            ProjectEdit::RenameTrack {
                track: TrackId(1),
                name: String::new()
            }
            .makes_step()
        );
    }

    #[test]
    fn view_state_edits_are_the_fold_and_expand_edits_only() {
        use crate::model::{DeviceId, TrackId};
        assert!(
            ProjectEdit::SetGroupFolded {
                group: TrackId(1),
                folded: true
            }
            .is_view_state()
        );
        assert!(
            ProjectEdit::SetDeviceExpanded {
                device: DeviceId(1),
                expanded: true
            }
            .is_view_state()
        );
        assert!(
            !ProjectEdit::RenameTrack {
                track: TrackId(1),
                name: String::new()
            }
            .is_view_state()
        );
    }
}
