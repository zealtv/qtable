//! Main's global Follow Actions button drives the engine's global command.
use super::update;
use crate::app::App;
use crate::grid;
use crate::messages::Message;
use qtable_engine::engine::Engine;
use qtable_engine::model::Project;

#[test]
fn the_follow_button_flips_the_global_switch_each_click() {
    let engine = Engine::new(Project::default()).expect("engine starts");
    let mut app = App::new(engine, String::new(), None);
    let before = app.project().follow_actions_enabled;
    let _ = update(&mut app, Message::Grid(grid::Action::ToggleFollowActions));
    assert_eq!(app.project().follow_actions_enabled, !before);
    let _ = update(&mut app, Message::Grid(grid::Action::ToggleFollowActions));
    assert_eq!(app.project().follow_actions_enabled, before);
}
