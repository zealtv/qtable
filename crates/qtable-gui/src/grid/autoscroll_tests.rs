//! Retained application widgets: real clipped viewports, redraws and scroll tasks.
use super::*;
use crate::{app::App, messages::Message};
use iced::advanced::{
    Layout as WidgetLayout, Shell, layout,
    widget::{Operation, Tree, operation::Outcome},
};
use iced::{Event, Point, Size, window};
use qtable_engine::{engine::Engine, model::Project};
use std::time::{Duration, Instant};

fn project() -> Project {
    let mut p = nested_header_tests::project();
    p.scenes.extend((9..=24).map(|id| Scene {
        id: SceneId(id),
        name: format!("Scene {id}"),
        follow_actions: Default::default(),
    }));
    for t in &mut p.tracks {
        t.slots.resize_with(24, Default::default);
    }
    p.tracks.insert(0, Track::new(TrackId(90), "Drag me", 24));
    p.tracks
        .extend((9..=14).map(|id| Track::new(TrackId(id), format!("Track {id}"), 24)));
    p.validate().unwrap();
    p
}

struct Harness {
    app: App,
    tree: Tree,
    renderer: iced::Renderer,
    size: Size,
    now: Instant,
    cursor: mouse::Cursor,
    actions: Vec<Action>,
    y_offset: u32,
}
impl Harness {
    fn new(outputs: bool) -> Self {
        let mut app = App::new(Engine::new(project()).unwrap(), "120".into(), None);
        app.chrome.outputs_open = outputs;
        let tree = Tree::new(crate::view::view(&app).as_widget());
        Self {
            app,
            tree,
            renderer: if std::env::var_os("QTABLE_AUTOSCROLL_GPU").is_some() {
                use iced::advanced::renderer::Headless;
                iced::futures::executor::block_on(iced::Renderer::new(
                    iced::Font::default(),
                    iced::Pixels(crate::style::detail::TEXT_SIZE as f32),
                    Some("wgpu"),
                ))
                .expect("requested GPU renderer")
            } else {
                last_row_tests::renderer()
            },
            size: Size::new(900., 768.),
            now: Instant::now(),
            cursor: mouse::Cursor::Unavailable,
            actions: vec![],
            y_offset: 0,
        }
    }
    fn bounds(&mut self) -> (Rectangle, Rectangle, Rectangle) {
        let mut widget = crate::view::view(&self.app);
        self.tree.diff(widget.as_widget());
        let node = widget.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, self.size),
        );
        (
            last_row_tests::rows_layout(&node).bounds(),
            last_row_tests::tracks_layout(&node).bounds(),
            last_row_tests::main_layout(&node).bounds(),
        )
    }
    fn operate(&mut self, operation: &mut dyn Operation) {
        let mut widget = crate::view::view(&self.app);
        self.tree.diff(widget.as_widget());
        let node = widget.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, self.size),
        );
        widget.as_widget_mut().operate(
            &mut self.tree,
            WidgetLayout::new(&node),
            &self.renderer,
            operation,
        );
    }
    fn header_pixels(&mut self) -> Vec<u8> {
        use iced::advanced::{Renderer as _, renderer::Headless};
        let mut widget = crate::view::view(&self.app);
        self.tree.diff(widget.as_widget());
        let node = widget.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(Size::ZERO, self.size),
        );
        let header = last_row_tests::header_layout(&node).bounds();
        let rows = last_row_tests::rows_layout(&node).bounds();
        let expected = self
            .app
            .grid_layout
            .get(self.app.project(), self.app.project_revision())
            .header_h;
        assert_eq!(header.y, rows.y);
        assert_eq!(
            header.height, expected,
            "pinned title row must keep every nested band and title"
        );
        self.renderer.reset(Rectangle::with_size(self.size));
        widget.as_widget().draw(
            &self.tree,
            &mut self.renderer,
            &iced::Theme::Dark,
            &iced::advanced::renderer::Style::default(),
            WidgetLayout::new(&node),
            mouse::Cursor::Unavailable,
            &Rectangle::with_size(self.size),
        );
        let pixels = self.renderer.screenshot(
            Size::new(self.size.width as u32, self.size.height as u32),
            1.,
            self.app.theme.desktop,
        );
        let stride = self.size.width as usize * 4;
        let mut crop = vec![];
        for y in header.y.ceil() as usize..(header.y + header.height - 2.).floor() as usize {
            crop.extend_from_slice(
                &pixels[y * stride + header.x.ceil() as usize * 4
                    ..y * stride + (header.x + header.width).floor() as usize * 4],
            );
        }
        crop
    }
    fn scroll_metrics(&mut self, target: &'static str, horizontal: bool) -> (f32, f32) {
        use iced::advanced::widget::operation::Scrollable;
        struct Query {
            target: &'static str,
            horizontal: bool,
            metrics: Option<(f32, f32)>,
        }
        impl Operation for Query {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
                visit(self);
            }
            fn scrollable(
                &mut self,
                id: Option<&iced::widget::Id>,
                bounds: Rectangle,
                content: Rectangle,
                translation: iced::Vector,
                _: &mut dyn Scrollable,
            ) {
                if id == Some(&iced::widget::Id::new(self.target)) {
                    self.metrics = Some(if self.horizontal {
                        (translation.x, (content.width - bounds.width).max(0.))
                    } else {
                        (translation.y, (content.height - bounds.height).max(0.))
                    });
                }
            }
        }
        let mut query = Query {
            target,
            horizontal,
            metrics: None,
        };
        self.operate(&mut query);
        query.metrics.expect("scrollable must exist")
    }
    fn assert_horizontal_alignment(&mut self) {
        use iced::advanced::widget::operation::Scrollable;
        struct Query(Vec<f32>);
        impl Operation for Query {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
                visit(self);
            }
            fn scrollable(
                &mut self,
                id: Option<&iced::widget::Id>,
                _: Rectangle,
                _: Rectangle,
                translation: iced::Vector,
                _: &mut dyn Scrollable,
            ) {
                if [
                    "session-clip-columns",
                    "session-header-columns",
                    "session-footer-columns",
                ]
                .iter()
                .any(|target| id == Some(&iced::widget::Id::new(target)))
                {
                    self.0.push(translation.x);
                }
            }
        }
        let mut query = Query(vec![]);
        self.operate(&mut query);
        assert_eq!(query.0.len(), 3);
        assert!(
            query
                .0
                .iter()
                .all(|&x| (x - self.app.grid_scroll_x as f32).abs() <= 1.),
            "header/slots/footer: {:?}",
            query.0
        );
    }
    fn task(&mut self, task: iced::Task<Message>) {
        use iced::futures::{FutureExt, StreamExt};
        let mut pending = vec![task];
        while let Some(task) = pending.pop() {
            let Some(mut stream) = iced_runtime::task::into_stream(task) else {
                continue;
            };
            while let Some(action) = stream
                .next()
                .now_or_never()
                .expect("grid widget task must be immediate")
            {
                match action {
                    iced_runtime::Action::Widget(mut op) => loop {
                        self.operate(op.as_mut());
                        match op.finish() {
                            Outcome::Chain(next) => op = next,
                            _ => break,
                        }
                    },
                    iced_runtime::Action::Output(message) => {
                        pending.push(crate::update::update(&mut self.app, message))
                    }
                    other => panic!("unexpected grid task: {other:?}"),
                }
            }
        }
    }
    fn send(&mut self, event: Event) -> window::RedrawRequest {
        let (messages, request) = {
            let mut widget = crate::view::view(&self.app);
            self.tree.diff(widget.as_widget());
            let node = widget.as_widget_mut().layout(
                &mut self.tree,
                &self.renderer,
                &layout::Limits::new(Size::ZERO, self.size),
            );
            let mut messages = vec![];
            let mut shell = Shell::new(&mut messages);
            widget.as_widget_mut().update(
                &mut self.tree,
                &event,
                WidgetLayout::new(&node),
                self.cursor,
                &self.renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &Rectangle::with_size(self.size),
            );
            let request = shell.redraw_request();
            (messages, request)
        };
        for message in messages {
            if let Message::Grid(action) = &message {
                if let Action::ScrollScenes { offset } | Action::DragScrollScenes { offset } =
                    action
                {
                    self.y_offset = *offset;
                }
                self.actions.push(action.clone());
            }
            let task = crate::update::update(&mut self.app, message);
            self.task(task);
        }
        request
    }
    fn redraw(&mut self) -> window::RedrawRequest {
        self.now += Duration::from_millis(16);
        self.send(Event::Window(window::Event::RedrawRequested(self.now)))
    }
    fn hold(&mut self, frames: usize) {
        for _ in 0..frames {
            self.redraw();
        }
    }
    fn point(&mut self, at: Point) {
        self.cursor = mouse::Cursor::Available(at);
        self.send(Event::Mouse(mouse::Event::CursorMoved { position: at }));
    }
    fn press(&mut self, at: Point) {
        self.point(at);
        self.send(Event::Mouse(mouse::Event::ButtonPressed(
            mouse::Button::Left,
        )));
    }
    fn release(&mut self) {
        self.send(Event::Mouse(mouse::Event::ButtonReleased(
            mouse::Button::Left,
        )));
    }
    fn header_drag(&mut self) -> (Rectangle, Rectangle) {
        let (rows, tracks, _) = self.bounds();
        self.redraw();
        self.press(Point::new(tracks.x + 15., rows.y + 35.));
        self.point(Point::new(tracks.x + tracks.width - 8., rows.y + 35.));
        (rows, tracks)
    }
    fn scrolls(&self) -> usize {
        self.actions
            .iter()
            .filter(|a| {
                matches!(a, Action::ScrollColumns { offset, .. } if *offset > 0)
                    || matches!(a, Action::DragScrollScenes { .. })
            })
            .count()
    }
}

