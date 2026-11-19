//! One read-only, Track-shaped view over tracks, groups and outputs.
//!
//! Live's Object Model has a single `Track` class whose kinds differ by flags and
//! missing children (2026-10-12). Storage stays three structs;
//! this view gives renderers and the control layer one shape to read.
use super::*;

/// What a column shows, whatever it stores. Output maps `enabled` to `mixer.active`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackFacts<'a> {
    pub kind: TrackKind,
    pub name: &'a str,
    pub colour: Colour,
    pub mixer: MixerState,
    pub solo_look: SoloLook,
    /// Track Delay in ms; None for an output, which has a latency instead.
    pub delay_ms: Option<f64>,
    /// None for an output: it is a destination, not a router.
    pub route: Option<&'a Route>,
    /// Live's track number from one; None for an output.
    pub number: Option<usize>,
    pub capabilities: Capabilities,
    pub is_grouped: bool,
    pub group_track: Option<TrackId>,
    /// A group's own folded flag; false for everything else.
    pub fold_state: bool,
    /// False for a member of a folded group, as in Live.
    pub is_visible: bool,
}

impl Project {
    /// What a stored track can do, by its kind; None for an unknown id.
    pub fn track_capabilities(&self, id: TrackId) -> Option<Capabilities> {
        self.track(id).map(Track::capabilities)
    }

    /// One Track-shaped view of a stored track, group track or output.
    pub fn track_facts(&self, id: TrackId) -> Option<TrackFacts<'_>> {
        let t = self.track(id)?;
        let kind = t.kind;
        let group = self.group_of(id);
        let solo_look = if t.mixer.solo {
            SoloLook::On
        } else if t.is_group() && self.group_has_soloed_member(t) {
            SoloLook::Partial
        } else {
            SoloLook::Off
        };
        // An output stands apart from Live's track numbers, and its delay is a latency.
        let output = t.is_output();
        Some(TrackFacts {
            kind,
            name: &t.name,
            colour: t.colour,
            mixer: t.mixer,
            solo_look,
            delay_ms: (!output).then_some(t.delay_ms),
            route: t.capabilities().routes.then_some(&t.route),
            number: self.strips().iter().position(|s| *s == id).map(|i| i + 1),
            capabilities: kind.capabilities(),
            is_grouped: group.is_some(),
            group_track: group.map(|g| g.id),
            fold_state: t.fold_state,
            is_visible: !self
                .ancestors(id)
                .filter_map(Ancestor::group)
                .any(|g| g.fold_state),
        })
    }
}
