//! One home for a parameter gesture on any automatable argument, keyed by the model's
//! `FieldRef`: the Mixer's Volume fader and the Target face's Slider and Dial both use it.
//!
//! The gesture decides only *when* its panel submits `SetParameter`: each drag step in
//! Continuous and Every Frame, and only on release in On Release, where the drag is held here
//! as a preview. A double click (`Control::default_to`) publishes the default as a release.
//! The gesture never decides automation override; the engine does, on the edit it receives
//! (`parameter-curves-and-model`, section 3).
use crate::app::App;
use qtable_engine::model::{DeviceRef, FieldRef, SendMode};
use qtable_engine::scheduler::AutomationValue;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long a submitted field shows its stored value while the engine's published automation
/// catches up: the worker publishes at most every 33 ms, and the fast poll lasts 500 ms.
const SUBMITTED_FOR: Duration = Duration::from_millis(250);

/// The On Release drags in progress: the value each shows, not yet sent, and the fields the
/// GUI submitted a moment ago. GUI session state.
#[derive(Debug, Default)]
pub(crate) struct State {
    held: HashMap<FieldRef, f64>,
    submitted: HashMap<FieldRef, Instant>,
}

impl State {
    pub(crate) fn held(&self, field: &FieldRef) -> Option<f64> {
        self.held.get(field).copied()
    }

    /// Shows `value` for the field without sending it.
    pub(crate) fn hold(&mut self, field: &FieldRef, value: f64) {
        self.held.insert(field.clone(), value);
    }

    /// Ends every hold without sending: the set they previewed has changed under them.
    pub(crate) fn clear_held(&mut self) {
        self.held.clear();
    }

    /// Ends the field's hold, if any, without sending.
    pub(crate) fn end(&mut self, field: &FieldRef) {
        self.held.remove(field);
    }

    /// Notes that the GUI just sent `field` to the engine, whose published value lags the edit.
    pub(crate) fn submitted(&mut self, field: &FieldRef, at: Instant) {
        self.submitted
            .retain(|_, sent| at.duration_since(*sent) < SUBMITTED_FOR);
        self.submitted.insert(field.clone(), at);
    }

    /// Whether the GUI submitted `field` within the last moments before `now`.
    pub(crate) fn just_submitted(&self, field: &FieldRef, now: Instant) -> bool {
        self.submitted
            .get(field)
            .is_some_and(|sent| now.saturating_duration_since(*sent) < SUBMITTED_FOR)
    }

    /// Ends every Target device's hold: the face that showed them was reset.
    pub(crate) fn end_chain(&mut self) {
        self.held
            .retain(|field, _| !matches!(field.parameter.device, DeviceRef::Chain(_)));
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// The fields held on Target devices.
    pub(crate) fn chain_fields(&self) -> impl Iterator<Item = &FieldRef> {
        self.held
            .keys()
            .filter(|field| matches!(field.parameter.device, DeviceRef::Chain(_)))
    }
}

/// Whether the argument follows a clip envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutomationState {
    Following,
    Overridden,
    None,
}

/// What the engine reports for a driven argument: its effective value (the curve while
/// following, or the curve's last after its clip stopped) and how that value relates to the
/// control. The one reading of `AutomationValue`: a held value drives nothing, so it maps to
/// `AutomationState::None` here and nowhere else.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Effective {
    pub value: f64,
    pub state: AutomationState,
}

impl Effective {
    pub(crate) fn of(reported: &AutomationValue) -> Self {
        use qtable_engine::scheduler::AutomationState as Engine;
        Self {
            value: reported.value,
            state: match reported.state {
                Engine::Following => AutomationState::Following,
                Engine::Overridden => AutomationState::Overridden,
                Engine::Held => AutomationState::None,
            },
        }
    }

    /// A manual change has overridden the envelope.
    pub(crate) fn overridden(&self) -> bool {
        self.state == AutomationState::Overridden
    }
}

/// The engine's runtime effective value of `field` among the published automation, which
/// runs independently of project edits.
pub(crate) fn effective_in(automation: &[AutomationValue], field: &FieldRef) -> Option<Effective> {
    automation
        .iter()
        .find(|value| value.field == *field)
        .map(Effective::of)
}

fn effective(app: &App, field: &FieldRef) -> Option<Effective> {
    effective_in(&app.snapshot.automation, field)
}

/// Whether any argument is overridden, in the published automation.
pub(crate) fn any_overridden(automation: &[AutomationValue]) -> bool {
    automation
        .iter()
        .any(|value| Effective::of(value).overridden())
}

