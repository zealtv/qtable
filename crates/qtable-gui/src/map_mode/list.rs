//! The mapping list, in the Browser's place while Map Mode is on, with the inspector pinned
//! under it. Live's identifying columns only (Ch, Note/Control, Path, Name); every setting of
//! the selected row is the inspector's (`inspector.rs`), so each has one home and the list
//! reads at the default 220 px Browser. Built from the control layer; the wording is `labels`.
use super::{Action, Mode};
use crate::app::App;
use crate::control::State as ControlState;
use crate::control::rule::{Axis, Level, rule};
use crate::info::{Help, Id};
use crate::messages::Message;
use crate::style::detail::{
    HEADER_SIZE, INLINE_ROW_HEIGHT, LABEL_GAP, MAPPING_CHANNEL_WIDTH, MAPPING_INSPECTOR_HEIGHT,
    MAPPING_LIST_MIN_ROWS, MAPPING_SOURCE_WIDTH, MAPPING_STRIP_HEIGHT, MAPPING_TEXT_MIN_WIDTH,
    REGION_FRAME, TEXT_SIZE,
};
use iced::widget::{column, container, mouse_area, responsive, row, scrollable, text};
use iced::{Alignment, Element, Length};
use qtable_engine::model::{ControlDescriptor, RemoteMapping};

use super::labels;
use crate::control::measure::{self, Face};

/// Room a clipped Path or Name keeps clear of its column's edge, so its closing `…` never
/// touches the next column.
const TEXT_MARGIN: f32 = 2.0;
/// Width the row's side padding takes (`padding([1, 4])`).
const ROW_PADDING: f32 = 8.0;
/// Rows sit 1 px apart.
const ROW_SPACING: f32 = 1.0;
/// The rows' scrollable, so the list can scroll its selection into view.
const ROWS: &str = "mapping-list-rows";

/// The widths of the list's flexible columns for one viewport: the list takes the Browser's
/// width, Path and Name share what the fixed columns leave equally, and when that is less than
/// `MAPPING_TEXT_MIN_WIDTH` each, the whole row grows past the viewport and the list scrolls
/// sideways inside itself. Key Map Mode has no channel, so its rows drop the Ch column.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Layout {
    channel: bool,
    text: f32,
}

impl Layout {
    fn for_viewport(width: f32, channel: bool) -> Self {
        let fixed = Self::fixed(channel);
        let room = ((width - fixed) / 2.0).max(MAPPING_TEXT_MIN_WIDTH);
        Self {
            channel,
            text: room,
        }
    }

    /// The fixed columns, the gaps between all columns and the row's padding.
    fn fixed(channel: bool) -> f32 {
        let (columns, fixed) = if channel {
            (4.0, MAPPING_CHANNEL_WIDTH + MAPPING_SOURCE_WIDTH)
        } else {
            (3.0, MAPPING_SOURCE_WIDTH)
        };
        fixed + (columns - 1.0) * LABEL_GAP + ROW_PADDING
    }

    /// The width of a whole row (and of the title strips).
    fn row_width(self) -> f32 {
        Self::fixed(self.channel) + 2.0 * self.text
    }
}

/// `label` clipped to `room` with a closing `…`, measured in the row's text face.
fn clipped(label: &str, room: f32) -> String {
    measure::fit_ellipsis(label, room - TEXT_MARGIN, Face::canvas(TEXT_SIZE as f32))
}

/// One row's pitch in the list.
const ROW_STEP: f32 = INLINE_ROW_HEIGHT + ROW_SPACING;

/// The rows' scroll offset that brings row `at` fully into a viewport `room` tall, or `None`
/// when it already shows.
pub(super) fn offset_to_show(at: usize, offset: f32, room: f32) -> Option<f32> {
    crate::control::viewport::reveal(at as f32 * ROW_STEP, INLINE_ROW_HEIGHT, offset, room)
}

pub(super) fn scroll_rows(offset: f32) -> iced::Task<Message> {
    iced::widget::operation::scroll_to(
        iced::widget::Id::new(ROWS),
        scrollable::AbsoluteOffset {
            x: None,
            y: Some(offset),
        },
    )
}

/// The inspector's height in a column `total` tall: its fixed height, or what is left once the
/// list keeps its title strips and `MAPPING_LIST_MIN_ROWS` rows (the inspector then scrolls).
fn inspector_height(total: f32) -> f32 {
    let list_min = 2.0 * MAPPING_STRIP_HEIGHT + MAPPING_LIST_MIN_ROWS * ROW_STEP;
    MAPPING_INSPECTOR_HEIGHT.min((total - list_min - 1.0).max(0.0))
}

pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let theme = app.theme;
    // The Browser's width, as Live's mapping browser takes it; what doesn't fit scrolls inside.
    // The frame is the outer container's border, so the content is laid out inside it
    // (`REGION_FRAME` padding, so `size` is the default width less the stroke on both sides)
    // and its strips never reach the stroke.
    container(responsive(move |size| {
        let inspector = inspector_height(size.height);
        let list_height = (size.height - inspector - 1.0).max(0.0);
        column![
            container(list(app, size.width, list_height))
                .width(Length::Fill)
                .height(list_height),
            rule(&theme, Axis::Horizontal, Level::Deep),
            super::inspector::view(app, inspector),
        ]
        .into()
    }))
    .padding(REGION_FRAME)
    .width(app.dividers.browser_width())
    .height(Length::Fill)
    .style(move |_| iced::widget::container::Style {
        background: Some(theme.surface_background.into()),
        text_color: Some(theme.control_foreground),
        border: crate::focus::border(app.focus == crate::focus::Focus::Browser),
        ..Default::default()
    })
    .into()
}