#[test]
fn stationary_header_hold_scrolls_beside_outputs_or_main_and_drop_matches_preview() {
    for outputs in [false, true] {
        let mut h = Harness::new(outputs);
        let (_, tracks) = h.header_drag();
        let revision = h.app.project_revision();
        h.hold(15);
        assert_eq!(h.app.grid_scroll_x, 0, "edge dwell");
        h.hold(100);
        assert!(
            h.app.grid_scroll_x > 200,
            "stationary hold must keep scrolling: {}",
            h.app.grid_scroll_x
        );
        assert_eq!(h.app.output_scroll_x, 0, "fixed Outputs did not scroll");
        h.assert_horizontal_alignment();
        assert_eq!(
            h.app.project_revision(),
            revision,
            "scrolling is not an edit"
        );
        let drag = h.app.header_gestures.tracks.get().unwrap();
        assert!((drag.cursor.x - (tracks.width - 8. + h.app.grid_scroll_x as f32)).abs() < 4.);
        let layout = h.app.grid_layout.get(h.app.project(), revision);
        let preview = header::drop_at(
            &h.app.project().tracks,
            &layout.columns,
            &layout.xs,
            drag.key,
            drag.cursor,
        )
        .unwrap();
        h.release();
        h.assert_horizontal_alignment();
        assert!(h.app.header_gestures.tracks.get().is_none());
        assert_eq!(
            h.actions
                .iter()
                .filter(|a| matches!(a, Action::PlaceTrack { .. }))
                .cloned()
                .collect::<Vec<_>>(),
            [Action::PlaceTrack {
                track: TrackId(90),
                before: preview.before,
                group: preview.group
            }]
        );
        assert_eq!(h.app.project_revision(), revision + 1);
        assert_eq!(h.app.project()[TrackId(90)].group, preview.group);
        h.app.project().validate().unwrap();
    }
}

