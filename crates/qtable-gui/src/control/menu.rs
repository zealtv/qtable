//! The one popup list (`menu-kind`, 2026-11-02; audit rows W7 and K7): the row, separator,
//! framed panel, item style, ink and placement that every popup list builds from. The context
//! menu, the sections menu, the route checklist and the Device and Control choosers use it;
//! iced's own `pick_list` list takes its panel and ink (`widgets::picker`).
//!
//! Geometry follows Live 12's dark menus (`style::detail::MENU_*`): a check column 4 px inside
//! the frame, labels at 15 px, a right-aligned shortcut 8 px from the frame, full-width 1 px
//! separators, rows flush against the frame. Ink is `control_foreground`, dim ink
//! `text_disabled`, for every kind.
//!
//! Help stays with the caller: an item carries a help id of the caller's own type `H`, and the
//! caller passes the hook that attaches it. The module imports nothing from the Info View.
//!
//! To add an item to a menu, build an `Item` and render it with `item`; the context menu's
//! `Entry::Command` is one.
use super::Theme;
use super::kinds::Glyph;
use super::multi_select::{Pip, pip};
use super::rule::{Axis, Level, rule};
use crate::style::detail::{
    MENU_CHECK_INSET, MENU_FRAME, MENU_LABEL_INSET, MENU_LEAD_WIDTH, MENU_ROW_HEIGHT,
    MENU_SEPARATOR_HEIGHT, MENU_SHORTCUT_INSET, NOTE_TEXT_SIZE, TEXT_SIZE,
};
use iced::widget::{
    Container, Space, button, column, container, mouse_area, opaque, responsive, row, scrollable,
    stack, text,
};
use iced::{Border, Color, Element, Length, Padding, Point, Rectangle, Size};

/// What a row's check column shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Check {
    #[default]
    None,
    /// ✓, as Live marks a checked line.
    Tick,
    /// ●, the chosen entry of a one-of list.
    Radio,
    /// –, where the targets disagree.
    Mixed,
}

impl Check {
    pub(crate) fn glyph(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Tick => "✓",
            Self::Radio => "●",
            Self::Mixed => "–",
        }
    }
}

/// A mark between the check column and the label, in a column of its own width so labels line
/// up whether a row shows one or not.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Lead {
    /// No lead column at all: the label starts at `MENU_LABEL_INSET`.
    #[default]
    None,
    /// A colour pip (a route's destination).
    Swatch(Pip),
    /// A dot in this colour (a chooser's automation LED); `None` keeps the room, unlit.
    Dot(Option<Color>),
    /// A small line glyph in the row's ink (an Insert Shape row's icon).
    Glyph(Glyph),
}

/// One row of a popup list, as data, so a responsive layer can rebuild it.
#[derive(Clone, Debug)]
pub(crate) struct Item<M, H> {
    pub label: String,
    pub shortcut: &'static str,
    pub check: Check,
    pub lead: Lead,
    /// A dim note after the label, at `NOTE_TEXT_SIZE`.
    pub note: Option<&'static str>,
    /// `None` disables the row: dim ink, no hover.
    pub press: Option<M>,
    /// Dim ink on a row that still takes presses (the checklist's rows chosen through a group).
    pub dim: bool,
    /// Drawn as hovered: the keyboard highlight, or a chooser's current entry.
    pub highlighted: bool,
    pub help: H,
}

impl<M, H> Item<M, H> {
    pub(crate) fn new(label: impl Into<String>, help: H) -> Self {
        Self {
            label: label.into(),
            shortcut: "",
            check: Check::None,
            lead: Lead::None,
            note: None,
            press: None,
            dim: false,
            highlighted: false,
            help,
        }
    }

    pub(crate) fn shortcut(mut self, shortcut: &'static str) -> Self {
        self.shortcut = shortcut;
        self
    }

    pub(crate) fn check(mut self, check: Check) -> Self {
        self.check = check;
        self
    }

    pub(crate) fn lead(mut self, lead: Lead) -> Self {
        self.lead = lead;
        self
    }

    pub(crate) fn note(mut self, note: Option<&'static str>) -> Self {
        self.note = note;
        self
    }

    pub(crate) fn on_press_maybe(mut self, press: Option<M>) -> Self {
        self.press = press;
        self
    }

    pub(crate) fn dim(mut self, dim: bool) -> Self {
        self.dim = dim;
        self
    }

    pub(crate) fn highlighted(mut self, highlighted: bool) -> Self {
        self.highlighted = highlighted;
        self
    }
}

/// Menu ink: `control_foreground`, or `text_disabled` when dim (K7: one ink for every list).
pub(crate) fn ink(theme: &Theme, dim: bool) -> Color {
    if dim {
        theme.text_disabled
    } else {
        theme.control_foreground
    }
}

/// A row's button style: the hover (or highlight) surface and the menu ink.
pub(crate) fn item_style(
    theme: Theme,
    highlighted: bool,
    dim: bool,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let disabled = status == button::Status::Disabled;
        let lit = !disabled
            && (highlighted || matches!(status, button::Status::Hovered | button::Status::Pressed));
        button::Style {
            background: lit.then_some(theme.surface_highlight.into()),
            text_color: ink(&theme, dim || disabled),
            border: Border::default(),
            ..button::Style::default()
        }
    }
}

