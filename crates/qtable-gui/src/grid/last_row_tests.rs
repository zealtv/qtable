//! Last-row hit tests and headless event routing through the application's real layout.
use super::*;
use crate::{app::App, messages::Message};
use iced::advanced::renderer::Headless;
use iced::advanced::{Layout as WidgetLayout, Shell, layout, widget::Tree};
use iced::{Event, Size};
use qtable_engine::engine::Engine;
use qtable_engine::model::{BeatTime, ClipId, Project};
use std::future::Future;
use std::task::{Context, Poll, Waker};

pub(crate) fn renderer() -> iced::Renderer {
    let mut future = std::pin::pin!(iced::Renderer::new(
        iced::Font::default(),
        iced::Pixels(crate::style::detail::TEXT_SIZE as f32),
        Some("tiny-skia"),
    ));
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(Some(renderer)) => renderer,
        _ => panic!("software renderer must be immediately available"),
    }
}

fn project(scene_count: usize, has_groups: bool, track_count: u64) -> Project {
    let mut tracks: Vec<_> = (1..=track_count)
        .map(|id| Track::new(TrackId(id), "Track", scene_count))
        .collect();
    for (ti, track) in tracks.iter_mut().enumerate() {
        for (si, slot) in track.slots.iter_mut().enumerate() {
            slot.clip = Some(
                Clip::new(
                    ClipId((ti * scene_count + si + 1) as u64),
                    "Clip",
                    BeatTime(960),
                    qtable_engine::model::LIVE_CLIP_PALETTE[0],
                )
                .unwrap(),
            );
        }
    }
    let mut project = Project {
        tracks,
        scenes: (0..scene_count)
            .map(|i| Scene {
                id: SceneId(i as u64 + 1),
                name: "Scene".into(),
                follow_actions: Default::default(),
            })
            .collect(),
        ..Project::default()
    };
    if has_groups {
        let mut group = Track::new_group(TrackId(101), "Group", scene_count);
        group.colour = qtable_engine::model::LIVE_CLIP_PALETTE[0];
        project.insert_group(group, &[TrackId(1), TrackId(2)]);
    }
    project
}

#[test]
fn last_scene_hit_testing_includes_the_whole_row() {
    for count in [1, 8] {
        for has_groups in [false, true] {
            let project = project(count, has_groups, 3);
            let cols = track_columns(&project.tracks);
            let top = super::layout::header_height(&cols) + (count - 1) as f32 * (CELL_H + GAP);
            for dy in [0.0, CELL_H / 2.0, CELL_H - 0.5] {
                let (launch, name) = if has_groups {
                    (Hit::GroupLaunch(0, count - 1), Hit::GroupName(0, count - 1))
                } else {
                    (Hit::ClipLaunch(0, count - 1), Hit::ClipName(0, count - 1))
                };
                assert_eq!(
                    hit_test_layout(Point::new(10.0, top + dy), &cols, count),
                    Some(launch),
                );
                assert_eq!(
                    hit_test_layout(Point::new(60.0, top + dy), &cols, count),
                    Some(name),
                );
            }
        }
    }
}

/// The responsive grid column: scene rows above each region's pinned footer.
fn grid_stack(node: &layout::Node) -> WidgetLayout<'_> {
    WidgetLayout::new(node)
        // The window is the first child of the overlay's stack, which is there whether or not a
        // menu is open (`overlay::view`).
        .children()
        .next()
        .unwrap()
        .children()
        .next()
        .unwrap()
        .children()
        .next()
        .unwrap()
        .children()
        .nth(1)
        .unwrap()
        // Workspace: optional Browser at the left, main column at the right.
        .children()
        .last()
        .unwrap()
        // The grid surface wraps the responsive grid widget.
        .children()
        .next()
        .unwrap()
        .children()
        .next()
        .unwrap()
}

