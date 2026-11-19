//! qtable's text field: iced's `TextInput` behind one thin wrapper, so a rule about what a
//! field accepts lives here and not at each call site.
//!
//! The one rule today: a key pressed with the command modifier (Cmd, or Ctrl off the Mac) never
//! types its character. iced 0.14's `text_input` inserts the event's `text` whatever the
//! modifiers, so Cmd-S typed an `s` and Cmd-, a `,` (v3.9 tour, 2026-11-15, steps 7, 8,
//! 12 and 13). The wrapper hands the inner input the same press with its `text` removed. Its
//! own handling by key (Cmd-A, -C, -V, -X, and on macOS Cmd with the arrows or Backspace) works
//! from the key and modifiers, so it keeps working. The press stays uncaptured unless the input
//! handled it as one of those, so it reaches the window's shortcut path (`shortcuts.rs`).
//!
//! `withhold_text` is the place for a later per-field input filter (2026-11-15, wants
//! Live's quiet rejection of invalid characters "at the level of the interface object"): it
//! sees each press before the input does. That filter is not built yet.
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::keyboard;
use iced::widget::{TextInput, text_input};
use iced::{Element, Event, Length, Pixels, Rectangle, Renderer, Size, Theme, Vector, alignment};

pub(crate) struct TextField<'a, M> {
    input: TextInput<'a, M>,
    on_enter: Option<M>,
}

impl<'a, M: Clone + 'a> TextField<'a, M> {
    pub(crate) fn new(placeholder: &str, value: &str) -> Self {
        Self {
            input: text_input(placeholder, value),
            on_enter: None,
        }
    }
    pub(crate) fn id(mut self, id: impl Into<iced::widget::Id>) -> Self {
        self.input = self.input.id(id);
        self
    }
    pub(crate) fn on_input(mut self, on_input: impl Fn(String) -> M + 'a) -> Self {
        self.input = self.input.on_input(on_input);
        self
    }
    pub(crate) fn on_submit(mut self, message: M) -> Self {
        self.input = self.input.on_submit(message);
        self
    }
    /// Published right after `on_submit` when Enter submits the field. The app answers it by
    /// releasing the keyboard unless the value was rejected (2026-11-15: "I would like enter
    /// to release the field"). A field with no `on_submit` never publishes it.
    pub(crate) fn on_enter(mut self, message: M) -> Self {
        self.on_enter = Some(message);
        self
    }
    pub(crate) fn font(mut self, font: iced::Font) -> Self {
        self.input = self.input.font(font);
        self
    }
    pub(crate) fn width(mut self, width: impl Into<Length>) -> Self {
        self.input = self.input.width(width);
        self
    }
    pub(crate) fn padding(mut self, padding: impl Into<iced::Padding>) -> Self {
        self.input = self.input.padding(padding);
        self
    }
    pub(crate) fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.input = self.input.size(size);
        self
    }
    pub(crate) fn align_x(mut self, alignment: impl Into<alignment::Horizontal>) -> Self {
        self.input = self.input.align_x(alignment);
        self
    }
    pub(crate) fn style(
        mut self,
        style: impl Fn(&Theme, text_input::Status) -> text_input::Style + 'a,
    ) -> Self {
        self.input = self.input.style(style);
        self
    }
    /// The input's own layout, which also takes the value to lay out (`NumberInput` uses it).
    pub(crate) fn layout_value(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
        value: Option<&text_input::Value>,
    ) -> layout::Node {
        self.input.layout(tree, renderer, limits, value)
    }
}

/// The press the inner input should see: Cmd held (and not Alt, which a Windows AltGr press
/// also reports with Ctrl) removes the typed text and nothing else. `None` leaves the event as is.
fn withhold_text(event: &Event) -> Option<Event> {
    let Event::Keyboard(keyboard::Event::KeyPressed {
        key,
        modified_key,
        physical_key,
        location,
        modifiers,
        text: Some(_),
        repeat,
    }) = event
    else {
        return None;
    };
    (modifiers.command() && !modifiers.alt()).then(|| {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: modified_key.clone(),
            physical_key: *physical_key,
            location: *location,
            modifiers: *modifiers,
            text: None,
            repeat: *repeat,
        })
    })
}

fn is_enter_press(event: &Event) -> bool {
    matches!(
        event,
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Enter),
            ..
        })
    )
}

impl<M: Clone> Widget<M, Theme, Renderer> for TextField<'_, M> {
    // The inner input's tag, state and tree stand in for this widget's, so code that reads a
    // field's `text_input::State` out of the tree (number_input, path_field) sees no wrapper.
    fn tag(&self) -> tree::Tag {
        Widget::<M, Theme, Renderer>::tag(&self.input)
    }
    fn state(&self) -> tree::State {
        Widget::<M, Theme, Renderer>::state(&self.input)
    }
    fn diff(&self, tree: &mut Tree) {
        Widget::<M, Theme, Renderer>::diff(&self.input, tree)
    }
    fn size(&self) -> Size<Length> {
        Widget::<M, Theme, Renderer>::size(&self.input)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        Widget::<M, Theme, Renderer>::layout(&mut self.input, tree, renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        Widget::<M, Theme, Renderer>::operate(&mut self.input, tree, layout, renderer, operation)
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
        let withheld = withhold_text(event);
        let captured = shell.is_event_captured();
        Widget::<M, Theme, Renderer>::update(
            &mut self.input,
            tree,
            withheld.as_ref().unwrap_or(event),
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        // The input captures Enter only when it is focused and has an `on_submit` to publish.
        if let Some(message) = &self.on_enter
            && !captured
            && shell.is_event_captured()
            && is_enter_press(event)
        {
            shell.publish(message.clone());
        }
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
        Widget::<M, Theme, Renderer>::draw(
            &self.input,
            tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        )
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        Widget::<M, Theme, Renderer>::mouse_interaction(
            &self.input,
            tree,
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        Widget::<M, Theme, Renderer>::overlay(
            &mut self.input,
            tree,
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, M: Clone + 'a> From<TextField<'a, M>> for Element<'a, M> {
    fn from(field: TextField<'a, M>) -> Self {
        Element::new(field)
    }
}

#[cfg(test)]
mod tests;
