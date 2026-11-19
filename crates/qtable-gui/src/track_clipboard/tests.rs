use crate::control_focus::{ControlOwner, View};
use crate::drafts::ControlId;
use crate::focus::Focus;
use crate::selection::ColumnOwner;
use crate::{app::App, grid, messages::Message};
use iced::keyboard::{Key, Modifiers, key::Named};
use qtable_engine::engine::Engine;
use qtable_engine::model::{Project, Route, Scene, SceneId, Track, TrackId};

/// Tracks 1, 2, 3 (group 101 holds 2 and 3) and 4; outputs 201 and 202; two scenes.
fn app() -> App {
    let mut project = Project {
        scenes: (1..=2)
            .map(|id| Scene {
                id: SceneId(id),
                name: format!("Scene {id}"),
                follow_actions: Default::default(),
            })
            .collect(),
        outputs: vec![
            Track::new_output(TrackId(201), "A", "127.0.0.1", 9000),
            Track::new_output(TrackId(202), "B", "127.0.0.1", 9001),
        ],
        ..Project::default()
    };
    for id in [1, 2, 3, 4] {
        let colour = project.take_next_colour();
        project.tracks.push(Track {
            colour,
            route: Route::single(project.main.id),
            ..Track::new(TrackId(id), format!("T{id}"), 2)
        });
    }
    let mut group = Track::new_group(TrackId(101), "G", 2);
    group.colour = project.take_next_colour();
    group.route = Route::single(project.main.id);
    project.insert_group(group, &[TrackId(2), TrackId(3)]);
    project[TrackId(2)].route = Route::Group;
    project[TrackId(3)].route = Route::Group;
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    app.focus = Focus::Grid;
    app
}

fn act(app: &mut App, action: grid::Action) {
    let _ = crate::update::update(app, Message::Grid(action));
    app.focus = Focus::Grid;
}

fn press(app: &mut App, key: Key, modifiers: Modifiers) {
    let message = crate::focus::event_message(&key, modifiers, false, true).unwrap();
    let _ = crate::update::update(app, message);
}

fn command(app: &mut App, letter: &str) {
    press(app, Key::Character(letter.into()), Modifiers::COMMAND);
}

fn order(app: &App) -> Vec<u64> {
    app.project().tracks.iter().map(|t| t.id.0).collect()
}

fn output_order(app: &App) -> Vec<u64> {
    app.project().outputs.iter().map(|t| t.id.0).collect()
}

fn selected(app: &App) -> Vec<u64> {
    app.selection.tracks.iter().map(|t| t.0).collect()
}

fn select_main(app: &mut App) {
    let main = app.project().main.id;
    act(
        app,
        grid::Action::SelectColumn(ColumnOwner::Track(main), None),
    );
}

fn select(app: &mut App, id: u64) {
    act(app, grid::Action::SelectTrack(TrackId(id), false));
}

#[test]
fn copy_then_paste_puts_the_copy_left_of_the_selected_track_and_selects_it() {
    let mut app = app();
    select(&mut app, 4);
    let revision = app.project_revision();
    command(&mut app, "c");
    assert_eq!(app.project_revision(), revision, "copy edits nothing");
    command(&mut app, "v");
    assert_eq!(
        app.project_revision(),
        revision + 1,
        "one revision per paste"
    );
    let new = app.project().tracks[4].id.0;
    assert_eq!(order(&app), [1, 101, 2, 3, new, 4]);
    assert_eq!(selected(&app), [new]);
    assert_eq!(app.selection.anchor, Some(TrackId(new)));
    // Repeated paste stacks copies leftwards: each goes left of the last paste.
    command(&mut app, "v");
    let newer = app.project().tracks[4].id.0;
    assert_eq!(order(&app), [1, 101, 2, 3, newer, new, 4]);
    assert_eq!(selected(&app), [newer]);
}

