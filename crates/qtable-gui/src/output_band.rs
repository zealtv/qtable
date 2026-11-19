//! Output routing and receiver destination, shared with the Detail View.
use crate::app::App;

#[cfg(any(test, feature = "dev-hooks"))]
mod capture;
mod dropdown;
use crate::control::multi_select::{self, FaceLed, Mark, MultiFace, MultiItem, Pip, Row};
use crate::control::{Readout, SizeRole};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::{Control, ControlId};
use crate::info::Id;
use crate::messages::{Message, OutputMessage};
use crate::outputs::to_colour;
use crate::style::{DISABLED, detail};
pub(crate) use dropdown::keyboard;
#[cfg(test)]
pub(crate) use dropdown::{RouteKey, key};
use iced::widget::{Space, column, container, text};
use iced::{Color, Element, Length};
use qtable_engine::model::{Project, Route, Track, TrackId};

/// Neutral ink for a face or row with no single owning colour.
const NEUTRAL: Color = crate::control::LIVE_DARK.neutral;

/// What a row of the route list chooses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RouteChoice {
    Destination(TrackId),
    Group,
}

/// Routes are sets; present them in the checklist's destination order.
fn ordered_destinations(project: &Project, ids: &[TrackId]) -> Vec<TrackId> {
    project
        .destinations()
        .filter(|output| ids.contains(&output.id))
        .map(|output| output.id)
        .collect()
}

/// Whether the track follows its group's route (a stray `Route::Group` outside a group sends nowhere).
fn follows_group(project: &Project, track: &Track) -> bool {
    track.route == Route::Group && project.group_of(track.id).is_some()
}

/// The tracks a route choice edits: the whole selection when the owner is in it and more than one
/// selected track routes, else just the owner. In project order.
pub(crate) fn route_targets(app: &App, owner: TrackId) -> Vec<TrackId> {
    let project = app.project();
    if app.selection.tracks.contains(&owner) {
        let selected: Vec<TrackId> = project
            .tracks
            .iter()
            .filter(|t| app.selection.tracks.contains(&t.id) && t.capabilities().routes)
            .map(|t| t.id)
            .collect();
        if selected.len() > 1 {
            return selected;
        }
    }
    vec![owner]
}

fn target_tracks<'a>(project: &'a Project, targets: &[TrackId]) -> Vec<&'a Track> {
    targets.iter().filter_map(|id| project.track(*id)).collect()
}

/// Group is offered only when every target is a member of a group, never a group itself
/// (a group cannot inherit a group).
fn group_offered(project: &Project, tracks: &[&Track]) -> bool {
    !tracks.is_empty()
        && tracks
            .iter()
            .all(|t| !t.is_group() && project.group_of(t.id).is_some())
}

/// The mark one target gives a destination row.
fn target_mark(project: &Project, track: &Track, destination: TrackId) -> Mark {
    let sends = project.resolved_route(track.id).contains(&destination);
    if follows_group(project, track) {
        Mark::Via { sends }
    } else if sends {
        Mark::Ticked
    } else {
        Mark::Off
    }
}

/// One mark for all targets: the shared one, or Mixed when they differ.
fn agree(marks: impl Iterator<Item = Mark>) -> Mark {
    let mut marks = marks;
    let Some(first) = marks.next() else {
        return Mark::Off;
    };
    if marks.all(|m| m == first) {
        first
    } else {
        Mark::Mixed
    }
}

fn group_colour(project: &Project, tracks: &[&Track]) -> Option<Color> {
    let groups: Vec<_> = tracks
        .iter()
        .map(|t| project.group_of(t.id).map(|g| g.id))
        .collect();
    let first = (*groups.first()?)?;
    groups
        .iter()
        .all(|g| *g == Some(first))
        .then(|| project.track(first).map(|g| to_colour(g.colour)))
        .flatten()
}

