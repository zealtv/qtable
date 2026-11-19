//! A transparent wrapper: observes hover even when a child captures the event.
use super::{Declared, DeclaredRegions, HelpId, Hover, Identity, Source};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer, widget};
use iced::{Element, Event, Length, Point, Rectangle, Renderer, Size, Theme, Vector, mouse};
use widget::{Operation, Tree, tree};

pub(crate) fn widget<'a, M: 'a, H: HelpId>(
    content: impl Into<Element<'a, M>>,
    id: H,
    on_hover: impl Fn(Hover<H>) -> M + 'a,
) -> Element<'a, M> {
    region(content, move |_, _| Some(id), on_hover)
}

/// Help for a control that stands for something in the set; the Info View lists that
/// subject's addresses beside the entry.
pub(crate) fn widget_about<'a, M: 'a, H: HelpId>(
    content: impl Into<Element<'a, M>>,
    id: H,
    subject: H::Subject,
    on_hover: impl Fn(Hover<H>) -> M + 'a,
) -> Element<'a, M> {
    Element::new(Area {
        content: content.into(),
        at: Box::new(move |_, _| Some(id)),
        subject: Some(subject),
        drives: None,
        regions: None,
        on_hover: Box::new(on_hover),
    })
}

/// Help for a control built from an `Identity`: its entry, the subject of its group of
/// controls, if any, and what it drives. Operations find the last as a `Declared` marker.
pub(crate) fn widget_with<'a, M: 'a, H: HelpId>(
    content: impl Into<Element<'a, M>>,
    identity: Identity<H>,
    subject: Option<H::Subject>,
    on_hover: impl Fn(Hover<H>) -> M + 'a,
) -> Element<'a, M> {
    let help = identity.help;
    Element::new(Area {
        content: content.into(),
        at: Box::new(move |_, _| Some(help)),
        subject,
        drives: identity.drives,
        regions: None,
        on_hover: Box::new(on_hover),
    })
}

/// Canvas help uses local coordinates and the canvas's size, and shares hit geometry with its
/// controls, so the entry can change as the pointer moves between a canvas's parts.
pub(crate) fn region<'a, M: 'a, H: HelpId>(
    content: impl Into<Element<'a, M>>,
    at: impl Fn(Point, Size) -> Option<H> + 'a,
    on_hover: impl Fn(Hover<H>) -> M + 'a,
) -> Element<'a, M> {
    Element::new(Area {
        content: content.into(),
        at: Box::new(at),
        subject: None,
        drives: None,
        regions: None,
        on_hover: Box::new(on_hover),
    })
}

/// `region` for a canvas whose parts drive controls: `regions` lists each mappable part's
/// rectangle (relative to the canvas) and what it drives, for Map Mode.
pub(crate) fn region_declaring<'a, M: 'a, H: HelpId>(
    content: impl Into<Element<'a, M>>,
    at: impl Fn(Point, Size) -> Option<H> + 'a,
    regions: impl Fn() -> Vec<(Rectangle, qtable_engine::model::ControlRef)> + 'a,
    on_hover: impl Fn(Hover<H>) -> M + 'a,
) -> Element<'a, M> {
    Element::new(Area {
        content: content.into(),
        at: Box::new(at),
        subject: None,
        drives: None,
        regions: Some(Box::new(regions)),
        on_hover: Box::new(on_hover),
    })
}

struct Area<'a, M, H: HelpId> {
    content: Element<'a, M>,
    at: Box<dyn Fn(Point, Size) -> Option<H> + 'a>,
    subject: Option<H::Subject>,
    drives: Option<qtable_engine::model::ControlRef>,
    #[allow(clippy::type_complexity)]
    regions: Option<Box<dyn Fn() -> Vec<(Rectangle, qtable_engine::model::ControlRef)> + 'a>>,
    on_hover: Box<dyn Fn(Hover<H>) -> M + 'a>,
}

struct State<H: HelpId> {
    source: Source,
    subject: Option<H::Subject>,
    current: Option<H>,
    position: Option<Point>,
    bounds: Rectangle,
    overlay_open: bool,
}

impl<H: HelpId> Default for State<H> {
    fn default() -> Self {
        Self {
            source: Source::new(),
            subject: None,
            current: None,
            position: None,
            bounds: Rectangle::default(),
            overlay_open: false,
        }
    }
}

