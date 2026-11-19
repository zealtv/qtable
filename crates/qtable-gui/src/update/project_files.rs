//! Files messages, routed as a complete typed domain.
use crate::app::App;
use crate::files::{
    DiscardAction, Intent, continue_intent, discard_action, finish_save, request_intent, save_to,
    start_save,
};
use crate::messages::{FileMessage, Message};
use iced::Task;
use qtable_engine::engine::Command;

pub(super) fn update(app: &mut App, message: FileMessage) -> Task<Message> {
    match message {
        FileMessage::New => return request_intent(app, Intent::New),
        FileMessage::Open => return request_intent(app, Intent::Open),
        FileMessage::CloseRequested(id) => return request_intent(app, Intent::Close(id)),
        FileMessage::Save => return start_save(app, None, false),
        FileMessage::SaveAs => return start_save(app, None, true),
        FileMessage::DiscardChoice(intent, answer) => {
            app.file_intent_pending = false;
            match discard_action(answer) {
                DiscardAction::Save => {
                    app.file_intent_pending = true;
                    return start_save(app, Some(intent), false);
                }
                DiscardAction::Discard => return continue_intent(app, intent),
                DiscardAction::Cancel => {}
            }
        }
        FileMessage::OpenPicked(Some(path)) => {
            return Task::perform(
                async move {
                    let result = qtable_engine::file::load(&path).map_err(|e| e.to_string());
                    result.map(|project| (path, Box::new(project)))
                },
                |result| Message::Files(FileMessage::ProjectLoaded(result)),
            );
        }
        FileMessage::OpenPicked(None) => {}
        FileMessage::SavePicked(path, after) => {
            if let Some(path) = path {
                return save_to(app, path, after);
            }
            if after.is_some() {
                app.file_intent_pending = false;
            }
        }
        FileMessage::ProjectLoaded(result) => match result {
            Ok((path, project)) => match app.engine.execute(Command::LoadProject(*project)) {
                Ok(_) => {
                    app.reset_for_project();
                    app.path = Some(path);
                }
                Err(error) => app.status.show_error(error.to_string()),
            },
            Err(error) => app.status.show_error(error),
        },
        FileMessage::ProjectSaved(result, stamp, after) => {
            return finish_save(app, result, stamp, after);
        }
    }
    Task::none()
}
