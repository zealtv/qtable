use super::*;

/// Native capture setup through the same semantic actions as the GUI. A long real
/// quantisation boundary leaves ample screenshot time; only the presentation phase
/// is held for a still. Engine snapshots keep updating normally.
pub(crate) fn capture_session(app: &mut crate::app::App, scenario: &str) -> bool {
    use crate::{
        edit_actions::execute,
        grid::Action,
        messages::{Message, OutputMessage},
    };
    use qtable_engine::{edit::ProjectEdit, engine::Command, model::GlobalQuantisation};
    let elapsed = match scenario {
        "scene-stop-burst" => Some(0),
        "scene-stop-on" => Some(200),
        "scene-stop-off" => Some(400),
        "scene-stop-landed" => None,
        _ => return false,
    };
    let send = |app: &mut crate::app::App, action| {
        let _ = crate::update::update(app, Message::Grid(action));
    };
    let track = app
        .project()
        .clip_tracks()
        .next()
        .expect("capture track")
        .id;
    send(app, Action::CreateClip(track, 0));
    send(app, Action::AddTrack);
    send(app, Action::AddTrack);
    let disabled = app.project().clip_tracks().last().unwrap().id;
    crate::edit_actions::execute_edit(
        app,
        Command::Edit(ProjectEdit::SetSlotStopButton {
            track: disabled,
            slot: 1,
            enabled: false,
        }),
    );
    send(app, Action::SelectTrack(track, false));
    for _ in 0..2 {
        let _ = crate::update::update(
            app,
            Message::Outputs(OutputMessage::CreateGroupAroundSelected),
        );
    }
    let tempo = if elapsed.is_some() { "30.00" } else { "120.00" };
    let _ = crate::update::update(app, Message::TempoChanged(tempo.into()));
    let _ = crate::update::update(app, Message::TempoSubmitted);
    execute(
        app,
        Command::SetGlobalQuantisation(if elapsed.is_some() {
            GlobalQuantisation::CHOICES[1]
        } else {
            GlobalQuantisation::default()
        }),
    );
    send(app, Action::LaunchClip(track, 0));
    wait_snapshot(app, |s| {
        s.tracks
            .iter()
            .any(|t| t.id == track && t.playing_slot == Some(0))
    });
    // Leave beat zero before requesting a quantised stop.
    std::thread::sleep(Duration::from_millis(20));
    let scene = app.project().scenes[1].id;
    send(app, Action::SelectScene(scene));
    send(app, Action::LaunchScene(scene));
    wait_snapshot(app, |s| s.pending_stop_slots.contains(&(track, 1)));
    if let Some(ms) = elapsed {
        app.scene_stop_feedback.capture_elapsed = Some(Duration::from_millis(ms));
    } else {
        // Let the scene stop land at its real musical boundary.
        wait_snapshot(app, |s| {
            s.pending_stop_slots.is_empty() && s.tracks.iter().all(|t| t.playing_slot.is_none())
        });
        std::thread::sleep(BURST);
    }
    true
}

fn wait_snapshot(app: &mut crate::app::App, ready: impl Fn(&Snapshot) -> bool) {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        let _ = crate::update::update(app, crate::messages::Message::Tick);
        if ready(&app.snapshot) {
            return;
        }
        assert!(
            Instant::now() < until,
            "scene-stop capture state did not arrive"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
