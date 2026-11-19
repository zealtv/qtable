use super::*;
use crate::app::App;
use crate::drafts::OutputField;
use crate::drafts::{DraftKey, SceneField};
use crate::messages::{Message, OutputMessage};
use crate::{grid, inspector};
use iced::advanced::{Layout as WidgetLayout, layout, renderer, widget::Tree};
use iced::{Rectangle, Size, mouse};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{BeatTime, SceneId};

fn app() -> App {
    let mut app = App::new(
        Engine::new(crate::files::new_project()).unwrap(),
        "120".into(),
        None,
    );
    send(&mut app, Message::Outputs(OutputMessage::AddOutput));
    let track = app.project().tracks[0].id;
    crate::edit_actions::execute_edit(
        &mut app,
        Command::Edit(ProjectEdit::CreateClip {
            track,
            slot: 0,
            length: BeatTime(3840),
        }),
    );
    app
}
fn send(app: &mut App, message: Message) {
    let _ = crate::update::update(app, message);
}
fn inspector(app: &mut App, message: inspector::Message) {
    send(app, Message::Inspector(message));
}
fn assert_finite(node: &layout::Node) {
    let b = node.bounds();
    assert!(b.x.is_finite() && b.y.is_finite() && b.width.is_finite() && b.height.is_finite());
    for child in node.children() {
        assert_finite(child);
    }
}

#[test]
fn all_five_producers_share_geometry_and_draw_at_short_and_wide_sizes() {
    let mut renderer = grid::test_renderer();
    let mut app = app();
    let track = app.project().tracks[0].id;
    let scene = app.project().scenes[0].id;
    let output = app.project().outputs[0].id;
    for message in [
        Message::Grid(grid::Action::Select(track, 0)),
        Message::Grid(grid::Action::SelectScene(scene)),
        Message::Grid(grid::Action::SelectTrack(track, false)),
        Message::Grid(grid::Action::SelectColumn(
            crate::selection::ColumnOwner::Track(output),
            None,
        )),
        Message::Grid(grid::Action::SelectColumn(
            crate::selection::ColumnOwner::Track(app.project().main.id),
            None,
        )),
    ] {
        send(&mut app, message);
        for size in [Size::new(320.0, 60.0), Size::new(1000.0, 400.0)] {
            let mut widget = crate::view::detail_view(&app);
            let mut tree = Tree::new(widget.as_widget());
            let node = widget.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, size),
            );
            assert_finite(&node);
            let body = &node.children()[0];
            if app.selection.inspector_tab == Tab::Track {
                assert_eq!(body.children()[0].bounds().width, size.width);
                assert_eq!(body.children()[0].bounds().height, detail::HEADER_HEIGHT);
                assert_eq!(body.children()[1].bounds().x, 0.0);
            } else {
                let left = &body.children()[0];
                let right = &body.children()[1];
                assert_eq!(left.bounds().width, LEFT_WIDTH);
                assert_eq!(left.children()[0].bounds().height, detail::HEADER_HEIGHT);
                assert_eq!(right.bounds().x, LEFT_WIDTH + detail::CONTROL_GAP);
            }
            assert_eq!(node.bounds().height, size.height);
            // Exercise actual canvas generation and viewport clipping, not only sizes.
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

#[test]
fn collapsed_section_keeps_its_header_and_only_left_content_scrolls() {
    let renderer = grid::test_renderer();
    for expanded in [false, true] {
        let frame = Layout {
            header: Header {
                colour: Color::BLACK,
                content: text("Title").into(),
            },
            leading: None,
            sections: vec![Section {
                id: LAUNCH,
                title: "Launch",
                icon: None,
                expanded,
                on_toggle: Message::ToggleDetailSection(Tab::Scene, LAUNCH),
                help: Id::SceneLaunchSection,
                body: Space::new().height(1000.0).into(),
            }],
            content: Some(Space::new().width(Length::Fill).height(Length::Fill).into()),
        };
        let mut widget = frame.view(Message::Info);
        let mut tree = Tree::new(widget.as_widget());
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(800.0, 150.0)),
        );
        let row = &node.children()[0];
        let left = &row.children()[0];
        let scroll = &left.children()[1];
        let sections = &scroll.children()[0];
        assert_eq!(sections.children().len(), if expanded { 2 } else { 1 });
        assert_eq!(
            sections.children()[0].bounds().height,
            detail::HEADER_HEIGHT
        );
        assert_eq!(scroll.bounds().height, 150.0 - detail::HEADER_HEIGHT);
        assert_eq!(row.children()[1].bounds().height, 150.0);
        if expanded {
            assert!(sections.bounds().height > scroll.bounds().height);
        }
    }
}

