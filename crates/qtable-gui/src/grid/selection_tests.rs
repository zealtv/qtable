//! Regression: a canvas inside a horizontal scroller must retain finite drawing bounds.
use super::*;
use crate::{app::App, messages::Message};
use iced::Size;
use iced::advanced::{Layout as WidgetLayout, layout, renderer, widget::Tree};
use qtable_engine::engine::Engine;
use qtable_engine::model::{Project, TrackId};

#[test]
fn selected_grid_draws_with_finite_bounds_for_short_and_overflowing_columns() {
    let mut renderer = super::last_row_tests::renderer();
    for track_count in [1, 12] {
        let project = Project {
            scenes: (1..=8)
                .map(|id| Scene {
                    id: SceneId(id),
                    name: format!("Scene {id}"),
                    follow_actions: Default::default(),
                })
                .collect(),
            tracks: (1..=track_count)
                .map(|id| Track::new(TrackId(id), "Track", 8))
                .collect(),
            outputs: vec![Track::new_output(TrackId(201), "Output", "127.0.0.1", 9000)],
            ..Project::default()
        };
        let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
        for size in [Size::new(800.0, 600.0), Size::new(1280.0, 800.0)] {
            for track_selected in [false, true] {
                let action = if track_selected {
                    Action::SelectTrack(TrackId(1), false)
                } else {
                    Action::Select(TrackId(1), 0)
                };
                let _ = crate::update::update(&mut app, Message::Grid(action));
                let mut widget = crate::view::view(&app);
                let mut tree = Tree::new(widget.as_widget());
                let node = widget.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, size),
                );
                fn assert_finite(node: &layout::Node) {
                    let bounds = node.bounds();
                    assert!(
                        bounds.x.is_finite()
                            && bounds.y.is_finite()
                            && bounds.width.is_finite()
                            && bounds.height.is_finite(),
                        "non-finite widget bounds: {bounds:?}"
                    );
                    for child in node.children() {
                        assert_finite(child);
                    }
                }
                assert_finite(&node);
                // Exercise canvas path generation, which previously panicked inside draw_rect.
                widget.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &crate::style::live_theme(&app.theme),
                    &renderer::Style::default(),
                    WidgetLayout::new(&node),
                    mouse::Cursor::Unavailable,
                    &Rectangle::with_size(size),
                );
            }
        }
    }
}
