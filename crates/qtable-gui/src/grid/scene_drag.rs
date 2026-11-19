//! One content-space gesture/marker across tracks, gutters, returns and Main.
use super::{Action, CELL_H, GAP, MASTER_W, draw, scene_area};
use iced::widget::canvas::{self, Frame, Geometry, Program};
use iced::{Point, Rectangle, Size, Theme, keyboard, mouse};
use qtable_engine::model::{Scene, SceneId};

pub(super) struct SceneDrag<'a> {
    pub header_h: f32,
    pub scenes: &'a [Scene],
    pub selected: &'a [SceneId],
    pub revision: u64,
    pub viewport: crate::control::viewport::Viewport,
}

pub(super) struct State {
    modifiers: keyboard::Modifiers,
    drag: Option<Drag>,
    clicks: super::cache::Clicks,
    edge: super::edge_scroll::EdgeScroll,
    capture: Option<super::drag_capture::Capture>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            modifiers: Default::default(),
            drag: None,
            clicks: Default::default(),
            edge: Default::default(),
            capture: crate::capture::session()
                .and_then(|s| super::drag_capture::Capture::from_scenario(&s, "autoscroll-scene")),
        }
    }
}
struct Drag {
    scene: SceneId,
    origin: Point,
    cursor: Point,
    started: bool,
    deferred: bool,
    double_click: bool,
    revision: u64,
    inside: bool,
}

impl Drag {
    fn new(scene: SceneId, at: Point, revision: u64, deferred: bool, double_click: bool) -> Self {
        Self {
            scene,
            origin: at,
            cursor: at,
            started: false,
            deferred,
            double_click,
            revision,
            inside: true,
        }
    }
    fn refresh(&mut self, at: Point, bounds: Rectangle, visible: Rectangle) {
        self.cursor = Point::new(at.x - bounds.x, at.y - bounds.y);
        self.started |= crate::selection::header_dragging(self.origin, self.cursor);
        self.inside = visible.contains(at);
    }
}

#[derive(Debug)]
struct Plan {
    scenes: Vec<SceneId>,
    boundary: usize,
}

impl SceneDrag<'_> {
    /// One candidate for the marker and the command, including no-op rejection.
    fn plan(&self, drag: &Drag) -> Option<Plan> {
        if !drag.started || !drag.inside || drag.revision != self.revision {
            return None;
        }
        let scenes = if self.selected.contains(&drag.scene) {
            self.selected.to_vec()
        } else {
            vec![drag.scene]
        };
        let boundary = boundary(drag.cursor.y, self.scenes.len(), self.header_h);
        let rows: Vec<_> = self
            .scenes
            .iter()
            .enumerate()
            .filter_map(|(row, scene)| scenes.contains(&scene.id).then_some(row))
            .collect();
        let unchanged = rows
            .first()
            .zip(rows.last())
            .is_some_and(|(&first, &last)| {
                last - first + 1 == rows.len() && (first..=last + 1).contains(&boundary)
            });
        (!unchanged && !rows.is_empty()).then_some(Plan { scenes, boundary })
    }

    fn release(&self, drag: &Drag) -> Option<Action> {
        if !drag.inside {
            return None;
        }
        if drag.started {
            self.plan(drag)
                .map(|Plan { scenes, boundary }| Action::MoveScenes { scenes, boundary })
        } else if drag.deferred {
            Some(super::detail_open::click(
                Action::ClickScene(drag.scene),
                crate::inspector_panel::Tab::Scene,
                drag.double_click,
            ))
        } else {
            None
        }
    }

    fn visible(&self, bounds: Rectangle) -> Rectangle {
        let visible = self.viewport.get().unwrap_or(bounds);
        Rectangle {
            y: visible.y + self.header_h,
            height: (visible.height - self.header_h).max(0.0),
            ..visible
        }
    }
}

fn boundary(y: f32, count: usize, header_h: f32) -> usize {
    ((y - header_h + (CELL_H + GAP) / 2.0) / (CELL_H + GAP))
        .floor()
        .clamp(0.0, count as f32) as usize
}

fn marker(width: f32, boundary: usize, header_h: f32) -> Rectangle {
    Rectangle::new(
        Point::new(
            0.0,
            header_h + boundary as f32 * (CELL_H + GAP) - GAP / 2.0 - super::SCENE_DROP_LINE / 2.0,
        ),
        Size::new(width, super::SCENE_DROP_LINE),
    )
}

