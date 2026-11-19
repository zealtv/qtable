//! Copy, Paste and Duplicate over nested groups (2026-11-06).
use crate::clip_actions::GridClipboard;
use crate::focus::Focus;
use crate::{app::App, grid, messages::Message};
use iced::keyboard::{Key, Modifiers};
use qtable_engine::engine::Engine;
use qtable_engine::model::{Project, Route, Scene, SceneId, Track, TrackId};

/// Outer 10 (Inner 20 (a 1, b 2), c 3) and a root track d 4; two scenes. Outer is folded,
/// so Inner, a and b are hidden. Every descendant routes to its group.
fn app() -> App {
    let mut project = Project {
        scenes: (1..=2)
            .map(|id| Scene {
                id: SceneId(id),
                name: format!("Scene {id}"),
                follow_actions: Default::default(),
            })
            .collect(),
        ..Project::default()
    };
    for (id, group, parent) in [
        (10, true, None),
        (20, true, Some(10)),
        (1, false, Some(20)),
        (2, false, Some(20)),
        (3, false, Some(10)),
        (4, false, None),
    ] {
        let mut track = if group {
            Track::new_group(TrackId(id), format!("G{id}"), 2)
        } else {
            Track::new(TrackId(id), format!("T{id}"), 2)
        };
        track.colour = project.take_next_colour();
        track.group = parent.map(TrackId);
        track.route = if parent.is_some() {
            Route::Group
        } else {
            Route::single(project.main.id)
        };
        project.tracks.push(track);
    }
    project[TrackId(10)].fold_state = true;
    project.validate().unwrap();
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    app.focus = Focus::Grid;
    app
}

fn act(app: &mut App, action: grid::Action) {
    let _ = crate::update::update(app, Message::Grid(action));
    app.focus = Focus::Grid;
}

fn command(app: &mut App, letter: &str) {
    let key = Key::Character(letter.into());
    let message = crate::focus::event_message(&key, Modifiers::COMMAND, false, true).unwrap();
    let _ = crate::update::update(app, message);
}

fn order(app: &App) -> Vec<u64> {
    app.project().tracks.iter().map(|t| t.id.0).collect()
}

fn select(app: &mut App, id: u64) {
    act(app, grid::Action::SelectTrack(TrackId(id), false));
}

fn select_group(app: &mut App, id: u64) {
    act(app, grid::Action::SelectGroup(TrackId(id), false));
}

fn copied(app: &App) -> Vec<u64> {
    match &app.clip_actions.clipboard {
        Some(GridClipboard::Tracks { tracks, .. }) => tracks.iter().map(|t| t.id.0).collect(),
        _ => panic!("no copied tracks"),
    }
}

#[test]
fn a_selected_ancestor_with_selected_descendants_copies_each_track_once() {
    let mut app = app();
    // Overlap is explicit; selecting Outer alone does not implicitly select Inner.
    select_group(&mut app, 10);
    assert_eq!(app.selection.tracks, [TrackId(10)]);
    app.selection.tracks.extend([TrackId(20), TrackId(1)]);
    command(&mut app, "c");
    assert_eq!(copied(&app), [10, 20, 1, 2, 3]);
    // A Shift range from a hidden leaf through the whole outer group adds nothing twice.
    select(&mut app, 1);
    act(&mut app, grid::Action::SelectTrack(TrackId(4), true));
    command(&mut app, "c");
    let held = copied(&app);
    assert_eq!(
        held.len(),
        held.iter().collect::<std::collections::HashSet<_>>().len()
    );
    assert!(held.ends_with(&[4]));
}

#[test]
fn hidden_inner_groups_and_leaves_travel_with_a_folded_outer_group() {
    let mut app = app();
    select_group(&mut app, 10);
    command(&mut app, "c");
    select(&mut app, 4);
    let revision = app.project_revision();
    command(&mut app, "v");
    assert_eq!(
        app.project_revision(),
        revision + 1,
        "one revision per paste"
    );
    let ids = order(&app);
    assert_eq!(ids[..6], [10, 20, 1, 2, 3, ids[5]]);
    assert_eq!(ids.len(), 11);
    assert_eq!(ids[10], 4);
    let project = app.project();
    let (outer, inner, a, b, c) = (ids[5], ids[6], ids[7], ids[8], ids[9]);
    for id in [outer, inner, a, b, c] {
        assert!(
            id > 10 && ![10, 20, 1, 2, 3, 4].contains(&id),
            "{id} is fresh"
        );
    }
    assert_eq!(project[TrackId(outer)].group, None);
    assert_eq!(project[TrackId(inner)].group, Some(TrackId(outer)));
    assert_eq!(project[TrackId(a)].group, Some(TrackId(inner)));
    assert_eq!(project[TrackId(b)].group, Some(TrackId(inner)));
    assert_eq!(project[TrackId(c)].group, Some(TrackId(outer)));
    // The folded flag of every level rides along, and the pasted outer group is selected.
    assert!(project[TrackId(outer)].fold_state);
    assert_eq!(app.selection.track(), Some(TrackId(outer)));
    project.validate().unwrap();
}

#[test]
fn paste_enters_the_parent_of_the_leftmost_selected_column_at_any_level() {
    let mut app = app();
    select(&mut app, 4);
    command(&mut app, "c");
    // Aimed at the nested group Inner: the copy joins Outer, just before Inner.
    select_group(&mut app, 20);
    command(&mut app, "v");
    let ids = order(&app);
    let new = ids[1];
    assert_eq!(ids, [10, new, 20, 1, 2, 3, 4]);
    let project = app.project();
    assert_eq!(project[TrackId(new)].group, Some(TrackId(10)));
    assert_eq!(project[TrackId(new)].route, Route::Group);
    // Aimed at a deeper member, the copy joins that member's own group.
    select(&mut app, 2);
    command(&mut app, "v");
    let ids = order(&app);
    assert_eq!(ids.len(), 8);
    let at = ids.iter().position(|id| *id == 2).unwrap();
    let project = app.project();
    assert_eq!(project[TrackId(ids[at - 1])].group, Some(TrackId(20)));
    project.validate().unwrap();
}

