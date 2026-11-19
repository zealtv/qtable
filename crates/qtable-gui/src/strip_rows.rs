//! The strip rows: one builder per row, shared by track, group,
//! output and Main columns. A kind that lacks a row's content draws nothing in it, so the rows
//! stay level; the rule for each kind lives in that row's builder (and in `TrackFacts`).
use crate::app::App;
use crate::grid::{CellBuilder, Row};
use crate::messages::Message;
use std::rc::Rc;

pub(crate) fn cells(app: &App) -> CellBuilder<'_, Message> {
    Rc::new(move |row, cell| match row {
        Row::Output => crate::output_band::output(app, cell),
        Row::Path => crate::path_band::path(app, cell),
        Row::Mixer => crate::mixer_band::mixer(app, cell),
        Row::Throughput => crate::throughput::strip(app, cell),
    })
}