/// One row: check column, optional lead, label, optional note, then the shortcut at the right.
/// `help` attaches the item's help id (the caller's Info View hook).
pub(crate) fn item<'a, M: Clone + 'a, H>(
    theme: &Theme,
    item: Item<M, H>,
    help: impl FnOnce(Element<'a, M>, H) -> Element<'a, M>,
) -> Element<'a, M> {
    let Item {
        label,
        shortcut,
        check,
        lead,
        note,
        press,
        dim,
        highlighted,
        help: id,
    } = item;
    let dim_ink = theme.text_disabled;
    let mut content = row![
        text(check.glyph())
            .size(TEXT_SIZE)
            .width(MENU_LABEL_INSET - MENU_CHECK_INSET)
    ]
    .align_y(iced::Alignment::Center);
    let lead: Option<Element<'a, M>> = match lead {
        Lead::None => None,
        Lead::Glyph(shape) => Some(glyph_view(shape, ink(theme, dim || press.is_none()))),
        Lead::Swatch(swatch) => Some(pip(swatch)),
        Lead::Dot(colour) => Some(
            text(if colour.is_some() { "●" } else { "" })
                .size(TEXT_SIZE)
                .color(colour.unwrap_or(Color::TRANSPARENT))
                .into(),
        ),
    };
    if let Some(lead) = lead {
        content = content.push(
            container(lead)
                .width(MENU_LEAD_WIDTH)
                .align_x(iced::alignment::Horizontal::Left),
        );
    }
    content = content.push(text(label).size(TEXT_SIZE));
    if let Some(note) = note {
        content = content
            .push(Space::new().width(MENU_CHECK_INSET))
            .push(text(note).size(NOTE_TEXT_SIZE).color(dim_ink));
    }
    content = content
        .push(Space::new().width(Length::Fill))
        .push(text(shortcut).size(TEXT_SIZE));
    let button = button(content)
        .width(Length::Fill)
        .height(MENU_ROW_HEIGHT)
        .padding(Padding {
            left: MENU_CHECK_INSET,
            right: MENU_SHORTCUT_INSET,
            ..Padding::ZERO
        })
        .style(item_style(*theme, highlighted, dim))
        .on_press_maybe(press);
    help(button.into(), id)
}

/// A glyph at its own size, in `ink`.
fn glyph_view<'a, M: 'a>(glyph: Glyph, ink: Color) -> Element<'a, M> {
    struct Drawn(Glyph, Color);
    impl<M> iced::widget::canvas::Program<M> for Drawn {
        type State = ();
        fn draw(
            &self,
            _: &(),
            renderer: &iced::Renderer,
            _: &iced::Theme,
            bounds: Rectangle,
            _: iced::mouse::Cursor,
        ) -> Vec<iced::widget::canvas::Geometry> {
            let mut frame = iced::widget::canvas::Frame::new(renderer, bounds.size());
            self.0.paint(
                &mut frame,
                Rectangle::new(Point::ORIGIN, bounds.size()),
                self.1,
            );
            vec![frame.into_geometry()]
        }
    }
    iced::widget::canvas(Drawn(glyph, ink))
        .width(Length::Fixed(Glyph::SIZE))
        .height(Length::Fixed(Glyph::SIZE))
        .into()
}

/// A label row inside a menu, in the menu's ink: it names the rows beneath it and takes no
/// press and no hover. It keeps the check column's room so labels line up.
pub(crate) fn heading<'a, M: 'a>(theme: &Theme, label: &'a str) -> Element<'a, M> {
    container(text(label).size(TEXT_SIZE))
        .height(MENU_ROW_HEIGHT)
        .width(Length::Fill)
        .align_y(iced::alignment::Vertical::Center)
        .padding(Padding {
            left: MENU_LABEL_INSET,
            ..Padding::ZERO
        })
        .style({
            let ink = ink(theme, false);
            move |_| container::Style {
                text_color: Some(ink),
                ..Default::default()
            }
        })
        .into()
}

/// A 1 px `Light` rule (`control::rule`) across the whole width, with equal room above and below.
pub(crate) fn separator<'a, M: 'a>(theme: &Theme) -> Element<'a, M> {
    let room = (MENU_SEPARATOR_HEIGHT - 1.0) / 2.0;
    container(rule(theme, Axis::Horizontal, Level::Light))
        .padding(Padding {
            top: room,
            bottom: room,
            ..Padding::ZERO
        })
        .height(MENU_SEPARATOR_HEIGHT)
        .into()
}

/// The framed panel behind every popup list.
pub(crate) fn panel_style(theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(theme.control_foreground),
        background: Some(theme.control_background.into()),
        border: Border {
            color: theme.surface_highlight,
            width: MENU_FRAME,
            radius: theme.corner_cell.into(),
        },
        ..Default::default()
    }
}