#[test]
fn per_tab_collapse_preferences_survive_selection_and_project_reset_without_edits() {
    let mut app = app();
    let revision = app.project_revision();
    send(&mut app, Message::ToggleDetailSection(Tab::Clip, LAUNCH));
    assert!(!app.detail_layout.expanded(Tab::Clip, LAUNCH));
    assert!(app.detail_layout.expanded(Tab::Scene, LAUNCH));
    let scene = app.project().scenes[0].id;
    send(&mut app, Message::Grid(grid::Action::SelectScene(scene)));
    app.reset_for_project();
    assert!(!app.detail_layout.expanded(Tab::Clip, LAUNCH));
    assert!(app.detail_layout.expanded(Tab::Scene, LAUNCH));
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn collapse_retains_rejected_drafts_for_reopening() {
    let mut app = app();
    let output = app.project().outputs[0].id;
    send(
        &mut app,
        Message::Grid(grid::Action::SelectColumn(
            crate::selection::ColumnOwner::Track(output),
            None,
        )),
    );
    let revision = app.project_revision();
    let key = DraftKey::Output(output, OutputField::Port);
    send(
        &mut app,
        Message::Outputs(OutputMessage::OutputPortChanged(output, "bad".into())),
    );
    send(&mut app, Message::ToggleDetailSection(Tab::Output, OUTPUT));
    assert_eq!(app.drafts.values[&key], "bad");
    assert!(app.drafts.errors.contains_key(&key));
    assert!(!app.detail_layout.expanded(Tab::Output, OUTPUT));
    send(&mut app, Message::ToggleDetailSection(Tab::Output, OUTPUT));
    assert_eq!(app.drafts.values[&key], "bad");
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn empty_or_deleted_selection_uses_the_common_empty_frame() {
    let mut app = app();
    app.selection.scene = Some(SceneId(u64::MAX));
    let scene = inspector::scene_view(
        app.project(),
        app.selection.scene,
        &app.drafts.values,
        &app.drafts.errors,
        &app.detail_layout,
        app.controls(),
    );
    assert!(scene.sections.is_empty());
    assert!(scene.content.is_none());
    let clip = inspector::view(
        app.project(),
        None,
        &app.drafts.values,
        &app.drafts.errors,
        (&app.clip_view, &app.detail_layout),
        app.controls(),
    );
    assert!(clip.sections.is_empty());
    assert!(clip.leading.is_some());
}

fn scene_weights(app: &App, scene: SceneId) -> (u32, u32) {
    let a = &app
        .project()
        .scenes
        .iter()
        .find(|s| s.id == scene)
        .unwrap()
        .follow_actions
        .actions;
    (a.chance_a, a.chance_b)
}

#[test]
fn scene_view_and_untouched_percentage_do_not_normalize_arbitrary_or_zero_weights() {
    let mut app = app();
    let scene = app.project().scenes[0].id;
    for weights in [(2, 1), (0, 0), (u32::MAX, u32::MAX)] {
        let mut follow_actions = app.project().scenes[0].follow_actions.clone();
        follow_actions.actions.chance_a = weights.0;
        follow_actions.actions.chance_b = weights.1;
        crate::edit_actions::execute_edit(
            &mut app,
            Command::Edit(ProjectEdit::SetSceneFollowActions {
                scene,
                follow_actions,
            }),
        );
        let revision = app.project_revision();
        let layout = inspector::scene_view(
            app.project(),
            Some(scene),
            &app.drafts.values,
            &app.drafts.errors,
            &app.detail_layout,
            app.controls(),
        );
        assert_eq!(layout.sections[0].id, LAUNCH);
        drop(layout);
        inspector(
            &mut app,
            inspector::Message::ScenePercent(scene, true, "67 %".into()),
        );
        assert_eq!(scene_weights(&app, scene), weights);
        assert_eq!(app.project_revision(), revision);
    }
}

#[test]
fn scene_percentage_slider_and_blur_edits_are_complementary_and_validate() {
    let mut app = app();
    let scene = app.project().scenes[0].id;
    inspector(&mut app, inspector::Message::SceneChanceSplit(scene, 25));
    assert_eq!(scene_weights(&app, scene), (75, 25));
    let key = DraftKey::scene(scene, SceneField::PercentA);
    inspector(&mut app, inspector::Message::SceneDraft(key, "101".into()));
    inspector(
        &mut app,
        inspector::Message::ScenePercent(scene, true, "101".into()),
    );
    assert_eq!(app.drafts.values[&key], "101");
    assert!(app.drafts.errors.contains_key(&key));
    assert_eq!(scene_weights(&app, scene), (75, 25));
    inspector(&mut app, inspector::Message::SceneDraft(key, "60 %".into()));
    send(&mut app, Message::ToggleDetailSection(Tab::Scene, LAUNCH));
    assert_eq!(scene_weights(&app, scene), (60, 40));
    assert!(!app.drafts.values.contains_key(&key));
    assert!(!app.drafts.errors.contains_key(&key));
}

#[test]
fn scene_time_uses_project_meter_rejects_legacy_beats_and_retains_invalid_text() {
    let mut app = app();
    let scene = app.project().scenes[0].id;
    inspector(
        &mut app,
        inspector::Message::SceneTime(scene, "1. 2. 0".into()),
    );
    assert_eq!(app.project().scenes[0].follow_actions.time, BeatTime(5760));
    inspector(&mut app, inspector::Message::SceneTime(scene, "1/3".into()));
    assert_eq!(app.project().scenes[0].follow_actions.time, BeatTime(5760));
    let key = DraftKey::scene(scene, SceneField::FollowTime);
    inspector(&mut app, inspector::Message::SceneDraft(key, "0".into()));
    send(&mut app, Message::ToggleDetailSection(Tab::Scene, LAUNCH));
    assert_eq!(app.drafts.values[&key], "0");
    assert!(app.drafts.errors.contains_key(&key));
    assert_eq!(app.project().scenes[0].follow_actions.time, BeatTime(5760));
}

#[test]
fn section_visibility_changes_dismiss_a_route_overlay_in_both_message_domains() {
    let mut app = app();
    for _ in 0..4 {
        send(&mut app, Message::Outputs(OutputMessage::AddOutput));
    }
    let track = app.project().tracks[0].id;
    let owner = track;
    for message in [
        Message::ToggleDetailSection(Tab::Track, OUTPUT),
        Message::Inspector(inspector::Message::ToggleSection(Tab::Clip, LAUNCH)),
    ] {
        app.open_overlay(crate::overlay::Overlay::Dropdown(
            crate::overlay::OpenDropdown {
                at: crate::control::menu::Anchor::point(iced::Point::ORIGIN),
                owner: crate::overlay::DropdownOwner::Route(owner),
                highlight: None,
                anchor_pending: false,
            },
        ));
        app.validate_overlay();
        assert_eq!(app.route_dropdown(), Some(owner));
        let revision = app.project_revision();
        send(&mut app, message);
        assert!(app.overlay.is_none());
        assert_eq!(app.project_revision(), revision);
    }
}

#[test]
fn output_and_main_settings_scroll_beside_the_fixed_device_chain() {
    let renderer = grid::test_renderer();
    let mut app = app();
    for tab in [Tab::Output, Tab::Main] {
        app.selection.inspector_tab = tab;
        app.selection.anchor = Some(app.project().outputs[0].id);
        for height in [219.0, 138.0] {
            let mut widget = crate::view::detail_view(&app);
            let mut tree = Tree::new(widget.as_widget());
            let node = widget.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(1200.0, height)),
            );
            let left = &node.children()[0].children()[0];
            let scroll = &left.children()[1];
            let sections = &scroll.children()[0];
            assert_eq!(scroll.bounds().height, height - detail::HEADER_HEIGHT);
            assert!(sections.bounds().height > scroll.bounds().height);
            // Output body contains the entire endpoint and stage, including Bundle.
            assert!(sections.children()[1].bounds().height > scroll.bounds().height);
            let cursor = mouse::Cursor::Available(iced::Point::new(100.0, 100.0));
            let mut shell_messages = vec![];
            let mut shell = iced::advanced::Shell::new(&mut shell_messages);
            widget.as_widget_mut().update(
                &mut tree,
                &iced::Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -1000.0 },
                }),
                WidgetLayout::new(&node),
                cursor,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &Rectangle::new(iced::Point::ORIGIN, Size::new(1200.0, height)),
            );
            assert!(
                shell.is_event_captured(),
                "{tab:?} settings accept vertical scrolling"
            );
        }
    }
}

