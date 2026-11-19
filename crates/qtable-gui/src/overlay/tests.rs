//! Opening or closing an overlay must not disturb the window beneath it: the widget tree's state
//! (a scrolled list's offset, say) has to carry across the frame that adds the menu layer.
use crate::app::App;
use crate::inspector::event_editor::lanes::Identity;
use crate::inspector::lane_model::tests::mixed_project;
use crate::messages::Message;
use iced::advanced::widget::{Operation, Tree, operation::Scrollable};
use iced::advanced::{Layout, Shell, layout};
use iced::{Event, Rectangle, Size, Vector, mouse};
use qtable_engine::engine::Engine;
use qtable_engine::model::TrackId;

const SIZE: Size = Size::new(1400.0, 700.0);

/// The viewport of every scrollable in the tree, with its vertical offset.
#[derive(Default)]
struct Scrollables(Vec<(Rectangle, f32)>);
impl Operation for Scrollables {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn scrollable(
        &mut self,
        _: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        _: Rectangle,
        translation: Vector,
        _: &mut dyn Scrollable,
    ) {
        self.0.push((bounds, translation.y));
    }
}

/// The window as the runtime holds it between frames: one state tree, diffed against each new
/// view.
struct Frames {
    tree: Option<Tree>,
    renderer: iced::Renderer,
    size: Size,
}
impl Frames {
    fn new() -> Self {
        Self::sized(SIZE)
    }
    fn sized(size: Size) -> Self {
        Self {
            tree: None,
            renderer: crate::grid::test_renderer(),
            size,
        }
    }
    /// Build the view of `app`, diff the state tree against it and lay it out.
    fn frame<'a>(&mut self, app: &'a App) -> (iced::Element<'a, Message>, layout::Node) {
        let mut widget = crate::view::view(app);
        match &mut self.tree {
            Some(tree) => tree.diff(widget.as_widget()),
            None => self.tree = Some(Tree::new(widget.as_widget())),
        }
        let node = widget.as_widget_mut().layout(
            self.tree.as_mut().unwrap(),
            &self.renderer,
            &layout::Limits::new(Size::ZERO, self.size),
        );
        (widget, node)
    }
    fn scrollables(&mut self, app: &App) -> Vec<(Rectangle, f32)> {
        let (mut widget, node) = self.frame(app);
        let mut found = Scrollables::default();
        widget.as_widget_mut().operate(
            self.tree.as_mut().unwrap(),
            Layout::new(&node),
            &self.renderer,
            &mut found,
        );
        found.0
    }
    /// Send `event` with the pointer at `at`; the messages it publishes.
    fn send(&mut self, app: &App, event: Event, at: iced::Point) -> Vec<Message> {
        let (mut widget, node) = self.frame(app);
        let mut messages = vec![];
        widget.as_widget_mut().update(
            self.tree.as_mut().unwrap(),
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(at),
            &self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(self.size),
        );
        messages
    }
}

fn overview_app() -> App {
    editor_app(crate::inspector::lane_view::EditorTab::Overview)
}

fn editor_app(tab: crate::inspector::lane_view::EditorTab) -> App {
    let mut app = App::new(Engine::new(mixed_project()).unwrap(), "120".into(), None);
    app.selection.clip = Some(crate::grid::Selection {
        track: TrackId(1),
        slot: 0,
    });
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    app.chrome.detail_open = true;
    crate::inspector::event_editor::reconcile(&mut app);
    crate::inspector::lane_view::choose_tab(&mut app, TrackId(1), 0, tab);
    app
}

/// The Overview's list: the lowest scrollable in the window (the Detail View's right-hand pane).
fn the_list(frames: &mut Frames, app: &App) -> (Rectangle, f32) {
    frames
        .scrollables(app)
        .into_iter()
        .max_by(|a, b| a.0.y.total_cmp(&b.0.y))
        .expect("the Overview scrolls")
}

