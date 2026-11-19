//! A non-interactive sizing layer for a stack whose alternatives must share one height.
//! Candidates are measured at the available width but never drawn or exposed to focus/events.
use iced::advanced::{Layout, Widget, layout, mouse, renderer, widget::Tree};
use iced::{Element, Length, Rectangle, Renderer, Size, Theme};

pub(crate) fn height_reserve<'a, M: 'a>(candidates: Vec<Element<'a, M>>) -> Element<'a, M> {
    Element::new(HeightReserve { candidates })
}

struct HeightReserve<'a, M> {
    candidates: Vec<Element<'a, M>>,
}

impl<M> Widget<M, Theme, Renderer> for HeightReserve<'_, M> {
    fn children(&self) -> Vec<Tree> {
        self.candidates.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.candidates);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let mut height = 0.0_f32;
        for (candidate, state) in self.candidates.iter_mut().zip(&mut tree.children) {
            height = height.max(
                candidate
                    .as_widget_mut()
                    .layout(state, renderer, limits)
                    .size()
                    .height,
            );
        }
        layout::Node::new(limits.resolve(Length::Fill, Length::Shrink, Size::new(0.0, height)))
    }

    fn draw(
        &self,
        _tree: &Tree,
        _renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::widget::{Operation, operation};
    use iced::widget::{container, stack, text_input};

    #[test]
    fn sizing_fields_reserve_height_without_entering_the_focus_chain() {
        struct Count(usize);
        impl Operation for Count {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn focusable(
                &mut self,
                _: Option<&iced::widget::Id>,
                _: Rectangle,
                _: &mut dyn operation::Focusable,
            ) {
                self.0 += 1;
            }
        }
        let renderer = crate::grid::test_renderer();
        let hidden: Element<'_, ()> = container(text_input("", "sizing").on_input(|_| ()))
            .height(100)
            .into();
        let mut widget: Element<'_, ()> = stack![
            height_reserve(vec![hidden]),
            text_input("", "visible").on_input(|_| ()),
        ]
        .into();
        let mut tree = Tree::new(&widget);
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(400.0, 300.0)),
        );
        assert_eq!(node.size(), Size::new(400.0, 100.0));
        let mut count = Count(0);
        widget
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut count);
        assert_eq!(
            count.0, 1,
            "only the visible input participates in focus operations"
        );
    }
}
