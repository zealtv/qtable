//! Activator and Solo clicks and keys, with Live's rules (live-solo-mute.md rules 3, 4, 18):
//! a plain Solo click is exclusive and ⌘-click adds or removes one track; a click on a track
//! inside a multi-track selection switches every selected track to the clicked track's new
//! state; S solos the selected tracks; F1–F8 toggle the first eight tracks' Activators.
use crate::app::App;
use crate::drafts::DraftKey;
use crate::drafts::MixerField;
use crate::edit_actions::{execute_edit, run_edit};
use crate::mixer_band::MixerMessage;
use crate::mixer_device::volume_field;
use crate::parameter_gesture;
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::{ParameterRef, TrackId};

pub(super) fn update(app: &mut App, message: MixerMessage) {
    // `parameter_gesture` decides when a drag sends: only On Release holds a step back.
    // The scheduler carries a sent value on its next tick while playing, or once while stopped.
    match message {
        MixerMessage::SetVolume(track, volume) => return set_volume(app, track, volume),
        MixerMessage::DragVolume(track, volume) => {
            if let Some(volume) = parameter_gesture::drag(app, &volume_field(track), volume) {
                set_volume(app, track, volume);
            }
            return;
        }
        MixerMessage::ReleaseVolume(track, volume) => {
            // Steps already sent in Continuous mode leave the model at the final value.
            if let Some(volume) = parameter_gesture::release(app, &volume_field(track), volume) {
                set_volume(app, track, volume);
            }
            return;
        }
        _ => {}
    }
    let exclusive = !app.selection.command_down;
    let edit = match message {
        MixerMessage::SetVolume(..)
        | MixerMessage::DragVolume(..)
        | MixerMessage::ReleaseVolume(..) => return,
        MixerMessage::Activator(strip) => {
            let Some(state) = app.project().track_facts(strip).map(|f| f.mixer) else {
                return;
            };
            ProjectEdit::SetActive {
                strips: with_selection(app, strip),
                active: !state.active,
            }
        }
        MixerMessage::Solo(strip) => {
            let Some(state) = app.project().track_facts(strip).map(|f| f.mixer) else {
                return;
            };
            ProjectEdit::SetSolo {
                strips: with_selection(app, strip),
                solo: !state.solo,
                exclusive,
            }
        }
        MixerMessage::OutputSolo(output) => {
            let Some(receiver) = app.project().output(output) else {
                return;
            };
            ProjectEdit::SetOutputSolo {
                output,
                solo: !receiver.mixer.solo,
                exclusive,
            }
        }
        MixerMessage::SoloSelected => {
            let tracks = selected_tracks(app);
            if tracks.is_empty() {
                return;
            }
            // Like pressing the Solo of one selected track: all follow its new state.
            let solo = !app
                .project()
                .track_facts(tracks[0])
                .is_some_and(|f| f.mixer.solo);
            ProjectEdit::SetSolo {
                strips: tracks,
                solo,
                exclusive: true,
            }
        }
        MixerMessage::ToggleActivatorAt(index) => {
            // Live's track order, so F1 toggles a group track that comes first.
            let Some(strip) = app.project().strips().get(index).copied() else {
                return;
            };
            ProjectEdit::SetActive {
                strips: vec![strip],
                active: !app
                    .project()
                    .track_facts(strip)
                    .is_some_and(|f| f.mixer.active),
            }
        }
    };
    execute_edit(app, Command::Edit(edit));
}

/// Typed values, Delete and sent drag steps: always one edit, ending any hold.
fn set_volume(app: &mut App, track: TrackId, volume: f64) {
    app.parameter_gesture.end(&volume_field(track));
    if run_edit(
        app,
        Command::Edit(ProjectEdit::set_value(ParameterRef::volume(track), volume)),
    ) {
        app.drafts
            .forget(DraftKey::Mixer(track, MixerField::Volume));
    }
}

/// The clicked strip, or every selected track when the clicked track is one of several
/// selected (Live 12 manual 18.1).
fn with_selection(app: &App, strip: TrackId) -> Vec<TrackId> {
    let selected = &app.selection.tracks;
    match strip {
        track if selected.len() > 1 && selected.contains(&track) => selected.clone(),
        _ => vec![strip],
    }
}

/// What S acts on: the selected track headers, or the selected clip's track.
fn selected_tracks(app: &App) -> Vec<TrackId> {
    if app.selection.tracks.is_empty() {
        app.selection.track().into_iter().collect()
    } else {
        app.selection.tracks.clone()
    }
}
