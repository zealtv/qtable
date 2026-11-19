//! The four rows under the grid: Output, Path, Mixer and Throughput. Every column (track, group, output, Main) gets a cell in each open
//! row at the same height, so the rows line up; a kind that lacks a row's content draws nothing.
use super::layout::ColumnKey;
use super::{CELL_H, FOOTER_H, GAP, HEADER_H, Layout};
use crate::control::LIVE_DARK;
use crate::control::rule::{Axis, Level, rule_sized};
use crate::selection::SelectionView;
use iced::widget::{Space, column, container, row};
use iced::{Element, Length};
use qtable_engine::model::TrackId;
use std::rc::Rc;

/// The darker rule that opens each row (Live's slightly thicker band border).
const RULE_H: f32 = 2.0;

/// One row of the strip, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    /// Routing, or receiver destination.
    Output,
    /// The track's Path, the address segment it adds to its events.
    Path,
    /// Activator, solo and meter.
    Mixer,
    /// Destination load meter and messages/s.
    Throughput,
}

impl Row {
    /// Content heights are qtable choices tuned against screenshots of Live 12.
    const fn content_height(self) -> f32 {
        match self {
            Self::Output => 56.0,
            Self::Path => 40.0,
            Self::Mixer => 104.0,
            Self::Throughput => crate::throughput::STRIP_HEIGHT,
        }
    }

    /// Height of the row including its rule.
    pub(crate) const fn height(self) -> f32 {
        self.content_height() + RULE_H
    }
}

/// Total height of these rows.
pub(crate) fn rows_height(rows: &[Row]) -> f32 {
    rows.iter().map(|r| r.height()).sum()
}

/// Slot rows Live keeps while it hides strip sections (8 shrink to about 4 first), then it
/// lets them shrink further, to the least below.
pub(crate) const PREFERRED_SLOT_ROWS: usize = 4;
/// The least slot rows kept beside the header row (Live keeps two or three).
pub(crate) const LEAST_SLOT_ROWS: usize = 3;

/// The order a short window hides sections, first to last. Live hides I/O, then Sends, and never
/// the Mixer. Output and Path stand for I/O (routing and addressing) and Throughput for the
/// lower-priority readout; Mixer is not listed, so it always stays.
const HIDE_ORDER: [Row; 3] = [Row::Output, Row::Path, Row::Throughput];

/// The checked `rows` that fit a grid `height` tall: the track header row, the status row and
/// `PREFERRED_SLOT_ROWS` slot rows (fewer when the set has fewer scenes) come first, then
/// sections hide in `HIDE_ORDER` until the rest fits. A pure function of the height, so growing
/// the window brings them back in reverse; the user's checks stay as they are.
pub(crate) fn fitting(rows: &[Row], height: f32, scenes: usize, header_h: f32) -> Vec<Row> {
    let wanted = scenes.min(PREFERRED_SLOT_ROWS) as f32 * (CELL_H + GAP);
    let mut shown = rows.to_vec();
    for hidden in HIDE_ORDER {
        if header_h + wanted + FOOTER_H + rows_height(&shown) <= height {
            break;
        }
        shown.retain(|row| *row != hidden);
    }
    shown
}

/// The least a grid column needs: the header row, the status row, the Mixer and
/// `LEAST_SLOT_ROWS` slot rows.
pub(crate) const fn least_height() -> f32 {
    HEADER_H + LEAST_SLOT_ROWS as f32 * (CELL_H + GAP) + FOOTER_H + Row::Mixer.height()
}

/// Draws a band cell under one column, whatever its kind: the grid decides the geometry, the
/// caller draws the content and branches on the track's capabilities.
pub(crate) type CellBuilder<'a, M> = Rc<dyn Fn(Row, TrackId) -> Element<'a, M> + 'a>;

/// A row's content under its rule, `row.height()` tall in all.
pub(crate) fn framed<'a, M: 'a>(row: Row, content: Element<'a, M>) -> Element<'a, M> {
    column![
        rule_sized(&LIVE_DARK, Axis::Horizontal, Level::Group, RULE_H),
        container(content)
            .width(Length::Fill)
            .height(row.content_height()),
    ]
    .into()
}

fn cell<'a, M: 'a>(content: Element<'a, M>, width: f32, selected: bool) -> Element<'a, M> {
    container(content)
        .width(Length::Fixed(width))
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(
                if selected {
                    crate::style::HIGHLIGHT
                } else {
                    crate::style::detail::DETAIL_BACKGROUND
                }
                .into(),
            ),
            ..Default::default()
        })
        .into()
}