#[test]
fn a_group_clipboard_nests_under_the_target_columns_group() {
    let mut app = app();
    select_group(&mut app, 20);
    command(&mut app, "c");
    select(&mut app, 3);
    command(&mut app, "v");
    let ids = order(&app);
    assert_eq!(ids[..5], [10, 20, 1, 2, ids[4]]);
    let header = TrackId(ids[4]);
    let project = app.project();
    assert!(project[header].is_group());
    assert_eq!(project[header].group, Some(TrackId(10)));
    assert_eq!(project[header].route, Route::Group);
    assert_eq!(
        project.group_member_ids(header).unwrap(),
        [TrackId(ids[5]), TrackId(ids[6])]
    );
    assert_eq!(ids[7..], [3, 4]);
    project.validate().unwrap();
}

#[test]
fn duplicate_lands_after_the_source_subtree_under_the_same_parent() {
    // A nested group: after its whole subtree, still inside Outer, keeping its route.
    let mut app = app();
    select_group(&mut app, 20);
    command(&mut app, "d");
    let ids = order(&app);
    assert_eq!(ids[..4], [10, 20, 1, 2]);
    assert_eq!(ids[7..], [3, 4]);
    let copy = TrackId(ids[4]);
    let project = app.project();
    assert!(project[copy].is_group());
    assert_eq!(project[copy].group, Some(TrackId(10)));
    assert_eq!(project[copy].route, Route::Group);
    assert_eq!(project.group_member_ids(copy).unwrap().len(), 2);
    assert_eq!(app.selection.track(), Some(copy));
    project.validate().unwrap();
    // The last child of Outer: after it, at the end of Outer, before the root track.
    let mut app = self::app();
    select(&mut app, 3);
    command(&mut app, "d");
    let ids = order(&app);
    assert_eq!(ids[..5], [10, 20, 1, 2, 3]);
    assert_eq!(ids[6], 4);
    assert_eq!(app.project()[TrackId(ids[5])].group, Some(TrackId(10)));
    // The last member of Inner, whose next neighbour belongs to Outer: stays in Inner.
    let mut app = self::app();
    select(&mut app, 2);
    command(&mut app, "d");
    let ids = order(&app);
    assert_eq!(ids[..4], [10, 20, 1, 2]);
    assert_eq!(ids[5..], [3, 4]);
    assert_eq!(app.project()[TrackId(ids[4])].group, Some(TrackId(20)));
    // A whole Outer subtree at top level duplicates after itself, ahead of the next root.
    let mut app = self::app();
    select_group(&mut app, 10);
    command(&mut app, "d");
    let ids = order(&app);
    assert_eq!(ids[..5], [10, 20, 1, 2, 3]);
    assert_eq!(ids.len(), 11);
    assert_eq!(ids[10], 4);
    assert_eq!(app.project()[TrackId(ids[5])].group, None);
    app.project().validate().unwrap();
}

#[test]
fn a_copy_survives_the_removal_of_its_source_parent() {
    let mut app = app();
    select_group(&mut app, 20);
    command(&mut app, "c");
    let _ = crate::update::update(
        &mut app,
        Message::Edit(crate::messages::EditMessage::DeleteTrack(TrackId(10))),
    );
    assert_eq!(order(&app), [4]);
    select(&mut app, 4);
    command(&mut app, "v");
    let ids = order(&app);
    assert_eq!(ids.len(), 4);
    assert_eq!(ids[3], 4);
    let project = app.project();
    // The copied Inner no longer has an Outer: it pastes at top level and sends to Main.
    assert_eq!(project[TrackId(ids[0])].group, None);
    assert_eq!(
        project[TrackId(ids[0])].route,
        Route::single(project.main.id)
    );
    project.validate().unwrap();
}

#[test]
fn a_partial_copied_group_is_refused_with_a_status_message_and_no_edit() {
    let mut app = app();
    select_group(&mut app, 10);
    command(&mut app, "c");
    let Some(GridClipboard::Tracks { tracks, .. }) = &mut app.clip_actions.clipboard else {
        panic!("tracks");
    };
    // Lose Inner's children, as a stale or hand-built clipboard might.
    tracks.retain(|t| t.id.0 == 10 || t.id.0 == 20);
    select(&mut app, 4);
    let revision = app.project_revision();
    let before = order(&app);
    command(&mut app, "v");
    assert_eq!(app.project_revision(), revision);
    assert_eq!(order(&app), before);
    assert!(
        app.status
            .message()
            .is_some_and(|m| m.contains("copied group must include its members"))
    );
}

#[test]
fn duplicating_a_group_with_another_root_keeps_every_copy_selected() {
    let mut app = app();
    select_group(&mut app, 10);
    act(&mut app, grid::Action::SelectTrack(TrackId(4), true));
    let before: std::collections::HashSet<_> = order(&app).into_iter().collect();
    command(&mut app, "d");
    let fresh: Vec<u64> = order(&app)
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(
        fresh.len(),
        6,
        "Outer's whole subtree and d are copied once"
    );
    let selected: Vec<u64> = app.selection.tracks.iter().map(|t| t.0).collect();
    assert_eq!(selected, [fresh[0], fresh[5]]);
}