#[test]
fn an_output_copies_and_pastes_among_outputs() {
    let mut app = app();
    act(
        &mut app,
        grid::Action::SelectColumn(ColumnOwner::Track(TrackId(202)), None),
    );
    command(&mut app, "c");
    command(&mut app, "v");
    let new = app.project().outputs[1].id.0;
    assert_eq!(output_order(&app), [201, new, 202]);
    assert_eq!(selected(&app), [new]);
    assert_eq!(app.project().outputs[1].name, "B");
}

#[test]
fn a_group_header_copies_the_group_with_its_members() {
    let mut app = app();
    act(&mut app, grid::Action::SelectGroup(TrackId(101), false));
    assert_eq!(app.selection.track(), Some(TrackId(101)));
    command(&mut app, "c");
    select(&mut app, 4);
    command(&mut app, "v");
    let ids = order(&app);
    assert_eq!(ids[..4], [1, 101, 2, 3]);
    let (header, a, b) = (ids[4], ids[5], ids[6]);
    assert_eq!(ids[7], 4);
    let project = app.project();
    assert!(project[TrackId(header)].is_group());
    assert_eq!(app.selection.track(), Some(TrackId(header)));
    assert_eq!(
        project.group_member_ids(TrackId(header)).unwrap(),
        [TrackId(a), TrackId(b)]
    );
    assert_eq!(selected(&app), [header]);
    assert_eq!(
        app.selection.owner,
        Some(ColumnOwner::Group(TrackId(header)))
    );
}

#[test]
fn a_shift_range_copies_in_project_order() {
    let mut app = app();
    select(&mut app, 1);
    act(&mut app, grid::Action::SelectTrack(TrackId(4), true));
    command(&mut app, "c");
    select(&mut app, 4);
    command(&mut app, "v");
    let ids = order(&app);
    // The range covered the whole group, so its header came along and the copy is a group.
    assert_eq!(ids.len(), 5 + 5);
    assert_eq!(ids[..4], [1, 101, 2, 3]);
    assert_eq!(ids[9], 4);
    let project = app.project();
    assert!(project[TrackId(ids[5])].is_group());
    assert_eq!(project[TrackId(ids[4])].name, "T1");
}

#[test]
fn a_partly_covered_group_copies_its_members_as_plain_tracks() {
    let mut app = app();
    app.engine
        .execute(qtable_engine::engine::Command::Edit(
            qtable_engine::edit::ProjectEdit::SetTrackRoutes {
                routes: vec![(TrackId(101), Route::single(TrackId(201)))],
            },
        ))
        .unwrap();
    select(&mut app, 3);
    act(&mut app, grid::Action::SelectTrack(TrackId(4), true));
    command(&mut app, "c");
    select(&mut app, 1);
    command(&mut app, "v");
    let ids = order(&app);
    assert_eq!(ids.len(), 7);
    let project = app.project();
    for id in [ids[0], ids[1]] {
        assert_eq!(project[TrackId(id)].group, None);
    }
    // Leaving its group, a Group-routed member goes to Main, even when the group sends elsewhere.
    assert_eq!(
        project[TrackId(ids[0])].route,
        Route::single(project.main.id)
    );
    assert_eq!(ids[2..6], [1, 101, 2, 3]);
}

#[test]
fn main_and_an_empty_selection_copy_nothing() {
    let mut app = app();
    select_main(&mut app);
    command(&mut app, "c");
    assert!(app.clip_actions.clipboard.is_none());
    app.selection = Default::default();
    command(&mut app, "c");
    assert!(app.clip_actions.clipboard.is_none());
    // A clipboard already holding tracks stays as it was.
    select(&mut app, 4);
    command(&mut app, "c");
    let held = app.clip_actions.clipboard.clone();
    select_main(&mut app);
    command(&mut app, "c");
    assert_eq!(app.clip_actions.clipboard, held);
}

