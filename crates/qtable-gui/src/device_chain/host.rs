//! Pointer selection and reorder gestures over the chain, clipped by its scroll viewport.
use super::{Action, State};
use crate::messages::Message;
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector};
use qtable_engine::model::{DeviceId, Track};
use widget::Tree;

struct Region<'a> {
    content: Element<'a, Message>,
    track: &'a Track,
    state: &'a State,
    cursor_position: Point,
}

pub(super) fn region<'a>(
    content: impl Into<Element<'a, Message>>,
    track: &'a Track,
    state: &'a State,
    cursor_position: Point,
) -> Element<'a, Message> {
    Element::new(Region {
        content: content.into(),
        track,
        state,
        cursor_position,
    })
}

impl Widget<Message, Theme, Renderer> for Region<'_> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        let bounds = layout.bounds();
        let point = cursor
            .position()
            .filter(|p| viewport.contains(*p) && bounds.contains(*p));
        let local = point.map(|p| p - Vector::new(bounds.x, bounds.y));
        let device = local.and_then(|p| {
            self.track
                .devices
                .iter()
                .enumerate()
                .find(|(i, d)| {
                    let x = offset(self.track, *i);
                    p.x >= x && p.x < x + super::face::width(d)
                })
                .map(|(i, d)| (i, d.id))
        });
        let publish =
            |shell: &mut Shell<'_, Message>, action| shell.publish(Message::Device(action));
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if !shell.is_event_captured() =>
            {
                if let Some((index, id)) = device {
                    let p = local.unwrap();
                    if p.y < 20.0 && p.x >= offset(self.track, index) + 24.0 {
                        publish(shell, Action::Begin(id, self.cursor_position));
                    } else {
                        publish(shell, Action::Select(id));
                    }
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                if let Some((_, id)) = device
                    && local.unwrap().y < 20.0
                {
                    publish(shell, Action::Menu(id, self.cursor_position));
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if self.state.drag.is_some() => {
                let over = local.map(|p| {
                    self.track
                        .devices
                        .iter()
                        .enumerate()
                        .find(|(i, d)| p.x < offset(self.track, *i) + super::face::width(d) / 2.0)
                        .map(|(_, d)| d.id)
                });
                publish(shell, Action::Over(over));
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if self.state.drag.is_some() =>
            {
                let over = local.map(|p| {
                    self.track
                        .devices
                        .iter()
                        .enumerate()
                        .find(|(i, d)| p.x < offset(self.track, *i) + super::face::width(d) / 2.0)
                        .map(|(_, d)| d.id)
                });
                publish(shell, Action::Over(over));
                publish(shell, Action::Finish);
            }
            Event::Window(iced::window::Event::Unfocused) => {
                publish(shell, Action::Over(None));
                publish(shell, Action::Finish);
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        if let Some(drag) = &self.state.drag
            && drag.moved
            && let Some(before) = drag.over
        {
            let index = insertion_index(self.track, drag.device, before);
            let bounds = layout.bounds();
            let bar = Rectangle {
                x: bounds.x + (offset(self.track, index) - 2.0).max(0.0),
                y: bounds.y,
                width: 2.0,
                height: bounds.height,
            };
            if let Some(bar) = bar.intersection(viewport) {
                use iced::advanced::Renderer as _;
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: bar,
                        ..Default::default()
                    },
                    crate::control::LIVE_DARK.selection_background,
                );
            }
        }
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

fn offset(track: &Track, index: usize) -> f32 {
    track.devices[..index]
        .iter()
        .map(|d| super::face::width(d) + crate::style::detail::CHAIN_GAP)
        .sum()
}

pub(super) fn insertion_index(track: &Track, device: DeviceId, before: Option<DeviceId>) -> usize {
    let Some(device) = track.device(device) else {
        return track.devices.len();
    };
    track.insert_index(device.kind.capabilities().category, before)
}
