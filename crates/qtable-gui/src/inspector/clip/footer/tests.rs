use super::*;
use crate::drafts::{ControlFocus, Controls};
use crate::inspector::{event_editor::content::Selection, event_strip};
use iced::Size;
use iced::advanced::{layout, widget::Tree};
use iced::widget::{Space, column, container, text};
use qtable_engine::model::BeatTime;
use std::collections::HashMap;

#[test]
fn selection_changes_keep_editor_height_in_every_tab() {
    let project =
        qtable_engine::file::load(std::path::Path::new("tests/fixtures/lanes.ron")).unwrap();
    let track = &project.tracks[0];
    let clip = track.slots[0].clip.as_ref().unwrap();
    let target = clip
        .lanes
        .iter()
        .find_map(|l| l.envelope())
        .unwrap()
        .target
        .clone();
    let key = ClipKey {
        track: track.id,
        slot: 0,
        id: clip.id,
        epoch: 0,
    };
    let (drafts, errors) = (HashMap::new(), HashMap::new());
    let focus = ControlFocus::default();
    let fields = Fields {
        track: track.id,
        slot: 0,
        drafts: &drafts,
        errors: &errors,
        controls: Controls::new(crate::control::LIVE_DARK, &focus),
    };
    let renderer = crate::grid::test_renderer();
    for tab in EditorTab::ALL {
        for width in [320.0, 1000.0] {
            let choice = Choice {
                tab,
                ..Choice::default()
            };
            let mut baseline = None;
            for selection in 0..6 {
                let inspector = match selection {
                    0 => event_strip::view(&project, clip, &fields, (None, None, None, None)),
                    1 => event_strip::view(
                        &project,
                        clip,
                        &fields,
                        (None, None, Some(target.clone()), None),
                    ),
                    2 => event_strip::view(
                        &project,
                        clip,
                        &fields,
                        (None, None, Some(target.clone()), Some(0)),
                    ),
                    3 => event_strip::view(&project, clip, &fields, (Some(0), Some(0), None, None)),
                    _ => {
                        let mut content = Selection::time(key, BeatTime(0)..clip.length(), vec![]);
                        if selection == 4 {
                            content.points = vec![(target.clone(), 0), (target.clone(), 1)];
                        } else {
                            content.hits = vec![0, 1];
                        }
                        detail::panel(
                            text(content.summary(project.time_signature).unwrap())
                                .size(detail::TEXT_SIZE),
                        )
                        .into()
                    }
                };
                let footer = view(&project, key, 0, (&choice, None), &fields, inspector);
                let mut editor: Element<'_, Message> =
                    column![container(Space::new()).height(Length::Fill), footer]
                        .spacing(detail::CONTROL_GAP)
                        .height(Length::Fill)
                        .into();
                let mut tree = Tree::new(&editor);
                let node = editor.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, Size::new(width, 300.0)),
                );
                let geometry = (node.children()[0].bounds(), node.children()[1].bounds());
                if let Some(expected) = baseline {
                    assert_eq!(
                        geometry, expected,
                        "{tab:?}, width {width}, selection {selection}"
                    );
                } else {
                    baseline = Some(geometry);
                }
                // Every visible panel fits without scrolling, including the hit arguments.
                if selection == 2 || selection == 3 {
                    let slot = node.children()[1].children().last().unwrap();
                    assert!(slot.children()[1].size().height <= slot.size().height + 0.01);
                }
            }
        }
    }
}

#[test]
fn wrapped_hits_fit_without_scrollbars_and_reserve_their_height_when_unselected() {
    let mut project =
        qtable_engine::file::load(std::path::Path::new("tests/fixtures/hit-arguments.ron"))
            .unwrap();
    let track = project.tracks[0].id;
    let empty = project
        .hit(
            track,
            BeatTime(960),
            qtable_engine::model::OscAddress::Relative("/empty".into()),
            vec![],
        )
        .unwrap();
    project.tracks[0].slots[0]
        .clip
        .as_mut()
        .unwrap()
        .events
        .push(empty);
    let clip = project.clip(track, 0).unwrap();
    let key = ClipKey {
        track,
        slot: 0,
        id: clip.id,
        epoch: 0,
    };
    let (drafts, errors) = (HashMap::new(), HashMap::new());
    let focus = ControlFocus::default();
    let fields = Fields {
        track,
        slot: 0,
        drafts: &drafts,
        errors: &errors,
        controls: Controls::new(crate::control::LIVE_DARK, &focus),
    };
    let renderer = crate::grid::test_renderer();
    let scroll: Element<'_, Message> = iced::widget::scrollable(Space::new()).into();
    let scroll_tag = Tree::new(&scroll).tag;
    fn has_tag(tree: &Tree, tag: iced::advanced::widget::tree::Tag) -> bool {
        tree.tag == tag || tree.children.iter().any(|t| has_tag(t, tag))
    }
    let mut heights = vec![];
    for width in [320.0, 600.0, 1000.0] {
        let mut baseline = None;
        for tab in [EditorTab::Events, EditorTab::Overview] {
            let choice = Choice {
                tab,
                ..Choice::default()
            };
            for selected in [None, Some(0), Some(1)] {
                let lane = selected.map(|i| {
                    clip.lane_refs()
                        .iter()
                        .position(|r| *r == clip.events[i].definition)
                        .unwrap()
                });
                let panel =
                    event_strip::view(&project, clip, &fields, (lane, selected, None, None));
                let mut panel_tree = Tree::new(&panel);
                assert!(
                    !has_tag(&panel_tree, scroll_tag),
                    "hit inspector must not scroll"
                );
                let mut natural =
                    event_strip::view(&project, clip, &fields, (lane, selected, None, None));
                let natural_size = natural
                    .as_widget_mut()
                    .layout(
                        &mut panel_tree,
                        &renderer,
                        &layout::Limits::new(Size::ZERO, Size::new(width, 2000.0)),
                    )
                    .size();
                let mut footer = view(&project, key, 0, (&choice, None), &fields, panel);
                let mut tree = Tree::new(&footer);
                assert!(!has_tag(&tree, scroll_tag), "footer must not scroll");
                let node = footer.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, Size::new(width, 600.0)),
                );
                let slot = &node.children()[0];
                let visible = &slot.children()[1];
                assert_eq!(
                    visible.size(),
                    natural_size,
                    "fields must retain their natural height"
                );
                assert!(visible.size().height <= slot.size().height);
                if let Some(expected) = baseline {
                    assert_eq!(node.size(), expected, "{tab:?}, selection {selected:?}");
                } else {
                    baseline = Some(node.size());
                }
            }
        }
        heights.push(baseline.unwrap().height);
    }
    assert!(
        heights[0] > heights[1],
        "narrow columns reserve extra rows: {heights:?}"
    );
    assert!(heights[1] >= heights[2]);
}