#[test]
fn the_wrong_family_main_or_nothing_selected_appends_at_the_end_of_the_region() {
    let mut app = app();
    select(&mut app, 1);
    command(&mut app, "c");
    // An output is selected: tracks append at the end of the tracks.
    act(
        &mut app,
        grid::Action::SelectColumn(ColumnOwner::Track(TrackId(201)), None),
    );
    command(&mut app, "v");
    let ids = order(&app);
    assert_eq!(ids[..5], [1, 101, 2, 3, 4]);
    assert_eq!(ids.len(), 6);
    assert_eq!(output_order(&app), [201, 202]);
    // Nothing selected, and Main selected: appended again.
    app.selection = Default::default();
    command(&mut app, "v");
    assert_eq!(order(&app).len(), 7);
    select_main(&mut app);
    command(&mut app, "v");
    assert_eq!(order(&app).len(), 8);
    // An output in the clipboard with a track selected appends to the outputs.
    select(&mut app, 4);
    act(
        &mut app,
        grid::Action::SelectColumn(ColumnOwner::Track(TrackId(201)), None),
    );
    command(&mut app, "c");
    select(&mut app, 4);
    command(&mut app, "v");
    assert_eq!(app.project().outputs.len(), 3);
    assert_eq!(output_order(&app)[..2], [201, 202]);
}

#[test]
fn pasting_tracks_with_a_slot_selected_goes_left_of_that_slots_track() {
    let mut app = app();
    select(&mut app, 4);
    command(&mut app, "c");
    act(&mut app, grid::Action::Select(TrackId(1), 0));
    command(&mut app, "v");
    let ids = order(&app);
    assert_eq!(ids[1], 1);
    assert_eq!(ids.len(), 6);
    assert!(app.project().tracks[0].slots[0].clip.is_none());
}

#[test]
fn a_member_target_joins_its_group_and_so_does_a_group_clipboard_as_a_nested_group() {
    let mut app = app();
    select(&mut app, 4);
    command(&mut app, "c");
    select(&mut app, 3);
    command(&mut app, "v");
    let ids = order(&app);
    let new = ids[3];
    assert_eq!(ids, [1, 101, 2, new, 3, 4]);
    assert_eq!(app.project()[TrackId(new)].group, Some(TrackId(101)));
    assert_eq!(app.project()[TrackId(new)].route, Route::Group);
    // A copied group aimed at a member nests inside that member's group, before the member.
    act(&mut app, grid::Action::SelectGroup(TrackId(101), false));
    command(&mut app, "c");
    select(&mut app, 3);
    command(&mut app, "v");
    let ids = order(&app);
    let header = ids[4];
    assert_eq!(ids[..4], [1, 101, 2, new]);
    assert_eq!(ids[8..], [3, 4]);
    let project = app.project();
    assert!(project[TrackId(header)].is_group());
    assert_eq!(project[TrackId(header)].group, Some(TrackId(101)));
    assert_eq!(project[TrackId(header)].route, Route::Group);
    assert_eq!(
        project.group_member_ids(TrackId(header)).unwrap(),
        [TrackId(ids[5]), TrackId(ids[6]), TrackId(ids[7])]
    );
    assert_eq!(app.selection.track(), Some(TrackId(header)));
    project.validate().unwrap();
}

#[test]
fn clip_copy_and_paste_are_unchanged_and_one_clipboard_serves_both() {
    let mut app = app();
    act(&mut app, grid::Action::Select(TrackId(1), 0));
    let _ = crate::update::update(
        &mut app,
        Message::Edit(crate::messages::EditMessage::CreateSelectedClip),
    );
    app.focus = Focus::Grid;
    command(&mut app, "c");
    assert!(crate::clip_actions::has_clip_to_paste(&app));
    act(&mut app, grid::Action::Select(TrackId(4), 1));
    command(&mut app, "v");
    assert!(app.project().tracks.last().unwrap().slots[1].clip.is_some());
    assert_eq!(order(&app).len(), 5, "a clip paste adds no track");
    // Copying a track replaces the clip; slot menus now offer Paste Tracks.
    select(&mut app, 4);
    command(&mut app, "c");
    assert!(!crate::clip_actions::has_clip_to_paste(&app));
}

