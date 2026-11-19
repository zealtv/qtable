//! Update loop: maps every Message onto engine commands and app state.
use crate::app::App;
use crate::edit_actions::execute;
use crate::fields::{format_tempo, parse_tempo};
use crate::grid;
use crate::inspector_update::handle_inspector;
use crate::messages::Message;
use crate::typing::Draft;
use iced::Task;
use qtable_engine::engine::Command;

mod edit;
mod grid_actions;
mod grid_keys;
mod mixer;
pub(crate) mod outputs;
mod project_files;
mod remote;
mod scene_actions;

#[cfg(test)]
mod blur_tests;
#[cfg(test)]
mod clip_action_tests;
#[cfg(test)]
mod delay_tests;
#[cfg(test)]
mod detail_follow_tests;
#[cfg(test)]
mod field_enter_tests;
#[cfg(test)]
mod float_display_tests;
#[cfg(test)]
mod follow_toggle_tests;
#[cfg(test)]
mod gesture_tests;
#[cfg(test)]
mod grid_key_tests;
#[cfg(test)]
mod header_action_tests;
#[cfg(test)]
mod legato_tests;
#[cfg(test)]
mod mixer_tests;
#[cfg(test)]
mod output_tests;
#[cfg(test)]
mod overlay_tests;
#[cfg(test)]
mod path_validation_tests;
#[cfg(test)]
mod route_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod time_field_tests;
#[cfg(test)]
mod volume_send_tests;

pub(crate) fn update(app: &mut App, message: Message) -> Task<Message> {
    let message = match message {
        Message::Inspector(crate::inspector::Message::FieldEntered) => Message::FieldEntered,
        other => other,
    };
    let entered = matches!(message, Message::FieldEntered);
    let reported = app.status.errors_reported();
    let context = app.engine.command_context(qtable_engine::engine::Receipt {
        received_at: std::time::Instant::now(),
        sequence: None,
    });
    let previous = app.command_context.replace(context);
    let ends_span = crate::gesture_steps::ends_span(app, &message);
    crate::gesture_steps::track_pointer(app, &message);
    let task = if matches!(message, Message::PointerPressed) {
        // Only the gesture bookkeeping hears of a press; no widget or draft acts on it.
        Task::none()
    } else {
        update_received(app, message)
    };
    app.command_context = previous;
    if !entered {
        // Enter's `FieldEntered` follows the field's submit message: it reads how that went.
        app.last_message_rejected = app.status.errors_reported() != reported
            || app
                .pending_field
                .is_some_and(|key| app.drafts.errors.contains_key(&key));
    }
    // After the message's own work: a release's edits belong to the span it ends.
    if ends_span {
        crate::gesture_steps::boundary(app);
    }
    task
}

