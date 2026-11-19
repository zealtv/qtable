//! D1: the projection and every canvas agree through three levels and independent folds.
use super::*;
use iced::{Point, Size};
use qtable_engine::model::{Project, Region, Scene, TrackKind};

pub(crate) fn project() -> Project {
    let mut project = Project {
        tracks: (1..=8)
            .map(|id| Track::new(TrackId(id), format!("Track {id}"), 8))
            .collect(),
        outputs: vec![Track::new_output(TrackId(201), "Output", "localhost", 9000)],
        scenes: (1..=8)
            .map(|id| Scene {
                id: SceneId(id),
                name: format!("Scene {id}"),
                follow_actions: Default::default(),
            })
            .collect(),
        ..Project::default()
    };
    // Outer(Middle(Inner(a,b),c),d),e: unequal subtree ends and independent folds.
    for (index, parent, group, colour) in [
        (0, None, true, [255, 155, 22]),
        (1, Some(1), true, [255, 155, 22]),
        (2, Some(2), true, [240, 240, 100]),
        (3, Some(3), false, [240, 240, 100]),
        (4, Some(3), false, [120, 145, 255]),
        (5, Some(2), false, [255, 155, 22]),
        (6, Some(1), false, [0, 180, 165]),
        (7, None, false, [240, 80, 170]),
    ] {
        let track = &mut project.tracks[index];
        track.group = parent.map(TrackId);
        track.colour = qtable_engine::model::Colour {
            red: colour[0],
            green: colour[1],
            blue: colour[2],
        };
        if group {
            track.kind = TrackKind::Group;
            track.name = "Group".into();
        }
    }
    for (index, row) in [(3, 1), (4, 2), (5, 0), (6, 3)] {
        let track = &mut project.tracks[index];
        track.slots[row].clip = Some(
            Clip::new(
                qtable_engine::model::ClipId(index as u64 + 100),
                format!("Clip {index}"),
                qtable_engine::model::BeatTime(3840),
                track.colour,
            )
            .unwrap(),
        );
    }
    project.validate().unwrap();
    project
}

#[test]
fn all_ancestors_control_visibility_without_changing_inner_flags_or_numbering() {
    let mut p = project();
    let cols = layout::columns(&p, Region::Tracks);
    assert_eq!(cols.len(), 8);
    assert_eq!(
        cols.iter().map(|c| c.ancestors.len()).collect::<Vec<_>>(),
        [0, 1, 2, 3, 3, 2, 1, 0]
    );
    assert_eq!(cols[2].parent, Some(TrackId(2)));
    assert_eq!(cols[2].key, layout::ColumnKey::Track(TrackId(3)));
    p.tracks[2].fold_state = true;
    p.tracks[0].fold_state = true;
    let folded = layout::columns(&p, Region::Tracks);
    assert_eq!(
        folded.iter().map(|c| c.index.unwrap()).collect::<Vec<_>>(),
        [0, 7]
    );
    assert_eq!(folded[1].number, Some(8));
    assert_eq!(folded[0].header_h, HEADER_H);
    p.tracks[0].fold_state = false;
    let reopened = layout::columns(&p, Region::Tracks);
    assert_eq!(
        reopened
            .iter()
            .map(|c| c.index.unwrap())
            .collect::<Vec<_>>(),
        [0, 1, 2, 5, 6, 7]
    );
    assert!(p.tracks[2].fold_state);
    assert_eq!(
        reopened[0].header_h,
        HEADER_H + 2.0 * crate::style::detail::GROUP_HEADER_STEP
    );
}

