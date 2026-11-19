//! Window keyboard gate: mapped keys run before widgets, except in a focused field.
//! Uses the rendered widget tree, so clicking into an untouched number or rename field
//! protects it just as typing a draft does. Policy and engine commands stay with the client.
use iced::advanced::widget::{Operation, Tree, operation::Focusable};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer};
use iced::keyboard::{Key, Modifiers};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Vector, mouse};

pub(crate) fn key_gate<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    captures: impl Fn(&Key, &Key, Modifiers, bool) -> bool + 'a,
    on_key: impl Fn(KeyInput) -> M + 'a,
) -> Element<'a, M> {
    Element::new(Gate {
        content: content.into(),
        captures: Box::new(captures),
        on_key: Box::new(on_key),
    })
}

/// A key event as the gate saw it: iced's unmodified `key`, the Shift-applied `modified_key`.
#[derive(Clone, Debug)]
pub(crate) struct KeyInput {
    pub key: Key,
    pub modified_key: Key,
    pub modifiers: Modifiers,
    pub pressed: bool,
    pub repeat: bool,
}

type CaptureKey<'a> = dyn Fn(&Key, &Key, Modifiers, bool) -> bool + 'a;

struct Gate<'a, M> {
    content: Element<'a, M>,
    captures: Box<CaptureKey<'a>>,
    on_key: Box<dyn Fn(KeyInput) -> M + 'a>,
}

#[derive(Default)]
struct FieldFocus(bool);
impl Operation for FieldFocus {
    fn focusable(
        &mut self,
        _: Option<&iced::advanced::widget::Id>,
        _: Rectangle,
        state: &mut dyn Focusable,
    ) {
        self.0 |= state.is_focused();
    }
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
}

impl<M> Widget<M, iced::Theme, Renderer> for Gate<'_, M> {
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
        operation: &mut dyn Operation,
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
        let input = match event {
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key,
                modified_key,
                modifiers,
                repeat,
                ..
            }) => Some(KeyInput {
                key: key.clone(),
                modified_key: modified_key.clone(),
                modifiers: *modifiers,
                pressed: true,
                repeat: *repeat,
            }),
            Event::Keyboard(iced::keyboard::Event::KeyReleased {
                key,
                modified_key,
                modifiers,
                ..
            }) => Some(KeyInput {
                key: key.clone(),
                modified_key: modified_key.clone(),
                modifiers: *modifiers,
                pressed: false,
                repeat: false,
            }),
            _ => None,
        };
        if let Some(input) = input
            && (self.captures)(
                &input.key,
                &input.modified_key,
                input.modifiers,
                input.pressed,
            )
        {
            // Query the actual widget focus before dispatch, including untouched fields.
            // All current iced focusables are text inputs/editors; latched button focus
            // lives in ControlFocus and deliberately does not exempt a mapped key.
            let mut focus = FieldFocus::default();
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                &mut focus,
            );
            if !focus.0 {
                shell.publish((self.on_key)(input));
                shell.capture_event();
                return;
            }
        }
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
        theme: &iced::Theme,
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
    ) -> Option<overlay::Element<'b, M, iced::Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests;