/// `content` in the framed panel, `width` wide including the frame.
pub(crate) fn panel<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    width: f32,
    theme: &Theme,
) -> Container<'a, M> {
    let theme = *theme;
    container(content)
        .padding(MENU_FRAME)
        .width(Length::Fixed(width))
        .style(move |_| panel_style(&theme))
}

/// A list's content height: its rows and separators, without the frame.
pub(crate) fn content_height(rows: usize, separators: usize) -> f32 {
    rows as f32 * MENU_ROW_HEIGHT + separators as f32 * MENU_SEPARATOR_HEIGHT
}

/// A list's content height when it shows at most `visible` rows before it scrolls: `entries`
/// says, in order, whether each is a row (`true`) or a separator. Entries after the `visible`th row
/// don't count. Never shorter than one row.
pub(crate) fn capped_height(entries: impl IntoIterator<Item = bool>, visible: usize) -> f32 {
    let (mut rows, mut separators) = (0, 0);
    for is_row in entries {
        if rows >= visible {
            break;
        }
        if is_row {
            rows += 1;
        } else {
            separators += 1;
        }
    }
    if rows + separators == 0 {
        return content_height(1, 0);
    }
    content_height(rows, separators)
}

/// A scrolling list: `lines` in a column, `height` tall (`capped_height`), the rest reached by
/// scrolling.
pub(crate) fn scrolling<'a, M: 'a>(
    lines: impl IntoIterator<Item = Element<'a, M>>,
    height: f32,
) -> Element<'a, M> {
    scrollable(column(lines))
        .height(Length::Fixed(height))
        .into()
}

/// A panel's height around `content` px of rows.
pub(crate) fn panel_height(content: f32) -> f32 {
    content + 2.0 * MENU_FRAME
}

/// Where a popup list opens from, for the placement rule (2026-11-02): a menu opens on the
/// side of its anchor with room, as macOS and Live do. A list opens below and to the right of a
/// click or face, and flips above or to the left when the window lacks room. A submenu opens
/// beside its parent row, right or else left, and its parent never moves: each list is placed
/// from its own anchor only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Anchor {
    at: At,
    beside: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum At {
    Rect(Rectangle),
    /// A point inset from the window's bottom right corner, known only per window size.
    Corner {
        right: f32,
        bottom: f32,
    },
}

impl Anchor {
    /// A click: the list's corner sits at the point.
    pub(crate) fn point(at: Point) -> Self {
        Self::below(Rectangle::new(at, Size::ZERO), 0.0)
    }

    /// A face (a dropdown): below it, `gap` px clear, or above it.
    pub(crate) fn below(face: Rectangle, gap: f32) -> Self {
        Self {
            at: At::Rect(Rectangle {
                y: face.y - gap,
                height: face.height + 2.0 * gap,
                ..face
            }),
            beside: false,
        }
    }

    /// A parent row: a submenu beside it, to the right or else the left.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn beside(row: Rectangle) -> Self {
        Self {
            at: At::Rect(row),
            beside: true,
        }
    }

    /// A point `right` and `bottom` px in from the window's bottom right corner (a status bar
    /// icon): the list opens up and to its left.
    pub(crate) fn corner(right: f32, bottom: f32) -> Self {
        Self {
            at: At::Corner { right, bottom },
            beside: false,
        }
    }

    /// The top left corner of a `menu`-sized list inside a `window`-sized area.
    pub(crate) fn place(self, menu: Size, window: Size) -> Point {
        let r = match self.at {
            At::Rect(rect) => rect,
            At::Corner { right, bottom } => Rectangle::new(
                Point::new(window.width - right, window.height - bottom),
                Size::ZERO,
            ),
        };
        let (left, right, top, bottom) = (r.x, r.x + r.width, r.y, r.y + r.height);
        if self.beside {
            Point::new(
                side(right, left, menu.width, window.width),
                side(top, bottom, menu.height, window.height),
            )
        } else {
            Point::new(
                side(left, right, menu.width, window.width),
                side(bottom, top, menu.height, window.height),
            )
        }
    }
}

/// One axis: start at `after` when `len` fits before `limit`, else end at `before` when it fits
/// above zero, else the side with more room, kept inside the window.
fn side(after: f32, before: f32, len: f32, limit: f32) -> f32 {
    if after + len <= limit {
        after
    } else if before - len >= 0.0 {
        before - len
    } else if limit - after >= before {
        (limit - len).max(0.0)
    } else {
        0.0
    }
}

/// The layer for one open list over the whole window: a click anywhere outside closes it
/// (`close`), and `build` returns the panel and its size, placed by `anchor`.
pub(crate) fn layer<'a, M: Clone + 'a>(
    anchor: Anchor,
    close: M,
    build: impl Fn() -> (Element<'a, M>, Size) + 'a,
) -> Element<'a, M> {
    responsive(move |window| {
        let (panel, size) = build();
        let at = anchor.place(size, window);
        let catcher = mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
            .on_press(close.clone())
            .on_right_press(close.clone());
        stack![
            catcher,
            container(opaque(panel)).padding(Padding {
                top: at.y,
                left: at.x,
                ..Padding::ZERO
            })
        ]
        .into()
    })
    .into()
}

#[cfg(test)]
mod tests;
