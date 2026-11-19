//! Which edits count as one undo step.
//!
//! The GUI keeps one current `Gesture` token. Every edit the GUI sends carries it (from
//! `edit_actions::submit`, the one place), so everything the user does between two input
//! boundaries joins one step: a whole drag, a held arrow key, several edits from one click.
//! A boundary replaces the token. When a span that made an edit ends, the GUI also closes
//! the engine's open run, so a controller's sweep that follows starts a new step.
//!
//! **Which events are boundaries.** iced delivers a widget's messages for an event before the
//! subscription's message for the same event. A
//! press or a key press is therefore seen only after the widget has answered it, and an edit
//! the press itself causes (a held key's first nudge, a toggle that fires on press) already
//! went out with the token that was current. Replacing the token at a press would then put
//! that edit and the drag's or the held key's later edits in different steps. So the
//! boundaries are the events that *end* a span: a pointer release (any button, or a touch
//! lifted), a key release and the window losing focus. The token a release makes serves the
//! whole next span, press to release, and nothing in it splits.
//!
//! **Remote input** keeps its own tokens, one per mapped control that takes continuous input
//! (`Remote`), for as long as the set stays loaded.
use crate::app::App;
use qtable_engine::engine::{Command, Gesture};
use qtable_engine::model::Source;
use std::collections::HashMap;

pub(crate) struct State {
    /// The token every GUI edit carries until the next boundary.
    token: Gesture,
    /// How many step-making commands the GUI has sent; a change over a span means it edited.
    edits: u64,
    /// Edits sent since the engine's open run was last closed.
    unclosed: bool,
    /// Whether a pointer button (or a finger) is down, from the subscription's press and
    /// release messages. A key release during a press ends nothing: the drag carries on.
    pointer_down: bool,
    /// What the remote path's command in flight carries: `Some(token)` while it executes.
    /// A GUI edit uses `token` instead.
    remote: Option<Option<Gesture>>,
    /// One token per mapped control taking continuous input, kept while the set stays loaded.
    controls: HashMap<Source, Gesture>,
}

impl State {
    pub(crate) fn new(token: Gesture) -> Self {
        Self {
            token,
            edits: 0,
            unclosed: false,
            pointer_down: false,
            remote: None,
            controls: HashMap::new(),
        }
    }
}

/// Whether this message is the subscription's report of an event that ends a span.
///
/// A modifier's release is no boundary (letting go of Shift first in a fine drag, or of Cmd
/// after a Cmd-Z, ends no span), and while a pointer button is down no key release is one
/// either: the drag is still going.
pub(crate) fn ends_span(app: &App, message: &crate::messages::Message) -> bool {
    use crate::messages::Message;
    match message {
        Message::Browser(crate::browser::Action::Release)
        | Message::CheckFieldFocus
        | Message::ReleaseKeys => true,
        Message::KeyReleased(key) => !is_modifier(key) && !app.gesture.pointer_down,
        _ => false,
    }
}

fn is_modifier(key: &iced::keyboard::Key) -> bool {
    use iced::keyboard::{Key, key::Named};
    matches!(
        key,
        Key::Named(
            Named::Shift
                | Named::Control
                | Named::Alt
                | Named::AltGraph
                | Named::Super
                | Named::Meta
                | Named::Hyper
                | Named::Fn
        )
    )
}

/// Track the pointer from the subscription's messages. Called for every message, after the
/// boundary decision, so a release's own boundary still sees the button as it was.
pub(crate) fn track_pointer(app: &mut App, message: &crate::messages::Message) {
    use crate::messages::Message;
    match message {
        Message::PointerPressed => app.gesture.pointer_down = true,
        // The pointer's releases, and the window losing focus (a release may never come).
        Message::Browser(crate::browser::Action::Release)
        | Message::CheckFieldFocus
        | Message::ReleaseKeys => app.gesture.pointer_down = false,
        _ => {}
    }
}

/// An input boundary: the next edit takes a fresh token, and if the span that just ended made
/// an edit, the engine's open run closes.
pub(crate) fn boundary(app: &mut App) {
    if app.gesture.unclosed {
        app.engine.close_gestures();
        app.gesture.unclosed = false;
    }
    app.gesture.token = app.engine.gesture();
}

/// Run a commit that happens because the user moved on (a click elsewhere, Tab, Enter): its
/// edits take a token of their own, so typing and the click that followed stay two steps.
///
/// A number box's own steps (a drag, a nudge, a reset) are not that: they send
/// `Message::CommitNumber`, which commits inside the current span. Sending them through here
/// made every step of a drag an undo step of its own (found
/// on the v3.8 tour, round 1).
pub(crate) fn isolated<R>(app: &mut App, run: impl FnOnce(&mut App) -> R) -> R {
    let saved = app.gesture.token;
    let edits = app.gesture.edits;
    app.gesture.token = app.engine.gesture();
    let result = run(app);
    // What follows (the click's own edit) must not join what was just committed.
    app.gesture.token = if app.gesture.edits == edits {
        saved
    } else {
        app.engine.gesture()
    };
    result
}

/// The token for a command about to run, and the bookkeeping for sending it.
pub(crate) fn token_for(app: &mut App, command: &Command) -> Option<Gesture> {
    if let Some(remote) = app.gesture.remote {
        return remote;
    }
    if makes_step(command) {
        app.gesture.edits += 1;
        app.gesture.unclosed = true;
    }
    Some(app.gesture.token)
}

/// Commands that can make a history step; a launch or a stop never does.
fn makes_step(command: &Command) -> bool {
    match command {
        // A fold or an expand makes no step, so it closes no run either.
        Command::Edit(edit) => !edit.is_view_state(),
        Command::SetTempo(_)
        | Command::SetGlobalQuantisation(_)
        | Command::SetFollowActionsEnabled(_) => true,
        _ => false,
    }
}

/// Remote input: run `run` with edits carrying the token of `control` (or none). One token per
/// control for as long as the set stays loaded; a control with none yet gets one.
pub(crate) fn remote<R>(
    app: &mut App,
    control: Option<&Source>,
    run: impl FnOnce(&mut App) -> R,
) -> R {
    let token = control.map(|source| {
        *app.gesture
            .controls
            .entry(source.clone())
            .or_insert_with(|| app.engine.gesture())
    });
    let previous = app.gesture.remote.replace(token);
    let result = run(app);
    app.gesture.remote = previous;
    result
}

/// A set was loaded: no token from before names anything now.
pub(crate) fn reset(app: &mut App) {
    app.gesture.controls.clear();
    app.gesture.unclosed = false;
    app.gesture.token = app.engine.gesture();
}

/// How many mapped controls hold a token.
#[cfg(test)]
pub(crate) fn control_tokens(app: &App) -> usize {
    app.gesture.controls.len()
}
