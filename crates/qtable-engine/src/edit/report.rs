//! What an edit adopted, removed or converted, for every client to report. None of
//! these can be undone yet (target-device design, "Undo"), so the status bar says what went.
use crate::model::FieldType;

/// Lane/content removal, definition adoption and value conformance across edits.
/// Refused edits return ModelError and leave the project untouched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditReport {
    /// Mapping rows deleted explicitly or with their controls.
    pub removed_mappings: usize,
    /// Lanes, declared or derived, removed explicitly, with definitions or by paste resolution.
    pub removed_lanes: usize,
    pub removed_hits: usize,
    /// Points discarded by lane deletion, paste trimming or duplicate resolution.
    pub removed_points: usize,
    /// Pasted lanes whose event or argument ref was adopted on the receiving track.
    /// Reusing a matching definition also counts; creating a definition is not required.
    pub adopted_lanes: usize,
    /// Clips whose own or incoming content lost a lane, hit or point.
    pub removed_in_clips: usize,
    /// Hits that had a value retyped or dropped. Appending a default changes no value.
    pub converted_hits: usize,
    /// Envelope points clamped, snapped or remapped by a definition edit.
    pub converted_points: usize,
    /// The signature every converted hit now matches; None when nothing converted, or when
    /// hits converted to more than one signature (a paste that conforms several events).
    pub signature: Option<Vec<FieldType>>,
    /// Set by `Command::Undo` and `Command::Redo`: the label of the step they applied, like
    /// "Rename Clip". None for every other command, and when there was nothing to undo or redo.
    pub history_step: Option<&'static str>,
}

impl EditReport {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub(super) fn removed(&mut self, lanes: usize, hits: usize) {
        if lanes + hits == 0 {
            return;
        }
        self.removed_lanes += lanes;
        self.removed_hits += hits;
        self.removed_in_clips += 1;
    }

    pub(super) fn converted(&mut self, hits: usize, signature: &[FieldType]) {
        if hits == 0 {
            return;
        }
        self.signature = if self.converted_hits == 0 || self.signature.as_deref() == Some(signature)
        {
            Some(signature.to_vec())
        } else {
            None
        };
        self.converted_hits += hits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversions_to_several_signatures_name_none() {
        let mut report = EditReport::default();
        report.converted(0, &[FieldType::Int]);
        assert!(report.is_empty());
        report.converted(2, &[FieldType::Int]);
        report.converted(1, &[FieldType::Int]);
        assert_eq!(report.signature, Some(vec![FieldType::Int]));
        report.converted(1, &[FieldType::Float]);
        report.converted(1, &[FieldType::Int]);
        assert_eq!((report.converted_hits, report.signature), (5, None));
    }
}