/// The route list: Main, then outputs, then Group (when every target is grouped), divided by
/// rules. The second vector parallels the rows: the choice an item row makes, `None` for dividers.
pub(crate) fn route_rows(
    project: &Project,
    targets: &[TrackId],
) -> (Vec<Row>, Vec<Option<RouteChoice>>) {
    let tracks = target_tracks(project, targets);
    let item = |output: &Track| {
        let mark = agree(tracks.iter().map(|t| target_mark(project, t, output.id)));
        let note = route_note(output, matches!(mark, Mark::Via { .. }));
        Row::Item(MultiItem {
            label: output.name.clone(),
            colour: to_colour(output.colour),
            mark,
            enabled: sends(output),
            radio: false,
            note,
        })
    };
    let mut rows = Vec::new();
    let mut choices = Vec::new();
    let mut push = |row, choice| {
        rows.push(row);
        choices.push(choice);
    };
    push(
        item(&project.main),
        Some(RouteChoice::Destination(project.main.id)),
    );
    if !project.outputs.is_empty() {
        push(Row::Divider, None);
        for output in &project.outputs {
            push(item(output), Some(RouteChoice::Destination(output.id)));
        }
    }
    if group_offered(project, &tracks) {
        let mark = agree(tracks.iter().map(|t| {
            if follows_group(project, t) {
                Mark::Ticked
            } else {
                Mark::Off
            }
        }));
        push(Row::Divider, None);
        push(
            Row::Item(MultiItem {
                label: "Group".into(),
                colour: group_colour(project, &tracks).unwrap_or(NEUTRAL),
                mark,
                enabled: true,
                radio: true,
                note: None,
            }),
            Some(RouteChoice::Group),
        );
    }
    (rows, choices)
}

/// The closed face for the targets' routes: pips and a name ("Stage +1"), "No Output", "Group"
/// in the group's colour, or "—" when the targets disagree.
pub(crate) fn route_face(project: &Project, targets: &[TrackId]) -> MultiFace {
    let tracks = target_tracks(project, targets);
    let plain = |text: &str| MultiFace {
        led: FaceLed::None,
        pips: Vec::new(),
        text: text.into(),
        text_colour: None,
    };
    let states: Vec<Option<Vec<TrackId>>> = tracks
        .iter()
        .map(|t| {
            (!follows_group(project, t))
                .then(|| ordered_destinations(project, project.resolved_route(t.id)))
        })
        .collect();
    let Some(first) = states.first() else {
        return plain("No Output");
    };
    if states.iter().any(|s| s != first) {
        return plain("—");
    }
    let Some(ids) = first else {
        return MultiFace {
            text_colour: group_colour(project, &tracks),
            ..plain("Group")
        };
    };
    let routed: Vec<&Track> = ids
        .iter()
        .filter_map(|id| project.destination(*id))
        .collect();
    let Some((head, rest)) = routed.split_first() else {
        return plain("No Output");
    };
    let more = if rest.is_empty() {
        String::new()
    } else {
        format!(" +{}", rest.len())
    };
    MultiFace {
        led: FaceLed::None,
        pips: routed
            .iter()
            .map(|o| Pip {
                colour: to_colour(o.colour),
                dim: !o.mixer.active,
            })
            .collect(),
        text: format!("{}{more}", output_label(head)),
        text_colour: None,
    }
}

/// Dispatch by capability: a destination shows itself, a router shows its toggles.
pub(crate) fn output(app: &App, owner: TrackId) -> Element<'_, Message> {
    let project = app.project();
    let Some(capabilities) = project.track_capabilities(owner) else {
        return Space::new().into();
    };
    if capabilities.destination {
        return project
            .destination(owner)
            .map_or_else(|| Space::new().into(), |output| receiver(app, output));
    }
    if capabilities.routes {
        return route_cell(app, owner);
    }
    Space::new().into()
}

/// Routing and destination stay visible together; editing belongs to the Detail View.
fn receiver<'a>(app: &'a App, output: &'a Track) -> Element<'a, Message> {
    let Some(destination) = output.destination() else {
        // An unset destination (Main in an old project) still says so.
        return container(
            column![
                label("Output"),
                value_box(app, output, "(no host)".into(), true)
            ]
            .spacing(detail::LABEL_GAP),
        )
        .padding(detail::STRIP_INSET)
        .width(Length::Fill)
        .into();
    };
    container(
        column![
            label("Output"),
            value_box(
                app,
                output,
                format!("{}:{}", destination.host, destination.port),
                false
            ),
        ]
        .spacing(detail::LABEL_GAP),
    )
    .padding(detail::STRIP_INSET)
    .width(Length::Fill)
    .into()
}

