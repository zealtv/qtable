use super::*;
use qtable_engine::model::{Colour, SceneId, TrackId, TrackKind};

fn labels(entries: &[Entry]) -> Vec<&'static str> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Command { label, .. } => Some(*label),
            _ => None,
        })
        .collect()
}

fn enabled(entries: &[Entry], wanted: &str) -> Option<bool> {
    entries.iter().find_map(|entry| match entry {
        Entry::Command { label, enabled, .. } if *label == wanted => Some(*enabled),
        _ => None,
    })
}

#[test]
fn a_clip_menu_lists_live_s_commands_then_the_palette() {
    let colour = Colour {
        red: 1,
        green: 2,
        blue: 3,
    };
    let entries = slot_entries(Some(colour), true, false);
    assert_eq!(
        labels(&entries),
        [
            "Cut",
            "Copy",
            "Paste",
            "Duplicate",
            "Delete",
            "Rename",
            "Move Clip…",
            "Toggle Stop Button"
        ]
    );
    assert!(
        matches!(entries.last(), Some(Entry::Palette { current, owner: None, .. }) if *current == colour)
    );
}

#[test]
fn an_empty_slot_menu_offers_insert_clip_paste_and_the_stop_button() {
    let entries = slot_entries(None, false, false);
    assert_eq!(
        labels(&entries),
        ["Insert Clip", "Paste", "Toggle Stop Button"]
    );
    assert_eq!(enabled(&entries, "Insert Clip"), Some(true));
}

#[test]
fn paste_is_enabled_only_with_something_on_the_clipboard() {
    assert_eq!(
        enabled(&slot_entries(None, false, false), "Paste"),
        Some(false)
    );
    assert_eq!(
        enabled(&slot_entries(None, true, false), "Paste"),
        Some(true)
    );
}

#[test]
fn the_palette_fills_five_rows_of_fourteen() {
    assert_eq!(LIVE_CLIP_PALETTE.len(), 70);
    assert_eq!(LIVE_CLIP_PALETTE.len() / SWATCHES_PER_ROW, 5);
    assert!(height(&slot_entries(Some(LIVE_CLIP_PALETTE[0]), true, false)) > palette_h());
}

fn command_message(entries: &[Entry], wanted: &str) -> Option<Message> {
    entries.iter().find_map(|entry| match entry {
        Entry::Command { label, message, .. } if *label == wanted => Some(message.clone()),
        _ => None,
    })
}

#[test]
fn a_track_menu_offers_rename_delete_and_the_palette_for_that_track() {
    let owner = TrackId(7);
    let entries = header_entries(
        owner,
        TrackKind::Track.capabilities(),
        LIVE_CLIP_PALETTE[3],
        false,
        false,
    );
    assert_eq!(
        labels(&entries),
        [
            "Rename",
            "Copy",
            "Paste",
            "Duplicate",
            "Delete",
            "Insert Track",
            "Group Tracks"
        ]
    );
    assert_eq!(enabled(&entries, "Copy"), Some(true));
    assert_eq!(enabled(&entries, "Paste"), Some(false));
    assert_eq!(enabled(&entries, "Duplicate"), Some(true));
    assert!(matches!(
        command_message(&entries, "Duplicate"),
        Some(Message::Edit(EditMessage::DuplicateTracks))
    ));
    let pasteable = header_entries(
        owner,
        TrackKind::Track.capabilities(),
        LIVE_CLIP_PALETTE[3],
        false,
        true,
    );
    assert_eq!(enabled(&pasteable, "Paste"), Some(true));
    assert!(matches!(
        command_message(&entries, "Copy"),
        Some(Message::Edit(EditMessage::CopyTracks))
    ));
    assert!(matches!(
        command_message(&entries, "Paste"),
        Some(Message::Edit(EditMessage::PasteTracks))
    ));
    assert_eq!(enabled(&entries, "Group Tracks"), Some(false));
    assert_eq!(
        enabled(
            &header_entries(
                owner,
                TrackKind::Track.capabilities(),
                LIVE_CLIP_PALETTE[3],
                true,
                false
            ),
            "Group Tracks"
        ),
        Some(true)
    );
    assert!(matches!(
        command_message(&entries, "Delete"),
        Some(Message::Edit(EditMessage::DeleteTrack(TrackId(7))))
    ));
    assert!(matches!(
        entries.last(),
        Some(Entry::Palette { owner: Some(o), .. }) if *o == owner
    ));
}

