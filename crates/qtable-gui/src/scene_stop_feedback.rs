//! One presentation clock for scene stop acknowledgements; pending stops stay engine-owned.
use qtable_engine::engine::Snapshot;
use qtable_engine::model::{Project, SceneId};
use std::collections::HashMap;
use std::time::{Duration, Instant};

// Observed in a screen recording (2026-11-07), not a tempo formula.
pub(super) const BURST: Duration = Duration::from_millis(100);
const PULSE_START: Duration = Duration::from_millis(150);
const HALF_CYCLE: Duration = Duration::from_millis(200);

#[derive(Default)]
pub(crate) struct State {
    launched: HashMap<SceneId, Instant>,
    // Native stills hold a sampled presentation phase, never engine playback state.
    capture_elapsed: Option<Duration>,
}

/// A sampled row phase, shared by ordinary and group cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    pub slot: usize,
    pub burst: bool,
    pub pending_on: bool,
}

impl State {
    pub(crate) fn launched(&mut self, scene: SceneId, now: Instant) {
        self.launched.insert(scene, now);
    }

    pub(crate) fn rows(&self, project: &Project, snapshot: &Snapshot, now: Instant) -> Vec<Row> {
        project
            .scenes
            .iter()
            .enumerate()
            .filter_map(|(slot, scene)| {
                let started = *self.launched.get(&scene.id)?;
                let elapsed = self
                    .capture_elapsed
                    .unwrap_or_else(|| now.saturating_duration_since(started));
                let burst = elapsed < BURST;
                let pending = snapshot
                    .pending_stop_slots
                    .iter()
                    .any(|(_, row)| *row == slot);
                (burst || pending).then_some(Row {
                    slot,
                    burst,
                    pending_on: elapsed >= PULSE_START
                        && ((elapsed - PULSE_START).as_millis() / HALF_CYCLE.as_millis())
                            .is_multiple_of(2),
                })
            })
            .collect()
    }

    pub(crate) fn prune(&mut self, project: &Project, snapshot: &Snapshot, now: Instant) {
        self.launched.retain(|scene, started| {
            project
                .scenes
                .iter()
                .position(|s| s.id == *scene)
                .is_some_and(|slot| {
                    self.capture_elapsed.is_some()
                        || now.saturating_duration_since(*started) < Duration::from_millis(500)
                        || snapshot
                            .pending_stop_slots
                            .iter()
                            .any(|(_, row)| *row == slot)
                })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtable_engine::{engine::Engine, model::TrackId};

    #[test]
    fn only_accepted_scene_commands_start_a_burst_and_project_replacement_resets_it() {
        use qtable_engine::engine::Command;
        let mut app = crate::app::App::new(
            Engine::new(crate::files::new_project()).unwrap(),
            "120".into(),
            None,
        );
        let scene = app.project().scenes[1].id;
        crate::edit_actions::execute(
            &mut app,
            Command::LaunchScene {
                scene: SceneId(999),
            },
        );
        assert!(app.scene_stop_feedback.launched.is_empty());
        crate::edit_actions::execute(&mut app, Command::LaunchScene { scene });
        assert!(app.scene_stop_feedback.launched.contains_key(&scene));
        app.reset_for_project();
        assert!(app.scene_stop_feedback.launched.is_empty());
    }

    #[test]
    fn burst_then_pending_only_pulses_and_landing_clears_feedback() {
        let project = crate::files::new_project();
        let mut snapshot = Engine::new(project.clone()).unwrap().snapshot();
        let scene = project.scenes[1].id;
        let now = Instant::now();
        let mut state = State::default();
        state.launched(scene, now);
        assert!(state.rows(&project, &snapshot, now)[0].burst);
        assert!(state.rows(&project, &snapshot, now + BURST).is_empty());
        snapshot.pending_stop_slots = vec![(TrackId(1), 1)];
        for (ms, on) in [
            (100, false),
            (150, true),
            (349, true),
            (350, false),
            (550, true),
        ] {
            let rows = state.rows(&project, &snapshot, now + Duration::from_millis(ms));
            assert_eq!(
                rows,
                vec![Row {
                    slot: 1,
                    burst: false,
                    pending_on: on
                }]
            );
        }
        snapshot.pending_stop_slots.clear();
        assert!(
            state
                .rows(&project, &snapshot, now + Duration::from_secs(1))
                .is_empty()
        );
        state.prune(&project, &snapshot, now + Duration::from_secs(1));
        assert!(state.launched.is_empty());
    }
}

#[cfg(any(test, feature = "dev-hooks"))]
mod capture;
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) use capture::capture_session;
