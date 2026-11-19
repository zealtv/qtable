//! A drag on the Track Delay or the output Latency box is one undo step, set on release.
//! The boxes are the real ones, driven through a retained
//! widget tree; their messages go through `update`, and the window subscription's message for each
//! event (`crate::window_event`) follows, as iced delivers them.
use crate::app::App;
use crate::control::SizeRole;
use crate::control_focus::View;
use crate::messages::Message;
use iced::advanced::{Layout, Shell, layout, widget::Tree};
use iced::{Element, Event, Point, Rectangle, Size, mouse};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{Project, TrackId};
use std::time::Duration;

const SIZE: Size = Size::new(200.0, 40.0);

struct Pointer {
    clock: crate::gesture_clock::test::Clock,
    tree: Tree,
    renderer: iced::Renderer,
}

impl Pointer {
    fn new() -> Self {
        Self {
            clock: crate::gesture_clock::test::Clock::new(),
            tree: Tree::empty(),
            renderer: crate::grid::test_renderer(),
        }
    }

    /// One input event: the box answers first, then the subscription's message for it arrives.
    fn input(
        &mut self,
        app: &mut App,
        view: &dyn Fn(&App) -> Element<'_, Message>,
        event: Event,
        point: Point,
    ) {
        self.clock.advance(Duration::from_millis(10));
        let mut widget = view(app);
        self.tree.diff(&widget);
        let node = widget.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, SIZE),
        );
        let mut messages = vec![];
        let mut shell = Shell::new(&mut messages);
        widget.as_widget_mut().update(
            &mut self.tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &Rectangle::with_size(SIZE),
        );
        let status = if shell.is_event_captured() {
            iced::event::Status::Captured
        } else {
            iced::event::Status::Ignored
        };
        drop(widget);
        for message in messages {
            let _ = crate::update::update(app, message);
        }
        if let Some(message) = crate::window_event(event, status) {
            let _ = crate::update::update(app, message);
        }
    }

    fn center(&mut self, app: &App, view: &dyn Fn(&App) -> Element<'_, Message>) -> Point {
        let mut widget = view(app);
        self.tree.diff(&widget);
        let node = widget.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, SIZE),
        );
        Layout::new(&node).bounds().center()
    }

    /// Press, drag up through each stop, release.
    fn drag(&mut self, app: &mut App, view: &dyn Fn(&App) -> Element<'_, Message>, stops: &[f32]) {
        let start = self.center(app, view);
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        self.input(app, view, press, start);
        for rise in stops {
            let point = Point::new(start.x, start.y - rise);
            let moved = Event::Mouse(mouse::Event::CursorMoved { position: point });
            self.input(app, view, moved, point);
        }
        let end = Point::new(start.x, start.y - stops.last().copied().unwrap_or(0.0));
        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        self.input(app, view, release, end);
    }
}

/// How many undo steps the history holds. Undoing and redoing closes the open run, so call it
/// when the input is done.
fn steps(app: &mut App) -> usize {
    let mut count = 0;
    while app.engine.can_undo() {
        app.engine.execute(Command::Undo).unwrap();
        count += 1;
    }
    for _ in 0..count {
        app.engine.execute(Command::Redo).unwrap();
    }
    count
}

fn app() -> App {
    let mut app = App::new(Engine::new(Project::default()).unwrap(), "120".into(), None);
    let _ = crate::update::update(&mut app, Message::Files(crate::messages::FileMessage::New));
    app
}

fn delay_view(app: &App) -> Element<'_, Message> {
    let track = app.project().tracks[0].id;
    super::delay_input(app, track, View::Track, SizeRole::Fill)
}

fn latency_view(app: &App) -> Element<'_, Message> {
    super::latency_input(app, &app.project().outputs[0], View::Track, SizeRole::Fill)
}

#[test]
fn a_track_delay_drag_is_one_step_and_one_undo_returns_the_start() {
    let mut app = app();
    let track: TrackId = app.project().tracks[0].id;
    let delay = |app: &App| app.project().track(track).unwrap().delay_ms;
    let view = delay_view;
    let start = delay(&app);
    let setup = steps(&mut app);
    let mut pointer = Pointer::new();
    pointer.drag(&mut app, &view, &[10.0, 20.0, 30.0, 40.0, 50.0, 60.0]);
    assert_ne!(delay(&app), start, "the drag moved the delay");
    assert_eq!(steps(&mut app) - setup, 1, "the whole drag is one step");
    app.engine.execute(Command::Undo).unwrap();
    assert_eq!(
        delay(&app),
        start,
        "one undo returns the delay to the start"
    );

    // Redo, then a second drag of its own.
    app.engine.execute(Command::Redo).unwrap();
    let first = delay(&app);
    pointer.clock.advance(Duration::from_secs(2));
    pointer.drag(&mut app, &view, &[10.0, 20.0, 30.0]);
    assert_eq!(steps(&mut app) - setup, 2, "a second drag is a second step");
    app.engine.execute(Command::Undo).unwrap();
    assert_eq!(delay(&app), first);
}

#[test]
fn an_output_latency_drag_is_one_step_and_one_undo_returns_the_start() {
    let mut app = app();
    let _ = crate::update::update(
        &mut app,
        Message::Outputs(crate::messages::OutputMessage::AddOutput),
    );
    let latency = |app: &App| app.project().outputs[0].delay_ms;
    let view = latency_view;
    let start = latency(&app);
    let setup = steps(&mut app);
    let mut pointer = Pointer::new();
    pointer.drag(&mut app, &view, &[10.0, 20.0, 30.0, 40.0, 50.0, 60.0]);
    assert_ne!(latency(&app), start, "the drag moved the latency");
    assert_eq!(steps(&mut app) - setup, 1, "the whole drag is one step");
    app.engine.execute(Command::Undo).unwrap();
    assert_eq!(latency(&app), start);
}
