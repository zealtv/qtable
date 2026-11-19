//! Group slots: clicks launch or stop the group's row through the engine commands, folded
//! or not, while member clips stay launchable; the slot summarises its members' state.
use super::cache::GridState;
use super::*;
use qtable_engine::model::{BeatTime, ClipId, LIVE_CLIP_PALETTE, Project, Scene};

/// Tracks 1–3 with two scenes; tracks 1 and 2 form group 7. Track 2 holds a clip in row 0;
/// row 1 has no clips, and only track 1 keeps its stop button there.
fn project(folded: bool) -> Project {
    let mut tracks: Vec<_> = (1..=3)
        .map(|i| Track::new(TrackId(i), format!("T{i}"), 2))
        .collect();
    tracks[1].slots[0].clip =
        Some(Clip::new(ClipId(1), "clip", BeatTime(960), LIVE_CLIP_PALETTE[3]).unwrap());
    tracks[1].slots[1].has_stop_button = false;
    let mut project = Project {
        tracks,
        scenes: (1..=2)
            .map(|id| Scene {
                id: SceneId(id),
                name: String::new(),
                follow_actions: Default::default(),
            })
            .collect(),
        ..Project::default()
    };
    let mut group = Track::new_group(TrackId(107), "G", 2);
    group.colour = LIVE_CLIP_PALETTE[0];
    group.fold_state = folded;
    project.insert_group(group, &[TrackId(1), TrackId(2)]);
    project
}

fn press(grid: &Grid<'_>, point: Point) -> Option<Action> {
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(1000.0, 400.0));
    let event = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    grid.update(
        &mut GridState::default(),
        &event,
        bounds,
        mouse::Cursor::Available(point),
    )
    .and_then(|action| action.into_inner().0)
}

fn slot(column: usize, row: usize, square: bool) -> Point {
    Point::new(
        GAP + column as f32 * (TRACK_W + GAP) + if square { 8.0 } else { 60.0 },
        HEADER_H + row as f32 * (CELL_H + GAP) + CELL_H / 2.0,
    )
}

#[test]
fn group_slot_launches_only_the_group_row_folded_or_not() {
    for folded in [false, true] {
        let project = project(folded);
        let mut grid = super::test_support::grid(&project);
        assert_eq!(
            press(&grid, slot(0, 0, true)),
            Some(Action::LaunchGroupSlot(TrackId(107), 0))
        );
        // A row with no clips but a member stop button stops through the same command.
        assert_eq!(
            press(&grid, slot(0, 1, true)),
            Some(Action::LaunchGroupSlot(TrackId(107), 1))
        );
        // The slot body selects the member clip it summarises.
        assert_eq!(
            press(&grid, slot(0, 0, false)),
            Some(Action::SelectColumn(
                crate::selection::ColumnOwner::Group(TrackId(107)),
                Some(0)
            ))
        );
        if !folded {
            // Member clips stay launchable beside their group track.
            assert_eq!(
                press(&grid, slot(2, 0, true)),
                Some(Action::LaunchClip(TrackId(2), 0))
            );
        }
        grid.status_row = true;
        assert_eq!(
            press(&grid, Point::new(GAP + 8.0, FOOTER_H / 2.0)),
            Some(Action::StopGroup(TrackId(107)))
        );
    }
}

#[test]
fn group_slot_without_member_clips_or_stop_buttons_only_selects() {
    let mut project = project(false);
    project[TrackId(1)].slots[1].has_stop_button = false;
    let grid = super::test_support::grid(&project);
    assert_eq!(
        press(&grid, slot(0, 1, true)),
        Some(Action::SelectColumn(
            crate::selection::ColumnOwner::Group(TrackId(107)),
            Some(1)
        ))
    );
    assert_eq!(grid.button_hit(slot(0, 1, true)), None);
}

#[test]
fn group_slot_summary_takes_a_playing_member_clip_colour_first() {
    let mut project = project(false);
    project[TrackId(1)].slots[0].clip =
        Some(Clip::new(ClipId(2), "first", BeatTime(960), LIVE_CLIP_PALETTE[5]).unwrap());
    let mut grid = super::test_support::grid(&project);
    let col = grid.layout.columns[0].clone();
    let colour_of = |i: usize| super::draw::colour(LIVE_CLIP_PALETTE[i]);
    let idle = super::group_column::summary(&grid, &col, 0);
    assert_eq!(
        idle.stripes,
        Some(colour_of(5)),
        "first member clip when idle"
    );
    assert!(!idle.playing && !idle.triggered);
    grid.cells[2][0].status = CellStatus::Playing(0.5);
    let playing = super::group_column::summary(&grid, &col, 0);
    assert_eq!(playing.stripes, Some(colour_of(3)));
    assert!(playing.playing);
    grid.cells[2][0].status = CellStatus::Triggered;
    assert!(super::group_column::summary(&grid, &col, 0).triggered);
    let empty = super::group_column::summary(&grid, &col, 1);
    assert_eq!(empty.stripes, None);
    assert!(empty.stop_button);
}
