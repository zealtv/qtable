//! Main properties use the same authoritative controls as the Control Bar.
use crate::control::{SizeRole, State, Toggle};
use crate::control_focus::{ControlOwner, View};
use crate::detail_layout::{self, Header, Layout};
use crate::drafts::{Control, ControlId};
use crate::info::Id;
use crate::inspector_panel::Tab;
use crate::style::detail;
use crate::{app::App, grid, messages::Message, transport};
use iced::Length;
use iced::widget::column;

pub(crate) fn view(app: &App) -> Layout<'_, Message> {
    let launching = column![
        app.controls().labelled(
            "Launch Quantisation",
            State::default(),
            transport::quantisation_field(app, View::Main)
        ),
        app.controls().toggle(
            Control::new(
                ControlId::action(View::Main, "follow-actions", ControlOwner::Song),
                Id::FollowActionsToggle,
                Toggle {
                    label: "Follow Actions".into(),
                },
            )
            .role(SizeRole::Fill)
            .lit(app.project().follow_actions_enabled),
            Message::Grid(grid::Action::ToggleFollowActions),
        ),
    ]
    .spacing(detail::CONTROL_GAP)
    .width(Length::Fill);
    let section = |id, title, help, body| {
        app.detail_layout.section(
            (Tab::Main, id),
            title,
            help,
            body,
            Message::ToggleDetailSection,
        )
    };

    Layout {
        header: Header {
            colour: app.theme.selection_background,
            content: detail::heading("Main").into(),
        },
        leading: None,
        sections: vec![
            section(
                detail_layout::OUTPUT,
                "Output",
                Id::DetailOutputSection,
                crate::output_detail::destination_section(app, &app.project().main, View::Main),
            ),
            section(
                detail_layout::THROUGHPUT,
                "Throughput",
                Id::DetailThroughputSection,
                crate::throughput::view(app, app.project().main.id),
            ),
            section(
                detail_layout::LAUNCH,
                "Launch",
                Id::DetailLaunchSection,
                launching.into(),
            ),
        ],
        content: Some(crate::device_chain::view(
            app,
            &app.project().main,
            View::Main,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn main_detail_has_destination_sections_and_chain_when_unset() {
        let mut project = qtable_engine::model::Project::default();
        project.main.output = None;
        let app = App::new(
            qtable_engine::engine::Engine::new(project).unwrap(),
            String::new(),
            None,
        );
        let layout = view(&app);
        assert!(
            layout
                .sections
                .iter()
                .any(|s| s.id == detail_layout::OUTPUT)
        );
        assert!(layout.sections.iter().all(|s| s.id.0 != "mixer"));
        let ids: Vec<_> = layout.sections.iter().map(|s| s.id.0).collect();
        assert_eq!(
            ids,
            ["output", "throughput", "launch"],
            "Transport lives in the transport bar only"
        );
        assert!(layout.content.is_some());
    }
}