#[test]
fn a_group_menu_offers_rename_ungroup_and_the_palette() {
    let entries = header_entries(
        TrackId(102),
        TrackKind::Group.capabilities(),
        LIVE_CLIP_PALETTE[3],
        false,
        false,
    );
    assert_eq!(
        labels(&entries),
        [
            "Rename",
            "Copy",
            "Paste",
            "Duplicate",
            "Delete",
            "Insert Track",
            "Group Tracks",
            "Ungroup Tracks"
        ]
    );
    assert!(matches!(
        command_message(&entries, "Ungroup Tracks"),
        Some(Message::Outputs(OutputMessage::Ungroup(TrackId(102))))
    ));
}

#[test]
fn an_output_menu_is_a_track_menu_without_grouping() {
    let owner = TrackId(201);
    let entries = header_entries(
        owner,
        TrackKind::Output.capabilities(),
        LIVE_CLIP_PALETTE[3],
        true,
        false,
    );
    assert_eq!(
        labels(&entries),
        [
            "Rename",
            "Copy",
            "Paste",
            "Duplicate",
            "Delete",
            "Insert Output"
        ]
    );
    assert!(matches!(
        command_message(&entries, "Delete"),
        Some(Message::Edit(EditMessage::DeleteTrack(TrackId(201))))
    ));
    assert!(matches!(
        entries.last(),
        Some(Entry::Palette { owner: Some(o), .. }) if *o == owner
    ));
}

#[test]
fn empty_outputs_space_offers_insert_output() {
    assert_eq!(labels(&region_entries(Region::Outputs)), ["Insert Output"]);
    assert_eq!(labels(&region_entries(Region::Tracks)), ["Insert Track"]);
}

#[test]
fn a_scene_menu_offers_rename_delete_and_insert_scene() {
    let entries = scene_entries(SceneId(3));
    assert_eq!(enabled(&entries, "Rename"), Some(true));
    assert_eq!(labels(&entries), ["Delete", "Rename", "Insert Scene"]);
}

#[test]
fn main_s_menu_leaves_out_copy_paste_and_duplicate() {
    let entries = header_entries(
        TrackId(0),
        TrackKind::Main.capabilities(),
        LIVE_CLIP_PALETTE[3],
        false,
        true,
    );
    let labels = labels(&entries);
    assert!(
        !labels.contains(&"Copy") && !labels.contains(&"Paste") && !labels.contains(&"Duplicate"),
        "{labels:?}"
    );
}

#[test]
fn slot_and_header_menus_offer_tracks_only_where_they_can_paste() {
    let mut app = crate::app::App::new(
        qtable_engine::engine::Engine::new(crate::track_clipboard::capture_project()).unwrap(),
        "120".into(),
        None,
    );
    let source = app
        .project()
        .tracks
        .iter()
        .find(|t| !t.is_group() && t.group.is_none())
        .unwrap()
        .id;
    let output = app.project().outputs[0].id;
    let _ = crate::update::update(
        &mut app,
        Message::Grid(crate::grid::Action::SelectTrack(source, false)),
    );
    crate::track_clipboard::copy(&mut app);
    assert_eq!(
        enabled(&current_entries(&app, MenuTarget::Header(source)), "Paste"),
        Some(true)
    );
    assert_eq!(
        enabled(&current_entries(&app, MenuTarget::Header(output)), "Paste"),
        Some(false)
    );
    let _ = crate::update::update(
        &mut app,
        Message::Grid(crate::grid::Action::ContextMenu(source, 0)),
    );
    let entries = current_entries(&app, MenuTarget::Slot);
    assert_eq!(enabled(&entries, "Paste Tracks"), Some(true));
    let message = command_message(&entries, "Paste Tracks").unwrap();
    let count = app.project().tracks.len();
    let _ = crate::update::update(&mut app, message);
    assert_eq!(app.project().tracks.len(), count + 1);
    assert!(app.project()[app.selection.anchor.unwrap()].group.is_none());
    let _ = crate::update::update(
        &mut app,
        Message::Grid(crate::grid::Action::SelectColumn(
            crate::selection::ColumnOwner::Track(output),
            None,
        )),
    );
    crate::track_clipboard::copy(&mut app);
    assert_eq!(
        enabled(&current_entries(&app, MenuTarget::Header(output)), "Paste"),
        Some(true)
    );
    assert_eq!(
        enabled(&current_entries(&app, MenuTarget::Header(source)), "Paste"),
        Some(false)
    );
}
