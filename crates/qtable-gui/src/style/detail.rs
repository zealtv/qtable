//! Detail View vocabulary derived on 2026-10-11.
//! These constructors leave edits and collapse state with the caller.
use super::{HIGHLIGHT, TEXT};
use crate::control::widgets::{self as control_widgets, Looks};
use crate::control::{self, LIVE_DARK, State};
use iced::widget::{Button, Container, PickList, Row, button, container, row, text};
use iced::{Border, Color, Element, Font, Length, alignment};

// Not used yet: the clip view takes the interactive chance control later.
#[allow(dead_code)]
pub(crate) mod chance;

/// Logical pixels chosen for qtable, rather than measurements of Live's font.
pub const TEXT_SIZE: u32 = 11;
pub const HEADER_SIZE: u32 = 12;
/// The one compact size: Session strip labels and values, the Mixer Solo and Target face compact
/// fields (mixer-live-layout, 2026-10-12). Popup lists use `TEXT_SIZE` (menu-kind, 2026-11-02).
pub const COMPACT_TEXT_SIZE: u32 = 10;
/// A device section heading's dim text, drawn in its `DEVICE_HEADING_ROW`.
pub const HEADING_TEXT_SIZE: u32 = 10;
/// A dim note after a popup list row's label (the route checklist's "(off)").
pub const NOTE_TEXT_SIZE: u32 = 9;
pub const CONTROL_HEIGHT: f32 = 20.0;
pub const HEADER_HEIGHT: f32 = 22.0;
/// One tab of a view switch (the clip editor's Events | Envelopes | Overview): equal widths that
/// fit "Envelopes" with Live's side margins.
pub const VIEW_TAB_WIDTH: f32 = 68.0;
/// The Envelopes tab's value scale: the only room left of its lane, as Live's Envelopes editor
/// keeps only its +24 … -24 scale there. Fits
/// "100.00" or a short level name at `TEXT_SIZE`.
pub const VALUE_SCALE_WIDTH: f32 = 52.0;
/// Space between a value-scale label and the lane's left rule.
pub const VALUE_SCALE_INSET: f32 = 6.0;
/// Clip timeline insert strip, immediately below the loop brace.
pub const INSERT_STRIP_TOP: f32 = 29.0;
/// Clip ruler marker rows: Start/End flags above the independent loop brace.
pub const CLIP_FLAG_TOP: f32 = 10.0;
pub const CLIP_FLAG_BOTTOM: f32 = 19.0;
pub const CLIP_FLAG_WIDTH: f32 = 6.0;
pub const CLIP_BRACE_TOP: f32 = 20.0;
pub const CLIP_MARKER_HIT_SLOP: f32 = 7.0;
pub const CLIP_MARKER_DRAG_THRESHOLD: f32 = 3.0;
pub const INSERT_MARKER_HALF: f32 = 3.0;
pub const INSERT_MARKER_INSET: f32 = 2.0;
pub const DEVICE_TITLE_HEIGHT: f32 = 20.0;
/// A device title strip's side padding, and the space between its Activator, name and disclosure.
pub const DEVICE_TITLE_INSET: f32 = 6.0;
pub const DEVICE_TITLE_SPACING: f32 = 6.0;
/// The round device Activator: its radius is half this size, not a corner token.
pub const DEVICE_ACTIVATOR_SIZE: f32 = 12.0;
/// The device title's fold button: a square as tall as the title strip.
pub const DEVICE_DISCLOSURE_SIZE: f32 = DEVICE_TITLE_HEIGHT;
pub const DEVICE_BODY_HEIGHT: f32 = 169.0;
pub const CHAIN_INSET: f32 = 4.0;
/// Space between neighbouring devices (and the Mixer), where the chain background shows through.
/// A later meter or activity strip can fill it.
pub const CHAIN_GAP: f32 = 8.0;
/// Darker than the device bodies, as Live's chain is behind its devices. Live 12 (sampled
/// 2026-10-15) draws its gaps at rgb 47 behind rgb 67 bodies; the same ratio on our rgb 62.
pub const CHAIN_BACKGROUND: Color = LIVE_DARK.chain_background;
pub const DETAIL_INSET: f32 = 6.0;
/// Device frame, chain insets, track title and outer Detail View insets.
pub const DEVICE_DETAIL_HEIGHT: f32 = DEVICE_TITLE_HEIGHT
    + DEVICE_BODY_HEIGHT
    + 2.0 * CHAIN_INSET
    + SCROLLBAR_GUTTER
    + HEADER_HEIGHT
    + 2.0 * DETAIL_INSET;