#[test]
fn header_hold_clamps_both_ends_and_cancels_outside_fixed_region_or_viewport() {
    let mut h = Harness::new(true);
    let (rows, tracks) = h.header_drag();
    h.hold(700);
    let end = h.app.grid_scroll_x;
    assert!(end > 500);
    let (offset, max) = h.scroll_metrics("session-header-columns", true);
    assert!((offset - max).abs() <= 1., "{offset} != {max}");
    let count = h.scrolls();
    h.hold(20);
    assert_eq!(h.app.grid_scroll_x, end);
    assert_eq!(h.scrolls(), count, "endpoint stops requesting scrolls");
    h.point(Point::new(tracks.x + 8., rows.y + 35.));
    h.hold(700);
    assert_eq!(h.app.grid_scroll_x, 0);
    let revision = h.app.project_revision();
    h.point(Point::new(tracks.x + tracks.width + 20., rows.y + 35.));
    h.hold(30);
    assert!(!h.app.header_gestures.tracks.get().unwrap().inside);
    h.release();
    assert_eq!(h.app.project_revision(), revision);
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::PlaceTrack { .. }))
    );
}

#[test]
fn header_scroll_cancellation_and_resize_never_use_stale_hover() {
    for cancel in 0..6 {
        let mut h = Harness::new(true);
        let (rows, tracks) = h.header_drag();
        h.hold(40);
        assert!(h.app.grid_scroll_x > 0);
        let revision = h.app.project_revision();
        match cancel {
            0 => {
                h.send(Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    physical_key: iced::keyboard::key::Physical::Unidentified(
                        iced::keyboard::key::NativeCode::Unidentified,
                    ),
                    location: iced::keyboard::Location::Standard,
                    modifiers: Default::default(),
                    text: None,
                    repeat: false,
                }));
            }
            1 => {
                h.send(Event::Mouse(mouse::Event::ButtonPressed(
                    mouse::Button::Right,
                )));
            }
            2 => {
                h.send(Event::Window(window::Event::Unfocused));
            }
            3 => {
                h.cursor = mouse::Cursor::Unavailable;
                h.redraw();
            }
            4 => {
                let _ = crate::update::update(
                    &mut h.app,
                    Message::Grid(Action::ToggleGroup(TrackId(3))),
                );
                h.redraw();
            }
            5 => {
                h.size.width -= 240.;
                h.redraw();
            }
            _ => unreachable!(),
        }
        let before_release = h.app.project_revision();
        h.release();
        assert_eq!(
            h.app.project_revision(),
            before_release,
            "cancel={cancel}, start={revision}, rows={rows:?}, tracks={tracks:?}"
        );
        h.hold(10);
        assert!(h.app.header_gestures.tracks.get().is_none());
        assert!(
            !h.actions
                .iter()
                .any(|a| matches!(a, Action::PlaceTrack { .. }))
        );
    }
}

