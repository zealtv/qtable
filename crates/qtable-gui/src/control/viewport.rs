//! Reports a child's actual clipped viewport in the same coordinates as its cursor.
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};
use std::{cell::Cell, rc::Rc};
use widget::Tree;

pub(crate) type Viewport = Rc<Cell<Option<Rectangle>>>;

pub(crate) fn report<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    visible: Viewport,
) -> Element<'a, M> {
    Element::new(Reporter {
        content: content.into(),
        visible,
        clip_top: None,
    })
}

/// Keep a pinned region clear while its content scrolls underneath it. Layout
/// and content coordinates stay unchanged; only painting is clipped.
pub(crate) fn clip_top<'a, M: 'a>(content: impl Into<Element<'a, M>>, top: f32) -> Element<'a, M> {
    Element::new(Reporter {
        content: content.into(),
        visible: Default::default(),
        clip_top: Some(top),
    })
}

struct Reporter<'a, M> {
    content: Element<'a, M>,
    visible: Viewport,
    clip_top: Option<f32>,
}

impl<M> Widget<M, Theme, Renderer> for Reporter<'_, M> {
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
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        self.visible.set(Some(
            layout.bounds().intersection(viewport).unwrap_or_default(),
        ));
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
        if let Some(top) = self.clip_top {
            use iced::advanced::Renderer as _;
            let bounds = layout.bounds();
            let body = Rectangle {
                y: bounds.y + top,
                height: (bounds.height - top).max(0.),
                ..bounds
            };
            if let Some(clip) = body.intersection(viewport) {
                renderer.with_layer(clip, |renderer| {
                    self.content.as_widget().draw(
                        &tree.children[0],
                        renderer,
                        theme,
                        style,
                        layout,
                        cursor,
                        &clip,
                    );
                });
            }
            return;
        }
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// The scroll offset that brings a row spanning `top` to `top + height` fully into a viewport
/// `room` tall now scrolled to `offset`, or `None` when it already shows: the row's top when it
/// sits above, its bottom edge at the viewport's when it sits below. The Session's scene rows
/// and the mapping list keep their selection in view with it.
pub(crate) fn reveal(top: f32, height: f32, offset: f32, room: f32) -> Option<f32> {
    let room = room.max(height);
    if top < offset {
        Some(top)
    } else if top + height > offset + room {
        Some(top + height - room)
    } else {
        None
    }
}