pub const LABEL_GAP: f32 = 2.0;
pub const CONTROL_GAP: f32 = 4.0;
pub const DEPENDENCY_RULE_WIDTH: f32 = 1.0;
pub const DEPENDENCY_DIM_ALPHA: f32 = 0.35;
/// Live insertion line: two logical pixels (four native pixels on Retina).
pub const TRACK_DROP_LINE: f32 = 2.0;
pub const INTO_MARKER_ALPHA: f32 = 0.5;
/// Dimmed route pips, checklist rows and timeline markers.
pub const DIM_ALPHA: f32 = 0.45;
/// Live's `NonEditableAutomationAlpha` (127 of 255): other envelopes behind the one being edited.
pub const NON_EDITABLE_ALPHA: f32 = 127.0 / 255.0;
/// Device cells and their compact fields.
pub const DEVICE_CELL_WIDTH: f32 = 118.0;
pub const DEVICE_CELL_EDGE: f32 = 1.0;
pub const DEVICE_CELL_STRIP: f32 = 18.0;
pub const DEVICE_LABEL_ROW: f32 = 14.0;
pub const DEVICE_ROW_GAP: f32 = 2.0;
pub const DEVICE_CELL_PADDING: f32 = CONTROL_GAP - DEVICE_CELL_EDGE;
pub const DEVICE_LABELLED_ROW: f32 = DEVICE_LABEL_ROW + LABEL_GAP + CONTROL_HEIGHT;
pub const DEVICE_CONTROL_WIDTH: f32 = DEVICE_CELL_WIDTH - 2.0 * CONTROL_GAP;
pub const DEVICE_NUMBER_WIDTH: f32 = (DEVICE_CONTROL_WIDTH - CONTROL_GAP) / 2.0;
/// Short choices keep room for their arrow and the full option text.
pub const DEVICE_CHOICE_WIDTH: f32 = 72.0;
/// A section heading above a run of cells: dim text, a 1 px rule, then a gap to the cells below.
pub const DEVICE_HEADING_ROW: f32 = 12.0;
pub const DEVICE_HEADING_GAP: f32 = 3.0;
pub const DEVICE_HEADING: f32 = DEVICE_HEADING_ROW + DEVICE_CELL_EDGE + DEVICE_HEADING_GAP;
/// A device body's edge to its outermost cell, after Live's CC Control. The Target face and
/// the Mixer device share it.
pub const DEVICE_BODY_INSET: f32 = CONTROL_GAP;
/// A scrolling column of cells: its 2 px scrollbar and 2 px spacing, beside the cells.
pub const DEVICE_SCROLL_GUTTER: f32 = 4.0;
/// A Session strip band's inset: the Path, Mixer, Output and Throughput bands.
pub const STRIP_INSET: f32 = 4.0;
/// One row of a popup list: the context and sections menus, the choosers and the route checklist.
pub const MENU_ROW_HEIGHT: f32 = 20.0;
/// Popup list geometry, measured on Live 12's dark menus (the envelope-point menu and
/// the sections menu,
/// sampled 2026-11-02 by `menu-kind`): the check column starts 4 px inside the frame, labels
/// at 15 px, and shortcuts end 8 px before it. A separator is a 1 px rule across the whole
/// width with 4 px above and below. Rows sit flush against the 1 px frame.
pub const MENU_CHECK_INSET: f32 = 4.0;
pub const MENU_LABEL_INSET: f32 = 15.0;
pub const MENU_SHORTCUT_INSET: f32 = 8.0;
pub const MENU_SEPARATOR_HEIGHT: f32 = 9.0;
pub const MENU_FRAME: f32 = 1.0;
/// Room for a mark and its gap before a row's label (a route pip, a chooser's LED), so labels line up
/// whether or not a row shows one. qtable choice.
pub const MENU_LEAD_WIDTH: f32 = 14.0;
/// One row of the Target editor's inline fields (`Controls::inline`).
pub const INLINE_ROW_HEIGHT: f32 = 22.0;
/// The stroke around a region (`focus::border`). iced draws a container's border over its own
/// bounds and lays its content out against them, so a region whose content paints its own
/// background (the mapping list's title strips) pads by this much to stay inside the stroke.
pub const REGION_FRAME: f32 = 1.0;
/// The mapping list in Map Mode: Live's identifying columns Ch, Note/Control, Path and Name
/// (the v3.10 design, 2026-11-15). Path and Name share what the fixed columns leave equally.
pub const MAPPING_CHANNEL_WIDTH: f32 = 22.0;
pub const MAPPING_SOURCE_WIDTH: f32 = 64.0;
/// Least room each of Path and Name keeps. Below it, the rows scroll sideways inside the list
/// instead of widening it.
pub const MAPPING_TEXT_MIN_WIDTH: f32 = 48.0;
/// The list's title strip ("MIDI Mappings") and its column titles, each one row this tall.
pub const MAPPING_STRIP_HEIGHT: f32 = 20.0;
/// Rows the list keeps above the inspector in a short window; the inspector scrolls instead.
pub const MAPPING_LIST_MIN_ROWS: f32 = 3.0;
/// The mapping inspector, pinned under the list: tall enough for its tallest case (an absolute
/// CC on a number, with a two-line Path), so selecting another row never moves the list.
pub const MAPPING_INSPECTOR_HEIGHT: f32 = 204.0;
/// The mapping inspector's padding: the space between its edge and its title strip's text, and
/// between its edge and its settings. Up from 4 px after the v3.10 tour ("a little bit" more
/// room); the panel's width stays `MAPPING_INSPECTOR_MIN_WIDTH`, so the settings get 2 x this less.
pub const MAPPING_INSPECTOR_INSET: f32 = 8.0;
/// The Browser's width before anyone drags its divider. Map Mode's list and inspector are sized
/// against it (`MAPPING_INSPECTOR_MIN_WIDTH`), so they fit at the default without scrolling.
pub const DEFAULT_BROWSER_WIDTH: f32 = 220.0;
/// The inspector's least content width: the default Browser inside its `REGION_FRAME` on both
/// sides. A narrower Browser scrolls the inspector sideways instead of squeezing its columns.
pub const MAPPING_INSPECTOR_MIN_WIDTH: f32 = DEFAULT_BROWSER_WIDTH - 2.0 * REGION_FRAME;
/// One Output Detail settings column, so fields stay a readable size in a wide Detail View.
pub const OUTPUT_COLUMN_WIDTH: f32 = 220.0;
/// The Info View beside the Detail View. A separate measure from the Output column, the
/// Browser's default width and `control::NAME_MAX_WIDTH`, which are also 220 today.
pub const INFO_VIEW_WIDTH: f32 = 220.0;
pub const PANEL_INSET: u16 = 8;
/// Thin scrollbar of the device chain's scrolling sections.
pub const SCROLLBAR_WIDTH: f32 = 4.0;
/// Space between a scrolling section's content and its scrollbar.
pub const SCROLLBAR_SPACING: f32 = 3.0;
/// Room a scrollbar takes from its section, so no control sits under it.
pub const SCROLLBAR_GUTTER: f32 = SCROLLBAR_WIDTH + SCROLLBAR_SPACING;
pub const DETAIL_BACKGROUND: Color = LIVE_DARK.detail_view_background;

