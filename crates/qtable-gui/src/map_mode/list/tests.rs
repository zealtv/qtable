use super::*;
use crate::map_mode::tests::{act, app, send, volume};
use crate::style::detail::{DEFAULT_BROWSER_WIDTH, MAPPING_INSPECTOR_MIN_WIDTH};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Layout as NodeLayout, Shell, layout};
use iced::keyboard::{Key, Location, Modifiers, key::Named};
use iced::{Event, Point, Rectangle, Size, Vector, keyboard, mouse};
use qtable_engine::model::{MappingId, TrackId, TransportControl};

/// The Browser's column at its default width, in a window 700 px tall.
const WINDOW: Size = Size::new(DEFAULT_BROWSER_WIDTH, 700.0);

#[test]
fn a_wide_viewport_fills_exactly_and_shares_path_and_name_equally() {
    for channel in [true, false] {
        let layout = Layout::for_viewport(900.0, channel);
        assert!((layout.row_width() - 900.0).abs() < 0.01);
    }
}

#[test]
fn the_default_browser_shows_every_column_without_scrolling() {
    // The default width inside the 1 px frame: the Ch and Note/Control columns, then about 59 px
    // each.
    let inside = DEFAULT_BROWSER_WIDTH - 2.0 * REGION_FRAME;
    assert_eq!(inside, MAPPING_INSPECTOR_MIN_WIDTH);
    let layout = Layout::for_viewport(inside, true);
    assert!(layout.text > MAPPING_TEXT_MIN_WIDTH, "{layout:?}");
    assert!((layout.row_width() - inside).abs() < 0.01);
    // Key Map Mode has no channel, so Path and Name share its room as well.
    let keys = Layout::for_viewport(inside, false);
    assert!(keys.text > layout.text);
}

#[test]
fn a_narrow_viewport_keeps_the_text_columns_readable_and_scrolls() {
    let layout = Layout::for_viewport(120.0, true);
    assert!((layout.text - MAPPING_TEXT_MIN_WIDTH).abs() < 0.01);
    assert!(layout.row_width() > 120.0);
    assert_eq!(layout, Layout::for_viewport(0.0, true));
}

#[test]
fn clipped_text_ends_in_an_ellipsis_only_when_it_does_not_fit() {
    assert_eq!(clipped("Rhythm", 500.0), "Rhythm");
    let shown = clipped("Rhythm | Mixer | Volume", 40.0);
    assert!(shown.ends_with('…'), "{shown}");
    assert!(shown.chars().count() < "Rhythm | Mixer | Volume".chars().count());
}

#[test]
fn a_short_window_keeps_three_rows_and_shrinks_the_inspector() {
    assert_eq!(inspector_height(700.0), MAPPING_INSPECTOR_HEIGHT);
    let list_min = 2.0 * MAPPING_STRIP_HEIGHT + 3.0 * ROW_STEP;
    let short = inspector_height(list_min + 1.0 + 100.0);
    assert!((short - 100.0).abs() < 0.01, "{short}");
    assert_eq!(inspector_height(10.0), 0.0);
}

#[test]
fn a_row_out_of_view_scrolls_just_far_enough_and_a_visible_one_stays() {
    let room = 3.0 * ROW_STEP;
    for at in 0..3 {
        assert_eq!(offset_to_show(at, 0.0, room), None, "row {at}");
    }
    let down = offset_to_show(5, 0.0, room).unwrap();
    assert!(
        (down - (6.0 * ROW_STEP - ROW_SPACING - room)).abs() < 0.01,
        "{down}"
    );
    assert_eq!(offset_to_show(5, down, room), None);
    assert_eq!(offset_to_show(1, 4.0 * ROW_STEP, room), Some(ROW_STEP));
}

/// Learns `count` CCs (1, 2, …), each onto its own control, in MIDI Map Mode.
fn learned(count: u8) -> crate::app::App {
    use qtable_engine::model::{SceneId, SceneNavigation};
    let mut app = app();
    act(&mut app, Action::Toggle);
    let controls = [
        volume(),
        ControlRef::Transport(TransportControl::Tempo),
        ControlRef::TrackSolo(TrackId(1)),
        ControlRef::Transport(TransportControl::Quantisation),
        ControlRef::TrackActivator(TrackId(1)),
        ControlRef::Transport(TransportControl::Play),
        ControlRef::Transport(TransportControl::StopAll),
        ControlRef::TrackStop(TrackId(1)),
        ControlRef::Scene(SceneId(1)),
        crate::map_mode::tests::slot(),
        ControlRef::SceneNavigation(SceneNavigation::Select),
        ControlRef::TrackSelect(TrackId(1)),
    ];
    for cc in 1..=count {
        act(
            &mut app,
            Action::Pick(controls[usize::from(cc - 1)].clone()),
        );
        send(&mut app, &[0xb0, cc, 1]);
    }
    assert_eq!(app.project().mappings.len(), usize::from(count));
    app
}
use qtable_engine::model::ControlRef;

