//! Unsaved typing: the drafts a person has started and not yet committed, on one path.
//!
//! Four things ask the same question about them: a click elsewhere (`update`'s outer wrapper),
//! `Message::CommitField`, the save and settings shortcuts (`shortcuts::commit_typing`) and
//! undo, which drops them (`undo::update`). Each used to keep its own list of draft kinds, so a
//! kind added to one list and missed in another let Cmd-S save without the value on screen.
//! `Draft` is the one list: `unsaved` asks whether any kind is open, and `commit` commits the
//! open ones, so a new kind has to say both how it is open (`is_open`) and how it commits
//! (`commit`), or the code does not compile. `typing/tests.rs` opens every kind in `Draft::ALL`
//! and checks both.
//!
//! Whether a commit was rejected is read from the errors the commits reported to the status
//! line (`StatusLine::errors_reported`), never from what the bar shows: a persistent scheduler
//! failure keeps new messages off the bar but not off that count.
use crate::app::App;
use crate::drafts::DraftKey;

/// Declares `Draft` and `Draft::ALL` together, so a kind cannot be in one and not the other.
/// `ALL` is also the commit order.
macro_rules! draft_kinds {
    ($($(#[$doc:meta])* $kind:ident),+ $(,)?) => {
        /// One kind of draft that a click elsewhere, Tab or a save commits.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) enum Draft {
            $($(#[$doc])* $kind),+
        }

        impl Draft {
            /// Every kind, in the order they commit.
            pub(crate) const ALL: &'static [Draft] = &[$(Draft::$kind),+];
        }
    };
}

draft_kinds! {
    /// A device's name being edited in the Detail View.
    DeviceRename,
    /// A value typed on a Target face.
    TargetFace,
    /// The pending field: any text field that has typing the set doesn't hold yet.
    Field,
    /// A clip's name being edited in the grid.
    ClipRename,
    /// A track's or scene's name being edited in the grid.
    HeaderRename,
}

impl Draft {
    /// Whether the person has typing of this kind that the set does not hold yet.
    pub(crate) fn is_open(self, app: &App) -> bool {
        match self {
            Self::DeviceRename => app.device_chain.renaming.is_some(),
            Self::TargetFace => app.device_chain.face.exact.is_some(),
            Self::Field => app.pending_field.is_some(),
            Self::ClipRename => app.clip_actions.renaming.is_some(),
            Self::HeaderRename => app.header_actions.renaming.is_some(),
        }
    }

    /// Hands the draft to its owner, which turns it into an edit. The owner reports a rejection
    /// to the status line.
    fn commit(self, app: &mut App) {
        match self {
            Self::DeviceRename => crate::device_chain::commit_rename(app),
            Self::TargetFace => crate::device_chain::target_face::commit_exact(app),
            Self::Field => crate::field_focus::commit(app),
            Self::ClipRename => crate::clip_actions::commit_draft(app),
            Self::HeaderRename => crate::header_actions::commit_draft(app),
        }
    }
}

/// Whether any draft is open: a pending field, a rename in progress or a value typed on a
/// Target face.
pub(crate) fn unsaved(app: &App) -> bool {
    Draft::ALL.iter().any(|kind| kind.is_open(app))
}

/// How a `commit` went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Outcome {
    rejected: bool,
    rejected_field: Option<DraftKey>,
}

impl Outcome {
    /// False if the engine rejected any of the drafts. (A rename that is rejected closes its
    /// editor and keeps no draft.)
    pub(crate) fn all_committed(&self) -> bool {
        !self.rejected
    }

    /// The pending field the engine rejected. Its error is in `drafts.errors`; the draft itself
    /// is no longer pending, so a caller that wants the field to keep refusing puts it back.
    pub(crate) fn rejected_field(&self) -> Option<DraftKey> {
        self.rejected_field
    }
}

/// Commits the open drafts among `kinds`, in the order of `Draft::ALL`, each as a history step
/// of its own: typing and the click or save that follows it stay two steps.
pub(crate) fn commit(app: &mut App, kinds: &[Draft]) -> Outcome {
    let reported = app.status.errors_reported();
    let field = app.pending_field;
    for kind in Draft::ALL {
        if kinds.contains(kind) && kind.is_open(app) {
            crate::gesture_steps::isolated(app, |app| kind.commit(app));
        }
    }
    let rejected = app.status.errors_reported() != reported;
    let mut rejected_field = None;
    if kinds.contains(&Draft::Field)
        && let Some(key) = field
    {
        if rejected && app.drafts.errors.contains_key(&key) {
            rejected_field = Some(key);
        } else if !rejected {
            // `field_focus::commit` records whatever message the bar shows as the field's error
            // when nothing changed. With no error reported, that is some earlier message.
            app.drafts.errors.remove(&key);
        }
    }
    Outcome {
        rejected,
        rejected_field,
    }
}

/// Commits every open draft, as Cmd-S and Cmd-, do.
pub(crate) fn commit_all(app: &mut App) -> Outcome {
    commit(app, Draft::ALL)
}

#[cfg(test)]
mod tests;