/// Colour title strip; pass the editable name widget as `content` when renaming.
pub fn title_bar<'a, M: 'a>(colour: Color, content: impl Into<Element<'a, M>>) -> Container<'a, M> {
    container(content)
        .width(Length::Fill)
        .height(HEADER_HEIGHT)
        .padding([2, PANEL_INSET])
        .align_y(alignment::Vertical::Center)
        .style(move |_| container::Style {
            background: Some(colour.into()),
            text_color: Some(crate::control::theme::ink(colour)),
            ..Default::default()
        })
}

/// Shared typography for title and section names.
pub fn heading<'a>(label: impl text::IntoFragment<'a>) -> iced::widget::Text<'a> {
    text(label).size(HEADER_SIZE).font(Font {
        weight: iced::font::Weight::Semibold,
        ..Font::DEFAULT
    })
}

/// Full-width section affordance. `icon` is a caller-supplied section symbol,
/// never the disclosure triangle. Use None until a faithful icon is available.
// Collapse messages and state arrive with the clip view that depends on this.
#[allow(dead_code)]
pub fn section_header<'a, M: Clone + 'a>(
    label: impl text::IntoFragment<'a>,
    icon: Option<Element<'a, M>>,
    expanded: bool,
    on_toggle: M,
) -> Button<'a, M> {
    let mut content = Row::new()
        .spacing(CONTROL_GAP)
        .align_y(alignment::Vertical::Center);
    if let Some(icon) = icon {
        content = content.push(icon);
    }
    content = content
        .push(heading(label).width(Length::Fill))
        .push(text(if expanded { "▼" } else { "◀" }).size(TEXT_SIZE));
    button(content)
        .width(Length::Fill)
        .height(HEADER_HEIGHT)
        .padding([2, PANEL_INSET])
        .on_press(on_toggle)
        .style(|_, status| button::Style {
            background: Some(
                match status {
                    button::Status::Hovered | button::Status::Pressed => DETAIL_BACKGROUND,
                    _ => HIGHLIGHT,
                }
                .into(),
            ),
            text_color: TEXT,
            border: Border::default(),
            ..Default::default()
        })
}

