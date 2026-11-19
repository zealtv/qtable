//! The engine's undo history: snapshots of the set, never inverse edits.
//!
//! Each step keeps the set as it stood on one side of a change, so restoring it puts back every
//! side effect of the change by construction. The history is
//! private to the engine; clients reach it through `Command::Undo`, `Command::Redo` and the
//! engine's read access.
//!
//! **Open step.** A step made by a command that carried a `Gesture` stays open to later commands
//! of the same gesture, but only while it is the top of the undo stack: a command joins the top
//! step when it carries that step's gesture. Any other step made above it (even of another
//! gesture), an undo, a redo, a load or a client's close ends it. So two controls moved in turn
//! make one step per move, undone in reverse order, as in Live (tour, 2026-11-11); one
//! control moved again with nothing else made between stays one step. The step keeps its label
//! and the snapshot it began with.
//!
//! **Mappings are outside the history** (2026-11-12). A mapping edit makes no step and
//! touches neither the open step nor the redo stack. Snapshots still hold the mappings of their
//! day, but an undo or redo never restores them (`engine/restore.rs` keeps the current ones).
//! The exception is a mapping that went with its control: the step whose change took it keeps
//! the row in `Step::dropped`, and restoring the step brings it back with the control (as of
//! 2026-11-12, Live brings the mapping back with a deleted track). The rows live
//! and die with the step; they are never part of the set, so a save never writes them.
use std::collections::VecDeque;
use std::sync::Arc;

use crate::engine::Gesture;
use crate::model::{Project, RemoteMapping};

/// The most steps kept. A new step beyond this drops the oldest.
pub(crate) const CAPACITY: usize = 200;

/// One step of the history.
///
/// On the undo stack, `before` is the set as it stood before the change and `before_id` its
/// content identity. When an undo moves the step to the redo stack, the same fields hold the set
/// the undo took back (the state a redo restores), so one type serves both stacks.
///
/// `gesture` is set while further commands may still join the step (see the module's open run).
/// A step on the redo stack, and any step an undo or redo landed, never carries one.
///
/// `dropped` holds the mappings that the change took away together with their control (an edit
/// deleting a mapped track, or an undo removing a track it had created). Restoring the step
/// gives them back along with the control, unless they conflict with a current mapping.
#[derive(Clone, Debug)]
pub(crate) struct Step {
    /// Names the change in the status bar, like "Rename Clip".
    pub label: &'static str,
    pub before: Arc<Project>,
    pub before_id: u64,
    pub gesture: Option<Gesture>,
    pub dropped: Vec<RemoteMapping>,
}

/// What `History::join` did with a command's gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Joined {
    /// The top step doesn't carry the gesture.
    No,
    /// The change belongs to the top step, which stays.
    Step,
    /// The change brought the set back to how the top step found it, so the step is gone;
    /// `before_id` is the content identity it began with. `mappings_differ` says the set's
    /// mappings changed meanwhile (they stay outside the history), so that identity no longer
    /// names the content.
    Vanished {
        before_id: u64,
        mappings_differ: bool,
    },
}

#[derive(Debug, Default)]
pub(crate) struct History {
    undo: VecDeque<Step>,
    redo: Vec<Step>,
}

impl History {
    /// A fresh change: it becomes the top of the undo stack and the redo stack is cleared,
    /// since a redo would replay a state that no longer follows from the set.
    pub fn record(&mut self, step: Step) {
        // A step with no gesture ends the open step beneath it. Undoing that step later must
        // not bring it back to life (a controller step, a switch with no token, an undo, then
        // the controller again once joined the old step).
        if step.gesture.is_none() {
            self.close_open_run();
        }
        self.redo.clear();
        self.push_undo(step);
    }

    /// Fold a change into the top step if it carries `gesture`. `after` is the set the change
    /// produced: a step now equal to how it began vanishes. Only the top step can take a change,
    /// so a step under another one stays closed even when its control moves again.
    pub fn join(&mut self, gesture: Gesture, after: &Project) -> Joined {
        let Some(top) = self.undo.back() else {
            return Joined::No;
        };
        if top.gesture != Some(gesture) {
            return Joined::No;
        }
        self.redo.clear();
        // Mappings are outside the history, so a mapping edit in between doesn't keep a step
        // alive that otherwise ended where it began.
        let mappings_differ = top.before.mappings != after.mappings;
        let same = if mappings_differ {
            let mut before = (*top.before).clone();
            before.mappings = after.mappings.clone();
            before == *after
        } else {
            *top.before == *after
        };
        if same {
            let step = self.undo.pop_back().expect("the top step found above");
            return Joined::Vanished {
                before_id: step.before_id,
                mappings_differ,
            };
        }
        Joined::Step
    }

    /// Add to the mappings the top step's change took away (a joined change dropped a control).
    pub fn extend_dropped(&mut self, rows: Vec<RemoteMapping>) {
        if let Some(top) = self.undo.back_mut() {
            top.dropped.extend(rows);
        }
    }

    /// End the open step: no step takes further commands. Every step loses its token, not only
    /// the top, so a step an undo uncovers can't be open again.
    pub fn close_open_run(&mut self) {
        for step in self.undo.iter_mut() {
            step.gesture = None;
        }
    }

    /// Put a step on the undo stack without touching the redo stack (a redo's own landing).
    pub fn push_undo(&mut self, step: Step) {
        self.undo.push_back(step);
        while self.undo.len() > CAPACITY {
            self.undo.pop_front();
        }
    }

    pub fn push_redo(&mut self, step: Step) {
        self.redo.push(step);
    }

    pub fn peek_undo(&self) -> Option<&Step> {
        self.undo.back()
    }

    pub fn peek_redo(&self) -> Option<&Step> {
        self.redo.last()
    }

    pub fn pop_undo(&mut self) -> Option<Step> {
        self.undo.pop_back()
    }

    pub fn pop_redo(&mut self) -> Option<Step> {
        self.redo.pop()
    }

    /// Loading a set starts a new history.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}
