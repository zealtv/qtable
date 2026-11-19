//! The engine owner's remote-input command path.
use crate::{App, remote::Action};
use qtable_engine::engine::Command;

pub(super) fn update(app: &mut App, action: Action) {
    match action {
        Action::Wake => {
            // Remote wakes independently of Tick/redraw; use current published values.
            app.snapshot = app.engine.snapshot();
            let (drain, commands) = if app.map_mode.on {
                // Learning must not mutate consumers' source history or Pick-Up state
                // with gestures whose commands will never be applied.
                (app.remote.ingress.drain(), Vec::new())
            } else {
                // The resolver follows the scene selection, so an encoder spun past either end
                // owes nothing; the host's own clamp still applies the moves.
                let selected = crate::grid::scene_area::selected_row(
                    app.selection.clip,
                    app.selection.scene,
                    app.selection.cell,
                    &app.project().scenes,
                );
                app.remote.router.drain_actions(
                    &app.remote.ingress,
                    app.engine.project(),
                    &app.snapshot,
                    selected,
                )
            };
            // In Map Mode a controller teaches the selected control and operates nothing.
            if app.map_mode.on {
                crate::map_mode::learn(app, &drain.events);
            }
            let dropped = drain.dropped.total();
            app.remote.observe(drain);
            let port_error = app.remote.refresh_ports();
            for action in commands {
                let continuous = action.continuous.clone();
                let context = action
                    .receipt
                    .map(|receipt| app.engine.command_context(receipt));
                let previous = std::mem::replace(&mut app.command_context, context);
                let command = match action.value {
                    qtable_remote::Action::Command(command) => command,
                    qtable_remote::Action::View(action) => {
                        let Some(command) = apply_view(app, action) else {
                            app.command_context = previous;
                            continue;
                        };
                        command
                    }
                };
                select_slot(app, &command);
                // A controller has no release: each control taking continuous input keeps one
                // undo gesture token while the set stays loaded; everything else sends none.
                crate::gesture_steps::remote(app, continuous.as_ref(), |app| {
                    if matches!(command, Command::Edit(_)) {
                        crate::edit_actions::execute_edit(app, command);
                    } else {
                        let tempo = matches!(command, Command::SetTempo(_));
                        crate::edit_actions::execute(app, command);
                        if tempo && app.pending_field != Some(crate::drafts::DraftKey::Tempo) {
                            app.tempo_input =
                                crate::fields::format_tempo(app.project().tempo.bpm());
                        }
                    }
                });
                app.command_context = previous;
            }
            // Successful mapped edits clear transient status. Report ingress errors
            // afterwards so a CC flood cannot erase its own overload diagnostic.
            if let Some(error) = port_error {
                app.status.show_error(error);
            }
            if dropped != 0 {
                app.status
                    .show_error(format!("MIDI input overloaded: {dropped} messages dropped"));
            }
        }
        Action::SetRemote(name, enabled) => {
            if let Err(error) = app.remote.set_remote(name, enabled) {
                app.status.show_error(error);
            }
        }
        Action::Error(error) => app.status.show_error(error),
    }
}

/// View-only work uses the same actions as clicking a header or scene. Resolve each
/// action against the latest selection, including preceding actions in this drain.
fn apply_view(app: &mut App, action: qtable_remote::ViewAction) -> Option<Command> {
    use crate::{grid, selection::ColumnOwner};
    use qtable_engine::model::Region;
    use qtable_remote::ViewAction;
    let current = grid::scene_area::selected_row(
        app.selection.clip,
        app.selection.scene,
        app.selection.cell,
        &app.project().scenes,
    );
    let count = app.project().scenes.len();
    let row = match action {
        ViewAction::SelectTrack(track) => {
            let caps = app.project().track_capabilities(track)?;
            let action = if caps.foldable {
                grid::Action::SelectGroup(track, false)
            } else if caps.region == Region::Tracks {
                grid::Action::SelectTrack(track, false)
            } else {
                grid::Action::SelectColumn(ColumnOwner::Track(track), None)
            };
            super::grid_actions::update(app, action);
            return None;
        }
        ViewAction::LaunchScene => {
            let row = current?;
            let launched = app.project().scenes.get(row)?.id;
            // Live's "Select Next Scene on Launch" (on by default; Session View manual: the
            // scene below a launched scene is selected as the next to be launched). It acts as
            // the press lands, not at the quantized launch, and stops at the last scene.
            if let Some(next) = app.project().scenes.get(row + 1).map(|scene| scene.id) {
                super::grid_actions::update(app, grid::Action::SelectScene(next));
            }
            return Some(Command::LaunchScene { scene: launched });
        }
        _ if count == 0 => return None,
        ViewAction::MoveScene(_) | ViewAction::SelectScene(_) | ViewAction::NextScene => {
            action.scene_after(current, count)?
        }
    };
    let scene = app.project().scenes[row].id;
    super::grid_actions::update(app, grid::Action::SelectScene(scene));
    None
}