fn ids(app: &crate::app::App) -> Vec<MappingId> {
    app.project().mappings.iter().map(|row| row.id).collect()
}

/// The bounds of every scrollable in the Browser's column, outermost first, with its id.
fn scrollables(app: &crate::app::App, window: Size) -> Vec<(Option<iced::widget::Id>, Rectangle)> {
    struct Find(Vec<(Option<iced::widget::Id>, Rectangle)>);
    impl Operation for Find {
        fn scrollable(
            &mut self,
            id: Option<&iced::widget::Id>,
            bounds: Rectangle,
            _: Rectangle,
            _: Vector,
            _: &mut dyn iced::advanced::widget::operation::Scrollable,
        ) {
            self.0.push((id.cloned(), bounds));
        }
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
    }
    let mut element = view(app);
    let renderer = crate::grid::test_renderer();
    let mut tree = Tree::new(&element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, window),
    );
    let mut find = Find(Vec::new());
    element
        .as_widget_mut()
        .operate(&mut tree, NodeLayout::new(&node), &renderer, &mut find);
    find.0
}

/// The bounds of the scrollable named `id` in the Browser's column.
fn scrollable_bounds(app: &crate::app::App, id: &'static str) -> Option<Rectangle> {
    let id = iced::widget::Id::new(id);
    scrollables(app, WINDOW)
        .into_iter()
        .find_map(|(found, bounds)| (found.as_ref() == Some(&id)).then_some(bounds))
}

#[test]
fn the_list_and_its_title_strips_sit_inside_the_frame_at_every_browser_width() {
    // The list's outermost scrollable holds the "MIDI Mappings" strip, the column titles and
    // the rows, so its bounds are the strips' bounds. The tour (2026-11-16) saw the strips
    // run onto the 1 px stroke at the default width and at wider ones.
    for key in [false, true] {
        for width in [120.0, DEFAULT_BROWSER_WIDTH, 360.0, 500.0] {
            let mut app = learned(2);
            if key {
                act(&mut app, Action::ToggleKey);
            }
            app.dividers.capture_browser_width(width);
            let width = app.dividers.browser_width();
            let all = scrollables(&app, Size::new(900.0, WINDOW.height));
            let (_, list) = all.first().unwrap();
            let rows_id = iced::widget::Id::new(ROWS);
            let rows = all
                .iter()
                .find_map(|(id, bounds)| (id.as_ref() == Some(&rows_id)).then_some(*bounds))
                .unwrap();
            // Below the default width the rows outgrow the list and scroll sideways inside it.
            let fits = Layout::for_viewport(width - 2.0 * REGION_FRAME, !key).row_width()
                <= width - 2.0 * REGION_FRAME + 0.01;
            let mut checked = vec![("list", *list)];
            if fits {
                checked.push(("rows", rows));
            }
            for (name, bounds) in checked {
                assert!(
                    (bounds.x - REGION_FRAME).abs() < 0.01
                        && (bounds.x + bounds.width - (width - REGION_FRAME)).abs() < 0.01,
                    "key {key}, {width} px: {name} at {bounds:?}"
                );
            }
        }
    }
}

#[test]
fn selecting_rows_never_moves_the_list() {
    let mut app = learned(4);
    let none = scrollable_bounds(&app, ROWS).unwrap();
    for id in ids(&app) {
        act(&mut app, Action::SelectRow(id));
        assert_eq!(scrollable_bounds(&app, ROWS), Some(none), "{id:?}");
    }
    // The rows end where the pinned inspector and its 1 px rule begin, which end at the frame.
    assert!(
        (none.y + none.height - (WINDOW.height - REGION_FRAME - MAPPING_INSPECTOR_HEIGHT - 1.0))
            .abs()
            < 0.01,
        "{none:?}"
    );
}

