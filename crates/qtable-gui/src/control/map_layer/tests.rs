use super::*;
use crate::control::help::{Hover, Identity, widget_with};
use crate::control_focus::test_registry::TestHelp;
use iced::advanced::widget::Tree;
use iced::widget::{Space, button, column, scrollable};
use iced::{Point, Size};
use qtable_engine::model::SceneId;

#[derive(Debug, Clone, PartialEq)]
enum Msg {
    Hover(Hover<TestHelp>),
    Pick(ControlRef),
    Pressed(&'static str),
}

fn scene(id: u64) -> ControlRef {
    ControlRef::Scene(SceneId(id))
}

fn declaring<'a>(content: impl Into<Element<'a, Msg>>, drives: ControlRef) -> Element<'a, Msg> {
    widget_with(
        content,
        Identity {
            help: TestHelp::Volume,
            drives: Some(drives),
        },
        None,
        Msg::Hover,
    )
}

fn run(
    mut element: Element<'_, Msg>,
    size: Size,
    events: &[(Event, Point)],
) -> (Vec<Msg>, Vec<bool>, Vec<(Rectangle, ControlRef)>) {
    let renderer = crate::grid::test_renderer();
    let mut tree = Tree::new(&element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, size),
    );
    let viewport = Rectangle::with_size(size);
    let mut messages = Vec::new();
    let mut captured = Vec::new();
    for (event, at) in events {
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            &mut tree,
            event,
            Layout::new(&node),
            mouse::Cursor::Available(*at),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &viewport,
        );
        captured.push(shell.is_event_captured());
    }
    let regions = collect(|operation| {
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, operation)
    });
    (messages, captured, regions)
}

fn click(at: Point) -> Vec<(Event, Point)> {
    vec![
        (Event::Mouse(mouse::Event::CursorMoved { position: at }), at),
        (
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            at,
        ),
        (
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            at,
        ),
    ]
}

fn two_buttons<'a>(active: bool) -> Element<'a, Msg> {
    map_layer(
        column![
            declaring(
                button("mapped")
                    .on_press(Msg::Pressed("mapped"))
                    .width(100)
                    .height(20),
                scene(1)
            ),
            button("plain")
                .on_press(Msg::Pressed("plain"))
                .width(100)
                .height(20),
        ],
        active,
        crate::control::LIVE_DARK,
        crate::control::LIVE_DARK.learn_midi,
        HashMap::new(),
        Msg::Pick,
    )
}

fn picks(messages: &[Msg]) -> Vec<&Msg> {
    messages
        .iter()
        .filter(|m| !matches!(m, Msg::Hover(_)))
        .collect()
}

#[test]
fn a_click_on_a_mappable_control_picks_it_and_does_not_operate_it() {
    let (messages, captured, _) = run(
        two_buttons(true),
        Size::new(100.0, 40.0),
        &click(Point::new(10.0, 10.0)),
    );
    assert_eq!(picks(&messages), [&Msg::Pick(scene(1))]);
    assert_eq!(captured, [false, true, false]);
}

#[test]
fn a_control_that_declares_nothing_still_operates_in_map_mode() {
    let (messages, _, _) = run(
        two_buttons(true),
        Size::new(100.0, 40.0),
        &click(Point::new(10.0, 30.0)),
    );
    assert_eq!(picks(&messages), [&Msg::Pressed("plain")]);
}

#[test]
fn outside_map_mode_the_layer_changes_nothing() {
    let (messages, _, regions) = run(
        two_buttons(false),
        Size::new(100.0, 40.0),
        &click(Point::new(10.0, 10.0)),
    );
    assert_eq!(picks(&messages), [&Msg::Pressed("mapped")]);
    // The declaration is still there for the operation; only the layer is off.
    assert_eq!(regions.len(), 1);
}

#[test]
fn regions_scrolled_out_of_their_viewport_are_cut_or_dropped() {
    let rows = column((1..=4).map(|id| declaring(Space::new().width(100).height(20), scene(id))));
    let (_, _, regions) = run(
        scrollable(rows).height(50).into(),
        Size::new(100.0, 50.0),
        &[],
    );
    let found: Vec<_> = regions
        .iter()
        .map(|(rect, control)| (control.clone(), rect.y, rect.height))
        .collect();
    assert_eq!(
        found,
        [
            (scene(1), 0.0, 20.0),
            (scene(2), 20.0, 20.0),
            (scene(3), 40.0, 10.0)
        ]
    );
}
