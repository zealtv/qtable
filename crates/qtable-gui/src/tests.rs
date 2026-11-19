use crate::edit_actions::{move_clip_edit, shifted_index};
use crate::fields::{parse_port, parse_tempo};
use crate::files::{
    CANCEL_LABEL, DISCARD_LABEL, DiscardAction, SAVE_LABEL, discard_action, title_text,
};
use crate::grid;
use qtable_engine::edit::ProjectEdit;
use qtable_engine::model::TrackId;
use rfd::MessageDialogResult;

#[test]
fn move_mode_maps_distinct_cells_to_a_clip_move() {
    let source = grid::Selection {
        track: TrackId(3),
        slot: 1,
    };
    let target = grid::Selection {
        track: TrackId(7),
        slot: 4,
    };
    assert_eq!(
        move_clip_edit(source, target),
        Some(ProjectEdit::MoveClip {
            from: (TrackId(3), 1),
            to: (TrackId(7), 4)
        })
    );
    assert_eq!(move_clip_edit(source, source), None);
}

#[test]
fn reorder_mapping_rejects_edges_and_maps_one_step() {
    assert_eq!(shifted_index(1, -1, 4), Some(0));
    assert_eq!(shifted_index(0, -1, 4), None);
    assert_eq!(shifted_index(3, 1, 4), None);
    assert_eq!(shifted_index(2, 1, 4), Some(3));
}

#[test]
fn tempo_input_accepts_positive_finite_numbers() {
    assert_eq!(parse_tempo(" 123.5 ").unwrap(), 123.5);
    for input in ["", "fast", "0", "-1", "NaN", "inf"] {
        assert!(parse_tempo(input).is_err(), "accepted {input:?}");
    }
}

#[test]
fn port_and_delay_fields_parse_valid_values_and_reject_invalid_values() {
    assert_eq!(parse_port(" 9000 ").unwrap(), 9000);
    for input in ["", "0", "65536", "udp"] {
        assert!(parse_port(input).is_err());
    }
    assert_eq!(
        qtable_engine::model::TRACK_DELAY.parse("-2.5").unwrap(),
        -2.5
    );
    for input in ["", "NaN", "inf", "fast"] {
        assert!(qtable_engine::model::TRACK_DELAY.parse(input).is_err());
    }
}

#[test]
fn dirty_title_includes_project_name_and_marker() {
    assert_eq!(
        title_text(Some(std::path::Path::new("shows/set.ron")), false),
        "qtable — set.ron"
    );
    assert_eq!(
        title_text(Some(std::path::Path::new("shows/set.ron")), true),
        "qtable — set.ron •"
    );
    assert_eq!(title_text(None, true), "qtable — Untitled •");
}

#[test]
fn discard_answers_choose_save_discard_or_cancel() {
    assert_eq!(
        discard_action(MessageDialogResult::Yes),
        DiscardAction::Save
    );
    assert_eq!(
        discard_action(MessageDialogResult::No),
        DiscardAction::Discard
    );
    assert_eq!(
        discard_action(MessageDialogResult::Cancel),
        DiscardAction::Cancel
    );
    assert_eq!(
        discard_action(MessageDialogResult::Custom(SAVE_LABEL.into())),
        DiscardAction::Save
    );
    assert_eq!(
        discard_action(MessageDialogResult::Custom(DISCARD_LABEL.into())),
        DiscardAction::Discard
    );
    assert_eq!(
        discard_action(MessageDialogResult::Custom(CANCEL_LABEL.into())),
        DiscardAction::Cancel
    );
}

#[test]
fn the_window_icon_decodes_and_shows_the_artwork() {
    // The embedded RGBA must match the 128 by 128 size window_icon() declares, or the icon is dropped.
    assert!(crate::window_icon().is_some());
    let rgba = include_bytes!("../assets/window-icon.rgba");
    assert_eq!(rgba.len(), 128 * 128 * 4);
    assert_eq!(rgba[3], 0, "the corner stays transparent");
    assert!(
        rgba.chunks_exact(4).any(|pixel| pixel[3] == 255),
        "the artwork is opaque somewhere"
    );
}