#[test]
fn the_list_titles_follow_live_and_key_mode_drops_the_channel() {
    // Ch, Note/Control, Path and Name only: no list cell is a control any more.
    let mut app = learned(2);
    app.map_mode.selected_row = None;
    let found: Vec<crate::drafts::ControlId> =
        crate::control_focus::test_registry::controls(&mut view(&app));
    assert!(
        found.is_empty(),
        "nothing selected, nothing to edit: {found:?}"
    );
    act(&mut app, Action::ToggleKey);
    let keys = Layout::for_viewport(
        MAPPING_INSPECTOR_MIN_WIDTH,
        app.map_mode.mode == super::Mode::Midi,
    );
    assert!(!keys.channel);
}

/// The Browser's column as the window holds it: real widgets, one tree kept across events, and
/// every message they publish run through the app's update, as the runtime does.
struct Column {
    app: crate::app::App,
    tree: Option<Tree>,
}

impl Column {
    fn new(app: crate::app::App) -> Self {
        Self { app, tree: None }
    }

    /// Sends `event` at `point` and reports whether a widget captured it.
    fn event(&mut self, event: Event, point: Point) -> bool {
        let renderer = crate::grid::test_renderer();
        let (messages, captured) = {
            let mut element = view(&self.app);
            let tree = self.tree.get_or_insert_with(|| Tree::new(&element));
            tree.diff(&element);
            let node = element.as_widget_mut().layout(
                tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, WINDOW),
            );
            let mut messages = Vec::new();
            let mut shell = Shell::new(&mut messages);
            element.as_widget_mut().update(
                tree,
                &event,
                NodeLayout::new(&node),
                mouse::Cursor::Available(point),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &Rectangle::with_size(WINDOW),
            );
            let captured = shell.is_event_captured();
            drop(shell);
            (messages, captured)
        };
        for message in messages {
            let _ = crate::update::update(&mut self.app, message);
        }
        captured
    }

    fn click(&mut self, point: Point) {
        self.event(
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            point,
        );
        self.event(
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            point,
        );
    }

    /// A key press as the window delivers it: through the widgets first, then to the app with
    /// `ignored` set when no widget took it (`main.rs`).
    fn press(&mut self, named: Named) {
        let key = Key::Named(named);
        let captured = self.event(
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key.clone(),
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: Location::Standard,
                modifiers: Modifiers::empty(),
                text: None,
                repeat: false,
            }),
            Point::ORIGIN,
        );
        let _ = crate::update::update(
            &mut self.app,
            crate::messages::Message::KeyPressed {
                key,
                modifiers: Modifiers::empty(),
                repeat: false,
                ignored: !captured,
            },
        );
    }

    /// The middle of row `at` of the list.
    fn row(at: usize) -> Point {
        Point::new(
            60.0,
            2.0 * MAPPING_STRIP_HEIGHT + at as f32 * ROW_STEP + INLINE_ROW_HEIGHT / 2.0,
        )
    }
}

#[test]
fn up_and_down_move_the_selection_while_the_list_has_focus() {
    let mut column = Column::new(learned(3));
    let ids = ids(&column.app);
    column.click(Column::row(0));
    assert_eq!(column.app.map_mode.selected_row, Some(ids[0]));
    assert_eq!(column.app.focus, crate::focus::Focus::Browser);
    column.press(Named::ArrowDown);
    assert_eq!(column.app.map_mode.selected_row, Some(ids[1]));
    column.press(Named::ArrowDown);
    column.press(Named::ArrowDown);
    // The last row stays selected at the end of the list.
    assert_eq!(column.app.map_mode.selected_row, Some(ids[2]));
    column.press(Named::ArrowUp);
    assert_eq!(column.app.map_mode.selected_row, Some(ids[1]));
    // Nothing editing was touched on the way.
    assert_eq!(column.app.project().mappings.len(), 3);
}

#[test]
fn up_and_down_nudge_a_selected_inspector_number_instead_of_moving_the_selection() {
    let mut column = Column::new(learned(1));
    let id = ids(&column.app)[0];
    column.click(Column::row(0));
    let found: Vec<(crate::drafts::ControlId, Rectangle)> =
        crate::control_focus::test_registry::rectangles_in(&mut view(&column.app), WINDOW);
    let max = found
        .iter()
        .find(|(control, _)| {
            *control
                == crate::drafts::ControlId::field(
                    crate::control_focus::View::Mappings,
                    crate::drafts::DraftKey::Mapping(id, super::super::Cell::Max),
                )
        })
        .unwrap()
        .1;
    column.click(max.center());
    let before = column.app.project().mappings[0].max;
    column.press(Named::ArrowDown);
    assert_eq!(column.app.map_mode.selected_row, Some(id));
    assert!(
        column.app.project().mappings[0].max < before,
        "the number took the key: {} from {before}",
        column.app.project().mappings[0].max
    );
}