/// What a wheel event over the clip editor's lanes publishes, found by trying points across
/// the pane: the real detail view, its real vertical scrollable and the real canvas.
fn wheel_over_the_lanes(
    app: &App,
    delta: mouse::ScrollDelta,
) -> Vec<inspector::event_editor::Action> {
    let renderer = grid::test_renderer();
    let (width, height) = (1400.0, 700.0);
    let mut widget = crate::view::detail_view(app);
    let mut tree = Tree::new(widget.as_widget());
    let node = widget.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(width, height)),
    );
    let mut published = vec![];
    for y in (200..height as i32).step_by(20) {
        for x in (600..width as i32).step_by(40) {
            let cursor = mouse::Cursor::Available(iced::Point::new(x as f32, y as f32));
            let mut messages = vec![];
            let mut shell = iced::advanced::Shell::new(&mut messages);
            widget.as_widget_mut().update(
                &mut tree,
                &iced::Event::Mouse(mouse::Event::WheelScrolled { delta }),
                WidgetLayout::new(&node),
                cursor,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &Rectangle::new(iced::Point::ORIGIN, Size::new(width, height)),
            );
            let captured = shell.is_event_captured();
            for message in messages {
                if let Message::Inspector(inspector::Message::Timeline(action)) = message {
                    published.push(action);
                }
            }
            let _ = captured;
        }
    }
    published
}