#[test]
fn bands_titles_fold_hits_and_slot_origins_share_geometry_in_every_region() {
    let p = project();
    let g = test_support::grid(&p);
    let height = HEADER_H + 3.0 * crate::style::detail::GROUP_HEADER_STEP;
    assert_eq!(g.layout.header_h, height);
    for region in [Region::Tracks, Region::Outputs, Region::Main] {
        for col in layout::columns(&p, region) {
            assert_eq!(col.header_h, height);
            assert_eq!(
                col.title(0.0).y,
                col.ancestors.len() as f32 * crate::style::detail::GROUP_HEADER_STEP
            );
            assert_eq!(col.title(0.0).height + col.title(0.0).y, height - GAP);
            assert_eq!(col.label_y(), height - GAP - TRACK_H / 2.0);
        }
    }
    let event = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
    for (ci, col) in g
        .layout
        .columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.caps.foldable)
    {
        let x = g.layout.xs[ci];
        let fold = col.fold(x).center();
        assert_eq!(
            g.header_action(fold),
            Some(Action::ToggleGroup(p.tracks[ci].id))
        );
        assert!(g.control_at(fold).is_some());
        // An ancestor band over the same x must never toggle the child's fold.
        if !col.ancestors.is_empty() {
            assert!(!matches!(
                g.header_action(Point::new(fold.x, 1.0)),
                Some(Action::ToggleGroup(_))
            ));
            assert!(g.control_at(Point::new(fold.x, 1.0)).is_none());
        }
        let at = Point::new(x + CELL_H / 2.0, height + CELL_H / 2.0);
        assert_eq!(
            hit_test_layout(at, &g.layout.columns, 8),
            Some(Hit::GroupLaunch(ci, 0))
        );
        assert!(
            g.update(
                &mut GridState::default(),
                &event,
                Rectangle::with_size(Size::new(1200.0, 500.0)),
                mouse::Cursor::Available(at)
            )
            .is_some()
        );
    }
    let mut m = test_support::master(&p.main, &p.scenes);
    m.header_h = height;
    assert_eq!(
        m.button_hit(Point::new(5.0, height + 5.0)),
        Some(Hit::SceneLaunch(0))
    );
    assert_eq!(m.button_hit(Point::new(5.0, height - 2.0)), None);
}

#[test]
fn nested_layout_height_follows_depth_even_when_deep_columns_are_offscreen() {
    let p = project();
    let cache = LayoutCache::default();
    let layout = cache.get(&p, 0);
    // Region projection is complete; viewport width/scroll never participates in sizing.
    assert_eq!(layout.header_h, layout.columns[7].header_h);
    assert_eq!(layout.columns[7].ancestors.len(), 0);
    assert_eq!(
        layout.columns[3]
            .ancestors
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [TrackId(1), TrackId(2), TrackId(3)]
    );
}

#[test]
#[ignore = "writes the native acceptance fixtures into the supplied artifact directory"]
fn write_native_capture_fixtures() {
    let dir = std::env::var("QTABLE_NESTED_CAPTURE_DIR").expect("artifact directory");
    let mut p = project();
    for (name, outer, inner) in [
        ("three-expanded", false, false),
        ("three-inner-folded", false, true),
        ("outer-folded", true, true),
    ] {
        p.tracks[0].fold_state = outer;
        p.tracks[2].fold_state = inner;
        qtable_engine::file::save(std::path::Path::new(&dir).join(format!("{name}.ron")), &p)
            .unwrap();
    }
    p.tracks[0].fold_state = false;
    p.tracks[2].fold_state = false;
    p.tracks[1].kind = TrackKind::Track;
    // Remove one wrapper, promoting just its direct children.
    p.tracks[2].group = Some(TrackId(1));
    p.tracks[5].group = Some(TrackId(1));
    p.tracks.remove(1);
    qtable_engine::file::save(std::path::Path::new(&dir).join("two-expanded.ron"), &p).unwrap();
    p.tracks[0].fold_state = true;
    qtable_engine::file::save(std::path::Path::new(&dir).join("two-folded.ron"), &p).unwrap();
}

#[test]
fn keyboard_visits_the_same_group_headers_and_cells_as_the_projection() {
    use crate::{
        app::App,
        focus::{Direction, Focus, GridKey},
        messages::Message,
        selection::ColumnOwner,
    };
    let mut app = App::new(
        qtable_engine::engine::Engine::new(project()).unwrap(),
        "120".into(),
        None,
    );
    app.focus = Focus::Grid;
    let _ = crate::update::update(
        &mut app,
        Message::Grid(Action::SelectGroup(TrackId(1), false)),
    );
    for id in [2, 3] {
        let _ = crate::update::update(&mut app, Message::GridKey(GridKey::Move(Direction::Right)));
        assert_eq!(app.selection.owner, Some(ColumnOwner::Group(TrackId(id))));
    }
    let _ = crate::update::update(&mut app, Message::GridKey(GridKey::Move(Direction::Down)));
    assert_eq!(
        app.selection.cell.unwrap().owner,
        ColumnOwner::Group(TrackId(3))
    );
    assert_eq!(app.selection.cell.unwrap().row, 0);
    let _ = crate::update::update(&mut app, Message::GridKey(GridKey::Move(Direction::Down)));
    assert_eq!(app.selection.cell.unwrap().row, 1);
    let _ = crate::update::update(&mut app, Message::GridKey(GridKey::Move(Direction::Right)));
    assert_eq!(
        app.selection.clip,
        Some(Selection {
            track: TrackId(4),
            slot: 1
        })
    );
}