/// Scrollbar that reserves its gutter instead of drawing over the content.
pub fn scrollbar() -> iced::widget::scrollable::Scrollbar {
    overlay_scrollbar().spacing(SCROLLBAR_SPACING)
}

/// Thin floating bar for the Session grid. Leaving spacing unset preserves its
/// overlay layout; even spacing(0) would reserve room in iced.
pub fn overlay_scrollbar() -> iced::widget::scrollable::Scrollbar {
    iced::widget::scrollable::Scrollbar::new()
        .width(SCROLLBAR_WIDTH)
        .scroller_width(SCROLLBAR_WIDTH)
}

/// Narrow scrollbar for compact device-face columns; keeps its own gutter.
pub fn compact_scrollbar() -> iced::widget::scrollable::Scrollbar {
    iced::widget::scrollable::Scrollbar::new()
        .width(2)
        .scroller_width(2)
        .spacing(2)
}

/// Section body: shared inset and neutral Detail View surface.
pub fn panel<'a, M: 'a>(content: impl Into<Element<'a, M>>) -> Container<'a, M> {
    container(content)
        .padding(PANEL_INSET)
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(DETAIL_BACKGROUND.into()),
            text_color: Some(TEXT),
            ..Default::default()
        })
}

/// Equal-width columns for Position/Length, Action A/B and other peer controls.
pub fn pair<'a, M: 'a>(
    left: impl Into<Element<'a, M>>,
    right: impl Into<Element<'a, M>>,
) -> Row<'a, M> {
    row![
        container(left).width(Length::Fill),
        container(right).width(Length::Fill)
    ]
    .spacing(CONTROL_GAP)
    .align_y(alignment::Vertical::Top)
}

