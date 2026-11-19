//! Rendered drop regions; scroll viewports clip both hit testing and feedback.
use super::{Drag, DropTarget, Geometry};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};
use widget::Tree;

pub(super) type Zones = Vec<(Rectangle, DropTarget, Rectangle)>;

struct Region<'a, M> {
    content: Element<'a, M>,
    key: &'static str,
    geometry: Geometry,
    zones: Box<dyn Fn(Size) -> Zones + 'a>,
    drag: Option<&'a Drag>,
}

pub(super) fn region<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    key: &'static str,
    geometry: Geometry,
    zones: impl Fn(Size) -> Zones + 'a,
    drag: Option<&'a Drag>,
) -> Element<'a, M> {
    Element::new(Region {
        content: content.into(),
        key,
        geometry,
        zones: Box::new(zones),
        drag,
    })
}

impl<M> Widget<M, Theme, Renderer> for Region<'_, M> {
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
        let bounds = layout.bounds();
        let zones = (self.zones)(bounds.size());
        let mut visible = Vec::new();
        for (hit, target, marker) in zones {
            let translate = |rect: Rectangle| rect + Vector::new(bounds.x, bounds.y);
            if let Some(hit) = translate(hit).intersection(viewport) {
                visible.push((hit, target));
                if self.drag.is_some_and(|drag| {
                    drag.moved
                        && drag.over == Some(target)
                        && (matches!(target, DropTarget::Column(_)) || hit.contains(drag.cursor))
                }) && let Some(marker) = translate(marker).intersection(viewport)
                {
                    use iced::advanced::Renderer as _;
                    let color = iced::Color {
                        a: if matches!(target, DropTarget::Column(_)) {
                            crate::style::detail::INTO_MARKER_ALPHA
                        } else {
                            1.0
                        },
                        ..crate::control::LIVE_DARK.selection_background
                    };
                    renderer.with_layer(*viewport, |renderer| {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: marker,
                                ..Default::default()
                            },
                            color,
                        );
                    });
                }
            }
        }
        self.geometry.borrow_mut().insert(self.key, visible);
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
