//! Focus for controls drawn on a canvas: a left press that lands on one of its buttons
//! claims that button's id, after the canvas has handled the press. Mirrors `control::help::region`.
use super::{ControlId, ControlKey};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector};
use widget::Tree;

struct Region<'a, M, Key> {
    content: Element<'a, M>,
    locate: Box<dyn Fn(Point) -> Option<ControlId<Key>> + 'a>,
    on_claim: Box<dyn Fn(ControlId<Key>) -> M + 'a>,
}

/// `locate` maps a canvas-local point to the button there, using the canvas's own hit-testing.
pub(crate) fn region<'a, M: 'a, Key: ControlKey>(
    content: impl Into<Element<'a, M>>,
    locate: impl Fn(Point) -> Option<ControlId<Key>> + 'a,
    on_claim: impl Fn(ControlId<Key>) -> M + 'a,
) -> Element<'a, M> {
    Element::new(Region {
        content: content.into(),
        locate: Box::new(locate),
        on_claim: Box::new(on_claim),
    })
}

impl<M, Key: ControlKey> Widget<M, Theme, Renderer> for Region<'_, M, Key> {
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
        if !matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) {
            return;
        }
        let bounds = layout.bounds();
        // Clipped buttons must not claim a click that lands outside their visible area.
        let Some(point) = cursor
            .position()
            .filter(|p| viewport.contains(*p) && bounds.contains(*p))
        else {
            return;
        };
        if let Some(id) = (self.locate)(point - Vector::new(bounds.x, bounds.y)) {
            shell.publish((self.on_claim)(id));
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
