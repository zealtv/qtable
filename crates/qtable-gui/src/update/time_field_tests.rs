//! Duration floors through the actual Clip, envelope-region and Scene field compositions.
use crate::{
    app::App,
    drafts::{ClipField, DraftKey, SceneField},
    grid, inspector, inspector_panel,
    messages::Message,
};
use iced::advanced::{
    Layout, Shell, layout,
    widget::{Operation, Tree, operation::Focusable},
};
use iced::{
    Event, Point, Rectangle, Size,
    keyboard::{self, Key, key::Named},
    mouse,
};
use qtable_engine::{engine::Engine, model::TrackId};

struct FocusField {
    id: iced::widget::Id,
    bounds: Option<Rectangle>,
}
impl Operation for FocusField {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn focusable(
        &mut self,
        id: Option<&iced::widget::Id>,
        bounds: Rectangle,
        state: &mut dyn Focusable,
    ) {
        if id == Some(&self.id) {
            state.focus();
            self.bounds = Some(bounds);
        }
    }
}
fn down() -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Named(Named::ArrowDown),
        modified_key: Key::Named(Named::ArrowDown),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::NONE,
        text: None,
        repeat: false,
    })
}
fn check_floor(app: &mut App, key: DraftKey) {
    let duration = match key {
        DraftKey::Clip { track, slot, field } => {
            let clip = app.project().clip(track, slot).unwrap();
            match field {
                ClipField::Length => clip.loop_end.0 - clip.loop_start.0,
                ClipField::FollowTime => clip.follow_actions.time.0,
                ClipField::LaneMarker(_) => {
                    let region = clip
                        .lanes
                        .iter()
                        .find_map(|l| l.envelope()?.region)
                        .unwrap();
                    region.loop_end.0 - region.loop_start.0
                }
                _ => unreachable!(),
            }
        }
        DraftKey::Scene { scene, .. } => {
            app.project()
                .scenes
                .iter()
                .find(|s| s.id == scene)
                .unwrap()
                .follow_actions
                .time
                .0
        }
        _ => unreachable!(),
    };
    assert_eq!(duration, 3840, "{key:?}: begin at one bar");
    let renderer = grid::test_renderer();
    let mut tree = Tree::new(crate::view::detail_view(app));
    let viewport = Rectangle::with_size(Size::new(1280.0, 1200.0));
    let mut focus = FocusField {
        id: key.input_id(),
        bounds: None,
    };
    {
        let mut element = crate::view::detail_view(app);
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut focus);
    }
    let bounds = focus
        .bounds
        .expect("the actual panel must contain the requested field");
    let point = Point::new(bounds.x + 1.0, bounds.center_y());
    let end = Point::new(
        point.x,
        point.y + 3.0 * crate::style::detail::TIME_DRAG_STEP,
    );
    let before = app.project_revision();
    for (event, point) in [
        (down(), point),
        (
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            point,
        ),
        (
            Event::Mouse(mouse::Event::CursorMoved { position: end }),
            end,
        ),
        (
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            end,
        ),
    ] {
        let mut messages = vec![];
        {
            let mut element = crate::view::detail_view(app);
            tree.diff(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, viewport.size()),
            );
            element.as_widget_mut().update(
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
        for message in messages {
            let _ = super::update(app, message);
        }
        assert_eq!(
            app.project_revision(),
            before,
            "{key:?}: a floor gesture must send no edit"
        );
        assert!(
            app.drafts.values.is_empty(),
            "{key:?}: no rejected zero draft"
        );
        assert!(app.drafts.errors.is_empty(), "{key:?}: no validation error");
    }
}
#[test]
fn duration_floors_send_nothing_for_clip_length_follow_time_region_length_and_scene() {
    let project = inspector::lane_model::tests::mixed_project();
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    let track = TrackId(1);
    app.selection.clip = Some(grid::Selection { track, slot: 0 });
    app.selection.inspector_tab = inspector_panel::Tab::Clip;
    app.chrome.detail_open = true;
    inspector::lane_view::capture(&mut app, "opacity-unlinked");
    let _ = super::update(
        &mut app,
        Message::Inspector(inspector::Message::FollowLinked(track, 0, false)),
    );
    let _ = super::update(
        &mut app,
        Message::Inspector(inspector::Message::FollowTime(track, 0, "1".into())),
    );
    for field in [
        ClipField::Length,
        ClipField::FollowTime,
        ClipField::LaneMarker(inspector::lane_view::region::Field::Length),
    ] {
        check_floor(&mut app, DraftKey::clip(track, 0, field));
    }
    let scene = app.project().scenes[0].id;
    app.selection.scene = Some(scene);
    app.selection.inspector_tab = inspector_panel::Tab::Scene;
    let _ = super::update(
        &mut app,
        Message::Inspector(inspector::Message::SceneTime(scene, "1".into())),
    );
    check_floor(&mut app, DraftKey::scene(scene, SceneField::FollowTime));
}
