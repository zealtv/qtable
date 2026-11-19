//! Edit helpers shared by the update loop.
use crate::app::App;
use crate::drafts::DraftKey;
use crate::grid;
use qtable_engine::edit::{EditReport, ProjectEdit};
use qtable_engine::engine::{Command, EngineError};
use qtable_engine::model::{BeatTime, FieldRef, TrackId};
use std::time::Duration;

/// The one place the GUI sends a command to the engine. It carries the receipt of the message
/// being handled and the undo gesture token (`gesture_steps`).
pub(crate) fn submit(app: &mut App, command: Command) -> Result<EditReport, EngineError> {
    let gesture = crate::gesture_steps::token_for(app, &command);
    let context = app
        .command_context
        .unwrap_or_else(|| {
            app.engine.command_context(qtable_engine::engine::Receipt {
                received_at: std::time::Instant::now(),
                sequence: None,
            })
        })
        .with_gesture(gesture);
    app.engine.execute_with_context(command, context)
}

pub(crate) fn execute(app: &mut App, command: Command) {
    let launched_scene = match &command {
        Command::LaunchScene { scene } => Some(*scene),
        _ => None,
    };
    let fired = match &command {
        Command::FireEvent { track, event } => Some((*track, event.clone())),
        _ => None,
    };
    if let Err(error) = submit(app, command) {
        app.status.show_error(error.to_string());
    } else {
        if let Some((track, event)) = fired {
            app.event_feedback
                .fired(track, event, std::time::Instant::now());
        }
        if let Some(scene) = launched_scene {
            app.scene_stop_feedback
                .launched(scene, std::time::Instant::now());
        }
        app.status.clear_transient();
        // Scheduler commands are asynchronous; poll promptly until their state arrives.
        app.fast_poll_until = Some(std::time::Instant::now() + Duration::from_millis(500));
    }
}

/// Creates a clip in an empty slot: one 4/4 bar (four beats), which the engine names and
/// colours from the track.
pub(crate) fn create_clip(app: &mut App, track: TrackId, slot: usize) -> bool {
    run_edit(
        app,
        Command::Edit(ProjectEdit::CreateClip {
            track,
            slot,
            // Four beats is a positive, representable clip length.
            length: BeatTime::from_beats(4).unwrap(),
        }),
    )
}

pub(crate) fn execute_edit(app: &mut App, command: Command) {
    run_edit(app, command);
}

/// Runs an edit and reports whether the engine accepted it.
pub(crate) fn run_edit(app: &mut App, command: Command) -> bool {
    run_edit_report(app, command, None).is_some()
}

/// Runs a synchronous edit/load command and handles its shared GUI bookkeeping.
/// A keyed draft is forgotten on success and retained with the engine error on rejection;
/// without a key, the caller retains control of draft lifetime.
pub(crate) fn run_edit_report(
    app: &mut App,
    command: Command,
    draft_key: Option<DraftKey>,
) -> Option<EditReport> {
    let submitted = parameter_fields(&command);
    match submit(app, command) {
        Ok(report) => {
            app.snapshot = app.engine.snapshot();
            // The scheduler applies a parameter edit after this snapshot: poll promptly for its
            // effect, and show the stored value of what was just sent until then.
            if !submitted.is_empty() {
                app.fast_poll_until = Some(std::time::Instant::now() + Duration::from_millis(500));
            }
            for field in &submitted {
                crate::parameter_gesture::submitted(app, field);
            }
            app.status.clear_transient();
            notify_conversions(app, &report);
            if let Some(key) = draft_key {
                app.drafts.forget(key);
            }
            Some(report)
        }
        Err(error) => {
            app.status.show_error(error.to_string());
            if let Some(key) = draft_key {
                app.drafts.errors.insert(key, error.to_string());
            }
            None
        }
    }
}