/// Mapped slots operate the same selection as a cell click. The scene highlight
/// derives from this cell, while the Detail View shows the launched clip.
fn select_slot(app: &mut App, command: &Command) {
    use crate::{grid, selection::ColumnOwner};
    let action = match *command {
        Command::LaunchClip { track, slot } => {
            // A controller launch is never the destination of an armed Move Clip action.
            app.selection.move_clip = false;
            grid::Action::Select(track, slot)
        }
        Command::LaunchGroupSlot { group, slot } => {
            grid::Action::SelectColumn(ColumnOwner::Group(group), Some(slot))
        }
        _ => return,
    };
    super::grid_actions::update(app, action);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Message;
    use qtable_engine::{engine::Engine, model::Project};
    use qtable_remote::{Endpoint, Input};

    #[test]
    fn incoming_midi_is_observed_without_editing_the_set_or_committing_a_draft() {
        let mut app = App::new(
            Engine::new(Project::default()).unwrap(),
            "120.00".into(),
            None,
        );
        let _ = crate::update::update(&mut app, Message::TempoChanged("131".into()));
        let revision = app.project_revision();
        app.remote.ingress.push(
            Endpoint::Midi {
                id: "test".into(),
                name: "Controller".into(),
            },
            Input::Midi(vec![0x90, 60, 100].into_boxed_slice()),
        );
        let _ = crate::update::update(&mut app, Message::Remote(Action::Wake));
        assert_eq!(app.remote.activity, 1);
        assert!(app.remote.activity_level() > 0.0);
        assert_eq!(app.remote.debug_log.len(), 1);
        assert_eq!(app.project_revision(), revision);
        assert_eq!(app.tempo_input, "131");
        assert_eq!(app.pending_field, Some(crate::drafts::DraftKey::Tempo));
        assert!(!app.is_dirty());
    }

    #[test]
    fn overflow_reaches_status_bar_and_debug_history_stays_bounded() {
        let mut app = App::new(
            Engine::new(Project::default()).unwrap(),
            "120.00".into(),
            None,
        );
        for _ in 0..qtable_remote::ingress::CONTINUOUS_CAPACITY + 9 {
            app.remote
                .ingress
                .push_midi(&Endpoint::Keyboard, &[0xb0, 1, 1]);
        }
        update(&mut app, Action::Wake);
        assert_eq!(
            app.status.message(),
            Some("MIDI input overloaded: 9 messages dropped")
        );
        assert_eq!(app.remote.debug_log.len(), 128);
        assert!(!app.is_dirty());
    }

    #[test]
    fn both_map_modes_learn_and_observe_without_running_router_consumers() {
        use crate::map_mode::Mode;
        use qtable_engine::{engine::Snapshot, model::*};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        struct Observer(Arc<AtomicUsize>);
        impl qtable_remote::Consumer for Observer {
            fn receive(
                &mut self,
                _: &[qtable_remote::Envelope],
                _: &Project,
                _: &Snapshot,
                _: &mut Vec<qtable_remote::Received<Command>>,
            ) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        for mode in [Mode::Midi, Mode::Key] {
            let mut app = App::new(Engine::new(Project::default()).unwrap(), "120".into(), None);
            let calls = Arc::new(AtomicUsize::new(0));
            app.remote.router.subscribe(Observer(calls.clone()));
            app.map_mode.on = true;
            app.map_mode.mode = mode;
            app.map_mode.selected_control = Some(ControlRef::Transport(TransportControl::Play));
            let input = match mode {
                Mode::Midi => Input::Midi(vec![0xb0, 1, 64].into()),
                Mode::Key => Input::Key {
                    key: "a".into(),
                    pressed: true,
                    repeat: false,
                },
            };
            app.remote.ingress.push(Endpoint::Keyboard, input);
            update(&mut app, Action::Wake);
            assert_eq!(calls.load(Ordering::Relaxed), 0);
            assert_eq!(
                app.project().mappings.len(),
                1,
                "learning still adds the mapping"
            );
            assert_eq!(app.remote.debug_log.len(), 1);
            assert_eq!(app.remote.activity, u64::from(mode == Mode::Midi));
            app.map_mode.on = false;
            update(&mut app, Action::Wake);
            assert_eq!(calls.load(Ordering::Relaxed), 1);
        }
    }
}

#[cfg(test)]
mod mapping_tests;

#[cfg(test)]
mod navigation_tests;

#[cfg(test)]
mod nested_parity_tests;
