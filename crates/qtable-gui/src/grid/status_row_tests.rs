//! Native-widget layout and scrolling tests; no desktop window or input is used.
use super::*;
use iced::advanced::{
    Layout as WidgetLayout, Shell, layout,
    widget::{Tree, operation},
};
use iced::{Color, Event, Size};
use qtable_engine::engine::Engine;
use qtable_engine::model::Project;

#[test]
fn status_controls_are_pinned_while_scene_rows_scroll_above_them() {
    let renderer = super::last_row_tests::renderer();
    let engine = Engine::new(Project::default()).unwrap();
    let snapshot = engine.snapshot();
    for scene_count in [8, 80] {
        for has_groups in [false, true] {
            let mut project = crate::files::new_project();
            project.scenes = (1..=scene_count)
                .map(|id| Scene {
                    id: SceneId(id),
                    name: format!("Scene {id}"),
                    follow_actions: Default::default(),
                })
                .collect();
            project.tracks = (1..=16)
                .map(|id| Track::new(TrackId(id), "Track", scene_count as usize))
                .collect();
            if has_groups {
                {
                    let mut group = Track::new_group(TrackId(101), "Group", project.scenes.len());
                    group.colour = qtable_engine::model::LIVE_CLIP_PALETTE[0];
                    group.fold_state = true;
                    project.insert_group(group, &[TrackId(1), TrackId(2)]);
                }
            }
            let cache = LayoutCache::default();
            let size = Size::new(800.0, 600.0);
            let mut widget = view(
                &project,
                &snapshot,
                ViewState {
                    owner: None,
                    cell: None,
                    ghost: None,
                    selected: None,
                    selected_scene: None,
                    selected_scenes: &[],
                    selected_tracks: &[],
                    shift_down: false,
                    scene_stop: Vec::new(),
                    blink_on: false,
                    revision: 0,
                },
                &cache,
            );
            let mut tree = Tree::new(widget.as_widget());
            let node = widget.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, size),
            );
            let top = HEADER_H;
            let status_y = size.height - FOOTER_H;
            for offset in [0.0, 21.0 * 20.0] {
                // Scroll the one region holding the track, output-gap and Main rows.
                let mut scroll = operation::scrollable::scroll_to::<()>(
                    iced::widget::Id::new("session-scene-rows"),
                    operation::scrollable::AbsoluteOffset {
                        x: None,
                        y: Some(offset),
                    },
                );
                widget.as_widget_mut().operate(
                    &mut tree,
                    WidgetLayout::new(&node),
                    &renderer,
                    &mut scroll,
                );
                let mut click = |point| {
                    let mut messages = vec![];
                    widget.as_widget_mut().update(
                        &mut tree,
                        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                        WidgetLayout::new(&node),
                        mouse::Cursor::Available(point),
                        &renderer,
                        &mut iced::advanced::clipboard::Null,
                        &mut Shell::new(&mut messages),
                        &Rectangle::with_size(size),
                    );
                    messages
                        .into_iter()
                        .filter(|a| {
                            !matches!(a, Action::ScrollColumns { .. } | Action::ControlFocused(_))
                        })
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    click(Point::new(GAP + 10.0, status_y + 10.0)),
                    vec![if has_groups {
                        Action::StopGroup(TrackId(101))
                    } else {
                        Action::StopTrack(TrackId(1))
                    }]
                );
                assert_eq!(
                    click(Point::new(size.width - MASTER_W + 10.0, status_y + 10.0)),
                    vec![Action::StopAll]
                );
                // Neither the position display nor the space between Main's buttons acts like a slot.
                assert!(click(Point::new(60.0, status_y + 10.0)).is_empty());
                assert!(
                    click(Point::new(size.width - MASTER_W + 60.0, status_y + 10.0)).is_empty()
                );
                assert_eq!(
                    click(Point::new(size.width - 10.0, status_y + 10.0)),
                    vec![Action::ToggleFollowActions]
                );
                if scene_count == 8 {
                    assert_eq!(
                        click(Point::new(60.0, top + 8.0 * (CELL_H + GAP) + 10.0)),
                        vec![Action::SelectBlankColumn(if has_groups {
                            crate::selection::ColumnOwner::Group(TrackId(101))
                        } else {
                            crate::selection::ColumnOwner::Track(TrackId(1))
                        })]
                    );
                    assert_eq!(
                        click(Point::new(size.width - MASTER_W + 40.0, status_y - 10.0)),
                        vec![Action::SelectBlankColumn(
                            crate::selection::ColumnOwner::Track(project.main.id)
                        )]
                    );
                } else {
                    let slot = if offset == 0.0 { 0 } else { 20 };
                    assert_eq!(
                        click(Point::new(60.0, top + 10.0)),
                        vec![if has_groups {
                            Action::SelectColumn(
                                crate::selection::ColumnOwner::Group(TrackId(101)),
                                Some(slot),
                            )
                        } else {
                            Action::Select(TrackId(1), slot)
                        }]
                    );
                    assert_eq!(
                        click(Point::new(size.width - MASTER_W + 40.0, top + 10.0)),
                        vec![Action::ClickScene(SceneId(slot as u64 + 1))]
                    );
                }
            }
            // The pinned stop row scrolls horizontally on its own container, mirrored from the clips.
            let mut scroll = operation::scrollable::scroll_to::<()>(
                iced::widget::Id::new("session-footer-columns"),
                operation::scrollable::AbsoluteOffset {
                    x: Some(TRACK_W + GAP),
                    y: None,
                },
            );
            widget.as_widget_mut().operate(
                &mut tree,
                WidgetLayout::new(&node),
                &renderer,
                &mut scroll,
            );
            let mut messages = vec![];
            widget.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                WidgetLayout::new(&node),
                mouse::Cursor::Available(Point::new(GAP + 10.0, status_y + 10.0)),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(size),
            );
            assert!(messages.contains(&Action::StopTrack(TrackId(if has_groups { 3 } else { 2 }))));
        }
    }
}

