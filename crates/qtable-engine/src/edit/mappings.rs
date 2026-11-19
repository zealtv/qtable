//! Mapping-list edits and cleanup when a stable control identity disappears.
use super::EditReport;
use crate::model::*;

#[derive(Clone, Debug, PartialEq)]
pub enum MappingEdit {
    /// Replace this control's binding of the same source kind with a fresh row and learn
    /// defaults. MIDI and key bindings coexist; repeated sources on other controls fan out.
    Add {
        source: Source,
        control: ControlRef,
        min: f64,
        max: f64,
    },
    Remove {
        mapping: MappingId,
    },
    SetRange {
        mapping: MappingId,
        min: f64,
        max: f64,
    },
    SetMode {
        mapping: MappingId,
        mode: SourceMode,
    },
    SetTakeover {
        mapping: MappingId,
        takeover: Takeover,
    },
    /// A relative encoder's step, as a factor of the standard step (1.0). Validation keeps it
    /// between 0.01 and 10; a source that is not a relative CC keeps the value and ignores it.
    SetSensitivity {
        mapping: MappingId,
        sensitivity: f64,
    },
}

pub(super) fn apply(project: &mut Project, edit: MappingEdit) -> Result<EditReport, ModelError> {
    let mut report = EditReport::default();
    match edit {
        MappingEdit::Add {
            source,
            control,
            min,
            max,
        } => {
            let id = project.next_mapping_id()?;
            let before = project.mappings.len();
            project
                .mappings
                .retain(|row| row.control != control || row.source.kind() != source.kind());
            report.removed_mappings = before - project.mappings.len();
            project
                .mappings
                .push(RemoteMapping::new(id, source, control, min, max));
        }
        MappingEdit::Remove { mapping } => {
            get(project, mapping)?;
            project.mappings.retain(|row| row.id != mapping);
            report.removed_mappings = 1;
        }
        MappingEdit::SetRange { mapping, min, max } => {
            let row = get(project, mapping)?;
            row.min = min;
            row.max = max;
        }
        MappingEdit::SetMode { mapping, mode } => get(project, mapping)?.mode = mode,
        MappingEdit::SetTakeover { mapping, takeover } => {
            get(project, mapping)?.takeover = takeover
        }
        MappingEdit::SetSensitivity {
            mapping,
            sensitivity,
        } => get(project, mapping)?.sensitivity = sensitivity,
    }
    Ok(report)
}

fn get(project: &mut Project, id: MappingId) -> Result<&mut RemoteMapping, ModelError> {
    project
        .mappings
        .iter_mut()
        .find(|row| row.id == id)
        .ok_or_else(|| ModelError("mapping not found".into()))
}

/// The rows of `old` whose control existed there and no longer exists in `new`.
pub(crate) fn deleted_mappings(old: &Project, new: &Project) -> Vec<RemoteMapping> {
    old.mappings
        .iter()
        .filter(|row| {
            describe_control(old, &row.control).is_some()
                && describe_control(new, &row.control).is_none()
        })
        .cloned()
        .collect()
}

/// Only remove rows whose control existed before this edit. An invalid newly added row
/// must fail validation, never silently disappear and turn a rejected edit into success.
// Not yet decided (2026-11-06): preserve stable mappings on all surviving
// descendants; remove bindings only when their controls disappear (existing policy).
pub(crate) fn remove_deleted_controls(old: &Project, new: &mut Project, report: &mut EditReport) {
    let deleted: std::collections::HashSet<_> = deleted_mappings(old, new)
        .into_iter()
        .map(|row| row.id)
        .collect();
    let before = new.mappings.len();
    new.mappings.retain(|row| !deleted.contains(&row.id));
    report.removed_mappings += before - new.mappings.len();
}
