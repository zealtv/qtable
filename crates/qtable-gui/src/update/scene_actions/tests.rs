//! Exercise the menu, pointer and keyboard routes against a real engine.
use crate::{
    app::App,
    focus::{self, Focus},
    grid,
    messages::{EditMessage, Message},
};
use iced::keyboard::{Key, Modifiers, key::Named};
use qtable_engine::{
    edit::ProjectEdit,
    engine::{Command, Engine},
    model::*,
};

fn app() -> App {
    let project = Project {
        scenes: (1..=7)
            .map(|n| Scene {
                id: SceneId(n * 10),
                name: format!("Scene {n}"),
                follow_actions: Default::default(),
            })
            .collect(),
        tracks: vec![Track::new(TrackId(1), "Track", 7)],
        ..Project::default()
    };
    App::new(Engine::new(project).unwrap(), "120".into(), None)
}

fn send(app: &mut App, message: Message) {
    let _ = crate::update::update(app, message);
}

fn click(app: &mut App, scene: u64, modifiers: Modifiers) {
    send(app, Message::ModifiersChanged(modifiers));
    send(app, Message::Grid(grid::Action::ClickScene(SceneId(scene))));
}

fn selected(app: &App) -> Vec<u64> {
    app.selection.scenes.members.iter().map(|s| s.0).collect()
}

fn ids(app: &App) -> Vec<u64> {
    app.project().scenes.iter().map(|s| s.id.0).collect()
}

fn delete_key(app: &mut App, key: Named) {
    let message = focus::event_message(&Key::Named(key), Modifiers::NONE, false, true).unwrap();
    send(app, message);
}

#[test]
fn scene_shift_ranges_keep_the_anchor_and_command_toggles_keep_project_order() {
    let mut app = app();
    let revision = app.project_revision();
    click(&mut app, 30, Modifiers::NONE);
    click(&mut app, 50, Modifiers::SHIFT);
    assert_eq!(selected(&app), [30, 40, 50]);
    assert_eq!(app.selection.scenes.anchor, Some(SceneId(30)));
    click(&mut app, 20, Modifiers::SHIFT);
    assert_eq!(selected(&app), [20, 30]);
    click(&mut app, 70, Modifiers::COMMAND);
    click(&mut app, 20, Modifiers::COMMAND);
    assert_eq!(selected(&app), [30, 70]);
    click(&mut app, 10, Modifiers::SHIFT);
    assert_eq!(
        selected(&app),
        [10, 20, 30, 70],
        "Shift keeps the disjoint base"
    );
    click(&mut app, 20, Modifiers::SHIFT);
    assert_eq!(
        selected(&app),
        [20, 30, 70],
        "shrinking releases only range additions"
    );
    assert_eq!(app.project_revision(), revision);
    assert!(!app.engine.snapshot().playing);
    assert_eq!(app.engine.snapshot().last_triggered_scene, None);
}

#[test]
fn scene_delete_and_backspace_remove_the_whole_range_in_one_revision() {
    for key in [Named::Delete, Named::Backspace] {
        let mut app = app();
        app.engine
            .execute(Command::Edit(ProjectEdit::CreateClip {
                track: TrackId(1),
                slot: 5,
                length: BeatTime(960),
            }))
            .unwrap();
        let survivor = app.project().tracks[0].slots[5].clone();
        click(&mut app, 20, Modifiers::NONE);
        click(&mut app, 40, Modifiers::SHIFT);
        let revision = app.project_revision();
        delete_key(&mut app, key);
        assert_eq!(app.project_revision(), revision + 1, "one atomic command");
        assert_eq!(ids(&app), [10, 50, 60, 70]);
        assert_eq!(selected(&app), [50]);
        assert_eq!(app.selection.cell.unwrap().row, 1);
        assert_eq!(app.project().tracks[0].slots[2], survivor);
        assert_eq!(app.project().tracks[0].slots.len(), 4);
    }
}

#[test]
fn scene_context_menu_preserves_a_selected_set_and_selects_an_unselected_scene() {
    let mut app = app();
    click(&mut app, 30, Modifiers::NONE);
    click(&mut app, 50, Modifiers::SHIFT);
    send(
        &mut app,
        Message::Grid(grid::Action::SceneMenu(SceneId(40))),
    );
    assert_eq!(selected(&app), [30, 40, 50]);
    let revision = app.project_revision();
    send(
        &mut app,
        Message::Edit(EditMessage::DeleteScenes(SceneId(40))),
    );
    assert_eq!(app.project_revision(), revision + 1);
    assert_eq!(ids(&app), [10, 20, 60, 70]);
    assert_eq!(selected(&app), [60]);
    send(
        &mut app,
        Message::Grid(grid::Action::SceneMenu(SceneId(10))),
    );
    assert_eq!(selected(&app), [10], "right click ignores held Shift");
}

#[test]
fn scene_noncontiguous_delete_lands_at_the_first_removed_position_and_tail_clamps() {
    let mut app = app();
    click(&mut app, 60, Modifiers::NONE);
    click(&mut app, 20, Modifiers::COMMAND);
    delete_key(&mut app, Named::Delete);
    assert_eq!(ids(&app), [10, 30, 40, 50, 70]);
    assert_eq!(selected(&app), [30]);
    click(&mut app, 70, Modifiers::NONE);
    delete_key(&mut app, Named::Backspace);
    assert_eq!(selected(&app), [50]);
}