#[test]
fn the_detail_focus_keeps_its_own_clipboard_and_leaves_the_grid_alone() {
    let mut app = app();
    select(&mut app, 4);
    app.focus = Focus::Detail;
    let revision = app.project_revision();
    command(&mut app, "c");
    command(&mut app, "v");
    assert!(app.clip_actions.clipboard.is_none());
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn a_latched_volume_fader_still_resets_on_delete_and_lets_copy_through() {
    let mut app = app();
    select(&mut app, 4);
    app.engine
        .execute(qtable_engine::engine::Command::Edit(
            qtable_engine::edit::ProjectEdit::set_value(
                qtable_engine::model::ParameterRef::volume(TrackId(4)),
                0.5,
            ),
        ))
        .unwrap();
    app.control_focus.claim(ControlId::action(
        View::Mixer,
        "volume",
        ControlOwner::Track(TrackId(4)),
    ));
    // Mixer strips live inside the grid focus area; a real fader click keeps Grid focus.
    app.focus = Focus::Grid;
    command(&mut app, "c");
    let Some(crate::clip_actions::GridClipboard::Tracks { tracks: copied, .. }) =
        &app.clip_actions.clipboard
    else {
        panic!("the latched fader should let ⌘C copy the track");
    };
    assert_eq!(copied.len(), 1);
    assert_eq!(copied[0].id, TrackId(4));
    press(&mut app, Key::Named(Named::Delete), Modifiers::NONE);
    assert_eq!(app.project()[TrackId(4)].mixer_device.volume.value, 1.0);
    assert_eq!(
        order(&app).len(),
        5,
        "Delete resets the fader and removes no track"
    );
}

#[test]
fn an_open_rename_blocks_copy_and_paste() {
    let mut app = app();
    select(&mut app, 4);
    app.header_actions.renaming = Some(grid::RenameTarget::Header(TrackId(4)));
    command(&mut app, "c");
    assert!(app.clip_actions.clipboard.is_none());
}

#[test]
fn the_paste_capture_fixture_is_a_valid_project_with_clips() {
    let project = super::capture_project();
    project.validate().unwrap();
    assert!(project.group_tracks().next().is_some());
    assert!(!project.outputs.is_empty());
    assert!(
        project
            .tracks
            .iter()
            .any(|t| t.slots.iter().any(|s| s.clip.is_some()))
    );
}

#[test]
fn a_pasted_capture_group_keeps_every_members_route() {
    let mut app = App::new(
        Engine::new(super::capture_project()).unwrap(),
        "120".into(),
        None,
    );
    app.focus = Focus::Grid;
    let before: Vec<_> = app
        .project()
        .tracks
        .iter()
        .map(|t| t.route.clone())
        .collect();
    act(&mut app, grid::Action::SelectGroup(TrackId(10), false));
    command(&mut app, "c");
    command(&mut app, "v");
    let after: Vec<_> = app
        .project()
        .tracks
        .iter()
        .map(|t| t.route.clone())
        .collect();
    assert_eq!(after[..3], before[..3], "{:?}", app.project().tracks);
    assert_eq!(after[3..6], before[..3]);
}

fn member_app(members: usize) -> App {
    let mut app = app();
    if members == 1 {
        let _ = crate::update::update(
            &mut app,
            Message::Edit(crate::messages::EditMessage::DeleteTrack(TrackId(3))),
        );
    }
    app
}

fn group_highlighted(app: &App) -> bool {
    let columns = crate::grid::layout::columns(app.project(), qtable_engine::model::Region::Tracks);
    let group = columns
        .iter()
        .find(|c| c.key == crate::grid::layout::ColumnKey::Track(TrackId(101)))
        .unwrap();
    app.selection
        .view(&app.project().tracks)
        .column_selected(group)
}

#[test]
fn member_headers_copy_paste_duplicate_and_delete_independently_of_group_size() {
    for members in [1, 2] {
        for operation in ["v", "d", "delete", "menu-duplicate", "menu-delete"] {
            let mut app = member_app(members);
            // The header publishes SelectColumn, which dispatches SelectTrack.
            act(
                &mut app,
                grid::Action::SelectColumn(ColumnOwner::Track(TrackId(2)), None),
            );
            assert_eq!(selected(&app), [2]);
            assert_eq!(app.selection.owner, Some(ColumnOwner::Track(TrackId(2))));
            assert!(!group_highlighted(&app));
            assert_eq!(crate::header_actions::selected_group(&app), None);
            act(&mut app, grid::Action::HeaderMenu(TrackId(2)));
            assert_eq!(selected(&app), [2]);
            assert!(!group_highlighted(&app));
            app.close_overlay();
            command(&mut app, "c");
            let Some(crate::clip_actions::GridClipboard::Tracks { tracks, .. }) =
                &app.clip_actions.clipboard
            else {
                panic!("tracks");
            };
            assert_eq!(
                tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
                [TrackId(2)]
            );
            let revision = app.project_revision();
            match operation {
                "delete" => press(&mut app, Key::Named(Named::Delete), Modifiers::NONE),
                "menu-delete" | "menu-duplicate" => {
                    act(&mut app, grid::Action::HeaderMenu(TrackId(2)));
                    let edit = if operation == "menu-delete" {
                        crate::messages::EditMessage::DeleteTrack(TrackId(2))
                    } else {
                        crate::messages::EditMessage::DuplicateTracks
                    };
                    let _ = crate::update::update(&mut app, Message::Edit(edit));
                }
                _ => command(&mut app, operation),
            }
            assert_eq!(app.project_revision(), revision + 1);
            if operation.ends_with("delete") {
                assert!(app.project().track(TrackId(2)).is_none());
                assert_eq!(app.project().members(TrackId(101)).count(), members - 1);
                // The engine removes a group when its last member goes.
                assert_eq!(app.project().track(TrackId(101)).is_some(), members > 1);
                assert!(app.project().track(TrackId(4)).is_some());
            } else {
                assert!(app.project()[TrackId(101)].is_group());
                assert_eq!(app.project().group_tracks().count(), 1);
                let new = app.selection.anchor.unwrap();
                assert_eq!(app.project()[new].group, Some(TrackId(101)));
                assert_eq!(app.project()[new].route, Route::Group);
                assert_eq!(app.project().members(TrackId(101)).count(), members + 1);
                let ids = order(&app);
                let source = ids.iter().position(|id| *id == 2).unwrap();
                let copy = ids.iter().position(|id| *id == new.0).unwrap();
                assert_eq!(
                    copy as isize - source as isize,
                    if operation == "v" { -1 } else { 1 }
                );
                assert!(!group_highlighted(&app));
            }
            app.project().validate().unwrap();
        }
    }
}

#[test]
fn group_headers_still_select_and_duplicate_the_whole_group_of_either_size() {
    for members in [1, 2] {
        let mut app = member_app(members);
        select(&mut app, 2);
        act(&mut app, grid::Action::HeaderMenu(TrackId(101)));
        assert_eq!(app.selection.owner, Some(ColumnOwner::Group(TrackId(101))));
        assert!(group_highlighted(&app));
        app.close_overlay();
        command(&mut app, "c");
        let Some(crate::clip_actions::GridClipboard::Tracks { tracks, .. }) =
            &app.clip_actions.clipboard
        else {
            panic!("tracks");
        };
        assert_eq!(tracks.len(), members + 1);
        assert!(tracks[0].is_group());
        command(&mut app, "d");
        assert_eq!(app.project().group_tracks().count(), 2);
        let Some(ColumnOwner::Group(copy)) = app.selection.owner else {
            panic!("group owner");
        };
        assert_eq!(app.project().members(copy).count(), members);
        app.project().validate().unwrap();
    }
}

#[test]
fn shift_selecting_every_member_without_its_header_does_not_select_the_group() {
    let mut app = app();
    select(&mut app, 2);
    act(&mut app, grid::Action::SelectTrack(TrackId(3), true));
    assert_eq!(selected(&app), [2, 3]);
    assert!(!group_highlighted(&app));
    command(&mut app, "c");
    let Some(crate::clip_actions::GridClipboard::Tracks { tracks, .. }) =
        &app.clip_actions.clipboard
    else {
        panic!("tracks");
    };
    assert_eq!(
        tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
        [TrackId(2), TrackId(3)]
    );
    press(&mut app, Key::Named(Named::Delete), Modifiers::NONE);
    assert_eq!(order(&app), [1, 4]);
}

#[test]
fn shift_ranges_include_group_headers_in_both_directions_and_from_a_group_anchor() {
    for members in [1, 2] {
        for (start, end, group_start) in [
            (1, 4, false),
            (4, 1, false),
            (4, 101, false),
            (101, 4, true),
        ] {
            let mut app = member_app(members);
            if group_start {
                act(&mut app, grid::Action::SelectGroup(TrackId(start), false));
            } else {
                select(&mut app, start);
            }
            let action = if end == 101 {
                grid::Action::SelectGroup(TrackId(end), true)
            } else {
                grid::Action::SelectTrack(TrackId(end), true)
            };
            act(&mut app, action);
            assert!(group_highlighted(&app));
            assert!(app.selection.tracks.contains(&TrackId(101)));
            let range = app.selection.tracks.clone();
            act(&mut app, grid::Action::HeaderMenu(TrackId(101)));
            assert_eq!(app.selection.tracks, range);
            app.close_overlay();
            command(&mut app, "c");
            let Some(crate::clip_actions::GridClipboard::Tracks { tracks, .. }) =
                &app.clip_actions.clipboard
            else {
                panic!("tracks");
            };
            assert!(tracks.iter().any(|t| t.id == TrackId(101)));
            press(&mut app, Key::Named(Named::Delete), Modifiers::NONE);
            // A range containing every track may be refused by the model's last-track rule.
            if start != 1 && end != 1 {
                assert_eq!(order(&app), [1]);
            }
        }
    }
}

#[test]
fn project_replacement_clears_tracks_and_a_stale_generation_cannot_paste() {
    let mut app = app();
    select(&mut app, 4);
    command(&mut app, "c");
    let stale = app.clip_actions.clipboard.clone();
    let _ = crate::update::update(&mut app, Message::Files(crate::messages::FileMessage::New));
    assert!(app.clip_actions.clipboard.is_none());
    let revision = app.project_revision();
    app.clip_actions.clipboard = stale;
    app.focus = Focus::Grid;
    command(&mut app, "v");
    assert_eq!(app.project_revision(), revision);
    assert!(!super::holds_tracks(&app));
    // Open uses the same reset after a successful load, including colliding ids.
    app.engine
        .execute(qtable_engine::engine::Command::LoadProject(
            super::capture_project(),
        ))
        .unwrap();
    app.reset_for_project();
    assert!(app.clip_actions.clipboard.is_none());
}

#[test]
fn missing_jump_refusal_reaches_the_status_line_without_an_edit() {
    let mut app = App::new(
        Engine::new(super::capture_project()).unwrap(),
        "120".into(),
        None,
    );
    select(&mut app, 3);
    command(&mut app, "c");
    let Some(crate::clip_actions::GridClipboard::Tracks { tracks, .. }) =
        &mut app.clip_actions.clipboard
    else {
        panic!("tracks");
    };
    tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .follow_actions
        .actions
        .action_b = qtable_engine::model::FollowAction::Jump(SceneId(9999));
    let revision = app.project_revision();
    command(&mut app, "v");
    assert_eq!(app.project_revision(), revision);
    assert_eq!(
        app.status.message(),
        Some("invalid project: copied tracks jump to a scene that no longer exists")
    );
}

#[test]
fn header_context_menu_preserves_a_range_when_opened_inside_it() {
    let mut app = app();
    select(&mut app, 1);
    act(&mut app, grid::Action::SelectTrack(TrackId(4), true));
    let tracks = app.selection.tracks.clone();
    let anchor = app.selection.anchor;
    for id in [2, 101, 4] {
        act(&mut app, grid::Action::HeaderMenu(TrackId(id)));
        assert_eq!(app.selection.tracks, tracks);
        assert_eq!(app.selection.anchor, anchor);
    }
    act(&mut app, grid::Action::HeaderMenu(TrackId(201)));
    assert_eq!(selected(&app), [201]);
}

#[test]
fn header_menu_messages_copy_and_paste_a_selected_range_end_to_end() {
    let mut app = app();
    select(&mut app, 1);
    act(&mut app, grid::Action::SelectTrack(TrackId(4), true));
    act(&mut app, grid::Action::HeaderMenu(TrackId(2)));
    let _ = crate::update::update(
        &mut app,
        Message::Edit(crate::messages::EditMessage::CopyTracks),
    );
    let revision = app.project_revision();
    let _ = crate::update::update(
        &mut app,
        Message::Edit(crate::messages::EditMessage::PasteTracks),
    );
    assert_eq!(app.project_revision(), revision + 1);
    assert_eq!(order(&app).len(), 10);
    assert_eq!(selected(&app).len(), 3);
    let copied: Vec<_> = app.project().tracks[..5]
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(copied, ["T1", "G", "T2", "T3", "T4"]);
    app.project().validate().unwrap();
}

#[test]
fn opening_a_selected_slots_header_menu_selects_the_header_and_copies_tracks() {
    for slot in [0, 1] {
        let mut app = App::new(
            Engine::new(super::capture_project()).unwrap(),
            "120".into(),
            None,
        );
        let track = TrackId(3);
        act(&mut app, grid::Action::Select(track, slot));
        assert!(app.selection.clip.is_some());
        assert_eq!(app.selection.tracks, [track]);
        act(&mut app, grid::Action::HeaderMenu(track));
        assert_eq!(app.selection.clip, None);
        assert_eq!(app.selection.scene, None);
        command(&mut app, "c");
        let Some(crate::clip_actions::GridClipboard::Tracks { tracks, .. }) =
            &app.clip_actions.clipboard
        else {
            panic!("header Copy must copy tracks after a filled or empty slot selection");
        };
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, track);
    }
}