fn update_received(app: &mut App, message: Message) -> Task<Message> {
    // Handle shortcuts before blur commits or timeline focus tracking: editing a
    // text field must never turn a clipboard key into a project mutation.
    if let Message::ClipShortcut(shortcut) = message {
        return crate::clip_shortcuts::update(app, shortcut);
    }
    // Selecting on the grid moves the click outline there, so a fader's latched focus
    // must not keep taking Delete.
    if matches!(
        message,
        Message::Grid(
            grid::Action::Select(..)
                | grid::Action::OpenDetail(..)
                | grid::Action::OpenClip(..)
                | grid::Action::SelectTrack(..)
                | grid::Action::SelectScene(..)
                | grid::Action::ClickScene(..)
                | grid::Action::MoveScenes { .. }
                | grid::Action::SelectColumn(..)
                | grid::Action::SelectBlankColumn(..)
        )
    ) {
        app.control_focus.clear();
    }
    let changed = crate::field_focus::changed_field(&message);
    let submitted = crate::field_focus::submitted_field(&message);
    let refocus = matches!(message,
        Message::Edit(crate::messages::EditMessage::FocusField(key))
        if app.pending_field == Some(key));
    let passive = refocus
        || matches!(
            message,
            Message::Browser(crate::browser::Action::Release | crate::browser::Action::Cancel)
                | Message::Device(crate::device_chain::Action::RenameFocusChecked(..))
                | Message::Device(crate::device_chain::Action::Target(
                    crate::device_chain::target_face::Action::ExactFocusChecked(..)
                ))
                | Message::Inspector(
                    crate::inspector::Message::Info(_)
                        | crate::inspector::Message::Timeline(
                            crate::inspector::event_editor::Action::Hover(_)
                        )
                        | crate::inspector::Message::CommitNumber
                        | crate::inspector::Message::ControlFocused(_)
                )
                | Message::Remote(crate::remote::Action::Wake | crate::remote::Action::Error(_))
                | Message::Tick
                | Message::Info(_)
                | Message::ControlFocused(_)
                | Message::Grid(grid::Action::ControlFocused(_))
                | Message::CursorMoved(_)
                | Message::WindowResized(_)
                | Message::RouteAnchored(..)
                | Message::KeyPressed { .. }
                | Message::MappedKey(_)
                | Message::KeyReleased(_)
                | Message::ReleaseKeys
                | Message::Grid(grid::Action::Info(_))
                | Message::ModifiersChanged(_)
                | Message::CheckFieldFocus
                | Message::FieldFocusChecked(..)
                | Message::CommitField
                | Message::CommitNumber
                | Message::FieldEntered
                | Message::CancelField
                | Message::TabField(_)
                | Message::SetFocus(_)
                | Message::Files(
                    crate::messages::FileMessage::ProjectSaved(..)
                        | crate::messages::FileMessage::ProjectLoaded(..)
                        | crate::messages::FileMessage::OpenPicked(..)
                        | crate::messages::FileMessage::SavePicked(..)
                        | crate::messages::FileMessage::DiscardChoice(..)
                )
        );
    crate::inspector::event_editor::track_focus(app, &message, passive);
    // A click elsewhere commits the typing as a step of its own, apart from the click.
    let mut commits = Vec::new();
    if !passive
        && !matches!(
            message,
            Message::Device(
                crate::device_chain::Action::Draft(_) | crate::device_chain::Action::CommitRename
            ) | Message::CancelField
        )
    {
        commits.push(Draft::DeviceRename);
    }
    if !passive
        && !matches!(
            message,
            Message::Device(crate::device_chain::Action::Target(
                crate::device_chain::target_face::Action::Exact(..)
                    | crate::device_chain::target_face::Action::CommitExact(..)
                    | crate::device_chain::target_face::Action::Reset(..)
            ))
        )
    {
        // A click that changes the view may remove the input before the release's
        // focus query runs. Commit its draft before applying that selection.
        commits.push(Draft::TargetFace);
    }
    if let Some(key) = app.pending_field {
        if submitted == Some(key) {
            app.pending_field = None;
        } else if changed != Some(key) && !passive {
            commits.push(Draft::Field);
        }
    }
    let rejected = crate::typing::commit(app, &commits)
        .rejected_field()
        .and_then(|key| app.drafts.errors.get(&key).cloned());
    if let Some(key) = changed {
        app.pending_field = Some(key);
        app.drafts.errors.remove(&key);
    }
    let revision = app.project_revision();
    let task = update_inner(app, message);
    // Leaving scenes clears their membership immediately. Reconcile identities only
    // after accepted edits: a cancelled/failed file operation preserves the entire UI.
    if app.selection.scene.is_none() || app.project_revision() != revision {
        app.selection.reconcile_scenes(&app.engine.project().scenes);
    }
    app.validate_overlay();
    crate::device_chain::reconcile(app);
    crate::inspector::event_editor::reconcile(app);
    if let Some(key) = submitted {
        if let Some(error) = crate::field_focus::error(app, key) {
            app.drafts.errors.insert(key, error.clone());
            app.status.show_error(error);
            app.pending_field = Some(key);
        } else if crate::field_focus::value(app, key).is_some()
            && (key != crate::drafts::DraftKey::Tempo
                || crate::fields::parse_tempo(&app.tempo_input).is_err())
            && let Some(error) = app.status.message().map(str::to_owned)
        {
            app.drafts.errors.insert(key, error);
            app.pending_field = Some(key);
        }
    }
    if let Some(error) = rejected {
        app.status.show_error(error);
    }
    Task::batch([task, crate::browser::reveal(app)])
}