#[test]
fn a_right_click_on_a_lower_overview_lane_keeps_the_scrolled_list_where_it_is() {
    let mut app = overview_app();
    let mut frames = Frames::new();
    let (view, _) = the_list(&mut frames, &app);
    // Scroll the lane list to its end.
    let inside = iced::Point::new(view.center_x(), view.center_y());
    let wheel = Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0.0, y: -30.0 },
    });
    frames.send(&app, wheel, inside);
    let (view, scrolled) = the_list(&mut frames, &app);
    assert!(scrolled > 0.0, "the list scrolled ({scrolled})");
    // Right-click the lane at the bottom of the view (the lower lanes), well inside the time area.
    let lane = iced::Point::new(view.x + 400.0, view.y + view.height - 12.0);
    let right = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right));
    let messages = frames.send(&app, right, lane);
    let asked = messages.iter().any(|m| {
        matches!(
            m,
            Message::Inspector(crate::inspector::Message::Timeline(
                crate::inspector::event_editor::Action::ContentMenu(click)
            )) if matches!(click.lane, Identity::Envelope(_))
        )
    });
    assert!(
        asked,
        "the right-click asks for a content menu: {messages:?}"
    );
    for message in messages {
        let _ = crate::update::update(&mut app, message);
    }
    assert!(app.overlay.is_some(), "the menu opened");
    // The next frame carries the menu layer over the window.
    assert_eq!(the_list(&mut frames, &app).1, scrolled, "menu open");
    // And closing it again moves nothing either.
    app.close_overlay();
    assert_eq!(the_list(&mut frames, &app).1, scrolled, "menu closed");
}

/// Whether `messages` hold the ruler's zoom drag publishing a window.
fn zoomed(messages: &[Message]) -> bool {
    messages.iter().any(|m| {
        matches!(
            m,
            Message::Inspector(crate::inspector::Message::Timeline(
                crate::inspector::event_editor::Action::Zoom(..)
            ))
        )
    })
}

/// A zoom drag that starts on the ruler's bar-number row at the top of the list and moves down
/// over the lanes.
fn ruler_zoom_drag(frames: &mut Frames, app: &App, view: Rectangle) -> Vec<Message> {
    let press = iced::Point::new(view.x + 400.0, view.y + 8.0);
    let left = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    let mut messages = frames.send(app, left, press);
    let down = iced::Point::new(press.x, press.y + 40.0);
    let moved = Event::Mouse(mouse::Event::CursorMoved { position: down });
    messages.extend(frames.send(app, moved, down));
    let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
    frames.send(app, release, down);
    messages
}

#[test]
fn the_overview_ruler_stays_in_place_and_zooms_while_the_lanes_are_scrolled() {
    ruler_stays_while_scrolled(overview_app(), SIZE, the_list);
}

#[test]
fn the_events_ruler_stays_in_place_and_zooms_while_the_lanes_are_scrolled() {
    // A short window, so that the Events lanes outgrow their pane.
    ruler_stays_while_scrolled(
        editor_app(crate::inspector::lane_view::EditorTab::Events),
        Size::new(1400.0, 420.0),
        the_list,
    );
}

fn ruler_stays_while_scrolled(
    app: App,
    size: Size,
    pick: fn(&mut Frames, &App) -> (Rectangle, f32),
) {
    let mut frames = Frames::sized(size);
    let (view, _) = pick(&mut frames, &app);
    // At the top, the ruler's bar-number row starts a zoom drag.
    assert!(
        zoomed(&ruler_zoom_drag(&mut frames, &app, view)),
        "the ruler zooms before any scrolling"
    );
    // Scroll the lanes to their end, with the pointer low in the pane: clear of the ruler.
    let inside = iced::Point::new(view.center_x(), view.y + view.height - 3.0);
    let wheel = Event::Mouse(mouse::Event::WheelScrolled {
        delta: mouse::ScrollDelta::Lines { x: 0.0, y: -30.0 },
    });
    frames.send(&app, wheel, inside);
    let (view, scrolled) = pick(&mut frames, &app);
    assert!(scrolled > 0.0, "the list scrolled ({scrolled})");
    // The same spot is still the ruler: the header did not scroll away with the lanes.
    assert!(
        zoomed(&ruler_zoom_drag(&mut frames, &app, view)),
        "the ruler still zooms with the lanes scrolled by {scrolled}"
    );
    assert_eq!(
        pick(&mut frames, &app).1,
        scrolled,
        "a ruler drag leaves the lane scroll alone"
    );
}
