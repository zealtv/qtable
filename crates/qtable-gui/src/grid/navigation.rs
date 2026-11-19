//! Live's five map-only scene controls. The same rectangles declare mapping and help;
//! the shared Button kind and glyphs supply the look.
use super::{Action, CELL_H, GAP, MASTER_W, draw};
use crate::control::{Button, ControlKind, Face, Glyph, Interaction, LIVE_DARK, State};
use crate::info::{Id, region_declaring};
use iced::widget::canvas::{Frame, Geometry, Program};
use iced::{Element, Length, Point, Rectangle, Size, Theme, mouse};
use qtable_engine::model::{ControlRef, SceneNavigation};

pub(super) fn regions() -> impl Iterator<Item = (Rectangle, SceneNavigation, Id)> {
    SceneNavigation::ALL
        .into_iter()
        .enumerate()
        .map(|(i, control)| {
            let width = (MASTER_W - GAP * 4.0) / 5.0;
            let area = Rectangle::new(
                Point::new(i as f32 * (width + GAP), 0.0),
                Size::new(width, CELL_H),
            );
            let help = match control {
                SceneNavigation::Launch => Id::MapSceneLaunch,
                SceneNavigation::CancelLaunch => Id::MapSceneCancel,
                SceneNavigation::Up => Id::MapSceneUp,
                SceneNavigation::Down => Id::MapSceneDown,
                SceneNavigation::Select => Id::MapSceneSelect,
            };
            (area, control, help)
        })
}

pub(super) fn view<'a>(selected_row: Option<usize>) -> Element<'a, Action> {
    region_declaring(
        iced::widget::canvas(Navigation { selected_row })
            .width(Length::Fixed(MASTER_W))
            .height(Length::Fixed(CELL_H)),
        |point, _| {
            regions()
                .find(|(area, _, _)| area.contains(point))
                .map(|(_, _, help)| help)
        },
        || {
            regions()
                .map(|(area, control, _)| (area, ControlRef::SceneNavigation(control)))
                .collect()
        },
        Action::Info,
    )
}

struct Navigation {
    selected_row: Option<usize>,
}

impl Program<Action> for Navigation {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &iced::Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        for (area, control, _) in regions() {
            let face = match control {
                SceneNavigation::Launch => Face::Glyph(Glyph::Play),
                SceneNavigation::CancelLaunch => Face::Glyph(Glyph::Stop),
                SceneNavigation::Up => Face::Glyph(Glyph::Up),
                SceneNavigation::Down => Face::Glyph(Glyph::Down),
                SceneNavigation::Select => Face::Text(String::new()),
            };
            let button = Button {
                face,
                latched: false,
            };
            let look = button.look(&LIVE_DARK, State::default(), Interaction::Idle);
            button.paint(&mut frame, area, &look);
            if control == SceneNavigation::Select {
                draw::label(
                    &mut frame,
                    self.selected_row
                        .map_or_else(|| "—".into(), |row| (row + 1).to_string()),
                    area.x + area.width - super::SCENE_NUMBER_INSET,
                    area.center_y(),
                    look.ink,
                    true,
                );
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{App, Message, map_mode};
    use iced::Event;
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    use qtable_engine::{
        engine::Engine,
        model::{Project, Scene, SceneId, Track, TrackId},
    };

    #[test]
    fn scene_navigation_is_only_in_map_mode_and_every_control_is_picked_in_both_modes() {
        let project = Project {
            tracks: vec![Track::new(TrackId(1), "One", 8)],
            scenes: (1..=8)
                .map(|id| Scene {
                    id: SceneId(id),
                    name: String::new(),
                    follow_actions: Default::default(),
                })
                .collect(),
            ..Project::default()
        };
        let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
        let renderer = crate::grid::test_renderer();
        let size = Size::new(1280.0, 800.0);
        for mode in [None, Some(map_mode::Mode::Midi), Some(map_mode::Mode::Key)] {
            app.map_mode.on = mode.is_some();
            app.map_mode.mode = mode.unwrap_or_default();
            let mut widget = crate::view::view(&app);
            let mut tree = Tree::new(&widget);
            let node = widget.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, size),
            );
            let found = crate::control::map_layer::collect(|op| {
                widget
                    .as_widget_mut()
                    .operate(&mut tree, Layout::new(&node), &renderer, op)
            });
            let navigation: Vec<_> = found
                .iter()
                .filter(|(_, control)| matches!(control, ControlRef::SceneNavigation(_)))
                .collect();
            assert_eq!(navigation.len(), if mode.is_some() { 5 } else { 0 });
            let header = found
                .iter()
                .find(|(_, control)| *control == ControlRef::TrackSelect(TrackId(1)))
                .unwrap();
            if mode.is_some() {
                for (area, control) in navigation.into_iter().chain([header]) {
                    let mut messages = Vec::new();
                    let mut shell = Shell::new(&mut messages);
                    widget.as_widget_mut().update(
                        &mut tree,
                        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                        Layout::new(&node),
                        mouse::Cursor::Available(area.center()),
                        &renderer,
                        &mut iced::advanced::clipboard::Null,
                        &mut shell,
                        &Rectangle::with_size(size),
                    );
                    assert!(shell.is_event_captured());
                    assert!(
                        matches!(&messages[..], [Message::Map(map_mode::Action::Pick(picked))] if picked == control),
                        "{messages:?}"
                    );
                }
            }
        }
    }
}