#[test]
fn up_and_down_leave_the_list_alone_when_the_browser_is_not_focused() {
    let mut column = Column::new(learned(2));
    let ids = ids(&column.app);
    column.click(Column::row(0));
    column.app.focus = crate::focus::Focus::Grid;
    column.press(Named::ArrowDown);
    assert_eq!(column.app.map_mode.selected_row, Some(ids[0]));
}

/// The centre of the inspector control `cell` of mapping `id`, as the registry lays it out.
fn inspector_control(column: &Column, id: MappingId, cell: super::super::Cell) -> Point {
    let want = |control: &crate::drafts::ControlId| {
        let key = crate::drafts::DraftKey::Mapping(id, cell);
        match cell {
            super::super::Cell::Invert => {
                *control
                    == crate::drafts::ControlId::field_action(
                        crate::control_focus::View::Mappings,
                        key,
                        "invert",
                    )
            }
            super::super::Cell::Mode => {
                *control
                    == crate::drafts::ControlId::field_action(
                        crate::control_focus::View::Mappings,
                        key,
                        "mode",
                    )
            }
            _ => unreachable!("a toggle or a dropdown"),
        }
    };
    crate::control_focus::test_registry::rectangles_in(&mut view(&column.app), WINDOW)
        .into_iter()
        .find(|(control, _)| want(control))
        .expect("the inspector shows the control")
        .1
        .center()
}

/// A Browser that is open, with a track selected to load onto: what Enter would act on.
fn browser_ready(column: &mut Column) {
    column.app.chrome.browser_open = true;
    column.app.browser.selected = None;
    column.app.selection.tracks = vec![TrackId(1)];
    column.app.selection.anchor = Some(TrackId(1));
}

fn devices_on_track_one(app: &crate::app::App) -> usize {
    app.project().track(TrackId(1)).unwrap().devices.len()
}

#[test]
fn up_down_and_enter_on_a_focused_invert_toggle_never_reach_the_hidden_browser() {
    let mut column = Column::new(learned(1));
    let id = ids(&column.app)[0];
    column.click(Column::row(0));
    browser_ready(&mut column);
    // Focus the Invert toggle with a real click, as a user would.
    let invert = inspector_control(&column, id, super::super::Cell::Invert);
    column.click(invert);
    column.app.browser.selected = None;
    let devices = devices_on_track_one(&column.app);
    for key in [Named::ArrowDown, Named::ArrowUp, Named::Enter] {
        column.press(key);
    }
    assert_eq!(
        column.app.browser.selected, None,
        "an arrow key selected a device in the Browser the list replaces"
    );
    assert_eq!(
        devices_on_track_one(&column.app),
        devices,
        "Enter loaded a device"
    );
    assert_eq!(column.app.map_mode.selected_row, Some(id));
}

#[test]
fn up_down_and_enter_on_a_focused_mode_dropdown_never_reach_the_hidden_browser() {
    let mut column = Column::new(learned(1));
    let id = ids(&column.app)[0];
    column.click(Column::row(0));
    browser_ready(&mut column);
    let mode = inspector_control(&column, id, super::super::Cell::Mode);
    column.click(mode);
    column.app.browser.selected = None;
    let devices = devices_on_track_one(&column.app);
    for key in [Named::ArrowDown, Named::ArrowUp, Named::Enter] {
        column.press(key);
    }
    assert_eq!(column.app.browser.selected, None);
    assert_eq!(devices_on_track_one(&column.app), devices);
}

#[test]
fn enter_in_the_list_does_not_load_a_device_either() {
    let mut column = Column::new(learned(2));
    column.click(Column::row(0));
    browser_ready(&mut column);
    column.app.browser.selected = Some(crate::browser::ITEMS[0]);
    let devices = devices_on_track_one(&column.app);
    column.press(Named::Enter);
    assert_eq!(devices_on_track_one(&column.app), devices);
}

#[test]
fn the_browser_keeps_its_keys_when_map_mode_is_off() {
    let mut column = Column::new(learned(1));
    column.click(Column::row(0));
    browser_ready(&mut column);
    column.app.focus = crate::focus::Focus::Browser;
    act(&mut column.app, Action::Toggle);
    column.app.focus = crate::focus::Focus::Browser;
    column.press(Named::ArrowDown);
    assert_eq!(
        column.app.browser.selected,
        crate::browser::ITEMS.first().copied()
    );
}

