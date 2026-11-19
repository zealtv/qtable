//! Dialogs and file operations: unsaved-changes prompt, open and save.
use crate::app::App;
use crate::messages::{FileMessage, Message};
use iced::Task;
use qtable_engine::engine::Command;
use qtable_engine::model::{Project, Scene, SceneId, Track, TrackId};
use rfd::{AsyncFileDialog, AsyncMessageDialog, MessageButtons, MessageDialogResult};
use std::path::PathBuf;

/// Live 12's new-set shape, observed on 2026-10-11.
pub(crate) fn new_project() -> Project {
    let mut project = Project {
        scenes: (1..=8)
            .map(|id| Scene {
                id: SceneId(id),
                name: format!("Scene {id}"),
                follow_actions: Default::default(),
            })
            .collect(),
        ..Project::default()
    };
    let colour = project.take_next_colour();
    project.tracks.push(Track {
        mixer: Default::default(),
        route: qtable_engine::model::Route::single(project.main.id),
        colour,
        ..Track::new(TrackId(1), "Track", 8)
    });
    project
}

pub(crate) fn title_text(path: Option<&std::path::Path>, dirty: bool) -> String {
    let name = path
        .and_then(std::path::Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("Untitled");
    format!("qtable — {name}{}", if dirty { " •" } else { "" })
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiscardAction {
    Save,
    Discard,
    Cancel,
}

pub(crate) const SAVE_LABEL: &str = "Save";
pub(crate) const DISCARD_LABEL: &str = "Don't Save";
pub(crate) const CANCEL_LABEL: &str = "Cancel";

pub(crate) fn discard_action(answer: MessageDialogResult) -> DiscardAction {
    match answer {
        MessageDialogResult::Yes => DiscardAction::Save,
        MessageDialogResult::No => DiscardAction::Discard,
        MessageDialogResult::Custom(label) if label == SAVE_LABEL => DiscardAction::Save,
        MessageDialogResult::Custom(label) if label == DISCARD_LABEL => DiscardAction::Discard,
        MessageDialogResult::Custom(label) if label == CANCEL_LABEL => DiscardAction::Cancel,
        _ => DiscardAction::Cancel,
    }
}

pub(crate) fn request_intent(app: &mut App, intent: Intent) -> Task<Message> {
    if app.file_intent_pending {
        return Task::none();
    }
    if app.is_dirty() {
        app.file_intent_pending = true;
        discard_prompt(intent)
    } else {
        continue_intent(app, intent)
    }
}

pub(crate) fn discard_prompt(intent: Intent) -> Task<Message> {
    let dialog = || {
        AsyncMessageDialog::new()
            .set_title("Unsaved changes")
            .set_description("Save changes before continuing?")
            .set_buttons(MessageButtons::YesNoCancelCustom(
                SAVE_LABEL.into(),
                DISCARD_LABEL.into(),
                CANCEL_LABEL.into(),
            ))
    };
    let parent = match intent {
        Intent::Close(id) => Task::done(Some(id)),
        _ => iced::window::latest(),
    };
    parent.then(move |id| {
        let build = if let Some(id) = id {
            iced::window::run(id, move |window| dialog().set_parent(window))
        } else {
            Task::done(dialog())
        };
        build.then(move |dialog| {
            Task::perform(dialog.show(), move |answer| {
                Message::Files(FileMessage::DiscardChoice(intent, answer))
            })
        })
    })
}

pub(crate) fn continue_intent(app: &mut App, intent: Intent) -> Task<Message> {
    match intent {
        Intent::New => {
            if crate::edit_actions::run_edit_report(app, Command::LoadProject(new_project()), None)
                .is_some()
            {
                app.reset_for_project();
                app.path = None;
            }
            Task::none()
        }
        Intent::Open => Task::perform(
            async {
                AsyncFileDialog::new()
                    .add_filter("qtable project", &["ron"])
                    .pick_file()
                    .await
                    .map(|file| file.path().to_path_buf())
            },
            |value| Message::Files(FileMessage::OpenPicked(value)),
        ),
        Intent::Close(id) => iced::window::close(id),
    }
}

#[cfg(test)]
mod tests;

pub(crate) fn start_save(app: &App, after: Option<Intent>, force_picker: bool) -> Task<Message> {
    if !force_picker && let Some(path) = app.path.clone() {
        save_to(app, path, after)
    } else {
        Task::perform(
            async {
                AsyncFileDialog::new()
                    .add_filter("qtable project", &["ron"])
                    .set_file_name("Untitled.ron")
                    .save_file()
                    .await
                    .map(|file| file.path().to_path_buf())
            },
            move |path| Message::Files(FileMessage::SavePicked(path, after)),
        )
    }
}

pub(crate) fn save_to(app: &App, path: PathBuf, after: Option<Intent>) -> Task<Message> {
    let project = app.engine.project().clone();
    let stamp = SaveStamp::capture(app);
    Task::perform(
        async move {
            qtable_engine::file::save(&path, &project)
                .map(|()| path)
                .map_err(|e| e.to_string())
        },
        move |result| Message::Files(FileMessage::ProjectSaved(result, stamp, after)),
    )
}

/// Identifies the project and content captured by an asynchronous save.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SaveStamp {
    generation: u64,
    content: u64,
}

impl SaveStamp {
    pub(crate) fn capture(app: &App) -> Self {
        Self {
            generation: app.project_generation,
            content: app.engine.content_id(),
        }
    }
}

pub(crate) fn finish_save(
    app: &mut App,
    result: Result<PathBuf, String>,
    stamp: SaveStamp,
    after: Option<Intent>,
) -> Task<Message> {
    // A save from a replaced project must not alter the new project's path or UI.
    if stamp.generation != app.project_generation {
        return Task::none();
    }
    app.file_intent_pending = false;
    match result {
        Ok(path) => {
            app.path = Some(path);
            app.saved_content = stamp.content;
            app.status.clear_transient();
            if let Some(intent) = after {
                // Re-prompt if edits landed while saving, before discarding or closing.
                return request_intent(app, intent);
            }
        }
        Err(error) => app.status.show_error(error),
    }
    Task::none()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Intent {
    New,
    Open,
    Close(iced::window::Id),
}
