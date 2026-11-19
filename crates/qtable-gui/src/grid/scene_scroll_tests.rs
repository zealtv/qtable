//! Real widget routing with overflowing scenes and the Detail View expanded.
use super::*;
use crate::{app::App, messages::Message};
use iced::advanced::{Layout as WidgetLayout, Shell, layout, widget::Tree};
use iced::{Event, Size};
use qtable_engine::engine::Engine;
use qtable_engine::model::{Project, TrackId};

#[test]
fn wheel_over_tracks_outputs_and_main_scrolls_the_same_rows_without_a_main_rail() {
    let renderer = super::last_row_tests::renderer();
    let size = Size::new(1280.0, 800.0);
    for output_count in [1, 4] {
        let project = Project {
            scenes: (1..=16)
                .map(|id| Scene {
                    id: SceneId(id),
                    name: format!("Scene {id}"),
                    follow_actions: Default::default(),
                })
                .collect(),
            tracks: vec![Track::new(TrackId(1), "Track", 16)],
            outputs: (1..=output_count)
                .map(|id| {
                    Track::new_output(TrackId(200 + id), format!("Output {id}"), "127.0.0.1", 9000)
                })
                .collect(),
            ..Project::default()
        };
        let app = App::new(Engine::new(project).unwrap(), "120".into(), None);
        assert!(app.chrome.detail_open && app.chrome.output_open && app.chrome.outputs_open);
        for area in 0..3 {
            let mut widget = crate::view::view(&app);
            let mut tree = Tree::new(widget.as_widget());
            let node = widget.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, size),
            );
            let rows = super::last_row_tests::rows_layout(&node).bounds();
            let tracks = super::last_row_tests::tracks_layout(&node).bounds();
            let main = super::last_row_tests::main_layout(&node).bounds();
            assert!(HEADER_H + 16.0 * (CELL_H + GAP) > rows.height);
            let track_x = tracks.x + GAP + 60.0;
            // Inside the actual output overlay, including when it overflows sideways.
            let output_x = main.x - 6.0 - crate::outputs::width(&app) + COLUMN_W / 2.0;
            // The former shared scrollbar covered the last ten pixels of Main.
            let main_x = main.x + main.width - 2.0;
            let y = rows.y + HEADER_H + CELL_H / 2.0;
            let mut send = |event: Event, x: f32| {
                let mut messages = vec![];
                widget.as_widget_mut().update(
                    &mut tree,
                    &event,
                    WidgetLayout::new(&node),
                    mouse::Cursor::Available(Point::new(x, y)),
                    &renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut Shell::new(&mut messages),
                    &Rectangle::with_size(size),
                );
                messages
                    .into_iter()
                    .filter_map(|m| match m {
                        Message::Grid(a) if !matches!(a, Action::ScrollColumns { .. }) => Some(a),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            // The desktop sends a redraw before input. Scrollable uses it to
            // initialise its viewport notification, even at zero offset.
            send(
                Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                )),
                track_x,
            );
            let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
            assert_eq!(
                send(press.clone(), main_x),
                vec![Action::ClickScene(SceneId(1))]
            );
            assert_eq!(
                send(press.clone(), output_x),
                vec![Action::SelectColumn(
                    crate::selection::ColumnOwner::Track(TrackId(201)),
                    Some(0)
                )],
                "output slots select without launching"
            );
            assert_eq!(
                send(press.clone(), main.x - crate::outputs::width(&app) - 8.0),
                vec![Action::SelectScene(SceneId(1))],
                "blank space selects the scene"
            );
            send(
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Pixels {
                        x: 0.0,
                        y: -(CELL_H + GAP),
                    },
                }),
                [track_x, output_x, main_x][area],
            );
            assert_eq!(
                send(press.clone(), track_x),
                vec![Action::Select(TrackId(1), 1)],
                "wheel area {area}, outputs {output_count}"
            );
            assert_eq!(
                send(press.clone(), output_x),
                vec![Action::SelectColumn(
                    crate::selection::ColumnOwner::Track(TrackId(201)),
                    Some(1)
                )],
                "output slots follow vertical scrolling"
            );
            assert_eq!(send(press, main_x), vec![Action::ClickScene(SceneId(2))]);
        }
    }
}
