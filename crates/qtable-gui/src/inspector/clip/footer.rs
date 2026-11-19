//! One bottom area for every clip-editor tab: the tab's lane tools, then the selected
//! hit/point inspector (or content summary). Events and Overview use the same inspector slot.
//! Event paths belong to the Target device; clip region fields live in the clip box.
use crate::inspector::Message;
use crate::inspector::event_editor::ClipKey;
use crate::inspector::event_strip::Fields;
use crate::inspector::lane_view::{Choice, EditorTab, chooser, region};
use crate::parameter_gesture::Effective;
use crate::style::detail;
use iced::widget::{Space, column, container, row, scrollable, stack};
use iced::{Element, Length};
use qtable_engine::model::Project;

pub(super) fn view(
    project: &Project,
    key: ClipKey,
    revision: u64,
    (choice, live): (&Choice, Option<Effective>),
    fields: &Fields<'_>,
    inspector: Element<'static, Message>,
) -> Element<'static, Message> {
    let mut bottom = column![];
    if choice.tab == EditorTab::Envelopes {
        let [device, control] = chooser::faces(
            project,
            &fields.controls,
            (key.track, key.slot),
            (choice, live),
        );
        let mut bar = row![device, control]
            .spacing(detail::CONTROL_GAP)
            .align_y(iced::alignment::Vertical::Center);
        if let Some(target) = choice.one_lane() {
            bar = bar.push(region::bar(
                project,
                key,
                revision,
                target,
                &fields.controls,
                fields.drafts,
                fields.errors,
            ));
        }
        // Reserve the scrollbar's own gutter so it never covers the controls. The natural
        // row width keeps every field reachable when the Detail View is narrow.
        bottom = bottom.push(
            scrollable(bar.width(Length::Shrink).height(detail::CONTROL_HEIGHT))
                .direction(scrollable::Direction::Horizontal(detail::scrollbar()))
                .width(Length::Fill)
                .height(detail::CONTROL_HEIGHT + detail::SCROLLBAR_GUTTER),
        );
    }
    // Measure one hit per event definition, at the real available width. Wrapping can add
    // rows, but changing selection cannot move the timeline. Point fields set the minimum.
    let label_height = iced::widget::text::LineHeight::default()
        .to_absolute(iced::Pixels(detail::TEXT_SIZE as f32))
        .0;
    let point_height = 2.0 * f32::from(detail::PANEL_INSET)
        + label_height
        + detail::LABEL_GAP
        + detail::CONTROL_HEIGHT;
    let mut candidates: Vec<Element<'static, Message>> =
        vec![container(Space::new()).height(point_height).into()];
    if let Some(clip) = project.clip(key.track, key.slot) {
        let mut seen = std::collections::HashSet::new();
        for (index, event) in clip.events.iter().enumerate() {
            if seen.insert(&event.definition) {
                candidates.push(crate::inspector::event_strip::hit_panel(
                    project, fields, index, event,
                ));
            }
        }
    }
    bottom
        .push(stack![
            crate::control::height_reserve::height_reserve(candidates),
            inspector
        ])
        .spacing(detail::CONTROL_GAP)
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests;