/// The strip's host readout: the shared readout box in its compact role.
fn value_box<'a>(
    app: &App,
    output: &Track,
    content: String,
    disabled: bool,
) -> Element<'a, Message> {
    app.controls().readout_box(
        Control::new(
            ControlId::action(View::Mixer, "host", ControlOwner::Track(output.id)),
            Id::OutputHost,
            Readout::left(),
        )
        .role(SizeRole::Strip)
        .enabled(!disabled),
        content,
    )
}

/// The route face under an "Output" label. Pressing it opens the list in the overlay slot.
fn route_cell(app: &App, owner: TrackId) -> Element<'_, Message> {
    let face = route_face(app.project(), &route_targets(app, owner));
    let control = Control::new(
        ControlId::action(View::Mixer, "route", ControlOwner::Track(owner)),
        Id::RouteMenu,
        face,
    );
    let body = column![
        label("Output"),
        app.controls().multi_face(
            control,
            Message::Outputs(OutputMessage::OpenRouteDropdown(owner))
        ),
    ]
    .spacing(detail::LABEL_GAP);
    container(body)
        .padding(detail::STRIP_INSET)
        .width(Length::Fill)
        .into()
}

fn label<'a>(name: &'a str) -> Element<'a, Message> {
    text(name)
        .size(detail::COMPACT_TEXT_SIZE)
        .color(DISABLED)
        .into()
}

/// The open list for the overlay slot, and its content height. Pressing a row sends `ChooseRoute`.
pub(crate) fn route_list(
    app: &App,
    owner: TrackId,
    highlight: Option<usize>,
) -> (Element<'_, Message>, f32) {
    let (rows, choices) = route_rows(app.project(), &route_targets(app, owner));
    let height = multi_select::list_height(&rows);
    let list = multi_select::list(&app.theme, &rows, highlight, move |index| {
        let choice = choices[index].expect("only item rows send a choice");
        Message::Outputs(OutputMessage::ChooseRoute(owner, choice))
    });
    (list, height)
}

/// Whether a route to this destination can send: it is on and, if it needs a host, has one.
fn sends(output: &Track) -> bool {
    output.mixer.active && !unset(output)
}

/// A destination-capable track whose destination is not set yet (Main in an old project).
fn unset(output: &Track) -> bool {
    output.capabilities().destination && output.destination().is_none()
}

fn route_note(output: &Track, via_group: bool) -> Option<&'static str> {
    match (via_group, unset(output), output.mixer.active) {
        (true, true, _) => Some("(via group, no host)"),
        (false, true, _) => Some("(no host)"),
        (true, false, false) => Some("(via group, off)"),
        (false, false, false) => Some("(off)"),
        (true, false, true) => Some("(via group)"),
        (false, false, true) => None,
    }
}

fn output_label(output: &Track) -> String {
    let note = if unset(output) {
        " (no host)"
    } else if output.mixer.active {
        ""
    } else {
        " ×"
    };
    format!("{}{note}", output.name)
}

/// Apply a route-list choice to every target as one edit, so a multi-track choice lands whole or
/// not at all.
pub(crate) fn choose_route(app: &mut App, owner: TrackId, choice: RouteChoice) {
    use qtable_engine::{edit::ProjectEdit, engine::Command};
    let targets = route_targets(app, owner);
    let edit = match choice {
        RouteChoice::Destination(output) => ProjectEdit::ToggleOutputInRoute {
            tracks: targets,
            output,
        },
        RouteChoice::Group => ProjectEdit::SetTrackRoutes {
            routes: targets.into_iter().map(|id| (id, Route::Group)).collect(),
        },
    };
    crate::edit_actions::execute_edit(app, Command::Edit(edit));
}

/// Unattended screenshots use a grouped owner and the same anchor operation as the keyboard.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture(app: &mut App) -> iced::Task<Message> {
    if std::env::var("QTABLE_CAPTURE_CLIP").as_deref() != Ok("route-dropdown") {
        return iced::Task::none();
    }
    capture::setup(app)
}

#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture_project() -> qtable_engine::model::Project {
    capture::fixture()
}