#[test]
fn duplicate_puts_a_track_copy_right_after_the_source_and_leaves_the_clipboard() {
    let mut app = app();
    select(&mut app, 1);
    let revision = app.project_revision();
    command(&mut app, "d");
    assert_eq!(app.project_revision(), revision + 1, "one revision");
    let new = app.project().tracks[1].id.0;
    assert_eq!(order(&app), [1, new, 101, 2, 3, 4]);
    assert_eq!(selected(&app), [new]);
    assert_eq!(app.selection.anchor, Some(TrackId(new)));
    assert!(
        app.clip_actions.clipboard.is_none(),
        "the clipboard is untouched"
    );
    app.project().validate().unwrap();
}

#[test]
fn duplicate_keeps_a_held_clipboard() {
    let mut app = app();
    select(&mut app, 4);
    command(&mut app, "c");
    let held = app.clip_actions.clipboard.clone();
    select(&mut app, 1);
    command(&mut app, "d");
    assert_eq!(app.clip_actions.clipboard, held);
}

#[test]
fn duplicate_puts_an_output_copy_right_after_the_source() {
    let mut app = app();
    act(
        &mut app,
        grid::Action::SelectColumn(ColumnOwner::Track(TrackId(201)), None),
    );
    command(&mut app, "d");
    let new = app.project().outputs[1].id.0;
    assert_eq!(output_order(&app), [201, new, 202]);
    assert_eq!(selected(&app), [new]);
    assert_eq!(order(&app), [1, 101, 2, 3, 4], "no track was added");
}