/// The arguments a parameter edit sends.
fn parameter_fields(command: &Command) -> Vec<FieldRef> {
    match command {
        Command::Edit(ProjectEdit::SetParameter { target, values }) => values
            .iter()
            .map(|(field, _)| FieldRef {
                parameter: target.clone(),
                field: field.clone(),
            })
            .collect(),
        Command::Edit(ProjectEdit::SetParameters(parameters)) => parameters
            .iter()
            .flat_map(|(target, values)| {
                values.iter().map(|(field, _)| FieldRef {
                    parameter: target.clone(),
                    field: field.clone(),
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Submits a name field: its draft is dropped only once the rename lands, so a rejected
/// name stays in the field for correction.
pub(crate) fn submit_name(app: &mut App, draft_key: DraftKey, edit: ProjectEdit) {
    if run_edit(app, Command::Edit(edit)) {
        app.drafts.forget(draft_key);
    }
}

/// Runs an edit that shifts what a draft key points at (event indices, clip slots), then
/// drops every draft and error, since they may now belong to a different entity.
pub(crate) fn execute_structural_edit(app: &mut App, command: Command) -> bool {
    let accepted = run_edit(app, command);
    if accepted {
        app.drafts.clear_inspector();
    }
    accepted
}

/// Clear unsubmitted inspector state on selection changes, retaining rejected edits
/// under their original field IDs so they remain available for correction.
pub(crate) fn clear_drafts(app: &mut App) {
    let rejected: Vec<_> = app
        .drafts
        .values
        .iter()
        .filter(|(key, _)| app.drafts.errors.contains_key(key))
        .map(|(key, value)| (*key, value.clone(), app.drafts.errors[key].clone()))
        .collect();
    app.drafts.clear_inspector();
    for (key, value, error) in rejected {
        app.drafts.values.insert(key, value);
        app.drafts.errors.insert(key, error);
    }
}

pub(crate) fn shifted_index(index: usize, delta: isize, len: usize) -> Option<usize> {
    let target = index.checked_add_signed(delta)?;
    (target < len).then_some(target)
}

pub(crate) fn move_clip_edit(
    source: grid::Selection,
    target: grid::Selection,
) -> Option<ProjectEdit> {
    (source != target).then_some(ProjectEdit::MoveClip {
        from: (source.track, source.slot),
        to: (target.track, target.slot),
    })
}

/// Reports values lost or retyped by an otherwise successful edit.
fn notify_conversions(app: &mut App, report: &EditReport) {
    if report.converted_hits == 0
        && report.converted_points == 0
        && report.adopted_lanes == 0
        && report.removed_lanes == 0
        && report.removed_hits == 0
        && report.removed_points == 0
    {
        return;
    }
    let hits = format!(
        "{} {}",
        report.converted_hits,
        if report.converted_hits == 1 {
            "hit"
        } else {
            "hits"
        }
    );
    let text = match &report.signature {
        Some(signature) => format!(
            "Converted {hits} to {}",
            qtable_engine::model::signature_text(signature)
        ),
        None => format!("Converted {hits}"),
    };
    let text = if report.converted_points > 0 {
        let points = format!(
            "{} envelope {}",
            report.converted_points,
            if report.converted_points == 1 {
                "point"
            } else {
                "points"
            }
        );
        if report.converted_hits == 0 {
            format!("Converted {points}")
        } else {
            format!("{text}; converted {points}")
        }
    } else {
        text
    };
    let mut parts = Vec::new();
    if report.converted_hits + report.converted_points > 0 {
        parts.push(text);
    }
    if report.adopted_lanes > 0 {
        parts.push(format!(
            "Adopted {} lane {}",
            report.adopted_lanes,
            if report.adopted_lanes == 1 {
                "reference"
            } else {
                "references"
            }
        ));
    }
    if report.removed_lanes + report.removed_hits + report.removed_points > 0 {
        parts.push(format!(
            "Removed {} {}, {} {}, {} envelope {}",
            report.removed_lanes,
            if report.removed_lanes == 1 {
                "lane"
            } else {
                "lanes"
            },
            report.removed_hits,
            if report.removed_hits == 1 {
                "hit"
            } else {
                "hits"
            },
            report.removed_points,
            if report.removed_points == 1 {
                "point"
            } else {
                "points"
            }
        ));
    }
    app.status
        .show(parts.join("; "), crate::status::Severity::Notice);
}

/// Reports the lanes and clips affected by removing a device or Target entry.
pub(crate) fn notify_removal(app: &mut App, report: &EditReport, name: &str) {
    let suffix = if report.removed_lanes + report.removed_hits > 0 {
        format!(
            " and {} {} in {} {}",
            report.removed_lanes,
            if report.removed_lanes == 1 {
                "lane"
            } else {
                "lanes"
            },
            report.removed_in_clips,
            if report.removed_in_clips == 1 {
                "clip"
            } else {
                "clips"
            }
        )
    } else {
        String::new()
    };
    app.status.show(
        format!("Removed {name}{suffix}"),
        crate::status::Severity::Notice,
    );
}

#[cfg(test)]
mod tests;