#[test]
fn stationary_scene_hold_moves_selected_block_after_scroll_and_clamps_endpoints() {
    let mut h = Harness::new(true);
    let (rows, _, main) = h.bounds();
    let header_h = h
        .app
        .grid_layout
        .get(h.app.project(), h.app.project_revision())
        .header_h;
    h.app.selection.scenes.single(SceneId(2));
    h.app.selection.scenes.members.push(SceneId(3));
    h.app.selection.scene = Some(SceneId(2));
    h.redraw();
    h.press(Point::new(
        main.x + main.width - 10.,
        rows.y + header_h + (CELL_H + GAP) + 10.,
    ));
    h.point(Point::new(
        main.x + main.width - 10.,
        rows.y + rows.height - 8.,
    ));
    let revision = h.app.project_revision();
    h.hold(90);
    assert!(
        h.y_offset > 150,
        "stationary scene hold must scroll: {}",
        h.y_offset
    );
    assert_eq!(h.app.project_revision(), revision);
    let expected = ((rows.height - 8. + h.y_offset as f32 - header_h + (CELL_H + GAP) / 2.)
        / (CELL_H + GAP))
        .floor()
        .min(24.) as usize;
    h.release();
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a, Action::MoveScenes { .. }))
            .cloned()
            .collect::<Vec<_>>(),
        [Action::MoveScenes {
            scenes: vec![SceneId(2), SceneId(3)],
            boundary: expected
        }]
    );
    assert_eq!(h.app.project_revision(), revision + 1);
    assert_eq!(h.app.project().scenes[expected - 2].id, SceneId(2));
    assert_eq!(h.app.project().scenes[expected - 1].id, SceneId(3));
}