#[test]
fn scene_last_scene_and_jump_rejections_keep_the_project_and_selection() {
    let mut app = app();
    click(&mut app, 10, Modifiers::NONE);
    click(&mut app, 70, Modifiers::SHIFT);
    let before = app.project().clone();
    let selection = app.selection.scenes.clone();
    delete_key(&mut app, Named::Delete);
    assert_eq!(app.project(), &before);
    assert_eq!(app.selection.scenes, selection);
    assert!(app.status.message().unwrap().contains("at least one scene"));

    app.engine
        .execute(Command::Edit(ProjectEdit::RemoveScenes {
            scenes: (2..=7).map(|n| SceneId(n * 10)).collect(),
        }))
        .unwrap();
    app.engine
        .execute(Command::Edit(ProjectEdit::CreateClip {
            track: TrackId(1),
            slot: 0,
            length: BeatTime(960),
        }))
        .unwrap();
    click(&mut app, 10, Modifiers::NONE);
    let before = app.project().clone();
    delete_key(&mut app, Named::Delete);
    assert_eq!(app.project(), &before, "the sole populated scene survives");
    assert_eq!(selected(&app), [10]);

    let mut app = self::app();
    let mut follow_actions = SceneFollowActions::default();
    follow_actions.actions.action_a = FollowAction::Jump(SceneId(30));
    app.engine
        .execute(Command::Edit(ProjectEdit::SetSceneFollowActions {
            scene: SceneId(10),
            follow_actions,
        }))
        .unwrap();
    click(&mut app, 20, Modifiers::NONE);
    click(&mut app, 30, Modifiers::SHIFT);
    let before = app.project().clone();
    delete_key(&mut app, Named::Delete);
    assert_eq!(app.project(), &before);
    assert_eq!(selected(&app), [20, 30]);
    assert!(app.status.message().unwrap().contains("Jump"));
}

#[test]
fn scene_keys_respect_fields_regions_and_repeat_routing() {
    for key in [Named::Delete, Named::Backspace] {
        assert!(focus::event_message(&Key::Named(key), Modifiers::NONE, false, false).is_none());
        assert!(focus::event_message(&Key::Named(key), Modifiers::NONE, true, true).is_none());
        for region in [Focus::Detail, Focus::Browser] {
            let mut app = app();
            click(&mut app, 30, Modifiers::NONE);
            app.focus = region;
            let revision = app.project_revision();
            delete_key(&mut app, key);
            assert_eq!(app.project_revision(), revision);
        }
    }
}

#[test]
fn scene_programmatic_navigation_is_single_and_leaving_scenes_clears_the_set() {
    let mut app = app();
    click(&mut app, 30, Modifiers::NONE);
    click(&mut app, 50, Modifiers::SHIFT);
    send(
        &mut app,
        Message::Grid(grid::Action::SelectScene(SceneId(60))),
    );
    assert_eq!(selected(&app), [60]);
    send(&mut app, Message::Grid(grid::Action::Select(TrackId(1), 1)));
    assert!(selected(&app).is_empty());
    click(&mut app, 20, Modifiers::COMMAND);
    assert_eq!(selected(&app), [20]);
    click(&mut app, 20, Modifiers::COMMAND);
    assert!(selected(&app).is_empty());
    assert!(app.selection.scene.is_none());
    assert!(app.selection.cell.is_none());
}

#[test]
fn scene_selection_ids_and_focused_cell_follow_reordering() {
    let mut app = app();
    click(&mut app, 30, Modifiers::NONE);
    click(&mut app, 50, Modifiers::SHIFT);
    send(&mut app, Message::Edit(EditMessage::ReorderScene(-1)));
    assert_eq!(ids(&app), [10, 20, 30, 50, 40, 60, 70]);
    assert_eq!(selected(&app), [30, 50, 40]);
    assert_eq!(app.selection.scene, Some(SceneId(50)));
    assert_eq!(app.selection.scenes.anchor, Some(SceneId(30)));
    assert_eq!(app.selection.cell.unwrap().row, 3);
}

#[test]
fn scene_drag_moves_disjoint_selection_once_and_keeps_focus_with_its_scene() {
    let mut app = app();
    click(&mut app, 20, Modifiers::NONE);
    click(&mut app, 40, Modifiers::COMMAND);
    let revision = app.project_revision();
    let scenes = app.selection.scenes.members.clone();
    send(
        &mut app,
        Message::Grid(grid::Action::MoveScenes {
            scenes,
            boundary: 7,
        }),
    );
    assert_eq!(ids(&app), [10, 30, 50, 60, 70, 20, 40]);
    assert_eq!(selected(&app), [20, 40]);
    assert_eq!(app.selection.scene, Some(SceneId(40)));
    assert_eq!(app.selection.cell.unwrap().row, 6);
    assert_eq!(app.project_revision(), revision + 1);
}
