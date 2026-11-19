//! Inspector picker choices and model mappings.
use super::Message;
use crate::control::widgets::ControlMessage;
use crate::control::{Dropdown, SizeRole, State};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::{Control, ControlId, Controls, DraftKey};
use crate::info::Id;
use crate::style::detail;
use iced::Element;
use qtable_engine::model::{FollowAction, LaunchQuantisation, OscArgument, Project, TrackId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchQuantChoice {
    Global,
    None,
    Bars8,
    Bars4,
    Bars2,
    Bar,
    Half,
    HalfT,
    Quarter,
    QuarterT,
    Eighth,
    EighthT,
    Sixteenth,
    SixteenthT,
    ThirtySecond,
    Custom(LaunchQuantisation),
}
impl LaunchQuantChoice {
    pub(super) const ALL: [Self; 15] = [
        Self::Global,
        Self::None,
        Self::Bars8,
        Self::Bars4,
        Self::Bars2,
        Self::Bar,
        Self::Half,
        Self::HalfT,
        Self::Quarter,
        Self::QuarterT,
        Self::Eighth,
        Self::EighthT,
        Self::Sixteenth,
        Self::SixteenthT,
        Self::ThirtySecond,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::Global => "Global",
            Self::None => "None",
            Self::Bars8 => "8 Bars",
            Self::Bars4 => "4 Bars",
            Self::Bars2 => "2 Bars",
            Self::Bar => "1 Bar",
            Self::Half => "1/2",
            Self::HalfT => "1/2T",
            Self::Quarter => "1/4",
            Self::QuarterT => "1/4T",
            Self::Eighth => "1/8",
            Self::EighthT => "1/8T",
            Self::Sixteenth => "1/16",
            Self::SixteenthT => "1/16T",
            Self::ThirtySecond => "1/32",
            Self::Custom(_) => "Custom",
        }
    }
    pub(super) fn from(q: LaunchQuantisation) -> Self {
        use qtable_engine::model::{NoteValue::*, QuantisationValue::*};
        match q {
            LaunchQuantisation::Global => Self::Global,
            LaunchQuantisation::None => Self::None,
            LaunchQuantisation::Value(Bars(8)) => Self::Bars8,
            LaunchQuantisation::Value(Bars(4)) => Self::Bars4,
            LaunchQuantisation::Value(Bars(2)) => Self::Bars2,
            LaunchQuantisation::Value(Bars(1)) => Self::Bar,
            LaunchQuantisation::Value(Note(Half)) => Self::Half,
            LaunchQuantisation::Value(Triplet(Half)) => Self::HalfT,
            LaunchQuantisation::Value(Note(Quarter)) => Self::Quarter,
            LaunchQuantisation::Value(Triplet(Quarter)) => Self::QuarterT,
            LaunchQuantisation::Value(Note(Eighth)) => Self::Eighth,
            LaunchQuantisation::Value(Triplet(Eighth)) => Self::EighthT,
            LaunchQuantisation::Value(Note(Sixteenth)) => Self::Sixteenth,
            LaunchQuantisation::Value(Triplet(Sixteenth)) => Self::SixteenthT,
            LaunchQuantisation::Value(Note(ThirtySecond)) => Self::ThirtySecond,
            other => Self::Custom(other),
        }
    }
    pub fn value(self) -> LaunchQuantisation {
        use qtable_engine::model::{NoteValue::*, QuantisationValue::*};
        match self {
            Self::Global => LaunchQuantisation::Global,
            Self::None => LaunchQuantisation::None,
            Self::Bars8 => LaunchQuantisation::Value(Bars(8)),
            Self::Bars4 => LaunchQuantisation::Value(Bars(4)),
            Self::Bars2 => LaunchQuantisation::Value(Bars(2)),
            Self::Bar => LaunchQuantisation::Value(Bars(1)),
            Self::Half => LaunchQuantisation::Value(Note(Half)),
            Self::HalfT => LaunchQuantisation::Value(Triplet(Half)),
            Self::Quarter => LaunchQuantisation::Value(Note(Quarter)),
            Self::QuarterT => LaunchQuantisation::Value(Triplet(Quarter)),
            Self::Eighth => LaunchQuantisation::Value(Note(Eighth)),
            Self::EighthT => LaunchQuantisation::Value(Triplet(Eighth)),
            Self::Sixteenth => LaunchQuantisation::Value(Note(Sixteenth)),
            Self::SixteenthT => LaunchQuantisation::Value(Triplet(Sixteenth)),
            Self::ThirtySecond => LaunchQuantisation::Value(Note(ThirtySecond)),
            Self::Custom(value) => value,
        }
    }
}
impl std::fmt::Display for LaunchQuantChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    Int,
    Float,
    String,
    Bool,
}
impl ArgKind {
    pub(crate) fn of(a: &OscArgument) -> Self {
        match a {
            OscArgument::Int(_) => Self::Int,
            OscArgument::Float(_) => Self::Float,
            OscArgument::String(_) => Self::String,
            OscArgument::Bool(_) => Self::Bool,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Float => "float",
            Self::String => "string",
            Self::Bool => "bool",
        }
    }
}
impl std::fmt::Display for ArgKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// A picker value wraps the engine action; only its presentation is GUI-specific.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FollowChoice(FollowAction);

impl FollowChoice {
    pub(super) fn from_action(action: FollowAction) -> Self {
        Self(action)
    }
    pub(super) fn action(self) -> FollowAction {
        self.0
    }
    pub(super) fn to_message(self, track: TrackId, slot: usize, a: bool) -> Message {
        if a {
            Message::FollowA(track, slot, self.0)
        } else {
            Message::FollowB(track, slot, self.0)
        }
    }
}

