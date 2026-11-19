use super::update;
use crate::app::App;
use crate::focus::{Direction, Focus, GridKey};
use crate::messages::{EditMessage, Message};
use crate::{files, grid};
use qtable_engine::engine::Engine;

fn app() -> App {
    let project = files::new_project();
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    let _ = update(&mut app, Message::Edit(EditMessage::AddTrack));
    app.focus = Focus::Grid;
    app
}

fn key(app: &mut App, direction: Direction) {
    let _ = update(app, Message::GridKey(GridKey::Move(direction)));
}

#[test]
fn arrows_walk_slots_scenes_and_track_titles() {
    let mut app = app();
    let tracks: Vec<_> = app.project().tracks.iter().map(|t| t.id).collect();
    key(&mut app, Direction::Down);
    assert_eq!(
        app.selection.clip,
        Some(grid::Selection {
            track: tracks[0],
            slot: 0
        })
    );
    key(&mut app, Direction::Right);
    assert_eq!(app.selection.clip.unwrap().track, tracks[1]);
    key(&mut app, Direction::Right);
    assert_eq!(app.selection.clip, None);
    assert_eq!(app.selection.scene, Some(app.project().scenes[0].id));
    key(&mut app, Direction::Down);
    assert_eq!(app.selection.scene, Some(app.project().scenes[1].id));
    key(&mut app, Direction::Left);
    assert_eq!(
        app.selection.clip,
        Some(grid::Selection {
            track: tracks[1],
            slot: 1
        })
    );
    key(&mut app, Direction::Up);
    key(&mut app, Direction::Up);
    assert_eq!(app.selection.clip, None);
    assert_eq!(app.selection.tracks, vec![tracks[1]]);
    key(&mut app, Direction::Left);
    assert_eq!(app.selection.tracks, vec![tracks[0]]);
}

#[test]
fn grid_keys_do_nothing_unless_the_grid_has_focus() {
    let mut app = app();
    app.focus = Focus::Detail;
    key(&mut app, Direction::Down);
    assert_eq!(app.selection.clip, None);
}

#[test]
fn command_arrows_reorder_the_selected_track_and_scene() {
    let mut app = app();
    let first = app.project().tracks[0].id;
    let scene = app.project().scenes[0].id;
    key(&mut app, Direction::Down);
    let _ = update(
        &mut app,
        Message::GridKey(GridKey::Reorder(Direction::Right)),
    );
    assert_eq!(app.project().tracks[1].id, first);
    let _ = update(
        &mut app,
        Message::GridKey(GridKey::Reorder(Direction::Down)),
    );
    assert_eq!(app.project().scenes[1].id, scene);
    assert_eq!(app.selection.clip.unwrap().slot, 1);
}

#[test]
fn clicks_and_focus_keys_move_the_active_region() {
    let mut app = app();
    let _ = update(&mut app, Message::SetFocus(Focus::Browser));
    assert_eq!(app.focus, Focus::Browser);
    let _ = update(&mut app, Message::Edit(EditMessage::AddScene));
    assert_eq!(app.focus, Focus::Detail);
    let track = app.project().tracks[0].id;
    let _ = update(&mut app, Message::Grid(grid::Action::Select(track, 0)));
    assert_eq!(app.focus, Focus::Grid);
}