/// Latched rectangular toggle. Disabled stays subdued even if its saved value is on.
pub fn toggle<'a, M: Clone + 'a>(label: impl text::IntoFragment<'a>, on: bool) -> Button<'a, M> {
    let kind = control::Toggle {
        label: String::new(),
    };
    let state = State {
        lit: if on {
            control::Lit::On
        } else {
            control::Lit::Off
        },
        ..State::default()
    };
    button(control_widgets::toggle_label(label, false))
        .height(CONTROL_HEIGHT)
        .padding([2, 4])
        .style(control_widgets::toggle_style(Looks::of(
            &kind, &LIVE_DARK, state,
        )))
}

/// Numeric field shares the existing draft/focus/error handling and aligns values right.
// Bars.beats.sixteenths fields arrive with clip-view; parsing remains outside style.
#[allow(dead_code)]
pub fn numeric<'a, M: Clone + 'a>(
    placeholder: &str,
    value: &str,
) -> crate::control::text_field::TextField<'a, M> {
    super::input(placeholder, value).align_x(alignment::Horizontal::Right)
}

/// Boxed ▼ picker and menu, with matching field typography and padding.
pub fn dropdown<'a, T, L, M>(
    options: L,
    selected: Option<T>,
    on_select: impl Fn(T) -> M + 'a,
) -> PickList<'a, T, L, T, M>
where
    T: ToString + PartialEq + Clone + 'a,
    L: std::borrow::Borrow<[T]> + 'a,
    M: Clone + 'a,
{
    let looks = Looks::of(&control::Dropdown, &LIVE_DARK, State::default());
    control_widgets::picker(looks, options, selected, on_select)
}

/// Vertical logical pixels per musical segment step, matching the former duration handle.
pub(crate) const TIME_DRAG_STEP: f32 = 6.0;

/// Number field drag: vertical logical pixels per keyboard-sized step (Shift makes the step
/// four times finer), and how far the pointer travels before a press counts as a drag, so a
/// click that wobbles still selects the value for typing. Chosen by feel, not measured in Live.
pub(crate) const NUMBER_DRAG_STEP: f32 = 2.0;
pub(crate) const NUMBER_DRAG_DEAD_ZONE: f32 = 3.0;

/// Activity indicator and Settings pane geometry in logical pixels.
pub(crate) const ACTIVITY_DOT_SIZE: f32 = 8.0;
pub(crate) const SETTINGS_PANE_WIDTH: f32 = 460.0;
pub(crate) const SETTINGS_PANE_HEIGHT: f32 = 320.0;

/// Live nested Session headers step once per visible ancestor (logical units).
pub const GROUP_HEADER_STEP: f32 = 6.0;
pub const FOLD_RADIUS: f32 = 6.5;
pub const FOLD_STROKE: f32 = 1.0;

#[cfg(test)]
mod tests {
    use super::*;

    /// The 2026-11-02 size tokens named these without changing a pixel; a new value here is
    /// a look change, not a refactor.
    #[test]
    fn named_sizes_keep_the_values_they_replaced() {
        assert_eq!((TEXT_SIZE, HEADER_SIZE), (11, 12));
        assert_eq!(
            (COMPACT_TEXT_SIZE, HEADING_TEXT_SIZE, NOTE_TEXT_SIZE),
            (10, 10, 9)
        );
        assert_eq!((STRIP_INSET, DEVICE_BODY_INSET), (4.0, 4.0));
        assert_eq!((MENU_ROW_HEIGHT, INLINE_ROW_HEIGHT), (20.0, 22.0));
        assert_eq!(
            (
                MENU_CHECK_INSET,
                MENU_LABEL_INSET,
                MENU_SHORTCUT_INSET,
                MENU_SEPARATOR_HEIGHT,
                MENU_FRAME,
                MENU_LEAD_WIDTH
            ),
            (4.0, 15.0, 8.0, 9.0, 1.0, 14.0)
        );
        assert_eq!((DEVICE_TITLE_INSET, DEVICE_TITLE_SPACING), (6.0, 6.0));
        assert_eq!(
            (DEVICE_ACTIVATOR_SIZE, DEVICE_DISCLOSURE_SIZE),
            (12.0, 20.0)
        );
        assert_eq!((OUTPUT_COLUMN_WIDTH, INFO_VIEW_WIDTH), (220.0, 220.0));
    }
}
