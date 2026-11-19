//! Reproducible route screenshots, with no desktop input or playback.
use crate::{app::App, grid, messages::Message};
use iced::Task;
use qtable_engine::model::{Project, Route, Track, TrackId};

/// Main, three outputs (Video is off), and two members of the coloured Rhythm group.
pub(super) fn fixture() -> Project {
    let mut project = crate::files::new_project();
    let slots = project.scenes.len();
    let mut group = Track::new_group(TrackId(10), "Rhythm", slots);
    group.colour = qtable_engine::model::LIVE_CLIP_PALETTE[8];
    group.route = Route::Outputs(vec![project.main.id, TrackId(201), TrackId(203)]);
    let mut cue = Track::new(TrackId(1), "Cue", slots);
    cue.colour = project.take_next_colour();
    cue.group = Some(group.id);
    cue.route = Route::Group;
    let mut other = Track::new(TrackId(2), "Other", slots);
    other.colour = project.take_next_colour();
    other.group = Some(group.id);
    other.route = Route::single(TrackId(201));
    project.tracks = vec![group, cue, other];
    project.outputs = ["Stage", "Video", "Media"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let mut output =
                Track::new_output(TrackId(201 + i as u64), name, "127.0.0.1", 9000 + i as u16);
            output.colour = qtable_engine::model::LIVE_CLIP_PALETTE[i + 3];
            output.mixer.active = i != 1;
            output
        })
        .collect();
    project
}

pub(super) fn setup(app: &mut App) -> Task<Message> {
    let state = std::env::var("QTABLE_CAPTURE_ROUTE_STATE").unwrap_or_else(|_| "list".into());
    setup_state(app, &state)
}

fn setup_state(app: &mut App, state: &str) -> Task<Message> {
    // Prefer a grouped owner in supplied fixtures as well as the built-in project.
    let owner = app
        .project()
        .clip_tracks()
        .find(|t| t.group.is_some())
        .or_else(|| app.project().clip_tracks().next())
        .map(|t| t.id);
    let Some(owner) = owner else {
        return Task::none();
    };
    let outputs: Vec<_> = app.project().outputs.iter().map(|t| t.id).collect();
    let route = match state {
        "no-output" => Some(Route::empty()),
        "group" | "list" if app.project().group_of(owner).is_some() => Some(Route::Group),
        "fanout" => Some(Route::Outputs(outputs.into_iter().take(2).collect())),
        "mixed" => Some(Route::empty()),
        _ => None,
    };
    if let Some(route) = route {
        crate::edit_actions::execute_edit(
            app,
            qtable_engine::engine::Command::Edit(
                qtable_engine::edit::ProjectEdit::SetTrackRoutes {
                    routes: vec![(owner, route)],
                },
            ),
        );
    }
    let _ = crate::update::update(app, Message::Grid(grid::Action::SelectTrack(owner, false)));
    if state == "mixed" {
        let other = app
            .project()
            .clip_tracks()
            .find(|t| t.id != owner)
            .map(|t| t.id);
        if let Some(other) = other {
            let _ =
                crate::update::update(app, Message::Grid(grid::Action::SelectTrack(other, true)));
        }
    }
    if state == "list" {
        app.open_route_dropdown_at_face(owner)
    } else {
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::multi_select::{Mark, Row};
    use crate::output_band::{route_face, route_rows, route_targets};

    #[test]
    fn fixture_and_capture_states_show_the_approved_faces_and_sections() {
        let project = fixture();
        project.validate().unwrap();
        assert_eq!(project.outputs.len(), 3);
        assert!(!project.outputs[1].mixer.active);
        for (state, expected) in [
            ("no-output", "No Output"),
            ("group", "Group"),
            ("fanout", "Stage +1"),
            ("mixed", "—"),
            ("list", "Group"),
        ] {
            let mut app = App::new(
                qtable_engine::engine::Engine::new(project.clone()).unwrap(),
                String::new(),
                None,
            );
            let _ = setup_state(&mut app, state);
            let targets = route_targets(&app, TrackId(1));
            let face = route_face(app.project(), &targets);
            assert_eq!(face.text, expected);
            if state == "fanout" {
                assert_eq!(face.pips.len(), 2);
                assert!(!face.pips[0].dim && face.pips[1].dim);
            }
            if state == "group" {
                assert_eq!(
                    face.text_colour,
                    Some(crate::outputs::to_colour(project.tracks[0].colour))
                );
            }
            if state == "list" {
                let (rows, _) = route_rows(app.project(), &targets);
                assert_eq!(rows.len(), 7);
                assert!(matches!(rows[1], Row::Divider) && matches!(rows[5], Row::Divider));
                assert!(
                    matches!(&rows[0], Row::Item(item) if item.mark == Mark::Via { sends: true })
                );
                assert!(matches!(&rows[3], Row::Item(item) if !item.enabled));
                assert!(
                    matches!(&rows[6], Row::Item(item) if item.radio && item.mark == Mark::Ticked)
                );
                assert_eq!(app.route_dropdown(), Some(TrackId(1)));
            }
        }
    }
}