#[test]
fn a_horizontal_wheel_over_a_zoomed_overview_reaches_the_canvas_through_the_scrollable() {
    use crate::inspector::event_editor::Action;
    let mut app = app();
    let track = app.project().tracks[0].id;
    send(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    crate::inspector::event_editor::add_hit(&mut app, track, 0, BeatTime(960));
    let id = app.project().clip(track, 0).unwrap().id;
    let key = crate::inspector::event_editor::ClipKey {
        track,
        slot: 0,
        id,
        epoch: app.clip_view.editor.epoch,
    };
    let window = crate::control::time_axis::Window {
        start: 960.0,
        visible: 1920.0,
    };
    inspector(
        &mut app,
        inspector::Message::Timeline(Action::Zoom(key, Some(window))),
    );
    for delta in [
        mouse::ScrollDelta::Lines { x: 1.0, y: 0.0 },
        mouse::ScrollDelta::Lines { x: -1.0, y: 0.0 },
        mouse::ScrollDelta::Pixels { x: 40.0, y: 0.0 },
        mouse::ScrollDelta::Pixels { x: -40.0, y: 0.0 },
    ] {
        let published = wheel_over_the_lanes(&app, delta);
        assert!(
            published.iter().any(
                |a| matches!(a, Action::Zoom(k, Some(w)) if *k == key && w.start != window.start)
            ),
            "{delta:?} reached the canvas and moved the window: {published:?}"
        );
    }
}
