//! The multi-select dropdown: a closed face of coloured pips plus a
//! name, and a checklist of rows split by dividers that opens through the app's overlay slot,
//! not inline. Route controls use it now; inputs will, unchanged.
use super::menu::{self, Check, Lead};
use super::{ControlKind, Interaction, SizeRole, State, Theme};
use crate::style::detail::DIM_ALPHA;
use iced::widget::{Space, container};
use iced::{Border, Color, Element};

/// Rows shown before the list scrolls.
pub(crate) const VISIBLE_ROWS: usize = 10;
/// Pips drawn on the closed face before the text.
pub(crate) const MAX_PIPS: usize = 4;

/// One destination's pip on the closed face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pip {
    pub colour: Color,
    pub dim: bool,
}

/// The same pip in faces and lists; a light outline keeps dark colours (Main) visible.
pub(crate) fn pip<'a, M: 'a>(pip: Pip) -> Element<'a, M> {
    let alpha = if pip.dim { DIM_ALPHA } else { 1.0 };
    container(Space::new().width(6).height(6))
        .style(move |_| container::Style {
            background: Some(
                Color {
                    a: alpha,
                    ..pip.colour
                }
                .into(),
            ),
            border: Border {
                color: Color {
                    a: alpha,
                    ..Color::from_rgb(0.65, 0.65, 0.65)
                },
                width: 1.0,
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// Live's automation LED before a face's text, as on the clip editor's Device and Control
/// choosers. A face with an LED keeps its room while it is unlit, so the name never moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FaceLed {
    /// No LED and no room for one (the route faces).
    #[default]
    None,
    Unlit,
    Lit(super::automation_mark::Mark),
}

impl FaceLed {
    /// The LED to draw before the text: `None` for no room, `Some(None)` for an unlit LED.
    pub(crate) fn slot(self) -> Option<Option<super::automation_mark::Mark>> {
        match self {
            Self::None => None,
            Self::Unlit => Some(None),
            Self::Lit(mark) => Some(Some(mark)),
        }
    }
}

/// The closed face: an optional LED, pips, then the first name with "+N" (or "No Output",
/// "Group", "—").
pub(crate) struct MultiFace {
    pub led: FaceLed,
    pub pips: Vec<Pip>,
    pub text: String,
    pub text_colour: Option<Color>,
}

impl ControlKind for MultiFace {
    fn look(&self, theme: &Theme, state: State, interaction: Interaction) -> super::Look {
        super::Dropdown.look(theme, state, interaction)
    }

    fn role(&self) -> SizeRole {
        SizeRole::Fill
    }
}

/// How one row reads in the checklist.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    Ticked,
    Off,
    /// Targets disagree.
    Mixed,
    /// Chosen through a parent; greyed, ticked when the parent sends there.
    Via {
        sends: bool,
    },
}

pub(crate) struct MultiItem {
    pub label: String,
    pub colour: Color,
    pub mark: Mark,
    pub enabled: bool,
    /// A radio entry shows ● when ticked instead of ✓.
    pub radio: bool,
    pub note: Option<&'static str>,
}

pub(crate) enum Row {
    Divider,
    Item(MultiItem),
}

/// What a row's check column shows.
pub(crate) fn check(item: &MultiItem) -> Check {
    match item.mark {
        Mark::Ticked | Mark::Via { sends: true } => {
            if item.radio {
                Check::Radio
            } else {
                Check::Tick
            }
        }
        Mark::Mixed => Check::Mixed,
        Mark::Off | Mark::Via { sends: false } => Check::None,
    }
}

/// The list's content height: menu rows and separators (`menu::content_height`), with at most
/// `VISIBLE_ROWS` items counted before it scrolls. Never shorter than one row.
pub(crate) fn list_height(rows: &[Row]) -> f32 {
    menu::capped_height(rows.iter().map(|r| matches!(r, Row::Item(_))), VISIBLE_ROWS)
}

/// The next item index from `from` going down or up, skipping dividers, stopping at the ends.
/// With no `from`, it lands on the first (down) or last (up) item. `None` when there are no items.
pub(crate) fn step(rows: &[Row], from: Option<usize>, down: bool) -> Option<usize> {
    let is_item = |i: &usize| matches!(rows[*i], Row::Item(_));
    match (from, down) {
        (None, true) => (0..rows.len()).find(is_item),
        (None, false) => (0..rows.len()).rev().find(is_item),
        (Some(i), true) => (i + 1..rows.len())
            .find(is_item)
            .or(Some(i).filter(|&i| i < rows.len() && is_item(&i))),
        (Some(i), false) => (0..i.min(rows.len()))
            .rev()
            .find(is_item)
            .or(Some(i).filter(|&i| i < rows.len() && is_item(&i))),
    }
}

/// The open list: a checklist of shared menu rows (`control::menu`), scrolling past
/// `VISIBLE_ROWS` items. Pressing an item sends `on_toggle(row index)`; the list stays open.
/// `highlight` is a row index from `step`. Disabled rows and rows chosen through a parent take
/// the dim ink and still take presses.
pub(crate) fn list<'a, M: Clone + 'a>(
    theme: &Theme,
    rows: &[Row],
    highlight: Option<usize>,
    on_toggle: impl Fn(usize) -> M,
) -> Element<'a, M> {
    let lines = rows.iter().enumerate().map(|(index, entry)| match entry {
        Row::Divider => menu::separator(theme),
        Row::Item(item) => {
            let dim = !item.enabled || matches!(item.mark, Mark::Via { .. });
            menu::item(
                theme,
                menu::Item::new(item.label.clone(), ())
                    .check(check(item))
                    .lead(Lead::Swatch(Pip {
                        colour: item.colour,
                        dim,
                    }))
                    .note(item.note)
                    .dim(dim)
                    .highlighted(highlight == Some(index))
                    .on_press_maybe(Some(on_toggle(index))),
                |row, ()| row,
            )
        }
    });
    menu::scrolling(lines, list_height(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> Row {
        Row::Item(MultiItem {
            label: "x".into(),
            colour: Color::WHITE,
            mark: Mark::Off,
            enabled: true,
            radio: false,
            note: None,
        })
    }

    // item, divider, item, item, divider, item
    fn rows() -> Vec<Row> {
        vec![item(), Row::Divider, item(), item(), Row::Divider, item()]
    }

    #[test]
    fn a_face_led_keeps_its_room_while_unlit() {
        use super::super::automation_mark::Mark as Automation;
        assert_eq!(FaceLed::default(), FaceLed::None);
        assert_eq!(FaceLed::None.slot(), None);
        assert_eq!(FaceLed::Unlit.slot(), Some(None));
        assert_eq!(
            FaceLed::Lit(Automation::Overridden).slot(),
            Some(Some(Automation::Overridden))
        );
    }

    #[test]
    fn step_skips_dividers_and_stops_at_the_ends() {
        let rows = rows();
        assert_eq!(step(&rows, Some(0), true), Some(2));
        assert_eq!(step(&rows, Some(3), true), Some(5));
        assert_eq!(step(&rows, Some(5), true), Some(5));
        assert_eq!(step(&rows, Some(2), false), Some(0));
        assert_eq!(step(&rows, Some(5), false), Some(3));
        assert_eq!(step(&rows, Some(0), false), Some(0));
    }

    #[test]
    fn step_without_a_start_lands_on_an_end_item() {
        let rows = vec![Row::Divider, item(), Row::Divider];
        assert_eq!(step(&rows, None, true), Some(1));
        assert_eq!(step(&rows, None, false), Some(1));
        assert_eq!(step(&[Row::Divider], None, true), None);
        assert_eq!(step(&[], None, false), None);
    }

    #[test]
    fn list_height_counts_dividers_short_and_caps_items() {
        use crate::style::detail::{MENU_ROW_HEIGHT as ROW, MENU_SEPARATOR_HEIGHT as SEP};
        assert_eq!(list_height(&[]), ROW);
        assert_eq!(list_height(&[item()]), ROW);
        // 4 items + 2 dividers, flush as in Live's menus: 4*20 + 2*9
        assert_eq!(list_height(&rows()), 4.0 * ROW + 2.0 * SEP);
        let many: Vec<Row> = (0..VISIBLE_ROWS + 5).map(|_| item()).collect();
        assert_eq!(list_height(&many), VISIBLE_ROWS as f32 * ROW);
    }

    #[test]
    fn check_follows_mark_and_radio() {
        let mut it = MultiItem {
            label: String::new(),
            colour: Color::WHITE,
            mark: Mark::Ticked,
            enabled: true,
            radio: false,
            note: None,
        };
        assert_eq!(check(&it), Check::Tick);
        it.radio = true;
        assert_eq!(check(&it), Check::Radio);
        it.mark = Mark::Mixed;
        assert_eq!(check(&it), Check::Mixed);
        it.mark = Mark::Via { sends: false };
        assert_eq!(check(&it), Check::None);
    }
}