#[cfg(test)]
mod tests {
    use super::*;
    use qtable_engine::engine::Engine;

    const LIGHTS: TrackId = TrackId(201);
    const VIDEO: TrackId = TrackId(202);

    /// Main; outputs Lights and Video; track 1 alone on Main; tracks 2 (follows) and 3 (own
    /// route Lights) in group 4, whose route is Main + Video.
    fn project() -> Project {
        let mut project = Project::default();
        let main = project.main.id;
        project.outputs = vec![
            Track::new_output(LIGHTS, "Lights", "127.0.0.1", 9000),
            Track::new_output(VIDEO, "Video", "127.0.0.1", 9001),
        ];
        let mut alone = Track::new(TrackId(1), "Alone", 0);
        alone.route = Route::single(main);
        let mut follower = Track::new(TrackId(2), "Follower", 0);
        follower.group = Some(TrackId(4));
        follower.route = Route::Group;
        let mut own = Track::new(TrackId(3), "Own", 0);
        own.group = Some(TrackId(4));
        own.route = Route::single(LIGHTS);
        let mut group = Track::new_group(TrackId(4), "Rhythm", 0);
        group.route = Route::Outputs(vec![main, VIDEO]);
        project.tracks = vec![alone, group, follower, own];
        project
    }

    fn marks(rows: &[Row]) -> Vec<Option<Mark>> {
        rows.iter()
            .map(|row| match row {
                Row::Divider => None,
                Row::Item(item) => Some(item.mark),
            })
            .collect()
    }

    #[test]
    fn rows_run_main_then_outputs_then_group_with_dividers() {
        let project = project();
        let (rows, choices) = route_rows(&project, &[TrackId(3)]);
        assert_eq!(
            choices,
            [
                Some(RouteChoice::Destination(project.main.id)),
                None,
                Some(RouteChoice::Destination(LIGHTS)),
                Some(RouteChoice::Destination(VIDEO)),
                None,
                Some(RouteChoice::Group),
            ]
        );
        assert!(matches!(rows[1], Row::Divider) && matches!(rows[4], Row::Divider));
        // An ungrouped track gets no Group section, and no trailing divider.
        let (rows, _) = route_rows(&project, &[TrackId(1)]);
        assert_eq!(rows.len(), 4);
    }

    #[test]
    fn group_shows_only_when_every_target_is_grouped() {
        let project = project();
        assert_eq!(route_rows(&project, &[TrackId(2), TrackId(3)]).0.len(), 6);
        assert_eq!(route_rows(&project, &[TrackId(1), TrackId(3)]).0.len(), 4);
        // A group track itself cannot inherit a group.
        assert_eq!(route_rows(&project, &[TrackId(4)]).0.len(), 4);
    }

    #[test]
    fn on_group_the_destinations_show_via_the_groups_route() {
        let project = project();
        let (rows, _) = route_rows(&project, &[TrackId(2)]);
        assert_eq!(
            marks(&rows),
            [
                Some(Mark::Via { sends: true }),
                None,
                Some(Mark::Via { sends: false }),
                Some(Mark::Via { sends: true }),
                None,
                Some(Mark::Ticked),
            ]
        );
        let Row::Item(main) = &rows[0] else { panic!() };
        assert_eq!(main.note, Some("(via group)"));
    }

    #[test]
    fn rows_across_targets_that_differ_are_mixed() {
        let project = project();
        // Track 1 sends to Main; track 3 sends to Lights.
        let (rows, _) = route_rows(&project, &[TrackId(1), TrackId(3)]);
        assert_eq!(
            marks(&rows),
            [Some(Mark::Mixed), None, Some(Mark::Mixed), Some(Mark::Off)]
        );
    }

