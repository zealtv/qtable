//! Track Activator and Solo, with Live 12's rules for who sends.
//!
//! Sources: Live 12 manual 18.1/18.3/18.6, a test in Live 12 and an AbletonOSC run, 2026-10-12.
//! Both flags are stored and never rewrite each other, so unsolo restores the
//! earlier state. A silenced track keeps playing; it only stops sending.
use super::*;

/// The mixer switches shared by tracks and groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MixerState {
    /// Live's Track Activator: off mutes the track (or every member of a group).
    pub active: bool,
    pub solo: bool,
}

impl Default for MixerState {
    fn default() -> Self {
        Self {
            active: true,
            solo: false,
        }
    }
}

/// How a Solo button draws: a group with only some soloed members shows a
/// half-coloured button (Live 12 manual 18.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoloLook {
    Off,
    Partial,
    On,
}

impl Project {
    /// Every strip in Live's track order: storage order, a group track just before its
    /// members. Live numbers tracks in this order, group tracks and folded members
    /// included, and F1–F8 follow it (a Live 12 set, 2026-10-12: "1 Group",
    /// "2 MIDI", "3 Analog").
    pub fn strips(&self) -> Vec<TrackId> {
        self.tracks.iter().map(|t| t.id).collect()
    }

    /// True while any track, group track or receiver selects tracks through Solo.
    pub fn any_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.mixer.solo) || self.outputs.iter().any(|o| o.mixer.solo)
    }

    /// Whether the track's events leave the engine, from the Activator and Solo rules.
    pub fn track_sends(&self, id: TrackId) -> bool {
        let Some(track) = self.tracks.iter().find(|t| t.id == id) else {
            return false;
        };
        // Inactive means silent, soloed or not: the manual's reading of a
        // conflicting MIDI observation (rule 8 of the Live solo and mute table).
        if !track.mixer.active
            || self
                .ancestors(id)
                .filter_map(Ancestor::group)
                .any(|g| !g.mixer.active)
        {
            return false;
        }
        if track.mixer.solo || !self.any_solo() {
            return true;
        }
        // A soloed group or receiver plays its members, or only its explicitly
        // soloed members when it has some. For receivers this is qtable's own rule,
        // inferred from the group analogy; Live's return buses weren't measured.
        self.ancestors(id)
            .filter_map(Ancestor::group)
            .any(|g| g.mixer.solo && !self.group_has_soloed_member(g))
            || self.routed_outputs(id).any(|o| {
                o.mixer.solo
                    && !self
                        .clip_tracks()
                        .any(|t| t.mixer.solo && self.resolved_route(t.id).contains(&o.id))
            })
    }

    /// Whether a source's own switches let its OSC out: `track_sends` for tracks and groups.
    /// An output's own Activator gates it as a destination instead, and Main has neither.
    pub fn source_sends(&self, id: TrackId) -> bool {
        self.track(id)
            .is_some_and(|track| track.capabilities().destination)
            || self.track_sends(id)
    }

    pub(super) fn group_has_soloed_member(&self, group: &Track) -> bool {
        self.tracks
            .iter()
            .any(|t| t.mixer.solo && self.is_descendant(t.id, group.id))
    }

    pub fn is_descendant(&self, track: TrackId, group: TrackId) -> bool {
        self.ancestors(track)
            .filter_map(Ancestor::group)
            .any(|g| g.id == group)
    }

    /// Enabled outputs this track sends to now: empty while it is silenced.
    pub fn sending_outputs(&self, id: TrackId) -> impl Iterator<Item = &Track> {
        let sends = self.track_sends(id);
        self.routed_outputs(id)
            .filter(move |o| sends && o.accepts_sends())
    }
}
