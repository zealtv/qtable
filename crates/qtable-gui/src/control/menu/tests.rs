use super::*;
use crate::control::LIVE_DARK;

const WINDOW: Size = Size {
    width: 1000.0,
    height: 700.0,
};
const MENU: Size = Size {
    width: 200.0,
    height: 300.0,
};

#[test]
fn a_menu_with_room_opens_below_and_right_of_the_click() {
    let at = Anchor::point(Point::new(100.0, 50.0)).place(MENU, WINDOW);
    assert_eq!(at, Point::new(100.0, 50.0));
}

#[test]
fn a_menu_near_the_right_edge_opens_to_the_left_of_the_click() {
    let at = Anchor::point(Point::new(900.0, 50.0)).place(MENU, WINDOW);
    assert_eq!(at, Point::new(700.0, 50.0));
}

#[test]
fn a_menu_near_the_bottom_opens_above_the_click() {
    let at = Anchor::point(Point::new(100.0, 600.0)).place(MENU, WINDOW);
    assert_eq!(at, Point::new(100.0, 300.0));
}

#[test]
fn a_menu_in_the_bottom_right_corner_flips_both_ways() {
    let at = Anchor::point(Point::new(990.0, 690.0)).place(MENU, WINDOW);
    assert_eq!(at, Point::new(790.0, 390.0));
}

#[test]
fn a_menu_without_room_on_either_side_stays_inside_the_window() {
    let tall = Size::new(200.0, 650.0);
    // More room below (650 → 50 left) than above (100): bottom-aligned with the window.
    let at = Anchor::point(Point::new(100.0, 100.0)).place(tall, WINDOW);
    assert_eq!(at, Point::new(100.0, 50.0));
    // More room above: pinned to the top.
    let at = Anchor::point(Point::new(100.0, 600.0)).place(tall, WINDOW);
    assert_eq!(at, Point::new(100.0, 0.0));
    // Taller than the window: from the top.
    let at = Anchor::point(Point::new(100.0, 100.0)).place(Size::new(200.0, 900.0), WINDOW);
    assert_eq!(at.y, 0.0);
}

#[test]
fn a_dropdown_opens_below_its_face_or_above_it_without_covering_it() {
    let face = Rectangle::new(Point::new(50.0, 100.0), Size::new(120.0, 20.0));
    let at = Anchor::below(face, 2.0).place(MENU, WINDOW);
    assert_eq!(at, Point::new(50.0, 122.0));
    let low = Rectangle::new(Point::new(850.0, 600.0), Size::new(120.0, 20.0));
    let at = Anchor::below(low, 2.0).place(MENU, WINDOW);
    // Above the face, 2 px clear, and right-aligned with it.
    assert_eq!(at, Point::new(770.0, 298.0));
}

#[test]
fn a_submenu_opens_beside_its_parent_on_the_side_with_room_and_the_parent_stays() {
    let click = Point::new(700.0, 100.0);
    let parent = Anchor::point(click).place(MENU, WINDOW);
    assert_eq!(parent, click);
    // The submenu's row in the open parent, at its right edge (900).
    let row = Rectangle::new(Point::new(parent.x, 140.0), Size::new(MENU.width, 20.0));
    let submenu = Anchor::beside(row).place(MENU, WINDOW);
    // No room right of 900 for 200 px, so it opens left of the parent.
    assert_eq!(submenu, Point::new(500.0, 140.0));
    // Placing the parent again ignores the submenu: it never moves.
    assert_eq!(Anchor::point(click).place(MENU, WINDOW), parent);
    // With room, the submenu opens to the right, top-aligned with its row.
    let row = Rectangle::new(Point::new(100.0, 140.0), Size::new(MENU.width, 20.0));
    assert_eq!(
        Anchor::beside(row).place(MENU, WINDOW),
        Point::new(300.0, 140.0)
    );
    // Near the bottom, it ends at its row's bottom edge.
    let row = Rectangle::new(Point::new(100.0, 600.0), Size::new(MENU.width, 20.0));
    assert_eq!(
        Anchor::beside(row).place(MENU, WINDOW),
        Point::new(300.0, 320.0)
    );
}

#[test]
fn a_corner_anchored_menu_opens_up_and_left_of_its_corner() {
    let at = Anchor::corner(34.0, 30.0).place(MENU, WINDOW);
    assert_eq!(at, Point::new(1000.0 - 34.0 - 200.0, 700.0 - 30.0 - 300.0));
}

#[test]
fn every_list_shares_one_ink_and_dim_ink() {
    assert_eq!(ink(&LIVE_DARK, false), LIVE_DARK.control_foreground);
    assert_eq!(ink(&LIVE_DARK, true), LIVE_DARK.text_disabled);
    let style = item_style(LIVE_DARK, false, false);
    let idle = style(&iced::Theme::Dark, button::Status::Active);
    assert_eq!(idle.text_color, LIVE_DARK.control_foreground);
    assert_eq!(idle.background, None);
    let hovered = style(&iced::Theme::Dark, button::Status::Hovered);
    assert_eq!(hovered.background, Some(LIVE_DARK.surface_highlight.into()));
    let disabled = style(&iced::Theme::Dark, button::Status::Disabled);
    assert_eq!(disabled.text_color, LIVE_DARK.text_disabled);
    assert_eq!(disabled.background, None);
    // A dim row that takes presses keeps the dim ink, and still shows its highlight.
    let dim = item_style(LIVE_DARK, true, true)(&iced::Theme::Dark, button::Status::Active);
    assert_eq!(dim.text_color, LIVE_DARK.text_disabled);
    assert_eq!(dim.background, Some(LIVE_DARK.surface_highlight.into()));
}

#[test]
fn heights_count_rows_separators_and_the_frame() {
    assert_eq!(
        content_height(3, 1),
        3.0 * MENU_ROW_HEIGHT + MENU_SEPARATOR_HEIGHT
    );
    assert_eq!(panel_height(60.0), 62.0);
}

#[test]
fn check_glyphs() {
    let glyphs: Vec<_> = [Check::None, Check::Tick, Check::Radio, Check::Mixed]
        .into_iter()
        .map(Check::glyph)
        .collect();
    assert_eq!(glyphs, ["", "✓", "●", "–"]);
}

#[test]
fn an_item_takes_its_help_id_through_the_callers_hook() {
    let mut seen = None;
    let row: Element<'_, ()> = item(
        &LIVE_DARK,
        Item::new("Rename", 7_u8).shortcut("⌘R"),
        |element, help| {
            seen = Some(help);
            element
        },
    );
    drop(row);
    assert_eq!(seen, Some(7));
}
