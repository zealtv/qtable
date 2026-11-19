//! What a `Track` is: one enum for the stored kind and the kind the views ask about.
//!
//! Live's `Track` class has one shape; qtable's kinds differ by flags and omissions, which
//! `TrackKind::capabilities` (`capabilities.rs`) answers in one place. A group track is a
//! track whose members name it as their `group_track`.
//! RON writes only the variant name, and `Track.kind` leaves a plain track out, so the saved
//! bytes stay as they were before `TrackKind` and `TrackKind` became one
//! (`tests/track_kind_bytes.rs` pins this).
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackKind {
    #[default]
    Track,
    Group,
    /// Lives in `Project.outputs`, never in `Project.tracks`.
    Output,
    /// Live's `master_track`, kept apart in `Project.main`.
    Main,
}

impl TrackKind {
    /// Serde's `skip_serializing_if` for `Track.kind`: a plain track writes no kind.
    pub fn is_track(&self) -> bool {
        *self == Self::Track
    }
}
