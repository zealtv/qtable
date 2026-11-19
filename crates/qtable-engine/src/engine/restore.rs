//! Undo and redo: put a snapshot back and tell the scheduler as an ordinary edit would.
//!
//! The scheduler learns of a restore through the commands it already has:
//!
//! 1. `SetTempo`, because a project update doesn't re-time the clock.
//! 2. A full `UpdateProject` holding the restored set, with the *current* stored value kept for
//!    every argument that exists on both sides with an unchanged definition. `automation::sync`
//!    keeps an old override value across a full update, so a stored value that changed must not
//!    travel this way (`override_tests` pins that reading).
//! 3. The value-only `UpdateParameters` for those arguments, which reaches receivers as a manual
//!    change and overrides any envelope that drives it, as a hand edit does.
use super::*;
use crate::history::Step;
use crate::model::{FieldRef, FieldView, RemoteMapping, describe_control};

#[derive(Clone, Copy)]
pub(super) enum Direction {
    Undo,
    Redo,
}

impl Engine {
    pub(super) fn step_history(&mut self, direction: Direction) -> Result<EditReport, EngineError> {
        let step = match direction {
            Direction::Undo => self.history.peek_undo(),
            Direction::Redo => self.history.peek_redo(),
        };
        let Some(step) = step else {
            return Ok(EditReport::default());
        };
        let label = step.label;
        let mut restored = (*step.before).clone();
        let mut restored_id = step.before_id;
        let brought_back = step.dropped.clone();
        // An undo or a redo ends the open run: a step it uncovers takes no more commands.
        self.history.close_open_run();
        let (carried, dropped) = carry_outside_history(&self.project, &mut restored, brought_back);
        if carried {
            // The file would differ from the snapshot's, so this is a state of its own.
            restored_id = self.fresh_content_id();
        }
        self.send_restore(&restored)?;
        let step = match direction {
            Direction::Undo => self.history.pop_undo(),
            Direction::Redo => self.history.pop_redo(),
        }
        .expect("the step peeked above");
        debug_assert_eq!(step.label, label);
        let taken_back = Step {
            label,
            before: Arc::new(std::mem::replace(&mut self.project, restored)),
            before_id: self.content_id,
            gesture: None,
            dropped,
        };
        match direction {
            Direction::Undo => self.history.push_redo(taken_back),
            Direction::Redo => self.history.push_undo(taken_back),
        }
        self.content_id = restored_id;
        self.bump_revision();
        Ok(EditReport {
            history_step: Some(label),
            ..EditReport::default()
        })
    }

    fn send_restore(&self, restored: &Project) -> Result<(), EngineError> {
        let current = &self.project;
        let tempo_changed = restored.tempo != current.tempo;
        if tempo_changed {
            self.send(scheduler::SchedulerCommand::SetTempo(restored.tempo))?;
        }
        let (mut kept, moved) = keep_current_values(current, restored);
        // `SetTempo` keeps the scheduler's own copy of the tempo in step, so a restore that
        // differs only in tempo needs no full update.
        let tempo = std::mem::replace(&mut kept.tempo, current.tempo);
        let differs = kept != *current;
        kept.tempo = tempo;
        if differs {
            self.send(scheduler::SchedulerCommand::UpdateProject(kept))?;
        }
        if !moved.is_empty() {
            self.send(scheduler::SchedulerCommand::UpdateParameters(ValueEdit {
                project: restored.clone(),
                fields: moved,
            }))?;
        }
        Ok(())
    }

    fn send(&self, command: scheduler::SchedulerCommand) -> Result<(), EngineError> {
        self.scheduler.send(command).map_err(EngineError::Scheduler)
    }
}

/// Give `restored` what the history doesn't hold: the fold of every group and the expanded flag
/// of every device that also exists in `current`, and the mappings (2026-11-12: mapping
/// isn't part of the undo steps). Returns whether that changed anything, and the mappings
/// the restore dropped with their control, for the step that takes this one back.
fn carry_outside_history(
    current: &Project,
    restored: &mut Project,
    brought_back: Vec<RemoteMapping>,
) -> (bool, Vec<RemoteMapping>) {
    let (mut changed, dropped) = carry_mappings(current, restored, brought_back);
    let strips = restored
        .tracks
        .iter_mut()
        .chain(restored.outputs.iter_mut())
        .chain(std::iter::once(&mut restored.main));
    for strip in strips {
        if strip.is_group()
            && let Some(now) = current.track(strip.id).filter(|t| t.is_group())
            && strip.fold_state != now.fold_state
        {
            strip.fold_state = now.fold_state;
            changed = true;
        }
        for device in &mut strip.devices {
            if let Some((_, now)) = current.device(device.id)
                && device.expanded != now.expanded
            {
                device.expanded = now.expanded;
                changed = true;
            }
        }
    }
    (changed, dropped)
}

