//! Keep playback smooth and continue supervision polling when transport is idle.
use crate::app::App;
use std::time::{Duration, Instant};

pub(crate) fn tick_interval(app: &App) -> Duration {
    let snapshot = &app.snapshot;
    let animating = app.remote.activity_level() > 0.0
        || snapshot.playing
        || !snapshot.track_activity.is_empty()
        || !snapshot.output_activity.is_empty()
        || !snapshot.pending_stops.is_empty()
        || snapshot
            .tracks
            .iter()
            .any(|track| track.triggered.is_some())
        || app
            .fast_poll_until
            .is_some_and(|until| Instant::now() < until);
    Duration::from_millis(if animating { 33 } else { 250 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drafts::TrackField;
    use crate::{
        drafts::DraftKey,
        grid,
        messages::{EditMessage, FileMessage, Message},
        update::update,
    };
    use qtable_engine::edit::ProjectEdit;
    use qtable_engine::engine::{Command, Engine, TriggeredSlot};
    use qtable_engine::model::{BeatTime, Project};

    fn app() -> App {
        let project = Project::default();
        let engine = Engine::new(project.clone()).expect("engine starts");
        App::new(engine, String::new(), None)
    }

    #[test]
    fn idle_polling_slows_down_but_playback_and_pending_launches_keep_animating() {
        let mut app = app();
        assert_eq!(tick_interval(&app), Duration::from_millis(250));
        app.snapshot.playing = true;
        assert_eq!(tick_interval(&app), Duration::from_millis(33));
        app.snapshot.playing = false;

        let _ = update(&mut app, Message::Edit(EditMessage::AddTrack));
        app.snapshot.tracks[0].triggered = Some(TriggeredSlot {
            slot: 0,
            launch_beat: BeatTime(0),
        });
        assert_eq!(tick_interval(&app), Duration::from_millis(33));
        app.snapshot.tracks[0].triggered = None;
        app.snapshot
            .pending_stops
            .push((app.project().tracks[0].id, BeatTime(960)));
        assert_eq!(tick_interval(&app), Duration::from_millis(33));
        app.snapshot.pending_stops.clear();
        assert_eq!(tick_interval(&app), Duration::from_millis(250));
    }

    #[test]
    fn accepted_scheduler_command_polls_promptly_until_state_arrives() {
        let mut app = app();
        let _ = update(&mut app, Message::Play);
        assert_eq!(tick_interval(&app), Duration::from_millis(33));
        // Expired grace periods do not keep a stopped project at 30 fps.
        app.fast_poll_until = Some(Instant::now() - Duration::from_secs(1));
        assert_eq!(tick_interval(&app), Duration::from_millis(250));
    }

    #[test]
    fn edits_and_project_replacement_invalidate_cached_layout() {
        let mut app = app();
        let initial = app.project_revision();
        let _ = update(&mut app, Message::Edit(EditMessage::AddTrack));
        assert_ne!(app.project_revision(), initial);
        let track = app.project().tracks[0].id;
        let before_inspector = app.project_revision();
        crate::inspector_update::execute_inspector_edit(
            &mut app,
            DraftKey::Track(track, TrackField::Name),
            ProjectEdit::RenameTrack {
                track,
                name: "New".into(),
            },
        );
        assert_ne!(app.project_revision(), before_inspector);
        let before_rejected = app.project_revision();
        crate::edit_actions::execute_edit(
            &mut app,
            Command::Edit(ProjectEdit::RemoveTracks {
                tracks: vec![qtable_engine::model::TrackId(999)],
            }),
        );
        assert_eq!(app.project_revision(), before_rejected);
        app.mark_saved();
        let _ = update(&mut app, Message::Files(FileMessage::New));
        assert_ne!(app.project_revision(), before_rejected);
        assert!(app.fast_poll_until.is_none());
        assert_eq!(app.project(), &crate::files::new_project());
        assert_eq!(
            app.selection.inspector_tab,
            crate::inspector_panel::Tab::Clip
        );
        // Keep the test checking the same generation used by the canvas view.
        let _ = grid::view(
            app.project(),
            &app.snapshot,
            grid::ViewState {
                ghost: None,
                owner: None,
                cell: None,
                selected: None,
                selected_scene: None,
                selected_scenes: &[],
                selected_tracks: &[],
                shift_down: false,
                scene_stop: Vec::new(),
                blink_on: true,
                revision: app.project_revision(),
            },
            &app.grid_layout,
        );
    }
}