impl Program<Action> for SceneDrag<'_> {
    type State = State;

    fn update(
        &self,
        state: &mut State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Action>> {
        if state.capture.is_some_and(|c| c.done)
            || (state.capture.is_some()
                && !matches!(
                    event,
                    canvas::Event::Window(iced::window::Event::RedrawRequested(_))
                ))
        {
            return None;
        }
        if let canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.modifiers = *modifiers;
        }
        if state
            .drag
            .as_ref()
            .is_some_and(|d| d.revision != self.revision)
        {
            state.drag = None;
            state.edge = Default::default();
        }
        let visible = self.visible(bounds);
        let redraw = || canvas::Action::request_redraw().and_capture();
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let at = cursor.position_in(bounds)?;
                if at.x < bounds.width - MASTER_W + CELL_H {
                    return None;
                }
                let scene = self
                    .scenes
                    .get(scene_area::row_at(at.y, self.scenes.len(), self.header_h)?)?
                    .id;
                let double_click = state.clicks.register(at, crate::gesture_clock::now());
                let modified = state.modifiers.shift() || state.modifiers.command();
                let deferred = !modified && self.selected.contains(&scene);
                state.edge = Default::default();
                state.drag = (!modified).then_some(Drag::new(
                    scene,
                    at,
                    self.revision,
                    deferred,
                    double_click,
                ));
                Some(if deferred {
                    redraw()
                } else {
                    canvas::Action::publish(super::detail_open::click(
                        Action::ClickScene(scene),
                        crate::inspector_panel::Tab::Scene,
                        double_click,
                    ))
                    .and_capture()
                })
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let drag = state.drag.as_mut()?;
                let Some(at) = cursor.land().position() else {
                    state.drag = None;
                    state.edge = Default::default();
                    return Some(redraw());
                };
                drag.refresh(at, bounds, visible);
                if !drag.inside {
                    state.edge = Default::default();
                }
                Some(redraw())
            }
            canvas::Event::Window(iced::window::Event::RedrawRequested(now)) => {
                let sample = state
                    .capture
                    .as_mut()
                    .map(|c| c.sample(*now, visible, self.header_h, false));
                if sample.as_ref().is_some_and(|s| s.first) {
                    let scene = self.scenes.get(1)?;
                    let at = Point::new(bounds.width - 8., self.header_h + CELL_H + GAP + 10.);
                    state.drag = Some(Drag::new(
                        scene.id,
                        at,
                        self.revision,
                        self.selected.contains(&scene.id),
                        false,
                    ));
                }
                let drag = state.drag.as_mut()?;
                let Some(at) = sample
                    .as_ref()
                    .map(|s| s.at)
                    .or_else(|| cursor.land().position())
                else {
                    state.drag = None;
                    state.edge = Default::default();
                    return Some(canvas::Action::request_redraw());
                };
                drag.refresh(at, bounds, visible);
                if !drag.started || !drag.inside {
                    state.edge = Default::default();
                    return None;
                }
                let offset = (visible.y - self.header_h - bounds.y).max(0.0);
                if sample.is_some_and(|s| s.finish) {
                    let dropping = state.capture.as_ref()?.drop;
                    #[cfg(any(test, feature = "dev-hooks"))]
                    eprintln!(
                        "autoscroll-scene offset={offset} cursor={:?} preview={:?} drop={dropping}",
                        drag.cursor,
                        self.plan(drag)
                    );
                    let action = dropping.then(|| self.release(drag)).flatten();
                    #[cfg(any(test, feature = "dev-hooks"))]
                    eprintln!("autoscroll-scene release={action:?}");
                    if dropping {
                        state.drag = None;
                    }
                    return Some(
                        action.map_or_else(canvas::Action::request_redraw, canvas::Action::publish),
                    );
                }
                let height = visible.height + self.header_h;
                state
                    .edge
                    .step(
                        *now,
                        at.y,
                        visible.y..visible.y + visible.height,
                        offset,
                        (bounds.height - height).max(0.0),
                    )
                    .map(|step| {
                        if step.offset.round() != offset.round() {
                            canvas::Action::publish(Action::DragScrollScenes {
                                offset: step.offset.round() as u32,
                            })
                        } else {
                            canvas::Action::request_redraw_at(step.next)
                        }
                    })
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let mut drag = state.drag.take()?;
                state.edge = Default::default();
                let Some(at) = cursor.land().position() else {
                    return Some(redraw());
                };
                drag.refresh(at, bounds, visible);
                if !drag.inside {
                    return Some(redraw());
                }
                Some(self.release(&drag).map_or_else(redraw, |action| {
                    canvas::Action::publish(action).and_capture()
                }))
            }
            canvas::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            })
            | canvas::Event::Window(iced::window::Event::Unfocused)
            | canvas::Event::Mouse(mouse::Event::CursorLeft)
            | canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                state.edge = Default::default();
                state.drag.take().map(|_| redraw())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        if let Some(drag) = &state.drag
            && let Some(plan) = self.plan(drag)
        {
            draw::rect(
                &mut frame,
                marker(bounds.width, plan.boundary, self.header_h),
                crate::control::LIVE_DARK.selection_background,
            );
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::HEADER_H;
    fn scenes() -> Vec<Scene> {
        (1..=7)
            .map(|id| Scene {
                id: SceneId(id),
                name: format!("Scene {id}"),
                follow_actions: Default::default(),
            })
            .collect()
    }
    fn event(
        view: &SceneDrag<'_>,
        state: &mut State,
        event: canvas::Event,
        at: Point,
    ) -> Option<Action> {
        view.update(
            state,
            &event,
            Rectangle::new(Point::ORIGIN, Size::new(1000.0, 400.0)),
            mouse::Cursor::Available(at),
        )
        .and_then(|a| a.into_inner().0)
    }
    fn at(row: usize) -> Point {
        Point::new(980.0, draw::row_y(row) + CELL_H / 2.0)
    }
    #[test]
    fn scene_double_click_opens_on_immediate_press_or_selected_block_release() {
        let scenes = scenes();
        let selected = [SceneId(3), SceneId(4), SceneId(5)];
        for selection in [&[][..], &selected[..]] {
            let clock = crate::gesture_clock::test::Clock::new();
            let view = SceneDrag {
                header_h: HEADER_H,
                scenes: &scenes,
                selected: selection,
                revision: 1,
                viewport: Default::default(),
            };
            let mut state = State::default();
            for double_click in [false, true] {
                clock.advance(std::time::Duration::from_millis(100));
                let pressed = event(
                    &view,
                    &mut state,
                    canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    at(3),
                );
                let released = event(
                    &view,
                    &mut state,
                    canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    at(3),
                );
                let expected = Some(if double_click {
                    Action::OpenDetail(Box::new(Action::ClickScene(SceneId(4))))
                } else {
                    Action::ClickScene(SceneId(4))
                });
                if selection.is_empty() {
                    assert_eq!(pressed, expected);
                    assert_eq!(released, None);
                } else {
                    assert_eq!(pressed, None);
                    assert_eq!(released, expected);
                }
                assert!(state.drag.is_none());
            }
        }
    }
    #[test]
    fn scene_drag_defers_selected_block_collapse_and_keeps_original_boundaries() {
        let scenes = scenes();
        let selected = [SceneId(3), SceneId(4), SceneId(5)];
        let view = SceneDrag {
            header_h: HEADER_H,
            scenes: &scenes,
            selected: &selected,
            revision: 1,
            viewport: Default::default(),
        };
        let mut state = State::default();
        let press = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let release = canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        assert_eq!(event(&view, &mut state, press.clone(), at(3)), None);
        assert!(state.drag.as_ref().unwrap().deferred);
        assert_eq!(
            event(&view, &mut state, release.clone(), at(3)),
            Some(Action::ClickScene(SceneId(4)))
        );
        event(&view, &mut state, press, at(3));
        let to = Point::new(50.0, draw::row_y(1));
        event(
            &view,
            &mut state,
            canvas::Event::Mouse(mouse::Event::CursorMoved { position: to }),
            to,
        );
        assert!(state.drag.as_ref().unwrap().started);
        assert_eq!(
            event(&view, &mut state, release, to),
            Some(Action::MoveScenes {
                scenes: selected.to_vec(),
                boundary: 1
            })
        );
        assert!(state.drag.is_none());
    }
    #[test]
    fn scene_drag_single_press_modifiers_launch_area_and_cancel() {
        let scenes = scenes();
        let view = SceneDrag {
            header_h: HEADER_H,
            scenes: &scenes,
            selected: &[],
            revision: 1,
            viewport: Default::default(),
        };
        let mut state = State::default();
        let press = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        assert_eq!(
            event(&view, &mut state, press.clone(), at(2)),
            Some(Action::ClickScene(SceneId(3)))
        );
        event(
            &view,
            &mut state,
            canvas::Event::Window(iced::window::Event::Unfocused),
            at(2),
        );
        assert!(state.drag.is_none());
        state.modifiers = keyboard::Modifiers::SHIFT;
        event(&view, &mut state, press.clone(), at(4));
        assert!(state.drag.is_none());
        assert_eq!(
            event(&view, &mut state, press, Point::new(900.0, at(2).y)),
            None
        );
    }
    #[test]
    fn scene_drag_marker_spans_every_region_and_end_boundary_is_reachable() {
        assert_eq!(boundary(-100.0, 7, HEADER_H), 0);
        assert_eq!(boundary(draw::row_y(3) - GAP / 2.0, 7, HEADER_H), 3);
        assert_eq!(boundary(1000.0, 7, HEADER_H), 7);
        let line = marker(1280.0, 7, HEADER_H);
        assert_eq!(line.width, 1280.0);
        assert_eq!(line.x, 0.0);
        assert_eq!(line.height, 3.0);
    }
}
