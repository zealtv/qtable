//! Cmd-Z and Cmd-Shift-Z over the engine's history (undo design, sections 8 and 9).
//!
//! One keyboard path: `key` recognises the chord and `update` runs it. The shortcut works
//! wherever the focus is. Unsaved typing is dropped as Escape drops it, never committed, so it
//! cannot land on top of the state the undo restores. After a step, `reset_after_history` is the
//! one place that drops the GUI state that may no longer match the set.
use crate::app::App;
use crate::messages::Message;
use crate::status::Severity;
use iced::Task;
use iced::keyboard::{Key, Modifiers};
use qtable_engine::engine::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Undo,
    Redo,
}

impl Step {
    fn command(self) -> Command {
        match self {
            Self::Undo => Command::Undo,
            Self::Redo => Command::Redo,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Undo => "Undo",
            Self::Redo => "Redo",
        }
    }
}

/// Cmd-Z undoes and Cmd-Shift-Z redoes (Ctrl on Linux and Windows, which also redo with Ctrl-Y).
pub(crate) fn key(key: &Key, modifiers: Modifiers) -> Option<Step> {
    match key.as_ref() {
        Key::Character("z" | "Z") if modifiers == Modifiers::COMMAND => Some(Step::Undo),
        Key::Character("z" | "Z") if modifiers == Modifiers::COMMAND | Modifiers::SHIFT => {
            Some(Step::Redo)
        }
        Key::Character("y" | "Y")
            if cfg!(not(target_os = "macos")) && modifiers == Modifiers::COMMAND =>
        {
            Some(Step::Redo)
        }
        _ => None,
    }
}

/// Runs one step. Nothing to undo or redo changes nothing and shows nothing, apart from
/// dropping unsaved typing and releasing its field.
pub(crate) fn update(app: &mut App, step: Step) -> Task<Message> {
    let mut release = Task::none();
    if crate::typing::unsaved(app) {
        // As Escape: the draft is dropped, never committed.
        let _ = crate::update::update(app, Message::CancelField);
        release = iced::advanced::widget::operate(
            iced::advanced::widget::operation::focusable::unfocus(),
        );
    }
    let available = match step {
        Step::Undo => app.engine.can_undo(),
        Step::Redo => app.engine.can_redo(),
    };
    if !available {
        return release;
    }
    let Some(report) = crate::edit_actions::run_edit_report(app, step.command(), None) else {
        return release;
    };
    // A report from undo or redo is not `is_empty()`; the step's label says what happened.
    if let Some(label) = report.history_step {
        reset_after_history(app);
        // The scheduler applies the restored values after this snapshot: poll promptly.
        app.fast_poll_until = Some(Instant::now() + Duration::from_millis(500));
        app.status
            .show(format!("{}: {label}", step.name()), Severity::Notice);
    }
    release
}

/// Drops what may no longer match the set after an undo or redo: drafts and field errors, On
/// Release previews, the open popover and any rename in progress. Keeps the selection where its
/// ids still resolve, and keeps scroll, zoom and clipboards. Called from `update` only.
pub(crate) fn reset_after_history(app: &mut App) {
    app.drafts = Default::default();
    app.pending_field = None;
    app.tempo_input = crate::fields::format_tempo(app.project().tempo.bpm());
    app.parameter_gesture.clear_held();
    crate::device_chain::target_face::reset_face(app);
    app.close_overlay();
    app.clip_actions.renaming = None;
    app.header_actions.renaming = None;
    app.device_chain.renaming = None;
    app.selection.reconcile_tracks(app.engine.project());
}

#[cfg(test)]
mod tests;