fn update_inner(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Browser(action) => {
            if matches!(action, crate::browser::Action::Release) {
                app.dividers.end();
                crate::browser::update(app, action);
                return Task::batch([
                    crate::field_focus::check(app),
                    crate::device_chain::check_rename(app),
                    crate::device_chain::target_face::check_exact(app),
                ]);
            }
            crate::browser::update(app, action);
        }
        Message::Device(action) => return crate::device_chain::update(app, action),
        Message::ToggleDetailSection(tab, section) => {
            app.toggle_detail_section(tab, section);
        }
        Message::CommitField => {
            app.device_chain.drag = None;
            crate::browser::update(app, crate::browser::Action::Cancel);
            crate::typing::commit(app, &[Draft::Field, Draft::TargetFace]);
            app.info.update(crate::info::Hover::Clear);
        }
        Message::FieldEntered => {
            // Enter in a field commits it, then lets go of the keyboard so Space plays (tour,
            // 2026-11-15). A rejected value keeps the keyboard in the field to be put right.
            if !app.last_message_rejected {
                return crate::shortcuts::release_field();
            }
        }
        Message::CommitNumber => {
            // One step of a number box's drag, nudge or reset. It joins the span it belongs to:
            // `isolated` here would make every drag step an undo step of its own.
            crate::field_focus::commit(app);
            crate::device_chain::target_face::commit_exact(app);
        }
        Message::CheckFieldFocus => {
            // Any mouse release ends a divider drag.
            app.dividers.end();
            let mappings = crate::map_mode::follow(app);
            return Task::batch([
                mappings,
                crate::grid::scene_follow::follow(
                    &app.scene_follow,
                    app.selection.clip,
                    app.selection.scene,
                    app.selection.cell,
                    &app.project().scenes,
                ),
                crate::field_focus::check(app),
                crate::device_chain::check_rename(app),
                crate::device_chain::target_face::check_exact(app),
            ]);
        }
        Message::FieldFocusChecked(key, value, generation, focused) => {
            if !focused
                && app.project_generation == generation
                && app.pending_field == Some(key)
                && crate::field_focus::value(app, key).as_ref() == Some(&value)
            {
                crate::typing::commit(app, &[Draft::Field]);
            }
        }
        Message::CursorMoved(position) => {
            app.cursor = position;
            app.dividers.drag_to(position);
            crate::device_chain::drag_cursor(app, position);
            crate::browser::cursor(app, position);
        }
        Message::BeginDrag(divider) => {
            if divider != crate::dividers::Divider::Detail || !crate::device_chain::shown(app) {
                app.dividers.begin(divider, app.cursor);
            }
        }
        Message::WindowResized(size) => app.dividers.resize_window(size),
        Message::CloseOverlay => app.close_overlay(),
        Message::CancelField => {
            app.close_overlay();
            crate::device_chain::target_face::reset_face(app);
            app.device_chain.renaming = None;
            app.device_chain.drag = None;
            crate::browser::update(app, crate::browser::Action::Cancel);
            crate::clip_actions::cancel_rename(app);
            crate::header_actions::cancel_rename(app);
            crate::field_focus::cancel(app)
        }
        Message::ControlFocused(id) | Message::Grid(grid::Action::ControlFocused(id)) => {
            if let Some(crate::drafts::DraftKey::Target(
                device,
                crate::drafts::TargetField::Face(parameter, index, position),
            )) = id.field_key()
            {
                app.selection.devices = vec![device];
                app.selection.device_anchor = Some(device);
                app.focus = crate::focus::Focus::Detail;
                crate::device_chain::target_face::select_face_control(
                    app, device, parameter, index, position,
                );
            }
            app.control_focus.claim(id)
        }
        Message::TabField(backwards) => {
            crate::typing::commit(app, &[Draft::Field]);
            app.control_focus.clear();
            return if backwards {
                iced::widget::operation::focus_previous()
            } else {
                iced::widget::operation::focus_next()
            };
        }
        Message::SetFocus(focus) => {
            app.focus = focus;
            if focus == crate::focus::Focus::Browser {
                app.chrome.browser_open = true;
            }
        }
        Message::GridKey(key) => grid_keys::update(app, key),
        Message::KeyPressed {
            key,
            modifiers,
            repeat,
            ignored,
        } => {
            // Undo, redo, Save, Save As and Settings run wherever the focus is, even in a field
            // that took the key (`shortcuts.rs`).
            if let Some(shortcut) = crate::shortcuts::chord(&key, modifiers) {
                return if repeat {
                    Task::none()
                } else {
                    crate::shortcuts::update(app, shortcut)
                };
            }
            // An open menu or chooser list holds the keyboard (`menu_keys.rs`).
            if let Some(task) = crate::menu_keys::keyboard(app, &key, modifiers, ignored) {
                return task;
            }
            if ignored && !repeat && modifiers == iced::keyboard::Modifiers::COMMAND {
                let action = match key.as_ref() {
                    iced::keyboard::Key::Character("k" | "K") => {
                        Some(crate::map_mode::Action::ToggleKey)
                    }
                    _ if crate::map_mode::shortcut(&key, modifiers, repeat) => {
                        Some(crate::map_mode::Action::Toggle)
                    }
                    _ => None,
                };
                if let Some(action) = action {
                    return update(app, Message::Map(action));
                }
            }
            // Map Mode owns Delete before any editing handler below can take it.
            if let Some(action) = crate::map_mode::keyboard(app, &key, modifiers, ignored) {
                return update(app, Message::Map(action));
            }
            if ignored && crate::chrome::insert_output_shortcut(&key, modifiers, repeat) {
                return update(
                    app,
                    Message::Outputs(crate::messages::OutputMessage::AddOutput),
                );
            }
            // B toggles Draw Mode; holding it makes the toggle temporary.
            if ignored
                && !repeat
                && app.pending_field.is_none()
                && key_modifiers_none(modifiers)
                && matches!(key.as_ref(), iced::keyboard::Key::Character("b" | "B"))
            {
                app.clip_view
                    .editor
                    .draw
                    .key_down(std::time::Instant::now());
                return Task::none();
            }
            if crate::inspector::event_editor::content::keyboard(
                app, &key, modifiers, repeat, ignored,
            ) {
                return Task::none();
            }
            if let Some(task) =
                crate::device_chain::target_face::keyboard(app, &key, modifiers, ignored)
            {
                return task;
            }
            if app.device_chain.renaming.is_some()
                && matches!(
                    key.as_ref(),
                    iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape)
                )
            {
                app.device_chain.renaming = None;
                return Task::none();
            }
            if key_modifiers_none(modifiers) && crate::browser::keyboard(app, &key, ignored) {
                return Task::none();
            }
            if key_modifiers_none(modifiers) && crate::device_chain::arrow(app, &key, ignored) {
                return Task::none();
            }
            if let Some(task) = crate::output_band::keyboard(app, &key, modifiers, ignored) {
                return task;
            }
            if let Some(message) = crate::focus::event_message(&key, modifiers, repeat, ignored) {
                return update(app, message);
            }
            // Space comes last: a field, a route list or a focused face takes it first.
            if crate::shortcuts::transport_key(app, &key, modifiers, repeat, ignored) {
                return crate::shortcuts::toggle_transport(app);
            }
        }
        Message::MappedKey(input) => crate::map_mode::keys::input(
            app,
            &input.key,
            &input.modified_key,
            input.modifiers,
            input.pressed,
            input.repeat,
        ),
        Message::KeyReleased(key) => {
            crate::map_mode::keys::release(app, &key);
            if matches!(key.as_ref(), iced::keyboard::Key::Character("b" | "B")) {
                return update(app, Message::DrawKeyReleased);
            }
        }
        Message::ReleaseKeys => {
            crate::map_mode::keys::release_all(app);
            return update(app, Message::CommitField);
        }
        Message::RouteAnchored(owner, generation, bounds) => {
            app.anchor_route_dropdown(owner, generation, bounds)
        }
        Message::ClipShortcut(shortcut) => return crate::clip_shortcuts::update(app, shortcut),
        Message::Inspector(crate::inspector::Message::ControlFocused(id)) => {
            app.control_focus.claim(id)
        }
        Message::Inspector(message) => {
            // Hover help does not move focus; any other Detail View use does.
            if !matches!(message, crate::inspector::Message::Info(_)) {
                app.focus = crate::focus::Focus::Detail;
            }
            handle_inspector(app, message);
        }
        Message::Map(action) => {
            let unfocus = matches!(
                action,
                crate::map_mode::Action::Pick(_)
                    | crate::map_mode::Action::Toggle
                    | crate::map_mode::Action::ToggleKey
            );
            let moved = matches!(action, crate::map_mode::Action::MoveSelection { .. });
            crate::map_mode::update(app, action);
            if moved {
                return crate::map_mode::follow(app);
            }
            // The overlay takes the click before the underlying field sees it.
            // Release that field's real widget focus so it cannot keep the learn key.
            if unfocus {
                return iced::advanced::widget::operate(
                    iced::advanced::widget::operation::focusable::unfocus(),
                );
            }
        }
        Message::DismissStatus => app.status.clear(),
        Message::ToggleRegion(region) => {
            app.close_overlay();
            app.chrome.toggle(region);
            crate::browser::update(app, crate::browser::Action::Cancel);
            if region != crate::chrome::Region::Info {
                app.info.update(crate::info::Hover::Clear);
            }
        }
        Message::ToggleSectionsMenu => {
            if app.overlay == Some(crate::overlay::Overlay::SectionsMenu) {
                app.close_overlay();
            } else {
                app.open_overlay(crate::overlay::Overlay::SectionsMenu);
            }
        }
        Message::Info(hover) | Message::Grid(grid::Action::Info(hover)) => app.info.update(hover),
        // `update` keeps a press to the gesture bookkeeping; nothing reaches here.
        Message::PointerPressed => {}
        Message::Tick => {
            app.engine.poll_events();
            app.snapshot = app.engine.snapshot();
            app.scene_stop_feedback.prune(
                app.engine.project(),
                &app.snapshot,
                std::time::Instant::now(),
            );
            app.status
                .poll(&app.snapshot.status, std::time::Instant::now());
            crate::capture::on_tick(app);
            app.tick_count = app.tick_count.wrapping_add(1);
            // A learn or a click on a mapped control selects a row; the list follows it here.
            let mappings = crate::map_mode::follow(app);
            return Task::batch([
                mappings,
                // Keys drain synchronously inside their adapter; their nested update's
                // Task is not scheduled. Follow on Tick for both keys and MIDI.
                crate::grid::scene_follow::follow(
                    &app.scene_follow,
                    app.selection.clip,
                    app.selection.scene,
                    app.selection.cell,
                    &app.project().scenes,
                ),
                crate::field_focus::check(app),
                crate::device_chain::check_rename(app),
                crate::device_chain::target_face::check_exact(app),
            ]);
        }
        Message::ToggleDrawMode => {
            // The Draw Mode row of a content menu sends this too; choosing it closes the menu.
            app.close_overlay();
            app.clip_view.editor.draw.toggle()
        }
        Message::DrawKeyReleased => app.clip_view.editor.draw.key_up(std::time::Instant::now()),
        Message::ModifiersChanged(modifiers) => {
            app.clip_view.editor.alt_down = modifiers.alt();
            app.clip_view.editor.shift_down = modifiers.shift();
            app.clip_view.editor.command_down = modifiers.command();
            app.selection.shift_down = modifiers.shift();
            app.selection.command_down = modifiers.command();
        }
        Message::Grid(grid::Action::ScrollScenes { offset }) => {
            app.scene_follow.scrolled(offset as f32);
        }
        Message::Grid(grid::Action::DragScrollScenes { offset }) => {
            app.scene_follow.scrolled(offset as f32);
            return grid::scroll_scenes(offset as f32);
        }
        Message::Grid(grid::Action::ScrollColumns {
            region,
            footer,
            offset,
        }) => {
            if !app.header_gestures.accepts_scroll(region, offset) {
                return iced::Task::none();
            }
            let current = match region {
                qtable_engine::model::Region::Tracks => &mut app.grid_scroll_x,
                qtable_engine::model::Region::Outputs => &mut app.output_scroll_x,
                qtable_engine::model::Region::Main => return iced::Task::none(),
            };
            if *current != offset {
                *current = offset;
                return grid::scroll_columns(region, footer, offset);
            }
        }
        Message::Grid(action) => {
            app.focus = crate::focus::Focus::Grid;
            // Like Live, clicking elsewhere ends the in-grid rename by keeping the typed name.
            crate::clip_actions::commit_draft(app);
            crate::header_actions::commit_draft(app);
            grid_actions::update(app, action)
        }
        Message::ReEnableAutomation(field) => {
            app.close_overlay();
            execute(app, Command::ReEnableAutomation { field });
        }
        Message::ShowEnvelope(field) => crate::inspector::lane_view::show_envelope(app, &field),
        Message::AutomationMenu(subject) => {
            app.open_overlay(crate::overlay::Overlay::Context(
                crate::context_menu::OpenMenu {
                    at: app.cursor,
                    target: crate::context_menu::MenuTarget::Argument {
                        subject,
                        revision: app.engine.project_revision(),
                    },
                },
            ));
        }
        Message::Remote(action) => remote::update(app, action),
        Message::Settings(action) => crate::settings_pane::update(app, action),
        Message::Play => execute(app, Command::Play),
        Message::Stop => execute(app, Command::Stop),
        Message::GlobalQuantisation(value) => execute(app, Command::SetGlobalQuantisation(value)),
        Message::TempoChanged(value) => {
            // The draft marks an explicit edit, so typing the shown text still applies it.
            app.drafts
                .values
                .insert(crate::drafts::DraftKey::Tempo, value.clone());
            app.tempo_input = value;
        }
        Message::TempoSubmitted => match parse_tempo(&app.tempo_input) {
            // An untouched two-decimal display must not round the precise model tempo.
            Ok(_)
                if !app
                    .drafts
                    .values
                    .contains_key(&crate::drafts::DraftKey::Tempo)
                    && crate::transport::TEMPO_FORMAT
                        .is_untouched_display(&app.tempo_input, app.project().tempo.bpm()) =>
            {
                app.status.clear_transient();
            }
            Ok(bpm) => {
                if let Err(error) = crate::edit_actions::submit(app, Command::SetTempo(bpm)) {
                    app.status.show_error(error.to_string());
                } else {
                    app.tempo_input = format_tempo(bpm);
                    app.drafts.forget(crate::drafts::DraftKey::Tempo);
                    app.status.clear_transient();
                    app.snapshot = app.engine.snapshot();
                }
            }
            Err(error) => app.status.show_error(error),
        },
        Message::Edit(message) => {
            app.focus = crate::focus::Focus::Detail;
            app.close_overlay();
            return edit::update(app, message);
        }
        Message::Files(message) => return project_files::update(app, message),
        Message::Outputs(message) => {
            app.focus = crate::focus::Focus::Detail;
            // Like a click in the grid, a click in the output columns keeps a typed name.
            crate::header_actions::commit_draft(app);
            return outputs::update(app, message);
        }
        Message::Mixer(message) => {
            app.close_overlay();
            crate::header_actions::commit_draft(app);
            mixer::update(app, message);
        }
    }
    Task::none()
}

fn key_modifiers_none(modifiers: iced::keyboard::Modifiers) -> bool {
    modifiers == iced::keyboard::Modifiers::NONE
}
