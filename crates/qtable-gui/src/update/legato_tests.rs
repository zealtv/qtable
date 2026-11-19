//! The clip view's Legato toggle edits the clip through the engine.
use super::update;
use crate::app::App;
use crate::edit_actions::create_clip;
use crate::inspector;
use crate::messages::{EditMessage, Message};
use qtable_engine::engine::Engine;
use qtable_engine::model::Project;

#[test]
fn the_legato_toggle_flips_the_clips_setting_each_click() {
    let engine = Engine::new(Project::default()).expect("engine starts");
    let mut app = App::new(engine, String::new(), None);
    let _ = update(&mut app, Message::Edit(EditMessage::AddTrack));
    let _ = update(&mut app, Message::Edit(EditMessage::AddScene));
    let track = app.project().tracks[0].id;
    create_clip(&mut app, track, 0);
    let legato = |app: &App| {
        app.project().tracks[0].slots[0]
            .clip
            .as_ref()
            .unwrap()
            .legato
    };
    assert!(!legato(&app));
    let toggle = || Message::Inspector(inspector::Message::ToggleLegato(track, 0));
    let _ = update(&mut app, toggle());
    assert!(legato(&app));
    let _ = update(&mut app, toggle());
    assert!(!legato(&app));
}