impl<H: HelpId> State<H> {
    fn observe(
        &mut self,
        next: Option<H>,
        subject: Option<&H::Subject>,
        position: Option<Point>,
        bounds: Rectangle,
    ) -> Option<Hover<H>> {
        let moved = self.position != position || self.bounds != bounds;
        let changed = self.current != next || self.subject.as_ref() != subject;
        if changed {
            self.subject = subject.cloned();
        }
        let previous = self.current;
        self.current = next;
        self.position = position;
        self.bounds = bounds;
        match next {
            Some(id) if moved || changed => {
                Some(Hover::Enter(self.source, id, self.subject.clone()))
            }
            None if previous.is_some() => Some(Hover::Leave(self.source)),
            _ => None,
        }
    }
}

impl<M, H: HelpId> Widget<M, Theme, Renderer> for Area<'_, M, H> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<H>>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::<H>::default())
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
        if let Some(drives) = &self.drives {
            operation.custom(None, layout.bounds(), &mut Declared(drives.clone()));
        }
        if let Some(regions) = &self.regions {
            let origin = layout.bounds().position();
            let moved = regions()
                .into_iter()
                .map(|(rect, control)| (rect + Vector::new(origin.x, origin.y), control))
                .collect();
            operation.custom(None, layout.bounds(), &mut DeclaredRegions(moved));
        }
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
            Event::Mouse(
                mouse::Event::CursorMoved { .. }
                    | mouse::Event::CursorEntered
                    | mouse::Event::CursorLeft
                    | mouse::Event::WheelScrolled { .. }
            ) | Event::Window(
                iced::window::Event::RedrawRequested(_) | iced::window::Event::Unfocused
            )
        ) {
            return;
        }
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State<H>>();
        // The runtime hides the base cursor while a menu or tooltip owns it.
        // Keep that control's help until the pointer leaves its overlay.
        if state.overlay_open
            && cursor.position().is_none()
            && !matches!(
                event,
                Event::Window(iced::window::Event::Unfocused)
                    | Event::Mouse(mouse::Event::CursorLeft)
            )
        {
            return;
        }
        // Clipped or overlay-covered controls must not claim the pointer.
        let position = if matches!(
            event,
            Event::Window(iced::window::Event::Unfocused) | Event::Mouse(mouse::Event::CursorLeft)
        ) {
            None
        } else {
            cursor.position().filter(|p| viewport.contains(*p))
        };
        let next = position
            .filter(|p| bounds.contains(*p))
            .and_then(|p| (self.at)(Point::new(p.x - bounds.x, p.y - bounds.y), bounds.size()));
        if let Some(hover) = state.observe(next, self.subject.as_ref(), position, bounds) {
            shell.publish((self.on_hover)(hover));
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
        let overlay = self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        );
        tree.state.downcast_mut::<State<H>>().overlay_open = overlay.is_some();
        overlay
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::renderer::Headless;
    use iced::widget::canvas::{self, Geometry, Program};
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestHelp {
        Tempo,
        Quantisation,
        Launch,
        Stop,
        Volume,
    }
    impl HelpId for TestHelp {
        type Subject = u8;
    }

    struct Capturing;
    impl Program<Hover<TestHelp>> for Capturing {
        type State = ();
        fn update(
            &self,
            _: &mut (),
            _: &Event,
            _: Rectangle,
            _: mouse::Cursor,
        ) -> Option<canvas::Action<Hover<TestHelp>>> {
            Some(canvas::Action::capture())
        }
        fn draw(
            &self,
            _: &(),
            _: &Renderer,
            _: &Theme,
            _: Rectangle,
            _: mouse::Cursor,
        ) -> Vec<Geometry> {
            vec![]
        }
    }

    fn renderer() -> Renderer {
        let mut future = std::pin::pin!(Renderer::new(
            iced::Font::default(),
            iced::Pixels(crate::style::detail::TEXT_SIZE as f32),
            Some("tiny-skia")
        ));
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(Some(renderer)) => renderer,
            _ => panic!("software renderer must be immediately available"),
        }
    }

    #[test]
    fn captured_child_events_still_report_hover_and_clipped_controls_do_not_claim_it() {
        let renderer = renderer();
        let mut control = widget(
            canvas::Canvas::new(Capturing).width(100).height(20),
            TestHelp::Tempo,
            |hover| hover,
        );
        let mut tree = Tree::new(&control);
        let size = Size::new(100.0, 20.0);
        let node = control.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        let point = Point::new(10.0, 10.0);
        let event = Event::Mouse(mouse::Event::CursorMoved { position: point });
        for visible in [true, false] {
            let mut messages = vec![];
            let mut shell = Shell::new(&mut messages);
            control.as_widget_mut().update(
                &mut tree,
                &event,
                Layout::new(&node),
                mouse::Cursor::Available(point),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &if visible {
                    Rectangle::with_size(size)
                } else {
                    Rectangle::new(Point::new(0.0, 30.0), size)
                },
            );
            assert!(shell.is_event_captured());
            assert!(
                matches!(
                    messages.as_slice(),
                    [Hover::Enter(_, TestHelp::Tempo, None)]
                ) == visible
            );
            if !visible {
                assert!(matches!(messages.as_slice(), [Hover::Leave(_)]));
            }
        }
    }

    #[test]
    fn dropdown_keeps_its_help_when_its_overlay_hides_the_base_cursor() {
        let renderer = renderer();
        let mut control = widget(
            iced::widget::pick_list(vec![1_u8, 2], Some(1), |_| Hover::Clear),
            TestHelp::Quantisation,
            |hover| hover,
        );
        let mut tree = Tree::new(&control);
        let size = Size::new(200.0, 200.0);
        let viewport = Rectangle::with_size(size);
        let node = control.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        let point = Point::new(5.0, 5.0);
        let mut messages = vec![];
        for event in [
            Event::Mouse(mouse::Event::CursorMoved { position: point }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        ] {
            control.as_widget_mut().update(
                &mut tree,
                &event,
                Layout::new(&node),
                mouse::Cursor::Available(point),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &viewport,
            );
        }
        assert!(matches!(
            messages.as_slice(),
            [Hover::Enter(_, TestHelp::Quantisation, None)]
        ));
        assert!(
            control
                .as_widget_mut()
                .overlay(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &viewport,
                    Vector::ZERO
                )
                .is_some()
        );
        messages.clear();
        control.as_widget_mut().update(
            &mut tree,
            &Event::Window(iced::window::Event::RedrawRequested(
                iced::time::Instant::now(),
            )),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &viewport,
        );
        assert!(messages.is_empty());
        assert_eq!(
            tree.state.downcast_ref::<State<TestHelp>>().current,
            Some(TestHelp::Quantisation)
        );
    }

    #[test]
    fn stationary_ticks_are_quiet_and_changes_under_a_stationary_pointer_are_reported() {
        let mut state = State::<TestHelp>::default();
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(100.0, 20.0));
        let position = Some(Point::new(4.0, 4.0));
        assert!(matches!(
            state.observe(Some(TestHelp::Launch), None, position, bounds),
            Some(Hover::Enter(_, TestHelp::Launch, None))
        ));
        assert!(
            state
                .observe(Some(TestHelp::Launch), None, position, bounds)
                .is_none()
        );
        assert!(matches!(
            state.observe(Some(TestHelp::Stop), None, position, bounds),
            Some(Hover::Enter(_, TestHelp::Stop, None))
        ));
        assert!(matches!(
            state.observe(None, None, None, bounds),
            Some(Hover::Leave(_))
        ));
        assert!(state.observe(None, None, None, bounds).is_none());
    }
    #[test]
    fn subject_changes_reenter_under_a_stationary_pointer() {
        let mut state = State::<TestHelp>::default();
        let bounds = Rectangle::with_size(Size::new(100.0, 20.0));
        let position = Some(Point::new(4.0, 4.0));
        let a = Some(1_u8);
        let b = Some(2_u8);
        assert!(
            matches!(state.observe(Some(TestHelp::Volume), a.as_ref(), position, bounds),
            Some(Hover::Enter(_, TestHelp::Volume, subject)) if subject == a)
        );
        assert!(
            state
                .observe(Some(TestHelp::Volume), a.as_ref(), position, bounds)
                .is_none()
        );
        assert!(
            matches!(state.observe(Some(TestHelp::Volume), b.as_ref(), position, bounds),
            Some(Hover::Enter(_, TestHelp::Volume, subject)) if subject == b)
        );
        assert_eq!(state.subject, b);
    }
}