#[test]
fn scrolled_nested_headers_slots_and_pinned_stops_keep_the_same_column() {
    use crate::{app::App, messages::Message};
    use iced::Event;
    use iced::advanced::{Layout as WidgetLayout, Shell, layout as widget_layout, widget::Tree};
    let mut app = App::new(
        qtable_engine::engine::Engine::new(project()).unwrap(),
        "120".into(),
        None,
    );
    app.chrome.detail_open = false;
    let renderer = test_renderer();
    let size = Size::new(1000.0, 700.0);
    for (offset, index) in [(0.0, 0), (3.0 * (TRACK_W + GAP), 3)] {
        let mut widget = crate::view::view(&app);
        let mut tree = Tree::new(&widget);
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &widget_layout::Limits::new(Size::ZERO, size),
        );
        for name in [
            "session-clip-columns",
            "session-header-columns",
            "session-footer-columns",
        ] {
            let mut op = iced::advanced::widget::operation::scrollable::scroll_to::<()>(
                iced::widget::Id::new(name),
                iced::widget::scrollable::AbsoluteOffset {
                    x: Some(offset),
                    y: None,
                },
            );
            widget
                .as_widget_mut()
                .operate(&mut tree, WidgetLayout::new(&node), &renderer, &mut op);
        }
        let rows = last_row_tests::tracks_layout(&node).bounds();
        let id = app.project().tracks[index].id;
        let send = |widget: &mut iced::Element<'_, Message>, tree: &mut Tree, point| {
            let mut messages = vec![];
            for event in [
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                widget.as_widget_mut().update(
                    tree,
                    &Event::Mouse(event),
                    WidgetLayout::new(&node),
                    mouse::Cursor::Available(point),
                    &renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut Shell::new(&mut messages),
                    &Rectangle::with_size(size),
                );
            }
            messages
                .into_iter()
                .filter_map(|message| {
                    if let Message::Grid(action) = message {
                        Some(action)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        let height = app
            .grid_layout
            .get(app.project(), app.project_revision())
            .header_h;
        let title = send(
            &mut widget,
            &mut tree,
            Point::new(rows.x + GAP + 60.0, rows.y + height - TRACK_H / 2.0),
        );
        assert!(
            title.contains(&Action::PressHeader(Box::new(if index == 0 {
                Action::SelectGroup(id, false)
            } else {
                Action::SelectTrack(id, false)
            }))),
            "{title:?}"
        );
        let slot = send(
            &mut widget,
            &mut tree,
            Point::new(rows.x + GAP + 60.0, rows.y + height + CELL_H / 2.0),
        );
        assert!(
            slot.contains(
                &(if index == 0 {
                    Action::SelectColumn(crate::selection::ColumnOwner::Group(id), Some(0))
                } else {
                    Action::Select(id, 0)
                })
            ),
            "{slot:?}"
        );
        let declared = crate::control::map_layer::collect(|op| {
            widget
                .as_widget_mut()
                .operate(&mut tree, WidgetLayout::new(&node), &renderer, op)
        });
        let stop_rect = declared
            .iter()
            .find(|(_, control)| *control == qtable_engine::model::ControlRef::TrackStop(id))
            .expect("visible pinned stop")
            .0;
        let header_rect = declared
            .iter()
            .find(|(_, control)| *control == qtable_engine::model::ControlRef::TrackSelect(id))
            .expect("visible header")
            .0;
        // The maximum scroll offset may clamp before the requested boundary.
        // Compare rendered columns rather than assuming the first begins at x=0.
        assert_eq!(stop_rect.x, header_rect.x);
        let stop = send(&mut widget, &mut tree, stop_rect.center());
        assert!(
            stop.contains(
                &(if index == 0 {
                    Action::StopGroup(id)
                } else {
                    Action::StopTrack(id)
                })
            ),
            "{stop:?}"
        );
        let headers: Vec<_> = declared
            .iter()
            .filter(|(_, control)| *control == qtable_engine::model::ControlRef::TrackSelect(id))
            .collect();
        assert!(!headers.is_empty());
        for (rect, _) in headers {
            let slot_rect = declared
                .iter()
                .find(|(_, control)| {
                    *control
                        == qtable_engine::model::ControlRef::ClipSlot {
                            track: id,
                            scene: app.project().scenes[0].id,
                        }
                })
                .expect("visible first slot")
                .0;
            assert_eq!(rect.y + rect.height + GAP, slot_rect.y);
        }
    }
}