    #[test]
    fn the_face_reads_none_one_many_group_and_mixed() {
        let mut project = project();
        let main = project.main.id;
        project.tracks[0].route = Route::empty();
        assert_eq!(route_face(&project, &[TrackId(1)]).text, "No Output");

        project.tracks[0].route = Route::Outputs(vec![LIGHTS, VIDEO]);
        project.outputs[1].mixer.active = false;
        let face = route_face(&project, &[TrackId(1)]);
        assert_eq!(face.text, "Lights +1");
        assert_eq!(face.pips.len(), 2);
        assert!(!face.pips[0].dim && face.pips[1].dim);
        project.tracks[0].route = Route::single(main);
        assert_eq!(route_face(&project, &[TrackId(1)]).pips.len(), 1);

        let face = route_face(&project, &[TrackId(2)]);
        assert_eq!(face.text, "Group");
        assert!(face.pips.is_empty());
        assert_eq!(
            face.text_colour,
            Some(to_colour(project.track(TrackId(4)).unwrap().colour))
        );

        assert_eq!(route_face(&project, &[TrackId(1), TrackId(2)]).text, "—");
        assert_eq!(route_face(&project, &[TrackId(1), TrackId(3)]).text, "—");
    }

    #[test]
    fn faces_compare_sets_in_checklist_order() {
        let mut project = project();
        let main = project.main.id;
        project[TrackId(1)].route = Route::Outputs(vec![main, LIGHTS]);
        project[TrackId(3)].route = Route::Outputs(vec![LIGHTS, main]);
        let face = route_face(&project, &[TrackId(1), TrackId(3)]);
        assert_eq!(face.text, "Main +1");
        assert_eq!(face.pips[0].colour, to_colour(project.main.colour));
        assert_eq!(face.pips[1].colour, to_colour(project.outputs[0].colour));
    }

    #[test]
    fn face_follows_reordered_outputs() {
        let mut project = project();
        project[TrackId(1)].route = Route::Outputs(vec![LIGHTS, VIDEO]);
        assert_eq!(route_face(&project, &[TrackId(1)]).text, "Lights +1");
        project.outputs.reverse();
        assert_eq!(route_face(&project, &[TrackId(1)]).text, "Video +1");
    }

    #[test]
    fn targets_are_the_selection_only_when_it_holds_the_owner() {
        let mut app = App::new(Engine::new(project()).unwrap(), String::new(), None);
        app.selection.tracks = vec![TrackId(1), TrackId(2), TrackId(3)];
        assert_eq!(
            route_targets(&app, TrackId(2)),
            [TrackId(1), TrackId(2), TrackId(3)]
        );
        app.selection.tracks = vec![TrackId(1), TrackId(3)];
        assert_eq!(route_targets(&app, TrackId(2)), [TrackId(2)]);
        app.selection.tracks = vec![TrackId(3)];
        assert_eq!(route_targets(&app, TrackId(3)), [TrackId(3)]);
    }

    #[test]
    fn an_unset_main_keeps_its_disabled_no_host_look() {
        let mut project = project();
        project.main.output = None;
        let main = project.main.id;
        project.tracks[0].route = Route::single(main);
        assert_eq!(route_face(&project, &[TrackId(1)]).text, "Main (no host)");
        assert!(!sends(&project.main));
        assert_eq!(route_note(&project.main, false), Some("(no host)"));
        let (rows, _) = route_rows(&project, &[TrackId(1)]);
        let Row::Item(item) = &rows[0] else { panic!() };
        assert!(!item.enabled);
        assert_eq!(item.note, Some("(no host)"));
    }

    #[test]
    fn inherited_rows_keep_disabled_destination_reasons() {
        let mut project = project();
        project.main.output = None;
        project.outputs[1].mixer.active = false;
        let (rows, _) = route_rows(&project, &[TrackId(2)]);
        for (index, expected) in [
            (0, "(via group, no host)"),
            (2, "(via group)"),
            (3, "(via group, off)"),
        ] {
            let Row::Item(item) = &rows[index] else {
                panic!()
            };
            assert_eq!(item.note, Some(expected));
        }
    }

    #[test]
    fn route_list_height_covers_every_row() {
        let mut main_only = Project::default();
        main_only.tracks.push(Track::new(TrackId(1), "Cue", 0));
        let app = App::new(Engine::new(main_only).unwrap(), String::new(), None);
        assert_eq!(route_list(&app, TrackId(1), None).1, 20.0); // One Main item.
        let app = App::new(Engine::new(project()).unwrap(), String::new(), None);
        assert_eq!(route_list(&app, TrackId(2), None).1, 98.0); // Four items, two separators, flush.
    }
}