#[test]
fn main_status_row_has_a_stop_all_square_and_a_follow_toggle_at_the_right() {
    let follow = super::status_row::follow_area();
    assert_eq!(
        super::status_row::master_hit(Point::new(4.0, 4.0)),
        Some(Hit::StopAll)
    );
    assert_eq!(
        super::status_row::master_hit(Point::new(follow.x + 4.0, 4.0)),
        Some(Hit::FollowToggle)
    );
    assert_eq!(
        super::status_row::master_hit(Point::new(follow.x - 4.0, 4.0)),
        None
    );
}

#[test]
fn pinned_status_cells_follow_the_shared_selection_for_every_column_kind() {
    use crate::selection::{ColumnOwner, SelectionState};
    use qtable_engine::model::Region;
    let mut project = Project {
        tracks: vec![
            Track::new(TrackId(1), "A", 0),
            Track::new(TrackId(2), "B", 0),
        ],
        outputs: vec![Track::new_output(TrackId(201), "Out", "localhost", 9000)],
        ..Project::default()
    };
    project.insert_group(Track::new_group(TrackId(101), "Group", 0), &[TrackId(2)]);
    let columns = super::layout::columns(&project, Region::Tracks);
    let output = super::layout::columns(&project, Region::Outputs);
    let main = Column::main(project.main.id);
    for (col, owner, selected_tracks, models) in [
        (
            &columns[0],
            ColumnOwner::Track(TrackId(1)),
            vec![TrackId(1)],
            &project.tracks,
        ),
        (
            &columns[1],
            ColumnOwner::Group(TrackId(101)),
            vec![TrackId(101)],
            &project.tracks,
        ),
        (
            &output[0],
            ColumnOwner::Track(TrackId(201)),
            vec![TrackId(201)],
            &project.outputs,
        ),
        (
            &main,
            ColumnOwner::Track(project.main.id),
            vec![],
            &project.tracks,
        ),
    ] {
        let selection = SelectionState {
            owner: Some(owner),
            tracks: selected_tracks,
            ..SelectionState::default()
        };
        assert_eq!(
            status_row::cell_background(&selection.view(models), col),
            crate::style::HIGHLIGHT
        );
        assert_eq!(
            status_row::cell_background(&SelectionState::default().view(models), col),
            Color::from_rgb(0.20, 0.20, 0.20)
        );
    }
    let selection = SelectionState {
        owner: Some(ColumnOwner::Track(project.main.id)),
        scene: Some(SceneId(1)),
        ..SelectionState::default()
    };
    assert_ne!(
        status_row::cell_background(&selection.view(&project.tracks), &main),
        crate::style::HIGHLIGHT
    );
}
