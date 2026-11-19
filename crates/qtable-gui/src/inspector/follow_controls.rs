//! Shared Clip/Scene follow-action presentation; adapters supply controls and messages.
use super::choices::{FollowChoice, follow_action_label};
use crate::info::{Help, Id};
use crate::style::{self, detail};
use iced::Element;
use iced::widget::{Column, column, text};
use qtable_engine::model::{FollowAction, Project};

pub(super) struct FollowControls<'a, M> {
    pub toggle: Element<'a, M>,
    pub actions: Element<'a, M>,
    pub chances: [Element<'a, M>; 2],
    pub percentage_b: Option<u8>,
    pub on_split: Box<dyn Fn(f32) -> M + 'a>,
    pub timing: Element<'a, M>,
}
impl<'a, M: Clone + 'a> FollowControls<'a, M> {
    pub fn view(self, on_info: impl Fn(crate::info::Hover) -> M + 'a) -> Column<'a, M> {
        let [a, b] = self.chances;
        let mut body = column![
            self.toggle,
            self.actions,
            detail::chance::with_readouts(
                self.percentage_b.unwrap_or(50) as f32,
                Some(self.on_split),
                a,
                b
            )
            .info(Id::FollowChance, on_info)
        ]
        .spacing(detail::CONTROL_GAP);
        if self.percentage_b.is_none() {
            body = body.push(
                text("0 : 0 · No Action")
                    .size(detail::TEXT_SIZE)
                    .color(style::DISABLED),
            );
        }
        body.push(self.timing)
    }
}
#[derive(Clone, PartialEq, Eq)]
pub(super) struct ActionOption {
    pub(super) choice: FollowChoice,
    jump_label: Option<String>,
}
impl ActionOption {
    pub(super) fn new(project: &Project, choice: FollowChoice) -> Self {
        let jump_label = matches!(choice.action(), FollowAction::Jump(_))
            .then(|| follow_action_label(project, choice.action()).replace("Jump →", "↵ Jump"));
        Self { choice, jump_label }
    }
}
impl std::fmt::Display for ActionOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use FollowAction::*;
        let (icon, label) = match self.choice.action() {
            NoAction => ("", "No Action"),
            Stop => ("■", "Stop"),
            PlayAgain => ("▶", "Again"),
            Previous => ("↑", "Previous"),
            Next => ("↓", "Next"),
            First => ("☰", "First"),
            Last => ("☰", "Last"),
            Any => ("✱", "Any"),
            Other => ("✳", "Other"),
            Jump(_) => return f.write_str(self.jump_label.as_deref().unwrap_or("↵ Jump")),
        };
        write!(f, "{icon} {label}")
    }
}