#[test]
fn duplicate_copies_a_group_with_its_members_after_the_last_member() {
    let mut app = app();
    act(&mut app, grid::Action::SelectGroup(TrackId(101), false));
    let revision = app.project_revision();
    command(&mut app, "d");
    assert_eq!(app.project_revision(), revision + 1);
    let ids = order(&app);
    assert_eq!(ids[..4], [1, 101, 2, 3]);
    let (header, a, b) = (ids[4], ids[5], ids[6]);
    assert_eq!(ids[7], 4);
    let project = app.project();
    assert!(project[TrackId(header)].is_group());
    assert_eq!(project[TrackId(header)].group, None);
    assert_eq!(
        project.group_member_ids(TrackId(header)).unwrap(),
        [TrackId(a), TrackId(b)]
    );
    assert_eq!(selected(&app), [header]);
    assert_eq!(
        app.selection.owner,
        Some(ColumnOwner::Group(TrackId(header)))
    );
    project.validate().unwrap();
}

#[test]
fn duplicate_copies_a_shift_range_as_one_block_after_it() {
    let mut app = app();
    select(&mut app, 1);
    act(&mut app, grid::Action::SelectTrack(TrackId(4), true));
    let revision = app.project_revision();
    command(&mut app, "d");
    assert_eq!(app.project_revision(), revision + 1);
    let ids = order(&app);
    assert_eq!(ids.len(), 10);
    assert_eq!(ids[..5], [1, 101, 2, 3, 4]);
    let project = app.project();
    let names: Vec<_> = project.tracks[5..]
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(names, ["T1", "G", "T2", "T3", "T4"]);
    assert_eq!(selected(&app).len(), 3);
    project.validate().unwrap();
}