/// A band's track side: a cell under every column, group tracks included.
pub(super) fn tracks<'a, M: 'a>(
    layout: &Layout,
    selection: &SelectionView<'_>,
    build: &dyn Fn(TrackId) -> Element<'a, M>,
    row: Row,
) -> Element<'a, M> {
    let mut cells = row![Space::new().width(0)].spacing(GAP);
    for col in &layout.columns {
        let ColumnKey::Track(owner) = col.key;
        cells = cells.push(cell(
            framed(row, build(owner)),
            col.width,
            selection.column_selected(col),
        ));
    }
    cells.height(Length::Fixed(row.height())).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_total_their_heights_and_include_the_rule() {
        let all = [Row::Output, Row::Path, Row::Mixer, Row::Throughput];
        assert_eq!(
            rows_height(&all),
            all.iter().map(|r| r.height()).sum::<f32>()
        );
        assert_eq!(rows_height(&[]), 0.0);
        assert_eq!(Row::Output.height(), 58.0);
        assert_eq!(rows_height(&all), 230.0);
    }

    const ALL: [Row; 4] = [Row::Output, Row::Path, Row::Mixer, Row::Throughput];
    const SCENES: usize = 8;

    fn needs(rows: &[Row]) -> f32 {
        HEADER_H + 4.0 * (CELL_H + GAP) + FOOTER_H + rows_height(rows)
    }

    #[test]
    fn a_tall_grid_shows_every_checked_section() {
        assert_eq!(fitting(&ALL, needs(&ALL), SCENES, HEADER_H), ALL);
        assert_eq!(fitting(&ALL, 2000.0, SCENES, HEADER_H), ALL);
    }

    #[test]
    fn sections_hide_output_then_path_then_throughput_and_the_mixer_stays() {
        let no_output = [Row::Path, Row::Mixer, Row::Throughput];
        assert_eq!(
            fitting(&ALL, needs(&ALL) - 1.0, SCENES, HEADER_H),
            no_output
        );
        let no_path = [Row::Mixer, Row::Throughput];
        assert_eq!(
            fitting(&ALL, needs(&no_output) - 1.0, SCENES, HEADER_H),
            no_path
        );
        assert_eq!(
            fitting(&ALL, needs(&no_path) - 1.0, SCENES, HEADER_H),
            [Row::Mixer]
        );
        assert_eq!(fitting(&ALL, 0.0, SCENES, HEADER_H), [Row::Mixer]);
    }

    #[test]
    fn growing_brings_sections_back_in_reverse_and_checks_are_not_touched() {
        let mut heights: Vec<f32> = (0..60).map(|i| 150.0 + i as f32 * 8.0).collect();
        let up: Vec<_> = heights
            .iter()
            .map(|h| fitting(&ALL, *h, SCENES, HEADER_H).len())
            .collect();
        heights.reverse();
        let mut down: Vec<_> = heights
            .iter()
            .map(|h| fitting(&ALL, *h, SCENES, HEADER_H).len())
            .collect();
        down.reverse();
        assert_eq!(up, down);
        assert!(up.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn unchecked_sections_stay_hidden_and_few_scenes_need_fewer_rows() {
        let checked = [Row::Path, Row::Mixer];
        assert_eq!(fitting(&checked, 2000.0, SCENES, HEADER_H), checked);
        // Two scenes need two rows, so a height that fits only those keeps every section.
        let height = HEADER_H + 2.0 * (CELL_H + GAP) + FOOTER_H + rows_height(&ALL);
        assert_eq!(fitting(&ALL, height, 2, HEADER_H), ALL);
        assert_eq!(fitting(&ALL, height, SCENES, HEADER_H).len(), 3);
    }

    #[test]
    fn least_height_holds_header_status_three_rows_and_the_mixer() {
        assert_eq!(least_height(), 21.0 + 63.0 + 20.0 + 106.0);
    }
}
