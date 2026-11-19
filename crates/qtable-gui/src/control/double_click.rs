//! Double-click to the default, as Live resets a slider, knob or number box. The canvas kinds
//! (Slider, Dial, fader) call `register` from their own press handling. `ResetOnDoubleClick`
//! does the same for an iced widget such as the Number box, whose text input would otherwise
//! capture the press first.
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer};
use iced::mouse;
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector};
use std::time::{Duration, Instant};

/// Presses closer than this many logical pixels, and within [`WINDOW`], make a double click.
const RADIUS: f32 = 6.0;
const WINDOW: Duration = Duration::from_millis(300);

/// The last left press on one control.
#[derive(Default)]
pub(super) struct Presses(Option<(Point, Instant)>);

impl Presses {
    /// Record a left press and say whether it completes a double click. A double click clears
    /// the record, so a third press starts a new pair.
    pub(super) fn register(&mut self, at: Point) -> bool {
        self.register_at(at, crate::gesture_clock::now())
    }

    /// `register` at a given time, so tests can use fixed instants.
    fn register_at(&mut self, at: Point, now: Instant) -> bool {
        let double = self.0.is_some_and(|(last, time)| {
            last.distance(at) < RADIUS && now > time && now - time <= WINDOW
        });
        self.0 = (!double).then_some((at, now));
        double
    }
}

/// Wraps a control so that a double click on it publishes its `on_reset` messages in order and never reaches the
/// control. The caller supplies the message: it knows the default and any hold to clear.
pub(super) struct ResetOnDoubleClick<'a, M> {
    content: Element<'a, M>,
    on_reset: Vec<M>,
}

impl<'a, M> ResetOnDoubleClick<'a, M> {
    pub(super) fn new(content: Element<'a, M>, on_reset: Vec<M>) -> Self {
        Self { content, on_reset }
    }
}

impl<M: Clone> Widget<M, Theme, Renderer> for ResetOnDoubleClick<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Presses>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(Presses::default())
    }
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
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && !shell.is_event_captured()
            && let Some(at) = cursor.position_over(layout.bounds())
            && tree.state.downcast_mut::<Presses>().register(at)
        {
            for message in &self.on_reset {
                shell.publish(message.clone());
            }
            shell.capture_event();
            return;
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

impl<'a, M: Clone + 'a> From<ResetOnDoubleClick<'a, M>> for Element<'a, M> {
    fn from(widget: ResetOnDoubleClick<'a, M>) -> Self {
        Element::new(widget)
    }
}

#[cfg(test)]
mod tests;