/// One drag step. In On Release it holds the value and returns `None`; otherwise it ends any
/// hold and returns the value for the panel to submit, unless it would change nothing.
pub(crate) fn drag(app: &mut App, field: &FieldRef, value: f64) -> Option<f64> {
    if on_release(app, field) {
        app.parameter_gesture.hold(field, value);
        None
    } else {
        app.parameter_gesture.end(field);
        changes(app, field, value).then_some(value)
    }
}

/// The gesture ended at `value`: a mouse release, a window blur, or the double-click reset
/// (`Control::default_to` publishes the default as a release). Ends the hold and returns the
/// value to submit, once, if it changes the model or what the engine drives.
pub(crate) fn release(app: &mut App, field: &FieldRef, value: f64) -> Option<f64> {
    app.parameter_gesture.end(field);
    changes(app, field, value).then_some(value)
}

/// Whether submitting `value` replaces something: the stored value, or an effective value the
/// engine drives (a curve, or the curve's last value held after a stop) that differs from it,
/// even when the stored base already equals `value`.
fn changes(app: &App, field: &FieldRef, value: f64) -> bool {
    app.project()
        .field(field)
        .is_some_and(|stored| stored.value != value)
        || effective(app, field).is_some_and(|current| current.value != value)
}

/// Notes that the GUI submitted `field`, so its control shows the stored value until the
/// engine's published automation reflects the edit.
pub(crate) fn submitted(app: &mut App, field: &FieldRef) {
    app.parameter_gesture.submitted(field, Instant::now());
}

/// What the control shows: the drag preview (while the argument sends On Release), else the
/// stored value while the argument is overridden or the GUI just submitted it (the engine's
/// published value lags a manual edit), else the engine's effective value, else the model's
/// stored value. `None` when the field no longer exists.
pub(crate) fn shown_value(app: &App, field: &FieldRef) -> Option<f64> {
    shown_from(app, field, Instant::now(), effective)
}

fn shown_from(
    app: &App,
    field: &FieldRef,
    now: Instant,
    source: impl FnOnce(&App, &FieldRef) -> Option<Effective>,
) -> Option<f64> {
    // A hold is an On Release preview: a mode changed under it (undo, another client) hides it.
    if let Some(held) = app
        .parameter_gesture
        .held(field)
        .filter(|_| on_release(app, field))
    {
        return Some(held);
    }
    let stored = || app.project().field(field).map(|stored| stored.value);
    match source(app, field) {
        // An override equals the stored base by construction, and so does a fresh edit.
        Some(driven) if driven.overridden() || app.parameter_gesture.just_submitted(field, now) => {
            stored()
        }
        Some(driven) => Some(driven.value),
        None => stored(),
    }
}

/// For the control's automation LED and its Re-Enable Automation entry.
pub(crate) fn automation_state(app: &App, field: &FieldRef) -> AutomationState {
    effective(app, field).map_or(AutomationState::None, |driven| driven.state)
}

fn on_release(app: &App, field: &FieldRef) -> bool {
    app.project()
        .parameter(&field.parameter)
        .is_some_and(|p| p.output.send_mode == SendMode::OnRelease)
}

#[cfg(test)]
mod tests;

/// Shared hook for any automatable control, including future lane headers: the automation LED
/// from `automation_state`, and the context menu.
pub(crate) fn menu<'a>(
    app: &App,
    widget: iced::Element<'a, crate::Message>,
    subject: &crate::info::Subject,
) -> iced::Element<'a, crate::Message> {
    let mark = subject_field(app, subject).and_then(|field| mark(automation_state(app, &field)));
    iced::widget::mouse_area(crate::control::automation_mark::automation_mark(
        widget, mark, &app.theme,
    ))
    .on_right_press(crate::Message::AutomationMenu(subject.clone()))
    .into()
}

/// The LED a control shows for its argument's state.
pub(crate) fn mark(state: AutomationState) -> Option<crate::control::automation_mark::Mark> {
    use crate::control::automation_mark::Mark;
    match state {
        AutomationState::Following => Some(Mark::Following),
        AutomationState::Overridden => Some(Mark::Overridden),
        AutomationState::None => None,
    }
}

/// The track an automatable control belongs to.
pub(crate) fn subject_track(subject: &crate::info::Subject) -> qtable_engine::model::TrackId {
    use crate::info::Subject;
    match subject {
        Subject::Volume(track)
        | Subject::Path(track)
        | Subject::TargetPath(track, _)
        | Subject::Lane { track, .. } => *track,
        Subject::TargetField(entry, _) | Subject::TargetEntry(entry) => entry.track(),
    }
}

/// The argument an automatable control drives, when it still exists.
pub(crate) fn subject_field(app: &App, subject: &crate::info::Subject) -> Option<FieldRef> {
    match subject {
        crate::info::Subject::Volume(track) => Some(crate::mixer_device::volume_field(*track)),
        other => other.parameter_field(),
    }
    .filter(|field| app.project().contains_field(field))
}
