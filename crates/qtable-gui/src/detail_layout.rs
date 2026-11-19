//! One Detail View frame. Producers supply content; this module owns geometry.
use crate::info::{Help, Id};
use crate::inspector_panel::Tab;
use crate::style::detail;
use iced::widget::{Space, column, container, row, scrollable, text};
use iced::{Color, Element, Length};
use std::collections::HashSet;

pub(crate) const LEFT_WIDTH: f32 = 248.0;

/// Stable GUI section identity, open to further sections without kind matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SectionId(pub &'static str);
pub(crate) const CLIP: SectionId = SectionId("clip");
pub(crate) const LAUNCH: SectionId = SectionId("launch");
pub(crate) const OUTPUT: SectionId = SectionId("output");
pub(crate) const THROUGHPUT: SectionId = SectionId("throughput");

/// Presentation preferences survive selection and project changes, not app restarts.
#[derive(Default)]
pub(crate) struct State {
    collapsed: HashSet<(Tab, SectionId)>,
}
impl State {
    pub(crate) fn expanded(&self, tab: Tab, id: SectionId) -> bool {
        !self.collapsed.contains(&(tab, id))
    }
    pub(crate) fn section<'a, M>(
        &self,
        (tab, id): (Tab, SectionId),
        title: &'static str,
        help: Id,
        body: Element<'a, M>,
        on_toggle: impl FnOnce(Tab, SectionId) -> M,
    ) -> Section<'a, M> {
        Section {
            id,
            title,
            icon: None,
            expanded: self.expanded(tab, id),
            on_toggle: on_toggle(tab, id),
            help,
            body,
        }
    }
    pub(crate) fn toggle(&mut self, tab: Tab, id: SectionId) {
        if !self.collapsed.remove(&(tab, id)) {
            self.collapsed.insert((tab, id));
        }
    }
}

pub(crate) struct Header<'a, M> {
    pub colour: Color,
    pub content: Element<'a, M>,
}
pub(crate) struct Section<'a, M> {
    pub id: SectionId,
    pub title: &'static str,
    pub icon: Option<Element<'a, M>>,
    pub expanded: bool,
    pub on_toggle: M,
    pub help: Id,
    pub body: Element<'a, M>,
}
pub(crate) struct Layout<'a, M> {
    pub header: Header<'a, M>,
    pub leading: Option<Element<'a, M>>,
    pub sections: Vec<Section<'a, M>>,
    pub content: Option<Element<'a, M>>,
}
impl<'a, M: Clone + 'a> Layout<'a, M> {
    pub(crate) fn empty(message: &'static str) -> Self {
        Self {
            header: Header {
                colour: detail::DETAIL_BACKGROUND,
                content: detail::heading("Detail View").into(),
            },
            leading: Some(text(message).size(detail::TEXT_SIZE).into()),
            sections: vec![],
            content: None,
        }
    }
    pub(crate) fn map<N: Clone + 'a>(self, map: impl Fn(M) -> N + Clone + 'a) -> Layout<'a, N> {
        Layout {
            header: Header {
                colour: self.header.colour,
                content: self.header.content.map(map.clone()),
            },
            leading: self.leading.map(|e| e.map(map.clone())),
            sections: self
                .sections
                .into_iter()
                .map(|section| Section {
                    id: section.id,
                    title: section.title,
                    icon: section.icon.map(|e| e.map(map.clone())),
                    expanded: section.expanded,
                    on_toggle: map(section.on_toggle),
                    help: section.help,
                    body: section.body.map(map.clone()),
                })
                .collect(),
            content: self.content.map(|e| e.map(map)),
        }
    }
    pub(crate) fn view(
        self,
        on_info: impl Fn(crate::info::Hover) -> M + Clone + 'a,
    ) -> Element<'a, M> {
        if self.sections.is_empty() && self.leading.is_none() {
            return container(
                column![
                    detail::title_bar(self.header.colour, self.header.content),
                    self.content
                        .unwrap_or_else(|| Space::new().height(Length::Fill).into())
                ]
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(detail::DETAIL_BACKGROUND.into()),
                ..Default::default()
            })
            .into();
        }
        let mut sections = column![].spacing(0);
        if let Some(leading) = self.leading {
            sections = sections.push(detail::panel(leading));
        }
        for section in self.sections {
            sections = sections.push(
                detail::section_header(
                    section.title,
                    section.icon,
                    section.expanded,
                    section.on_toggle,
                )
                .info(section.help, on_info.clone()),
            );
            if section.expanded {
                sections = sections.push(detail::panel(section.body));
            }
        }
        let left = column![
            detail::title_bar(self.header.colour, self.header.content),
            scrollable(sections)
                .height(Length::Fill)
                .direction(scrollable::Direction::Vertical(detail::scrollbar()))
        ]
        .width(Length::Fixed(LEFT_WIDTH))
        .height(Length::Fill);
        let content = self
            .content
            .unwrap_or_else(|| Space::new().width(Length::Fill).height(Length::Fill).into());
        container(
            row![left, content]
                .spacing(detail::CONTROL_GAP)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_| container::Style {
            background: Some(detail::DETAIL_BACKGROUND.into()),
            ..Default::default()
        })
        .into()
    }
}

#[cfg(test)]
mod tests;