#[test]
fn duplicating_a_group_member_keeps_the_copy_in_its_group() {
    let mut app = app();
    select(&mut app, 2);
    let revision = app.project_revision();
    command(&mut app, "d");
    assert_eq!(app.project_revision(), revision + 1);
    let new = app.project().tracks[3].id.0;
    assert_eq!(order(&app), [1, 101, 2, new, 3, 4]);
    assert_eq!(app.project()[TrackId(new)].group, Some(TrackId(101)));
    assert_eq!(app.project()[TrackId(new)].route, Route::Group);
    // The group's last member: the copy still lands inside the group, before the next top-level track.
    select(&mut app, 3);
    command(&mut app, "d");
    let last = app.project().tracks[5].id.0;
    assert_eq!(order(&app), [1, 101, 2, new, 3, last, 4]);
    assert_eq!(app.project()[TrackId(last)].group, Some(TrackId(101)));
    assert_eq!(selected(&app), [last]);
    app.project().validate().unwrap();
}

#[test]
fn duplicate_on_main_or_an_empty_selection_does_nothing() {
    let mut app = app();
    select_main(&mut app);
    let revision = app.project_revision();
    command(&mut app, "d");
    assert_eq!(app.project_revision(), revision);
    assert_eq!(order(&app), [1, 101, 2, 3, 4]);
    app.selection.tracks.clear();
    command(&mut app, "d");
    assert_eq!(app.project_revision(), revision);
}

#[test]
fn clip_duplicate_is_unchanged_when_a_clip_is_selected() {
    let mut app = app();
    act(&mut app, grid::Action::Select(TrackId(1), 0));
    let _ = crate::update::update(
        &mut app,
        Message::Edit(crate::messages::EditMessage::CreateSelectedClip),
    );
    app.focus = Focus::Grid;
    command(&mut app, "d");
    assert_eq!(order(&app), [1, 101, 2, 3, 4], "no track was added");
    assert!(app.project()[TrackId(1)].slots[0].clip.is_some());
    assert!(app.project()[TrackId(1)].slots[1].clip.is_some());
}

#[test]
fn the_header_menu_duplicates_the_selection_once() {
    let mut app = app();
    select(&mut app, 4);
    let revision = app.project_revision();
    let _ = crate::update::update(
        &mut app,
        Message::Edit(crate::messages::EditMessage::DuplicateTracks),
    );
    assert_eq!(app.project_revision(), revision + 1);
    assert_eq!(order(&app).len(), 6);
}
