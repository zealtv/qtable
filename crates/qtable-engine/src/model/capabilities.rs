//! What each kind of track can do, answered in one place. Widgets and edits ask a capability
//! question instead of matching the kind, so a new kind breaks the build here and nowhere else.
use super::{DeviceCategory, TrackKind};

/// Where a kind's columns sit: with the tracks, with the outputs on their right, or Main.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Tracks,
    Outputs,
    Main,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub region: Region,
    /// Has clip slots that can hold clips.
    pub holds_clips: bool,
    /// Has members that fold away behind its own column.
    pub foldable: bool,
    /// May be a member of a group.
    pub groupable: bool,
    /// Has an output picker: sends to a set of outputs.
    pub routes: bool,
    /// Is a destination: host and port.
    pub destination: bool,
    /// Its destination may be unset (no host): it then sends nothing and keeps its other settings.
    pub destination_optional: bool,
    /// Has Activator and Solo switches.
    pub switched: bool,
    /// Live numbers it ("1 Group", "2 MIDI").
    pub numbered: bool,
    pub removable: bool,
    /// Can be dragged to a new place within its region.
    pub movable: bool,
    pub renamable: bool,
    /// Can be copied and pasted as a column.
    pub copyable: bool,
    pub recolourable: bool,
    /// Has a mixer volume: the strip shows a fader and the Detail View a Volume field.
    pub has_volume: bool,
    /// Has a chain of devices, separate from its Mixer.
    pub device_chain: bool,
    /// Accepts Targets, whose events need clips on their own track.
    pub target_devices: bool,
}

impl Capabilities {
    pub fn accepts_device(self, category: DeviceCategory) -> bool {
        self.device_chain && (category != DeviceCategory::Target || self.target_devices)
    }
}

impl TrackKind {
    /// The one exhaustive match over the kinds.
    pub const fn capabilities(self) -> Capabilities {
        let track = Capabilities {
            region: Region::Tracks,
            holds_clips: true,
            foldable: false,
            groupable: true,
            routes: true,
            destination: false,
            destination_optional: false,
            switched: true,
            numbered: true,
            removable: true,
            movable: true,
            renamable: true,
            copyable: true,
            recolourable: true,
            has_volume: true,
            device_chain: true,
            target_devices: true,
        };
        match self {
            Self::Track => track,
            Self::Group => Capabilities {
                target_devices: false,
                holds_clips: false,
                foldable: true,
                ..track
            },
            Self::Output => Capabilities {
                region: Region::Outputs,
                target_devices: false,
                holds_clips: false,
                groupable: false,
                routes: false,
                destination: true,
                numbered: false,
                ..track
            },
            // Decided 2026-10-13: Main needs neither rename nor recolour.
            Self::Main => Capabilities {
                region: Region::Main,
                destination: true,
                destination_optional: true,
                switched: false,
                target_devices: false,
                holds_clips: false,
                groupable: false,
                routes: false,
                numbered: false,
                removable: false,
                movable: false,
                renamable: false,
                copyable: false,
                recolourable: false,
                ..track
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_differs_by_omission() {
        let c = |k: TrackKind| k.capabilities();
        assert!(c(TrackKind::Track).holds_clips && c(TrackKind::Track).groupable);
        assert!(c(TrackKind::Group).foldable && !c(TrackKind::Group).holds_clips);
        assert!(c(TrackKind::Group).routes && c(TrackKind::Group).numbered);
        let out = c(TrackKind::Output);
        assert_eq!(out.region, Region::Outputs);
        assert!(out.destination && !out.destination_optional && !out.routes && !out.numbered);
        assert!(out.removable && out.movable && out.renamable && out.recolourable);
        let main = c(TrackKind::Main);
        assert_eq!(main.region, Region::Main);
        assert!(main.destination_optional && main.has_volume && c(TrackKind::Track).has_volume);
        assert!(!main.removable && !main.movable && !main.renamable && !main.recolourable);
        assert!(!main.copyable);
        for kind in [TrackKind::Track, TrackKind::Group, TrackKind::Output] {
            assert!(c(kind).copyable);
        }
    }
}