/// The restored set takes the current mappings, less those whose control the restore removes
/// (as when an edit deletes the control; they are returned for the step that undoes this
/// one), plus `brought_back`: the mappings that went with their control when the step's change
/// was made, now that the control is back.
///
/// Only a cheap guard decides a clash (2026-11-12): a row is skipped when the control
/// already holds a binding of its source kind (the rule `MappingEdit::Add` and
/// `validate_mappings` enforce) or a current mapping holds its id. The same source on other
/// controls fans out, so it isn't one.
fn carry_mappings(
    current: &Project,
    restored: &mut Project,
    brought_back: Vec<RemoteMapping>,
) -> (bool, Vec<RemoteMapping>) {
    let snapshot = std::mem::replace(&mut restored.mappings, current.mappings.clone());
    let dropped = crate::edit::deleted_mappings(current, restored);
    crate::edit::remove_deleted_controls(current, restored, &mut EditReport::default());
    let mut returned = false;
    for row in brought_back {
        let held = restored.mappings.iter().any(|m| {
            m.id == row.id || (m.control == row.control && m.source.kind() == row.source.kind())
        });
        if held || describe_control(restored, &row.control).is_none() {
            continue;
        }
        restored.mappings.push(row);
        returned = true;
    }
    if returned {
        // `Add` appends with the next id, so the list stays in id order.
        restored.mappings.sort_by_key(|m| m.id.0);
    }
    (restored.mappings != snapshot, dropped)
}

/// `restored` with the current stored value kept for every argument whose definition is the
/// same on both sides, and every argument whose restored value still has to be sent by the
/// value path: those it replaced, and those whose definition changed.
fn keep_current_values(current: &Project, restored: &Project) -> (Project, Vec<FieldRef>) {
    let mut kept = restored.clone();
    let mut moved = Vec::new();
    let strips = restored
        .tracks
        .iter()
        .chain(&restored.outputs)
        .chain(std::iter::once(&restored.main));
    for field in strips.flat_map(|strip| strip.fields()) {
        let (Some(now), Some(then)) = (current.field(&field), restored.field(&field)) else {
            continue;
        };
        if now.value == then.value {
            continue;
        }
        if !same_definition(&now, &then) {
            // `kept` already holds the restored value. The full update alone would let `sync`
            // carry the old override (clamped into the restored definition) across it, so the
            // value still goes down the value path.
            moved.push(field);
            continue;
        }
        let set = kept
            .set_parameter_values(&field.parameter, vec![(field.field.clone(), now.value)])
            .is_ok();
        if set && kept.field(&field).is_some_and(|f| f.value == now.value) {
            moved.push(field);
        } else if set {
            // The value snapped to something else; give the restored one back.
            let _ = kept.set_parameter_values(&field.parameter, vec![(field.field, then.value)]);
        }
    }
    (kept, moved)
}

/// Everything about an argument except its stored value.
fn same_definition(a: &FieldView<'_>, b: &FieldView<'_>) -> bool {
    a.spec == b.spec
        && a.value_type == b.value_type
        && a.control == b.control
        && a.options == b.options
        && a.threshold == b.threshold
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ControlRef, MappingId, MidiMessage, MidiSource, Source, Track, TrackId};

    fn volume() -> ControlRef {
        ControlRef::Argument(FieldRef::volume(TrackId(1)))
    }

    fn row(id: u64, number: u8) -> RemoteMapping {
        let source = Source::Midi(MidiSource {
            channel: 1,
            message: MidiMessage::Cc(number),
        });
        RemoteMapping::new(MappingId(id), source, volume(), 0.0, 1.0)
    }

    fn scene() -> crate::model::Scene {
        crate::model::Scene {
            id: crate::model::SceneId(1),
            name: "Scene".into(),
            follow_actions: Default::default(),
        }
    }

    fn set() -> Project {
        Project {
            tracks: vec![Track::new(TrackId(1), "T1", 1)],
            scenes: vec![scene()],
            ..Project::default()
        }
    }

    /// The guard: one binding per control and source kind, as `MappingEdit::Add` and
    /// `validate_mappings` have it. The current mapping wins and the old row stays gone.
    #[test]
    fn a_brought_back_row_yields_to_the_current_binding_of_its_control_and_source_kind() {
        let mut current = set();
        current.mappings.push(row(2, 5));
        let mut restored = set();
        let (_, dropped) = carry_mappings(&current, &mut restored, vec![row(1, 9)]);
        assert_eq!(restored.mappings, vec![row(2, 5)]);
        assert!(dropped.is_empty());
        restored.validate().unwrap();
    }

    #[test]
    fn a_row_whose_control_stays_absent_stays_gone() {
        let current = set();
        let mut restored = set();
        restored.tracks.clear();
        restored.tracks.push(Track::new(TrackId(7), "T7", 1));
        carry_mappings(&current, &mut restored, vec![row(1, 9)]);
        assert!(restored.mappings.is_empty());
    }
}