fn list(app: &App, width: f32, height: f32) -> Element<'_, Message> {
    let layout = Layout::for_viewport(width, app.map_mode.mode == Mode::Midi);
    app.map_mode
        .scroll
        .viewport
        .set((height - 2.0 * MAPPING_STRIP_HEIGHT).max(0.0));
    // Live's two titles.
    let title = match app.map_mode.mode {
        Mode::Midi => "MIDI Mappings",
        Mode::Key => "Key Mappings",
    };
    scrollable(
        column![
            container(text(title).size(HEADER_SIZE))
                .padding([2, 4])
                .height(MAPPING_STRIP_HEIGHT),
            header(app, layout),
            scrollable(rows(app, layout))
                .id(iced::widget::Id::new(ROWS))
                .on_scroll(|viewport| {
                    Message::Map(Action::ListScrolled(viewport.absolute_offset().y))
                })
                .height(Length::Fill)
                .width(Length::Fill),
        ]
        .width(layout.row_width()),
    )
    .direction(scrollable::Direction::Horizontal(
        crate::style::detail::scrollbar(),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn rows(app: &App, layout: Layout) -> iced::widget::Column<'_, Message> {
    let mut rows = column![].spacing(ROW_SPACING);
    let mut any = false;
    // A row whose control no longer resolves is removed by the edit that deleted the control,
    // so `shown` leaves nothing out that a user could still reach.
    for (mapping, descriptor) in super::shown(app) {
        any = true;
        rows = rows.push(entry(app, mapping, &descriptor, layout));
    }
    if !any {
        rows = rows.push(
            container(
                app.controls()
                    .readout(
                        if app.map_mode.mode == Mode::Key {
                            "No mappings. Click a red control, then press a key."
                        } else {
                            "No mappings. Click a blue control, then move a controller."
                        },
                        ControlState {
                            enabled: false,
                            ..Default::default()
                        },
                    )
                    .wrapping(text::Wrapping::WordOrGlyph),
            )
            .padding([2, 4]),
        );
    }
    rows
}

/// The columns, in order: the cells of a row and the titles above them share this layout.
/// `channel` is dropped in Key Map Mode.
fn columns<'a>(
    [channel, source, path, name]: [Element<'a, Message>; 4],
    layout: Layout,
) -> Element<'a, Message> {
    let fixed = |cell, width| container(cell).width(width).clip(true);
    let mut cells = row![].spacing(LABEL_GAP).align_y(Alignment::Center);
    if layout.channel {
        cells = cells.push(fixed(channel, MAPPING_CHANNEL_WIDTH));
    }
    cells
        .push(fixed(source, MAPPING_SOURCE_WIDTH))
        .push(fixed(path, layout.text))
        .push(fixed(name, layout.text))
        .into()
}

fn title<'a>(app: &App, label: &'static str, help: Id) -> Element<'a, Message> {
    app.controls()
        .readout(label, ControlState::default())
        .wrapping(text::Wrapping::None)
        .info(help, Message::Info)
}

fn header(app: &App, layout: Layout) -> Element<'_, Message> {
    let theme = app.theme;
    container(columns(
        [
            title(app, "Ch", Id::MappingSource),
            title(app, "Note/Control", Id::MappingSource),
            title(app, "Path", Id::MappingPath),
            title(app, "Name", Id::MappingName),
        ],
        layout,
    ))
    .padding([2, 4])
    .height(MAPPING_STRIP_HEIGHT)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .style(move |_| iced::widget::container::Style {
        background: Some(theme.device_title_background.into()),
        ..Default::default()
    })
    .into()
}

fn entry<'a>(
    app: &'a App,
    mapping: &'a RemoteMapping,
    descriptor: &ControlDescriptor,
    layout: Layout,
) -> Element<'a, Message> {
    let theme = app.theme;
    let id = mapping.id;
    let selected = app.map_mode.selected_row == Some(id);
    let readout = move |label: String, help: Id| -> Element<'a, Message> {
        let ink = if selected {
            theme.control_on_foreground
        } else {
            theme.control_foreground
        };
        Element::from(
            text(label)
                .size(TEXT_SIZE)
                .color(ink)
                .wrapping(text::Wrapping::None),
        )
        .info(help, Message::Info)
    };
    let cells = [
        readout(labels::channel(&mapping.source), Id::MappingSource),
        readout(labels::control_cell(&mapping.source), Id::MappingSource),
        readout(
            clipped(&labels::path(descriptor), layout.text),
            Id::MappingPath,
        ),
        readout(clipped(&descriptor.label, layout.text), Id::MappingName),
    ];
    mouse_area(
        container(columns(cells, layout))
            .padding([1, 4])
            .height(INLINE_ROW_HEIGHT)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .style(move |_| iced::widget::container::Style {
                background: selected.then_some(theme.selection_background.into()),
                ..Default::default()
            }),
    )
    .on_press(Message::Map(Action::SelectRow(id)))
    .into()
}

#[cfg(test)]
mod tests;
