//! Cell-body clicks highlight the same row in every region without launching playback.
use super::*;
use crate::{app::App, inspector_panel::Tab, messages::Message};
use iced::advanced::{Layout as WidgetLayout, Shell, layout, widget::Tree};
use iced::{Event, Size};
use qtable_engine::{engine::Engine, model::Project};

fn click_cell(kind: &str) {
    let mut project = Project {
        tracks: vec![Track::new(TrackId(1), "Track", 3)],
        outputs: vec![Track::new_output(TrackId(201), "Output", "127.0.0.1", 9000)],
        scenes: (1..=3)
            .map(|id| Scene {
                id: SceneId(id),
                name: format!("Scene {id}"),
                follow_actions: Default::default(),
            })
            .collect(),
        ..Project::default()
    };
    project.insert_group(Track::new_group(TrackId(101), "Group", 3), &[TrackId(1)]);
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    let renderer = test_renderer();
    let size = Size::new(1280.0, 800.0);
    // Start with another row selected to catch stale highlights.
    let _ = crate::update::update(&mut app, Message::Grid(Action::Select(TrackId(1), 0)));
    let revision = app.project_revision();
    let messages = {
        let mut widget = crate::view::view(&app);
        let mut tree = Tree::new(widget.as_widget());
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        let rows = last_row_tests::rows_layout(&node).bounds();
        let tracks = last_row_tests::tracks_layout(&node).bounds();
        let main = last_row_tests::main_layout(&node).bounds();
        let cols = track_columns(&app.project().tracks);
        let xs = x_positions(&cols);
        let x = match kind {
            "track" => tracks.x + xs[1] + 60.0,
            "group" => tracks.x + xs[0] + 60.0,
            "output" => main.x - 6.0 - crate::outputs::width(&app) + COLUMN_W / 2.0,
            "main" => main.x + main.width - 2.0,
            "middle" => main.x - crate::outputs::width(&app) - 8.0,
            _ => unreachable!(),
        };
        let at = Point::new(x, rows.y + HEADER_H + CELL_H + GAP + CELL_H / 2.0);
        let mut messages = vec![];
        widget.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            WidgetLayout::new(&node),
            mouse::Cursor::Available(at),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
        messages
    };
    let expected = match kind {
        "track" => Action::Select(TrackId(1), 1),
        "group" => {
            Action::SelectColumn(crate::selection::ColumnOwner::Group(TrackId(101)), Some(1))
        }
        "output" => {
            Action::SelectColumn(crate::selection::ColumnOwner::Track(TrackId(201)), Some(1))
        }
        "main" => Action::ClickScene(SceneId(2)),
        _ => Action::SelectScene(SceneId(2)),
    };
    let actions: Vec<_> = messages
        .iter()
        .filter_map(|m| match m {
            Message::Grid(a) => Some(a.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(actions, vec![expected], "cell body must only select");
    for message in messages {
        let _ = crate::update::update(&mut app, message);
    }
    assert_eq!(
        scene_area::selected_row(
            app.selection.clip,
            app.selection.scene,
            app.selection.cell,
            &app.project().scenes
        ),
        Some(1)
    );
    assert_eq!(
        app.selection.inspector_tab,
        match kind {
            "track" => Tab::Clip,
            "group" => Tab::Track,
            "output" => Tab::Output,
            _ => Tab::Scene,
        }
    );
    assert_eq!(app.project_revision(), revision);
    let snapshot = app.engine.snapshot();
    assert!(!snapshot.playing);
    assert_eq!(snapshot.last_triggered_scene, None);
    assert_eq!(snapshot.queued_follow_scene, None);
    assert!(
        snapshot
            .tracks
            .iter()
            .all(|t| t.playing_slot.is_none() && t.triggered.is_none())
    );
    if matches!(kind, "group" | "output") {
        assert_eq!(
            app.selection.scene, None,
            "row highlight must not open scene properties"
        );
        let owner = app.selection.owner.unwrap();
        let _ = crate::update::update(&mut app, Message::Grid(Action::SelectColumn(owner, None)));
        assert_eq!(
            scene_area::selected_row(
                app.selection.clip,
                app.selection.scene,
                app.selection.cell,
                &app.project().scenes
            ),
            None,
            "header ghosts must not highlight a row"
        );
    }
}

#[test]
fn track_cell_highlights_without_launching() {
    click_cell("track");
}
#[test]
fn group_cell_highlights_without_launching() {
    click_cell("group");
}
#[test]
fn output_cell_highlights_without_launching() {
    click_cell("output");
}
#[test]
fn main_cell_highlights_without_launching() {
    click_cell("main");
}
#[test]
fn middle_cell_highlights_without_launching() {
    click_cell("middle");
}

#[test]
fn scene_drag_through_composed_grid_preserves_block_until_one_drop_command() {
    let project = Project {
        tracks: vec![Track::new(TrackId(1), "Track", 7)],
        outputs: vec![Track::new_output(TrackId(201), "Output", "127.0.0.1", 9000)],
        scenes: (1..=7)
            .map(|id| Scene {
                id: SceneId(id),
                name: format!("Scene {id}"),
                follow_actions: Default::default(),
            })
            .collect(),
        ..Project::default()
    };
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    for action in [
        Action::ClickScene(SceneId(3)),
        Action::ClickScene(SceneId(4)),
    ] {
        if matches!(action, Action::ClickScene(SceneId(4))) {
            app.selection.shift_down = true;
        }
        let _ = crate::update::update(&mut app, Message::Grid(action));
    }
    app.selection.shift_down = false;
    let revision = app.project_revision();
    let renderer = test_renderer();
    let size = Size::new(1280.0, 800.0);
    let messages = {
        let mut widget = crate::view::view(&app);
        let mut tree = Tree::new(widget.as_widget());
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        let rows = last_row_tests::rows_layout(&node).bounds();
        let main = last_row_tests::main_layout(&node).bounds();
        let from = Point::new(
            main.x + main.width - 3.0,
            rows.y + draw::row_y(3) + CELL_H / 2.0,
        );
        let to = Point::new(rows.x + 30.0, rows.y + draw::row_y(1));
        let mut result = vec![];
        for (event, at) in [
            (
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                from,
            ),
            (Event::Mouse(mouse::Event::CursorMoved { position: to }), to),
            (
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                to,
            ),
        ] {
            widget.as_widget_mut().update(
                &mut tree,
                &event,
                WidgetLayout::new(&node),
                mouse::Cursor::Available(at),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut result),
                &Rectangle::with_size(size),
            );
        }
        result
    };
    let actions: Vec<_> = messages
        .into_iter()
        .filter_map(|m| {
            if let Message::Grid(a) = m {
                Some(a)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        actions,
        [Action::MoveScenes {
            scenes: vec![SceneId(3), SceneId(4)],
            boundary: 1
        }]
    );
    for action in actions {
        let _ = crate::update::update(&mut app, Message::Grid(action));
    }
    assert_eq!(
        app.project()
            .scenes
            .iter()
            .map(|s| s.id.0)
            .collect::<Vec<_>>(),
        [1, 3, 4, 2, 5, 6, 7]
    );
    assert_eq!(app.selection.scenes.members, [SceneId(3), SceneId(4)]);
    assert_eq!(app.project_revision(), revision + 1);
}
