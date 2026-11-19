//! Grid launches, selection and group controls.
use crate::app::App;
use crate::edit_actions::{
    clear_drafts, create_clip, execute, execute_edit, execute_structural_edit, move_clip_edit,
};
use crate::selection::ColumnOwner;
use crate::{grid, inspector_panel};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::Region;

pub(super) fn update(app: &mut App, action: grid::Action) {
    match action {
        grid::Action::ScrollColumns { .. }
        | grid::Action::ScrollScenes { .. }
        | grid::Action::DragScrollScenes { .. }
        | grid::Action::Info(_)
        | grid::Action::ControlFocused(_) => {}
        grid::Action::LaunchClip(track, slot) => execute(app, Command::LaunchClip { track, slot }),
        grid::Action::LaunchGroupSlot(group, slot) => {
            execute(app, Command::LaunchGroupSlot { group, slot })
        }
        grid::Action::LaunchScene(scene) => execute(app, Command::LaunchScene { scene }),
        grid::Action::SelectBlankColumn(owner) => {
            update(app, grid::Action::SelectColumn(owner, None));
            app.selection.clear_cell();
        }
        grid::Action::SelectColumn(owner, row) => {
            if app.selection.clip.is_some()
                || app.selection.scene.is_some()
                || matches!(owner, ColumnOwner::Track(id) if app.project().track_capabilities(id).is_some_and(|c| c.region == Region::Main))
            {
                clear_drafts(app);
            }
            app.close_overlay();
            app.selection
                .select_column_cell(app.engine.project(), owner, row);
        }
        grid::Action::MoveScenes { scenes, boundary } => {
            super::scene_actions::move_scenes(app, scenes, boundary);
        }
        grid::Action::ClickScene(scene) => super::scene_actions::click(app, scene),
        grid::Action::SelectScene(scene) => {
            let row = app.project().scenes.iter().position(|s| s.id == scene);
            let main = app.project().main.id;
            app.selection.select_column(ColumnOwner::Track(main), row);
            app.close_overlay();
            if app.selection.scene != Some(scene) || app.selection.clip.is_some() {
                clear_drafts(app);
            }
            app.selection.tracks.clear();
            app.selection.anchor = None;
            app.selection.scene = Some(scene);
            app.selection.scenes.single(scene);
            app.selection.clip = None;
            app.selection.move_clip = false;
            app.selection.inspector_tab = inspector_panel::Tab::Scene;
        }
        grid::Action::StopSlot(track, slot) => execute(app, Command::LaunchClip { track, slot }),
        grid::Action::StopTrack(track) => execute(app, Command::StopTrack { track }),
        grid::Action::AddTrack => execute_edit(
            app,
            Command::Edit(ProjectEdit::AddTrack {
                name: format!("Track {}", app.project().clip_tracks().count() + 1),
            }),
        ),
        grid::Action::StopAll => execute(app, Command::StopAll),
        grid::Action::ToggleFollowActions => {
            let enabled = !app.project().follow_actions_enabled;
            execute(app, Command::SetFollowActionsEnabled(enabled))
        }
        grid::Action::Select(track, slot) => {
            app.selection.scene = None;
            app.selection.tracks = vec![track];
            app.selection.anchor = Some(track);
            app.selection
                .select_column(ColumnOwner::Track(track), Some(slot));
            app.selection.inspector_tab = inspector_panel::Tab::Clip;
            let destination = grid::Selection { track, slot };
            if app.selection.move_clip {
                if let Some(source) = app.selection.clip
                    && let Some(edit) = move_clip_edit(source, destination)
                    && execute_structural_edit(app, Command::Edit(edit))
                {
                    // Live keeps the moved clip selected.
                    app.selection.clip = Some(destination);
                }
                app.selection.move_clip = false;
            } else {
                if app.selection.clip != Some(destination) {
                    clear_drafts(app);
                }
                app.selection.clip = Some(destination)
            }
        }
        grid::Action::OpenDetail(selection) => {
            update(app, *selection);
            app.chrome.detail_open = true;
        }
        grid::Action::OpenClip(track, slot) => {
            update(app, grid::Action::Select(track, slot));
            app.chrome.detail_open = true;
        }
        grid::Action::ContextMenu(track, slot) => {
            update(app, grid::Action::Select(track, slot));
            app.open_overlay(crate::overlay::Overlay::Context(
                crate::context_menu::OpenMenu {
                    at: app.cursor,
                    target: crate::context_menu::MenuTarget::Slot,
                },
            ));
        }
        grid::Action::HeaderMenu(owner) => {
            let group = app
                .project()
                .track_capabilities(owner)
                .is_some_and(|c| c.foldable);
            let selected = (if group {
                crate::selection::SelectionState::group_selected(
                    &app.selection.tracks,
                    &app.project().tracks,
                    owner,
                )
            } else {
                app.selection.tracks.contains(&owner)
                    || app.selection.owner == Some(ColumnOwner::Track(owner))
            }) && app.selection.clip.is_none()
                && app.selection.scene.is_none();
            if !selected {
                if group {
                    update(app, grid::Action::SelectGroup(owner, false));
                } else {
                    update(
                        app,
                        grid::Action::SelectColumn(ColumnOwner::Track(owner), None),
                    );
                }
            }
            app.open_overlay(crate::overlay::Overlay::Context(
                crate::context_menu::OpenMenu {
                    at: app.cursor,
                    target: crate::context_menu::MenuTarget::Header(owner),
                },
            ));
        }
        grid::Action::RegionMenu(region) => {
            app.open_overlay(crate::overlay::Overlay::Context(
                crate::context_menu::OpenMenu {
                    at: app.cursor,
                    target: crate::context_menu::MenuTarget::Region(region),
                },
            ));
        }
        grid::Action::SceneMenu(scene) => {
            if !app.selection.scenes.members.contains(&scene) {
                update(app, grid::Action::SelectScene(scene));
            }
            app.open_overlay(crate::overlay::Overlay::Context(
                crate::context_menu::OpenMenu {
                    at: app.cursor,
                    target: crate::context_menu::MenuTarget::Scene(scene),
                },
            ));
        }
        grid::Action::CreateClip(track, slot) => {
            update(app, grid::Action::Select(track, slot));
            if create_clip(app, track, slot) {
                app.chrome.detail_open = true;
            }
        }
        grid::Action::PressHeader(action) => {
            let (track, extend) = match *action {
                grid::Action::SelectTrack(id, extend) | grid::Action::SelectGroup(id, extend) => {
                    (id, extend)
                }
                grid::Action::SelectColumn(ColumnOwner::Track(id), None) => (id, false),
                action => {
                    update(app, action);
                    return;
                }
            };
            let Some(region) = app.project().track_capabilities(track).map(|c| c.region) else {
                return;
            };
            let previous = app.selection.tracks.clone();
            let additive = app.selection.command_down;
            let preserve = !extend && !additive && previous.contains(&track);
            // Focus, drafts and Detail tab use the ordinary header selection path.
            update(app, *action);
            if preserve {
                app.selection.tracks = previous;
            } else if additive && !extend {
                let mut selected = previous;
                if selected.contains(&track) {
                    selected.retain(|id| *id != track);
                } else {
                    selected.push(track);
                }
                app.selection.tracks = selected;
            }
            // Header selections stay in their own region; hidden selections in it survive.
            let selected = app
                .selection
                .tracks
                .iter()
                .copied()
                .filter(|id| {
                    app.project()
                        .track_capabilities(*id)
                        .is_some_and(|c| c.region == region)
                })
                .collect();
            app.header_gestures
                .select_for_drag(region, selected, preserve);
        }
        grid::Action::SelectTrack(track, extend) => {
            app.selection.select_column(ColumnOwner::Track(track), None);
            if app.selection.clip.is_some() || app.selection.scene.is_some() {
                clear_drafts(app);
            }
            app.selection.clip = None;
            app.selection.scene = None;
            app.selection.move_clip = false;
            app.selection.inspector_tab = inspector_panel::Tab::Track;
            if extend {
                let anchor = app.selection.anchor.unwrap_or(track);
                let columns = crate::grid::layout::columns(app.project(), Region::Tracks);
                let ids: Vec<_> = columns
                    .iter()
                    .map(|c| match c.key {
                        crate::grid::layout::ColumnKey::Track(id) => id,
                    })
                    .collect();
                let a = app.project().tracks.iter().position(|t| t.id == anchor);
                let b = app.project().tracks.iter().position(|t| t.id == track);
                if let (Some(a), Some(b)) = (a, b) {
                    app.selection.tracks = app.project().tracks
                        [crate::selection::header_range(a, b)]
                    .iter()
                    .filter(|t| ids.contains(&t.id) || t.id == anchor || t.id == track)
                    .map(|t| t.id)
                    .collect();
                }
            } else {
                app.selection.anchor = Some(track);
                app.selection.tracks = vec![track];
            }
        }
        grid::Action::SelectGroup(group, extend) => {
            let previous_cell = app
                .selection
                .cell
                .filter(|c| c.owner == ColumnOwner::Group(group));
            if !app.project().track(group).is_some_and(|t| t.is_group()) {
                return;
            }
            update(app, grid::Action::SelectTrack(group, extend));
            app.selection.select_column(ColumnOwner::Group(group), None);
            app.selection.cell =
                previous_cell.map(|c| crate::selection::ColumnCell { active: false, ..c });
        }
        grid::Action::ToggleGroup(group) => {
            let folded = !app
                .project()
                .group_tracks()
                .find(|g| g.id == group)
                .is_some_and(|g| g.fold_state);
            execute_edit(
                app,
                Command::Edit(ProjectEdit::SetGroupFolded { group, folded }),
            )
        }
        grid::Action::StopGroup(group) => execute(app, Command::StopGroup { group }),
        grid::Action::DropClip { from, to, copy } => {
            if from == to {
                return;
            }
            let edit = if copy {
                let Some(track) = app.project().track(from.track) else {
                    return;
                };
                let Some(clip) = track.clip(from.slot).cloned() else {
                    return;
                };
                ProjectEdit::PasteClip {
                    track: to.track,
                    slot: to.slot,
                    carried: qtable_engine::edit::Carried::from_clip(track, &clip),
                    clip: Box::new(clip),
                }
            } else {
                ProjectEdit::MoveClip {
                    from: (from.track, from.slot),
                    to: (to.track, to.slot),
                }
            };
            if execute_structural_edit(app, Command::Edit(edit)) {
                app.selection.move_clip = false;
                update(app, grid::Action::Select(to.track, to.slot));
            }
        }
        grid::Action::PlaceTracks {
            tracks,
            before,
            group,
        } => execute_edit(
            app,
            Command::Edit(ProjectEdit::PlaceTracks {
                tracks,
                before,
                group,
            }),
        ),
        grid::Action::PlaceTrack {
            track,
            before,
            group,
        } => execute_edit(
            app,
            Command::Edit(ProjectEdit::PlaceTrack {
                track,
                before,
                group,
            }),
        ),
    }
}