/// The vertical region holding every scene row (tracks, output gap and Main).
pub(super) fn rows_layout(node: &layout::Node) -> WidgetLayout<'_> {
    // The scene rows sit under the pinned header row in a stack.
    grid_stack(node)
        .children()
        .next()
        .unwrap()
        .children()
        .next()
        .unwrap()
        .children()
        .next()
        .unwrap()
}

/// The pinned stop row and I/O band's scroll container, below the scene rows.
fn footer_tracks_layout(node: &layout::Node) -> WidgetLayout<'_> {
    let footer = grid_stack(node)
        .children()
        .next()
        .unwrap()
        .children()
        .nth(1)
        .unwrap();
    footer.children().next().unwrap()
}

/// The scrolling clip columns, inside the scene-row region.
pub(super) fn tracks_layout(node: &layout::Node) -> WidgetLayout<'_> {
    rows_layout(node)
        .children()
        .next()
        .unwrap()
        .children()
        .next()
        .unwrap()
        .children()
        .next()
        .unwrap()
}

/// Main's canvas, the last child of the scene-row region's row.
pub(super) fn main_layout(node: &layout::Node) -> WidgetLayout<'_> {
    rows_layout(node)
        .children()
        .next()
        .unwrap()
        .children()
        .next()
        .unwrap()
        .children()
        .last()
        .unwrap()
}

/// Presses the left button `presses` times in a row at `point` (relative to the tracks
/// area) through the real widget tree; returns the grid actions each press published.
pub(super) fn press_sequence(
    app: &App,
    renderer: &iced::Renderer,
    size: Size,
    point: Point,
    presses: usize,
) -> Vec<Vec<Action>> {
    press_button_sequence(app, renderer, size, point, mouse::Button::Left, presses)
}