impl std::fmt::Display for FollowChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use FollowAction::*;
        match self.0 {
            Jump(id) => write!(f, "Jump → scene {}", id.0),
            NoAction => f.write_str("No Action"),
            Stop => f.write_str("Stop"),
            PlayAgain => f.write_str("Play Again"),
            Previous => f.write_str("Previous"),
            Next => f.write_str("Next"),
            First => f.write_str("First"),
            Last => f.write_str("Last"),
            Any => f.write_str("Any"),
            Other => f.write_str("Other"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FollowOption {
    pub(super) choice: FollowChoice,
    label: String,
}

impl FollowOption {
    pub(super) fn new(project: &Project, choice: FollowChoice) -> Self {
        Self {
            choice,
            label: follow_action_label(project, choice.action()),
        }
    }
}

impl std::fmt::Display for FollowOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

pub(super) fn follow_action_label(project: &Project, action: FollowAction) -> String {
    if let FollowAction::Jump(id) = action {
        return project
            .scenes
            .iter()
            .enumerate()
            .find(|(_, scene)| scene.id == id)
            .map(|(index, scene)| format!("Jump → {}: {}", index + 1, scene.name))
            .unwrap_or_else(|| "Jump → missing scene".into());
    }
    FollowChoice::from_action(action).to_string()
}

pub(super) fn follow_options(project: &Project) -> Vec<FollowOption> {
    use FollowAction::*;
    let mut v = vec![
        NoAction, Stop, PlayAgain, Previous, Next, First, Last, Any, Other,
    ];
    v.extend(project.scenes.iter().map(|scene| Jump(scene.id)));

    v.into_iter()
        .map(|choice| FollowOption::new(project, FollowChoice::from_action(choice)))
        .collect()
}

/// Action A and Action B pickers side by side, owned by `owner` for focus.
pub(super) fn action_pair<'a, M: ControlMessage<DraftKey> + 'a>(
    project: &Project,
    (controls, view, owner): (Controls<'_>, View, ControlOwner),
    (action_a, action_b): (FollowAction, FollowAction),
    on_a: impl Fn(FollowChoice) -> M + 'a,
    on_b: impl Fn(FollowChoice) -> M + 'a,
) -> Element<'a, M> {
    use super::follow_controls::ActionOption;
    let options: Vec<_> = follow_options(project)
        .into_iter()
        .map(|option| ActionOption::new(project, option.choice))
        .collect();
    let picker = |name, help| {
        Control::new(ControlId::action(view, name, owner), help, Dropdown).role(SizeRole::Fill)
    };
    detail::pair(
        controls.labelled(
            "Action A",
            State::default(),
            controls.dropdown(
                picker("follow-a", Id::FollowA),
                options.clone(),
                Some(ActionOption::new(
                    project,
                    FollowChoice::from_action(action_a),
                )),
                move |option: ActionOption| on_a(option.choice),
            ),
        ),
        controls.labelled(
            "Action B",
            State::default(),
            controls.dropdown(
                picker("follow-b", Id::FollowB),
                options,
                Some(ActionOption::new(
                    project,
                    FollowChoice::from_action(action_b),
                )),
                move |option: ActionOption| on_b(option.choice),
            ),
        ),
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtable_engine::model::{NoteValue, QuantisationValue, Scene, SceneId};

    #[test]
    fn quantisation_choices_preserve_every_model_value() {
        for choice in LaunchQuantChoice::ALL {
            assert_eq!(LaunchQuantChoice::from(choice.value()), choice);
        }
        for bars in [1, 2, 3, 4, 8, 16, u16::MAX] {
            let value = LaunchQuantisation::Value(QuantisationValue::Bars(bars));
            assert_eq!(LaunchQuantChoice::from(value).value(), value);
            if ![1, 2, 4, 8].contains(&bars) {
                assert_eq!(LaunchQuantChoice::from(value).to_string(), "Custom");
            }
        }
        for note in [
            NoteValue::Whole,
            NoteValue::Half,
            NoteValue::Quarter,
            NoteValue::Eighth,
            NoteValue::Sixteenth,
            NoteValue::ThirtySecond,
            NoteValue::SixtyFourth,
        ] {
            for value in [
                QuantisationValue::Note(note),
                QuantisationValue::Triplet(note),
            ] {
                let value = LaunchQuantisation::Value(value);
                assert_eq!(LaunchQuantChoice::from(value).value(), value);
            }
        }
    }

    #[test]
    fn jump_labels_follow_scene_order_and_keep_ids_in_commands() {
        let mut project = Project {
            scenes: vec![
                Scene {
                    id: SceneId(42),
                    name: "Intro".into(),
                    follow_actions: Default::default(),
                },
                Scene {
                    id: SceneId(7),
                    name: "Chorus".into(),
                    follow_actions: Default::default(),
                },
            ],
            ..Default::default()
        };
        let choice = FollowChoice::from_action(FollowAction::Jump(SceneId(7)));
        let option = FollowOption::new(&project, choice);
        assert_eq!(option.to_string(), "Jump → 2: Chorus");
        assert_eq!(option.choice.action(), FollowAction::Jump(SceneId(7)));
        project.scenes.swap(0, 1);
        assert_eq!(
            FollowOption::new(&project, choice).to_string(),
            "Jump → 1: Chorus"
        );
        assert_eq!(follow_action_label(&project, FollowAction::Stop), "Stop");
    }
}
