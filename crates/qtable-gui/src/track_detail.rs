//! The Detail View's Track tab: the selected track's mixer.
use crate::app::App;
use crate::control_focus::View;
use crate::detail_layout::{Header, Layout};
use crate::drafts::{DraftKey, TrackField};
use crate::messages::{EditMessage, Message, OutputMessage};
use crate::style::detail;
use qtable_engine::edit::ProjectEdit;
use qtable_engine::model::TrackId;

pub(crate) fn view(app: &App) -> Layout<'_, Message> {
    let Some(track) = app
        .selection
        .track()
        .and_then(|id| app.project().tracks.iter().find(|t| t.id == id))
    else {
        return Layout::empty("No track selected.");
    };

    Layout {
        header: Header {
            colour: crate::outputs::to_colour(track.colour),
            content: detail::heading(track.name.clone()).into(),
        },
        leading: None,
        sections: vec![],
        content: Some(crate::device_chain::view(app, track, View::Track)),
    }
}

/// Commits a draft of one of the track's own fields, whichever panel shows it: the header's
/// name, the Delay field or the Path row.
pub(crate) fn commit(app: &mut App, track: TrackId, field: TrackField, value: String) {
    match field {
        TrackField::Name => crate::edit_actions::submit_name(
            app,
            DraftKey::Track(track, field),
            ProjectEdit::RenameTrack { track, name: value },
        ),
        TrackField::Delay => {
            let _ = crate::update::outputs::update(
                app,
                OutputMessage::TrackDelaySubmitted(track, value),
            );
        }
        TrackField::Path => {
            let _ =
                crate::update::update(app, Message::Edit(EditMessage::PathSubmitted(track, value)));
            app.pending_field = None;
        }
    }
}