/// As `press_sequence`, for any mouse button.
pub(super) fn press_button_sequence(
    app: &App,
    renderer: &iced::Renderer,
    size: Size,
    point: Point,
    button: mouse::Button,
    presses: usize,
) -> Vec<Vec<Action>> {
    let mut widget = crate::view::view(app);
    let mut tree = Tree::new(widget.as_widget());
    let node =
        widget
            .as_widget_mut()
            .layout(&mut tree, renderer, &layout::Limits::new(Size::ZERO, size));
    let origin = tracks_layout(&node).bounds().position();
    let point = Point::new(origin.x + point.x, origin.y + point.y);
    let clock = crate::gesture_clock::test::Clock::new();
    (0..presses)
        .map(|_| {
            clock.advance(std::time::Duration::from_millis(100));
            let mut messages = vec![];
            widget.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(mouse::Event::ButtonPressed(button)),
                WidgetLayout::new(&node),
                mouse::Cursor::Available(point),
                renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(size),
            );
            messages
                .into_iter()
                .filter_map(|message| match message {
                    // Focus claims are tested in `grid::focus::tests`.
                    Message::Grid(Action::ControlFocused(_)) => None,
                    Message::Grid(action) => Some(action),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

fn layout_clicks(
    app: &App,
    renderer: &iced::Renderer,
    size: Size,
    require_overflow: bool,
    slot: usize,
) -> Vec<Action> {
    let mut widget = crate::view::view(app);
    let mut tree = Tree::new(widget.as_widget());
    let node =
        widget
            .as_widget_mut()
            .layout(&mut tree, renderer, &layout::Limits::new(Size::ZERO, size));
    let tracks = tracks_layout(&node);
    let canvas = tracks.children().next().unwrap();
    if require_overflow {
        assert!(canvas.bounds().width > tracks.bounds().width);
    }
    let last = slot;
    let top = super::layout::header_height(&track_columns(&app.project().tracks))
        + last as f32 * (CELL_H + GAP);
    let origin = tracks.bounds().position();
    // Track columns only: a group track's slots act for the whole group.
    let cols = track_columns(&app.project().tracks);
    let fits = ((tracks.bounds().width - GAP) / (TRACK_W + GAP)).floor() as usize;
    let visible = cols
        .iter()
        .enumerate()
        .filter_map(|(ci, c)| Some((ci, c.track()?)))
        .collect::<Vec<_>>();
    // A narrow window may show only a group track, whose slots group tests cover.
    let visible = visible
        .into_iter()
        .filter(|(ci, _)| *ci < fits.max(1))
        .collect::<Vec<_>>();
    let mut actions = vec![];
    let (Some(&first), Some(&last_shown)) = (visible.first(), visible.last()) else {
        return actions;
    };
    for (ci, ti) in [first, last_shown] {
        for dy in [0.5, CELL_H / 2.0, CELL_H - 0.5] {
            for (dx, expected) in [
                (
                    10.0,
                    if app.project().tracks[ti]
                        .slots
                        .get(last)
                        .is_some_and(|slot| slot.clip.is_some())
                    {
                        Action::LaunchClip(app.project().tracks[ti].id, last)
                    } else {
                        Action::StopSlot(app.project().tracks[ti].id, last)
                    },
                ),
                (60.0, Action::Select(app.project().tracks[ti].id, last)),
            ] {
                let point = Point::new(
                    origin.x + GAP + ci as f32 * (TRACK_W + GAP) + dx,
                    origin.y + top + dy,
                );
                assert!(tracks.bounds().contains(point), "last row must be visible");
                let mut messages = vec![];
                widget.as_widget_mut().update(
                    &mut tree,
                    &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    WidgetLayout::new(&node),
                    mouse::Cursor::Available(point),
                    renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut Shell::new(&mut messages),
                    &Rectangle::with_size(size),
                );
                // A press in the grid also focuses it, and a button press claims the focus
                // outline (tested in `grid::focus::tests`); only the grid actions matter here.
                messages.retain(|message| {
                    !matches!(
                        message,
                        Message::SetFocus(_) | Message::Grid(Action::ControlFocused(_))
                    )
                });
                assert!(
                    matches!(messages.as_slice(), [Message::Grid(action)] if *action == expected),
                    "{} scenes, size={size:?}, point={point:?}, messages={messages:?}",
                    last + 1,
                );
                actions.push(expected);
            }
        }
    }
    if require_overflow {
        // The rail is hidden (2026-10-11), so prove the pinned footer still scrolls:
        // a sideways wheel over it mirrors a new horizontal offset to the clips.
        let bounds = footer_tracks_layout(&node).bounds();
        let point = Point::new(bounds.center_x(), bounds.center_y());
        let mut messages = vec![];
        for event in [
            // The desktop sends a redraw before input; Scrollable initialises its viewport on it.
            Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: -40.0, y: 0.0 },
            }),
        ] {
            widget.as_widget_mut().update(
                &mut tree,
                &event,
                WidgetLayout::new(&node),
                mouse::Cursor::Available(point),
                renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(size),
            );
        }
        assert!(
            messages.iter().any(|m| matches!(
                m,
                Message::Grid(Action::ScrollColumns { footer: true, offset, .. }) if *offset > 0
            )),
            "a sideways wheel over the footer must scroll it: {messages:?}"
        );
    }
    actions
}

fn apply_and_check_actions(app: &mut App, actions: Vec<Action>) {
    for action in actions {
        let _ = crate::update::update(app, Message::Grid(action.clone()));
        match action {
            Action::Select(track, slot) => {
                assert_eq!(app.selection.clip, Some(Selection { track, slot }))
            }
            Action::LaunchClip(track, slot) => {
                assert!(app.status.message().is_none());
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
                loop {
                    let snapshot = app.engine.snapshot();
                    if snapshot.tracks.iter().any(|t| {
                        t.id == track
                            && (t.playing_slot == Some(slot)
                                || t.triggered
                                    .as_ref()
                                    .is_some_and(|trigger| trigger.slot == slot))
                    }) {
                        break;
                    }
                    assert!(
                        std::time::Instant::now() < deadline,
                        "last-row launch must reach the scheduler"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
            _ => panic!("unexpected action"),
        }
    }
}

#[test]
fn last_scene_clicks_reach_the_grid_through_the_application_layout() {
    let renderer = renderer();
    for scene_count in [1, 8] {
        for has_groups in [false, true] {
            let mut app = App::new(
                Engine::new(project(scene_count, has_groups, 3)).unwrap(),
                "120".into(),
                None,
            );
            // Like the Browser, the output columns would leave a 220 px window no grid.
            app.chrome.outputs_open = false;
            app.chrome.strip_open = false;
            for size in [Size::new(800.0, 600.0), Size::new(220.0, 600.0)] {
                // A 220 px window fits the grid only with its Browser collapsed.
                for browser_open in [false, true] {
                    if browser_open && size.width < 800.0 {
                        continue;
                    }
                    app.chrome.browser_open = browser_open;
                    for detail_open in [false, true] {
                        app.chrome.detail_open = detail_open;
                        layout_clicks(&app, &renderer, size, false, scene_count - 1);
                    }
                }
            }
        }
    }
}

#[test]
fn horizontal_overflow_does_not_swallow_last_scene_launch_or_selection() {
    let renderer = renderer();
    for scene_count in [1, 8] {
        for size in [Size::new(800.0, 600.0), Size::new(1440.0, 900.0)] {
            let mut app = App::new(
                Engine::new(project(scene_count, false, 16)).unwrap(),
                "120".into(),
                None,
            );
            // The I/O and mixer bands take 192 px of a 600 px window's grid, as the outputs would.
            app.chrome.strip_open = false;
            for browser_open in [false, true] {
                app.chrome.browser_open = browser_open;
                for detail_open in [false, true] {
                    app.chrome.detail_open = detail_open;
                    let actions = layout_clicks(&app, &renderer, size, true, scene_count - 1);
                    apply_and_check_actions(&mut app, actions);
                }
            }
        }
    }
}

#[test]
fn fresh_new_project_has_eight_clickable_slots_and_can_create_and_launch() {
    use crate::messages::{EditMessage, FileMessage};
    let renderer = renderer();
    let mut app = App::new(Engine::new(Project::default()).unwrap(), "120".into(), None);
    app.chrome.strip_open = false;
    let send = |app: &mut App, message| {
        let _ = crate::update::update(app, message);
    };
    send(&mut app, Message::Files(FileMessage::New));
    assert_eq!(app.project().tracks.len(), 1);
    for slot in 0..8 {
        layout_clicks(&app, &renderer, Size::new(800.0, 600.0), false, slot);
    }
    let size = Size::new(800.0, 600.0);
    // The first row is a selectable model slot before any Add Scene action.
    let actions = layout_clicks(&app, &renderer, size, false, 0);
    apply_and_check_actions(
        &mut app,
        actions
            .into_iter()
            .filter(|action| matches!(action, Action::Select(..)))
            .collect(),
    );
    assert_eq!(app.project().scenes.len(), 8);
    assert_eq!(app.project().tracks[0].slots.len(), 8);
    send(&mut app, Message::Edit(EditMessage::CreateSelectedClip));
    assert!(app.project().tracks[0].slots[0].clip.is_some());
    assert!(app.path.is_none());
    let actions = layout_clicks(&app, &renderer, size, false, 0);
    apply_and_check_actions(&mut app, actions);

    send(&mut app, Message::Edit(EditMessage::AddScene));
    assert_eq!(app.project().scenes.len(), 9);
    assert_eq!(app.project().tracks[0].slots.len(), 9);
    for slot in [0, 1] {
        let actions = layout_clicks(&app, &renderer, size, false, slot);
        apply_and_check_actions(
            &mut app,
            actions
                .into_iter()
                .filter(|action| matches!(action, Action::Select(..)))
                .collect(),
        );
        if slot == 1 {
            send(&mut app, Message::Edit(EditMessage::CreateSelectedClip));
        }
        let actions = layout_clicks(&app, &renderer, size, false, slot);
        apply_and_check_actions(&mut app, actions);
    }
    // The space after the last real scene no longer masquerades as a slot.
    let cols = track_columns(&app.project().tracks);
    let below = HEADER_H + 9.0 * (CELL_H + GAP) + CELL_H / 2.0;
    assert_eq!(hit_test_layout(Point::new(60.0, below), &cols, 9), None);
}

/// Every scene row stays reachable with the I/O band showing, and the track, output-gap and
/// Main rows scroll as one region: at any offset, one screen y selects the same row in both.
#[test]
fn scene_rows_scroll_as_one_region_with_the_band_pinned() {
    use iced::advanced::widget::operation;
    let clock = crate::gesture_clock::test::Clock::new();
    let renderer = renderer();
    let size = Size::new(800.0, 700.0);
    for scene_count in [8usize, 30] {
        let app = App::new(
            Engine::new(project(scene_count, false, 3)).unwrap(),
            "120".into(),
            None,
        );
        assert!(app.chrome.strip_open, "the band shows by default");
        let mut widget = crate::view::view(&app);
        let mut tree = Tree::new(widget.as_widget());
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        let region = rows_layout(&node).bounds();
        let content_h = HEADER_H + scene_count as f32 * (CELL_H + GAP);
        let max_offset = (content_h - region.height).max(0.0);
        assert!(max_offset > 0.0, "{scene_count} scenes must need scrolling");
        let track_x = tracks_layout(&node).bounds().x + GAP + 60.0;
        let main_x = main_layout(&node).bounds().x + 40.0;
        for offset in [0.0, max_offset / 2.0, max_offset] {
            let mut scroll = operation::scrollable::scroll_to::<()>(
                iced::widget::Id::new("session-scene-rows"),
                operation::scrollable::AbsoluteOffset {
                    x: None,
                    y: Some(offset),
                },
            );
            widget.as_widget_mut().operate(
                &mut tree,
                WidgetLayout::new(&node),
                &renderer,
                &mut scroll,
            );
            let mut press = |x: f32, y: f32| {
                // Independent hit probes, not a double-click sequence.
                clock.advance(std::time::Duration::from_millis(400));
                let mut messages = vec![];
                widget.as_widget_mut().update(
                    &mut tree,
                    &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
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
            // Press the middle of each row that this offset shows fully.
            let mut seen = 0;
            for slot in 0..scene_count {
                let y = region.y + HEADER_H + slot as f32 * (CELL_H + GAP) - offset + CELL_H / 2.0;
                // The header row stays pinned over the top of the region.
                if y < region.y + HEADER_H + 1.0 || y > region.y + region.height - 1.0 {
                    continue;
                }
                seen += 1;
                assert_eq!(
                    press(track_x, y),
                    vec![Action::Select(TrackId(1), slot)],
                    "tracks, offset {offset}"
                );
                assert_eq!(
                    press(main_x, y),
                    vec![Action::ClickScene(SceneId(slot as u64 + 1))],
                    "Main, offset {offset}"
                );
            }
            assert!(seen > 0);
            // At any offset, the track header row is still there to press.
            assert_eq!(
                press(track_x, region.y + HEADER_H / 2.0),
                vec![Action::PressHeader(Box::new(Action::SelectTrack(
                    TrackId(1),
                    false
                )))],
                "pinned header, offset {offset}"
            );
            if offset == max_offset {
                // The last row sits flush with the bottom of the region, above the stop row.
                let y = region.y + HEADER_H + (scene_count - 1) as f32 * (CELL_H + GAP) - offset
                    + CELL_H / 2.0;
                assert!(y < region.y + region.height);
                assert_eq!(
                    press(track_x, y),
                    vec![Action::Select(TrackId(1), scene_count - 1)]
                );
            }
        }
    }
}

/// Header row pinned above the shared vertical scrollable.
pub(super) fn header_layout(node: &layout::Node) -> WidgetLayout<'_> {
    grid_stack(node)
        .children()
        .next()
        .unwrap()
        .children()
        .next()
        .unwrap()
        .children()
        .nth(1)
        .unwrap()
}