#[test]
fn scene_hold_clamps_both_ends_and_cancellation_or_viewport_exit_drops_nothing() {
    for cancel in 0..7 {
        let mut h = Harness::new(true);
        let (rows, _, main) = h.bounds();
        let header_h = h
            .app
            .grid_layout
            .get(h.app.project(), h.app.project_revision())
            .header_h;
        h.redraw();
        h.press(Point::new(
            main.x + main.width - 10.,
            rows.y + header_h + 10.,
        ));
        h.point(Point::new(
            main.x + main.width - 10.,
            rows.y + rows.height - 8.,
        ));
        h.hold(220);
        let end = h.y_offset;
        assert!(end > 150);
        let (offset, max) = h.scroll_metrics(view::SCENE_ROWS, false);
        assert!((offset - max).abs() <= 1., "{offset} != {max}");
        let count = h.scrolls();
        h.hold(20);
        assert_eq!(h.y_offset, end);
        assert_eq!(h.scrolls(), count);
        if cancel == 0 {
            h.point(Point::new(main.x + 40., rows.y + header_h + 8.));
            h.hold(220);
            assert_eq!(h.y_offset, 0);
            // A stationary point beyond the bottom must neither scroll nor drop.
            h.point(Point::new(main.x + 40., rows.y + rows.height + 20.));
        } else if cancel == 1 {
            h.send(Event::Window(window::Event::Unfocused));
        } else if cancel == 2 {
            h.send(Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right,
            )));
        } else if cancel == 3 {
            h.cursor = mouse::Cursor::Unavailable;
        } else if cancel == 4 {
            let _ =
                crate::update::update(&mut h.app, Message::Grid(Action::ToggleGroup(TrackId(3))));
        } else if cancel == 5 {
            h.size.height -= 120.;
        } else {
            h.send(Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                physical_key: iced::keyboard::key::Physical::Code(
                    iced::keyboard::key::Code::Escape,
                ),
                location: iced::keyboard::Location::Standard,
                modifiers: Default::default(),
                text: None,
                repeat: false,
            }));
        }
        let revision = h.app.project_revision();
        h.hold(20);
        h.release();
        assert_eq!(h.app.project_revision(), revision, "scene cancel={cancel}");
        assert!(
            !h.actions
                .iter()
                .any(|a| matches!(a, Action::MoveScenes { .. }))
        );
    }
}

#[test]
#[ignore = "writes the native autoscroll fixture into QTABLE_AUTOSCROLL_EVIDENCE_DIR"]
fn write_native_autoscroll_fixture() {
    let directory = std::path::PathBuf::from(
        std::env::var_os("QTABLE_AUTOSCROLL_EVIDENCE_DIR").expect("artifact directory"),
    );
    std::fs::create_dir_all(&directory).unwrap();
    qtable_engine::file::save(directory.join("autoscroll.ron"), &project()).unwrap();
}

#[test]
fn scene_scroll_keeps_complete_pinned_titles_and_body_origin() {
    let mut h = Harness::new(true);
    let _ = crate::update::update(&mut h.app, Message::Grid(Action::SelectScene(SceneId(2))));
    let (rows, _, main) = h.bounds();
    let header_h = h
        .app
        .grid_layout
        .get(h.app.project(), h.app.project_revision())
        .header_h;
    h.redraw();
    h.press(Point::new(
        main.x + main.width - 10.,
        rows.y + header_h + 10.,
    ));
    h.point(Point::new(
        main.x + main.width - 10.,
        rows.y + rows.height - 8.,
    ));
    let before = h.header_pixels();
    for frame in 0..90 {
        h.redraw();
        let pixels = h.header_pixels();
        assert!(
            before == pixels,
            "header changed on native-style redraw {frame}, offset={}",
            h.y_offset
        );
    }
    assert!(h.y_offset > 150);
    let after = h.header_pixels();
    assert!(
        before == after,
        "vertical scrolling must not cover the pinned title text"
    );
    for offset in [185., 189., 190.] {
        h.task(scroll_scenes(offset));
        let after = h.header_pixels();
        assert!(before == after, "title row changed at offset {offset}");
    }
    h.send(Event::Window(window::Event::Unfocused));
    h.actions.clear();
    h.press(Point::new(
        main.x + main.width - 10.,
        rows.y + header_h + 10.,
    ));
    assert!(
        h.actions.contains(&Action::ClickScene(SceneId(10))),
        "first visible scene row uses the pinned header origin"
    );
    h.release();
    h.actions.clear();
    let (_, tracks, _) = h.bounds();
    h.press(Point::new(tracks.x + 15., rows.y + header_h - 4.));
    assert!(
        h.actions
            .contains(&Action::PressHeader(Box::new(Action::SelectTrack(
                TrackId(90),
                false
            )))),
        "title hit area stays pinned"
    );
    h.release();
}
