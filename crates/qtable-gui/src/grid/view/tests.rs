use super::*;
use crate::grid::HEADER_H;
use crate::{app::App, messages::Message};
use iced::advanced::{Layout as WidgetLayout, Shell, layout, widget::Tree};
use iced::{Event, Size, mouse};
use qtable_engine::{
    engine::Engine,
    model::{Region, Scene, SceneId, Track, TrackId},
};

fn app() -> App {
    App::new(
        Engine::new(Project {
            outputs: (201..=205)
                .map(|id| Track::new_output(TrackId(id), format!("Output {id}"), "localhost", 9000))
                .collect(),
            tracks: (1..=12)
                .map(|id| Track::new(TrackId(id), "Track", 16))
                .collect(),
            scenes: (1..=16)
                .map(|id| Scene {
                    id: SceneId(id),
                    name: format!("Scene {id}"),
                    follow_actions: Default::default(),
                })
                .collect(),
            ..Project::default()
        })
        .unwrap(),
        "120".into(),
        None,
    )
}

#[test]
fn scroll_updates_keep_region_offsets_independent_and_ignore_main() {
    let mut app = app();
    for (region, offset) in [
        (Region::Tracks, 80),
        (Region::Outputs, 105),
        (Region::Main, 999),
    ] {
        let _ = crate::update::update(
            &mut app,
            Message::Grid(Action::ScrollColumns {
                region,
                footer: true,
                offset,
            }),
        );
    }
    assert_eq!(app.grid_scroll_x, 80);
    assert_eq!(app.output_scroll_x, 105);
    let tracks = scroll_targets(Region::Tracks);
    let outputs = scroll_targets(Region::Outputs);
    assert!(tracks.iter().all(|id| !outputs.contains(id)));
    assert_eq!(tracks.len(), 3, "clips, pinned footer and pinned header");
    assert!(scroll_targets(Region::Main).is_empty());
}

#[test]
fn output_header_slots_and_footer_controls_align_after_mirrored_scroll() {
    let app = app();
    let renderer = crate::grid::last_row_tests::renderer();
    let size = Size::new(1280.0, 800.0);
    for offset in [0.0, COLUMN_STRIDE, 2.0 * COLUMN_STRIDE] {
        let mut widget = crate::view::view(&app);
        let mut tree = Tree::new(widget.as_widget());
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        // Apply precisely the target operation emitted by the update loop for each side.
        for target in scroll_targets(Region::Outputs) {
            let mut operation = iced::advanced::widget::operation::scrollable::scroll_to::<()>(
                iced::widget::Id::new(target),
                scrollable::AbsoluteOffset {
                    x: Some(offset),
                    y: None,
                },
            );
            widget.as_widget_mut().operate(
                &mut tree,
                WidgetLayout::new(&node),
                &renderer,
                &mut operation,
            );
        }
        let rows = crate::grid::last_row_tests::rows_layout(&node).bounds();
        let main = crate::grid::last_row_tests::main_layout(&node).bounds();
        let width = crate::outputs::width(&app);
        let x = main.x - 6.0 - width + GAP + 22.0;
        let mut send = |event: Event, x: f32, y: f32| {
            let mut messages = vec![];
            widget.as_widget_mut().update(
                &mut tree,
                &event,
                WidgetLayout::new(&node),
                mouse::Cursor::Available(iced::Point::new(x, y)),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &iced::Rectangle::with_size(size),
            );
            messages
        };
        let expected = TrackId(201 + (offset / COLUMN_STRIDE) as u64);
        let gutter = send(
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            main.x - 2.0,
            rows.y + 10.0,
        );
        assert!(
            gutter
                .iter()
                .any(|m| matches!(m, Message::Grid(Action::RegionMenu(Region::Outputs)))),
            "right gutter must contain no output header: {gutter:?}"
        );

        let _ = send(
            Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            x,
            rows.y + 10.0,
        );
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let header = send(press.clone(), x, rows.y + 10.0);
        assert!(header.iter().any(|m| matches!(m, Message::Grid(Action::PressHeader(action)) if matches!(action.as_ref(), Action::SelectColumn(crate::selection::ColumnOwner::Track(id), None) if *id == expected))), "header offset={offset}: {header:?}");
        let _ = send(
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            x,
            rows.y + 10.0,
        );
        let slot = send(press.clone(), x, rows.y + HEADER_H + 10.0);
        assert!(slot.iter().any(|m| matches!(m, Message::Grid(Action::SelectColumn(crate::selection::ColumnOwner::Track(id), Some(0))) if *id == expected)), "slot offset={offset}: {slot:?}");
        let activator_y =
            rows.y + rows.height + FOOTER_H + Row::Output.height() + Row::Path.height() + 62.0;
        let mut mixer = send(press, x, activator_y);
        mixer.extend(send(
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            x,
            activator_y,
        ));
        assert!(mixer.iter().any(|m| matches!(m, Message::Outputs(crate::messages::OutputMessage::ToggleOutputEnabled(id)) if *id == expected)), "mixer offset={offset}: {mixer:?}");
    }
}
const COLUMN_STRIDE: f32 = super::super::COLUMN_W + GAP;