#[test]
fn the_list_scrolls_to_keep_the_selection_in_view() {
    // A short window: three rows show above the inspector.
    let mut app = learned(12);
    let ids = ids(&app);
    app.map_mode.selected_row = Some(ids[0]);
    app.map_mode.scroll.viewport.set(3.0 * ROW_STEP);
    let _ = crate::map_mode::follow(&mut app);
    for _ in 0..9 {
        let _ = crate::update::update(
            &mut app,
            crate::messages::Message::Map(Action::MoveSelection { down: true }),
        );
    }
    assert_eq!(app.map_mode.selected_row, Some(ids[9]));
    // Row 9 now sits at the viewport's bottom edge.
    let offset = app.map_mode.scroll.offset;
    assert!(
        offset_to_show(9, offset, 3.0 * ROW_STEP).is_none() && offset > 0.0,
        "{offset}"
    );
    // A wheel scroll elsewhere is left alone until the selection or viewport changes.
    let _ = crate::update::update(
        &mut app,
        crate::messages::Message::Map(Action::ListScrolled(0.0)),
    );
    let _ = crate::map_mode::follow(&mut app);
    assert_eq!(app.map_mode.scroll.offset, 0.0);
    // Selecting a row from the window (a learn, a click on a mapped control) follows too.
    app.map_mode.selected_row = Some(ids[11]);
    let _ = crate::map_mode::follow(&mut app);
    assert!(offset_to_show(11, app.map_mode.scroll.offset, 3.0 * ROW_STEP).is_none());
}

#[test]
fn follow_does_no_work_while_selection_revision_mode_and_viewport_hold_still() {
    let mut app = learned(12);
    let ids = ids(&app);
    app.map_mode.selected_row = Some(ids[9]);
    app.map_mode.scroll.viewport.set(3.0 * ROW_STEP);
    let _ = crate::map_mode::follow(&mut app);
    // A sentinel that only a fresh look at the rows could overwrite.
    app.map_mode.scroll.handled = (None, 0);
    for _ in 0..3 {
        let _ = crate::map_mode::follow(&mut app);
    }
    assert_eq!(app.map_mode.scroll.handled, (None, 0));
    // Each of the four changes makes it look again.
    let looked = |app: &mut crate::app::App| {
        app.map_mode.scroll.handled = (None, 0);
        let _ = crate::map_mode::follow(app);
        app.map_mode.scroll.handled != (None, 0)
    };
    app.map_mode.selected_row = Some(ids[8]);
    assert!(looked(&mut app), "a new selection");
    app.map_mode.scroll.viewport.set(4.0 * ROW_STEP);
    assert!(looked(&mut app), "a new viewport height");
    act(&mut app, Action::Invert(ids[0]));
    assert!(looked(&mut app), "a new project revision");
    // Switching modes clears the selection; with it clear on both sides only the mode differs.
    act(&mut app, Action::ToggleKey);
    let _ = crate::map_mode::follow(&mut app);
    act(&mut app, Action::Toggle);
    assert!(looked(&mut app), "a new mode");
}

#[test]
fn an_edit_elsewhere_leaves_a_wheel_scrolled_list_where_it_is() {
    let mut app = learned(12);
    let ids = ids(&app);
    app.map_mode.selected_row = Some(ids[0]);
    app.map_mode.scroll.viewport.set(3.0 * ROW_STEP);
    let _ = crate::map_mode::follow(&mut app);
    // The user wheel-scrolls to the end, then inverts another mapping.
    app.map_mode.scroll.offset = 9.0 * ROW_STEP;
    act(&mut app, Action::Invert(ids[5]));
    let _ = crate::map_mode::follow(&mut app);
    assert_eq!(app.map_mode.scroll.offset, 9.0 * ROW_STEP);
    // Removing a row above the selection moves it up one place, and the list follows.
    app.map_mode.selected_row = Some(ids[11]);
    let _ = crate::map_mode::follow(&mut app);
    app.map_mode.scroll.offset = 0.0;
    act(&mut app, Action::SelectRow(ids[0]));
    act(&mut app, Action::DeleteSelected);
    app.map_mode.selected_row = Some(ids[11]);
    let _ = crate::map_mode::follow(&mut app);
    assert!(
        app.map_mode.scroll.offset > 0.0,
        "the selected row was brought into view"
    );
}
