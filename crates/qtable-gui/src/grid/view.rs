//! Borrowed canvas models; only lightweight cell status is built on each tick.
use super::{
    Action, CELL_H, Cell, CellBuilder, FOLD_W, FOOTER_H, GAP, Grid, Layout, LayoutCache, MASTER_W,
    Master, RenameTarget, Row, Selection, cell_status, follow_queued,
};

use super::band;
use crate::messages::Message;
use iced::widget::{Space, column, container, responsive, row, scrollable, stack};
use iced::{Element, Length, Padding};
use qtable_engine::engine::Snapshot;
use qtable_engine::model::{Project, SceneId, TrackId};
use std::rc::Rc;

pub struct ViewState<'a> {
    pub selected: Option<Selection>,
    pub ghost: Option<Selection>,
    pub owner: Option<crate::selection::ColumnOwner>,
    pub cell: Option<crate::selection::ColumnCell>,
    pub selected_scene: Option<SceneId>,
    pub selected_scenes: &'a [SceneId],
    pub selected_tracks: &'a [TrackId],
    pub shift_down: bool,
    pub scene_stop: Vec<crate::scene_stop_feedback::Row>,
    pub blink_on: bool,
    pub revision: u64,
}

/// The slot being renamed and a builder for its text field, given the name area's width.
/// The grid places the field over that area so it scrolls with the clips.
pub(crate) type RenameEditor<'a, M> = (Selection, Rc<dyn Fn(f32) -> Element<'a, M> + 'a>);

/// A header, scene or output being renamed and a builder for its text field, given the
/// width to fill. Each region places its header fields through the shared helper.
pub(crate) type NameEditor<'a, M> = (RenameTarget, Rc<dyn Fn(f32) -> Element<'a, M> + 'a>);

/// Optional extras the window adds around the grid.
pub(crate) struct Extras<'a, M> {
    pub header_gestures: super::header::Gestures,
    pub map_mode: bool,
    /// The strip rows open under the pinned status rows in every region.
    pub rows: Vec<Row>,
    pub device_geometry: Option<crate::browser::Geometry>,
    pub device_drag: Option<&'a crate::browser::Drag>,
    /// Builds each row's cell for a column; rows draw only when this is set.
    pub band_cell: Option<CellBuilder<'a, M>>,
    pub rename: Option<RenameEditor<'a, M>>,
    pub name_editor: Option<NameEditor<'a, M>>,
    /// The window's focused control, for outlining canvas buttons.
    pub focused: Option<crate::drafts::ControlId>,
    /// Where the grid reports the height of its scene rows' viewport.
    pub scene_viewport: Option<super::scene_follow::Viewport>,
}

impl<M> Default for Extras<'_, M> {
    fn default() -> Self {
        Self {
            header_gestures: Default::default(),
            map_mode: false,
            rows: Vec::new(),
            device_geometry: None,
            device_drag: None,
            band_cell: None,
            rename: None,
            name_editor: None,
            focused: None,
            scene_viewport: None,
        }
    }
}

/// The grid alone, without output columns; the tests exercise it directly.
#[cfg(test)]
pub fn view<'a>(
    project: &'a Project,
    snapshot: &'a Snapshot,
    state: ViewState<'a>,
    layout_cache: &LayoutCache,
) -> Element<'a, Action> {
    build(
        project,
        snapshot,
        state,
        layout_cache,
        |action| action,
        0.0,
        Extras::default(),
    )
}

/// The window's grid, with the output columns between the tracks and the Main column.
pub fn view_with_outputs<'a>(
    project: &'a Project,
    snapshot: &'a Snapshot,
    state: ViewState<'a>,
    layout_cache: &LayoutCache,
    outputs_width: f32,
    extras: Extras<'a, Message>,
) -> Element<'a, Message> {
    build(
        project,
        snapshot,
        state,
        layout_cache,
        Message::Grid,
        outputs_width,
        extras,
    )
}

/// One vertically scrolling scene region above a pinned footer for each column region.
fn build<'a, M: 'a>(
    project: &'a Project,
    snapshot: &'a Snapshot,
    state: ViewState<'a>,
    layout_cache: &LayoutCache,
    wrap: fn(Action) -> M,
    between_width: f32,
    Extras {
        header_gestures,
        map_mode,
        rows: band_rows,
        device_geometry,
        device_drag,
        band_cell,
        rename,
        name_editor,
        focused,
        scene_viewport,
    }: Extras<'a, M>,
) -> Element<'a, M> {
    let ViewState {
        selected,
        ghost,
        owner,
        cell,
        selected_scene,
        selected_scenes,
        selected_tracks,
        shift_down,
        scene_stop,
        blink_on,
        revision,
    } = state;
    let layout = layout_cache.get(project, revision);
    let cols = &layout.columns;
    let grid_width = cols.iter().map(|c| c.width + GAP).sum::<f32>();
    let width = GAP + grid_width;
    let cells = project
        .tracks
        .iter()
        .map(|track| {
            let state = snapshot.tracks.iter().find(|s| s.id == track.id);
            track
                .slots
                .iter()
                .enumerate()
                .map(|(slot, model)| {
                    let status = cell_status(state, slot);
                    Cell {
                        clip: model.clip.as_ref(),
                        stop_button: model.has_stop_button,
                        status,
                        follow_queued: follow_queued(state, slot, status),
                        ghost: ghost
                            == Some(Selection {
                                track: track.id,
                                slot,
                            }),
                        selected: selected
                            == Some(Selection {
                                track: track.id,
                                slot,
                            })
                            || (matches!(owner, Some(crate::selection::ColumnOwner::Group(_)))
                                && cell.is_some_and(|c| c.active && c.row == slot)
                                && selected_tracks.contains(&track.id)
                                && track.capabilities().holds_clips),
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let grid = Grid {
        drag: Rc::clone(&header_gestures.tracks),
        drag_viewport: Default::default(),
        drag_h: 0.,
        status_row: false,
        tracks: &project.tracks,
        scenes: &project.scenes,
        cells,
        scene_stop,
        pending_stop: project
            .tracks
            .iter()
            .map(|t| {
                snapshot
                    .pending_stop_slots
                    .iter()
                    .find_map(|(id, slot)| (*id == t.id).then_some(*slot))
            })
            .collect(),
        blink_on,
        selected_tracks,
        revision,
        layout: Rc::clone(&layout),
        shift_down,
        selected: selected.or(ghost),
        selected_scene,
        owner,
        cell,
        focused,
    };
    let header_h = layout.header_h;
    let main_columns = super::layout::columns(project, qtable_engine::model::Region::Main);
    let main_layout = Rc::new(Layout {
        header_h,
        xs: vec![0.0],
        columns: main_columns,
    });
    let selected_row =
        super::scene_area::selected_row(selected, selected_scene, cell, &project.scenes);
    let output_columns = super::layout::columns(project, qtable_engine::model::Region::Outputs);
    let output_layout = Rc::new(Layout {
        header_h,
        xs: super::layout::x_positions(&output_columns),
        columns: output_columns,
    });
    let scene_area = super::scene_area::OutputArea {
        drag: Rc::clone(&header_gestures.outputs),
        drag_viewport: Default::default(),
        drag_h: 0.,
        revision,
        scenes: &project.scenes,
        selected_row,
        outputs: &project.outputs,
        selected_tracks,
        owner,
        cell,
        layout: Rc::clone(&output_layout),
        visible: between_width > 0.0,
        shift_down,
    };
    let master = Master {
        header_h,
        status_row: false,
        main: &project.main,
        clip: selected,
        scenes: &project.scenes,
        selected_scene,
        selected_scenes,
        owner,
        cell,
        last_triggered_scene: snapshot.last_triggered_scene,
        queued_follow_scene: snapshot.queued_follow_scene,
        follow_enabled: project.follow_actions_enabled,
        blink_on,
        revision,
        focused,
    };
    responsive(move |size| {
        // Scene rows scroll vertically together. Each region mirrors its horizontal
        // position between the canvas and its pinned footer.
        // A short window hides whole sections in Live's order; the checks are not touched.
        let navigation_h = if map_mode { CELL_H } else { 0.0 };
        let fitted = if band_cell.is_some() {
            band::fitting(
                &band_rows,
                size.height - navigation_h,
                project.scenes.len(),
                header_h,
            )
        } else {
            Vec::new()
        };
        let open_rows: &[Row] = &fitted;
        let band_h = band::rows_height(open_rows);
        let footer_h = FOOTER_H + navigation_h;
        let column_h = size.height.max(footer_h + band_h);
        let body_h = (column_h - footer_h - band_h).max(0.0);
        let mut grid = grid.clone();
        grid.drag_h = body_h;
        let mut scene_area = scene_area.clone();
        scene_area.drag_h = body_h;
        if let Some(viewport) = &scene_viewport {
            viewport.set((body_h, header_h));
        }
        let content_h = (header_h + project.scenes.len() as f32 * (CELL_H + GAP)).max(body_h);
        let track_viewport = (size.width - between_width - MASTER_W - 2.0 * REGION_GAP).max(0.0);
        // The hint's gap only adds scroll range once the tracks overflow; until then the
        // blank space right of the last track already holds the hint.
        let width = if width > track_viewport {
            width + ADD_TRACK_GAP_W
        } else {
            width
        };
        let canvas =
            Element::from(grid.clone().with_help(width.max(track_viewport), content_h)).map(wrap);
        let canvas = crate::browser::columns(
            canvas,
            "tracks",
            device_geometry.clone(),
            &layout.columns,
            &layout.xs,
            device_drag,
        );
        let mut layers = stack![canvas];
        let editors = [rename_layer(project, &layout, rename.as_ref())];
        for editor in editors.into_iter().flatten() {
            layers = layers.push(editor.width(width).height(content_h));
        }
        let clips = scrollable(layers)
            .id(iced::widget::Id::new(CLIP_COLUMNS))
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::hidden(),
            ))
            .on_scroll(move |v| {
                wrap(Action::ScrollColumns {
                    region: qtable_engine::model::Region::Tracks,
                    footer: false,
                    offset: v.absolute_offset().x.round() as u32,
                })
            })
            .width(Length::Fill)
            .height(Length::Fixed(content_h));
        let output_width = if between_width > 0.0 {
            scene_area.layout.end().max(between_width)
        } else {
            0.0
        };
        let canvas = Element::from(scene_area.clone().with_help(output_width, content_h)).map(wrap);
        let canvas = crate::browser::columns(
            canvas,
            "outputs",
            device_geometry.clone(),
            &output_layout.columns,
            &output_layout.xs,
            device_drag,
        );
        let output_layers = stack![canvas];
        let outputs = scrollable(output_layers)
            .id(iced::widget::Id::new(OUTPUT_COLUMNS))
            .on_scroll(move |v| {
                wrap(Action::ScrollColumns {
                    region: qtable_engine::model::Region::Outputs,
                    footer: false,
                    offset: v.absolute_offset().x.round() as u32,
                })
            })
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::hidden(),
            ))
            .width(Length::Fixed(between_width))
            .height(Length::Fixed(content_h));
        let gutter = || {
            let mut area = scene_area.clone();
            area.visible = false;
            Element::from(area.with_help(REGION_GAP, content_h)).map(wrap)
        };
        // The track header row stays pinned above the scrolling slot rows, as in Live: the same
        // canvases, one header row tall, mirrored horizontally with the columns beneath.
        let header_scroll = |canvas: Element<'a, M>, id: &'static str, region, width: Length| {
            scrollable(canvas)
                .id(iced::widget::Id::new(id))
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::hidden(),
                ))
                .on_scroll(move |v| {
                    wrap(Action::ScrollColumns {
                        region,
                        footer: true,
                        offset: v.absolute_offset().x.round() as u32,
                    })
                })
                .width(width)
                .height(Length::Fixed(header_h))
        };
        let mut track_header_layers = stack![
            Element::from(grid.clone().with_help(width.max(track_viewport), header_h)).map(wrap)
        ];
        if let Some(editor) = header_editor_layer(&layout, name_editor.as_ref()) {
            track_header_layers =
                track_header_layers.push(editor.width(width.max(track_viewport)).height(header_h));
        }
        let header_tracks = header_scroll(
            crate::browser::columns(
                track_header_layers.into(),
                "tracks-header",
                device_geometry.clone(),
                &layout.columns,
                &layout.xs,
                device_drag,
            ),
            HEADER_COLUMNS,
            qtable_engine::model::Region::Tracks,
            Length::Fill,
        );
        let mut output_header_layers =
            stack![Element::from(scene_area.clone().with_help(output_width, header_h)).map(wrap)];
        if let Some(editor) = header_editor_layer(&output_layout, name_editor.as_ref()) {
            output_header_layers =
                output_header_layers.push(editor.width(output_width).height(header_h));
        }
        let header_outputs = header_scroll(
            crate::browser::columns(
                output_header_layers.into(),
                "outputs-header",
                device_geometry.clone(),
                &output_layout.columns,
                &output_layout.xs,
                device_drag,
            ),
            OUTPUT_HEADER_COLUMNS,
            qtable_engine::model::Region::Outputs,
            Length::Fixed(between_width),
        );
        let header_gutter = || {
            let mut area = scene_area.clone();
            area.visible = false;
            Element::from(area.with_help(REGION_GAP, header_h)).map(wrap)
        };
        let header = row![
            header_tracks,
            header_gutter(),
            header_outputs,
            header_gutter(),
            crate::browser::columns(
                Element::from(master.clone().with_help(MASTER_W, header_h)).map(wrap),
                "main-header",
                device_geometry.clone(),
                &main_layout.columns,
                &main_layout.xs,
                device_drag
            ),
        ]
        .width(Length::Fill)
        .height(Length::Fixed(header_h));
        let scene_content = row![
            Element::from(clips),
            gutter(),
            Element::from(outputs),
            gutter(),
            main_column(
                crate::browser::columns(
                    Element::from(master.clone().with_help(MASTER_W, content_h)).map(wrap),
                    "main",
                    device_geometry.clone(),
                    &main_layout.columns,
                    &main_layout.xs,
                    device_drag
                ),
                scene_editor_layer(project, name_editor.as_ref(), header_h),
                content_h,
            )
        ]
        .spacing(0);
        let mut scene_layers = stack![scene_content];
        if !map_mode && !matches!(name_editor.as_ref(), Some((RenameTarget::Scene(_), _))) {
            let viewport = crate::control::viewport::Viewport::default();
            scene_layers = scene_layers.push(
                crate::control::viewport::report(
                    iced::widget::canvas(super::scene_drag::SceneDrag {
                        viewport: viewport.clone(),
                        header_h,
                        scenes: &project.scenes,
                        selected: selected_scenes,
                        revision,
                    })
                    .width(Length::Fill)
                    .height(Length::Fixed(content_h)),
                    viewport,
                )
                .map(wrap),
            );
        }
        let rows = scrollable(scene_layers)
            .id(iced::widget::Id::new(SCENE_ROWS))
            // The shared rail would cover Main's scene numbers when the Detail View
            // reduces the viewport. Keep wheel/trackpad scrolling across the rows.
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::hidden(),
            ))
            .on_scroll(move |v| {
                wrap(Action::ScrollScenes {
                    offset: v.absolute_offset().y.round() as u32,
                })
            })
            .width(Length::Fill)
            .height(Length::Fixed(body_h));
        // Opaque, so the header row's copy scrolled beneath never answers the pointer as well.
        let rows = stack![
            crate::control::viewport::clip_top(rows, header_h),
            iced::widget::opaque(header)
        ];
        let mut status = grid.clone();
        status.status_row = true;
        let mut pinned = column![Element::from(status.with_help(width, FOOTER_H)).map(wrap)]
            .width(Length::Fixed(width))
            .height(Length::Fixed(footer_h + band_h));
        if map_mode {
            pinned = pinned.push(Space::new().height(navigation_h));
        }
        if let Some(build) = &band_cell {
            for &row in open_rows {
                pinned = pinned.push(band::tracks(
                    &layout,
                    &grid.selection_view(),
                    &|owner| build(row, owner),
                    row,
                ));
            }
        }
        let continuation = iced::widget::canvas(super::header::DropContinuation(grid.clone()))
            .width(Length::Fixed(width))
            .height(Length::Fixed(footer_h + band_h));
        let footer_tracks = scrollable(stack![pinned, Element::from(continuation).map(wrap)])
            .id(iced::widget::Id::new(FOOTER_COLUMNS))
            // Observed 2026-10-12: Live shows a subtle scrollbar at the bottom when tracks overflow
            // (this revises his 2026-10-11 "no scrollbars"). The clips above stay railless.
            .direction(scrollable::Direction::Horizontal(
                crate::style::detail::overlay_scrollbar(),
            ))
            .on_scroll(move |v| {
                wrap(Action::ScrollColumns {
                    region: qtable_engine::model::Region::Tracks,
                    footer: true,
                    offset: v.absolute_offset().x.round() as u32,
                })
            })
            .width(Length::Fill)
            .height(Length::Fixed(footer_h + band_h));
        let mut status = master.clone();
        status.status_row = true;
        let status_tip = |width: f32, label: &'static str| {
            crate::control::tooltip(
                Space::new()
                    .width(Length::Fixed(width))
                    .height(Length::Fixed(FOOTER_H)),
                label,
                iced::widget::tooltip::Position::Top,
            )
        };
        let tips = row![
            status_tip(CELL_H, "Stop All Clips"),
            Space::new().width(Length::Fill),
            status_tip(CELL_H, "Global Follow Actions"),
            Space::new().width(Length::Fixed(GAP)),
        ]
        .width(Length::Fixed(MASTER_W));
        let status = iced::widget::stack![status.with_help(MASTER_W, FOOTER_H), tips];
        let output_content_width = output_layout.end().max(between_width);
        let output_status = iced::widget::Canvas::new(super::status_row::Outputs {
            layout: Rc::clone(&output_layout),
            selection: scene_area.selection(),
        })
        .width(Length::Fixed(output_content_width))
        .height(Length::Fixed(FOOTER_H));
        let mut output_pinned = column![output_status]
            .width(Length::Fixed(output_content_width))
            .height(Length::Fixed(footer_h + band_h));
        if map_mode {
            output_pinned = output_pinned.push(Space::new().height(navigation_h));
        }
        if let Some(build) = &band_cell
            && between_width > 0.0
        {
            for &row in open_rows {
                output_pinned = output_pinned.push(band::tracks(
                    &output_layout,
                    &scene_area.selection(),
                    &|owner| build(row, owner),
                    row,
                ));
            }
        }
        let output_footer = scrollable(output_pinned)
            .id(iced::widget::Id::new(OUTPUT_FOOTER_COLUMNS))
            .direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::hidden(),
            ))
            .on_scroll(move |v| {
                wrap(Action::ScrollColumns {
                    region: qtable_engine::model::Region::Outputs,
                    footer: true,
                    offset: v.absolute_offset().x.round() as u32,
                })
            })
            .width(Length::Fixed(between_width))
            .height(Length::Fixed(footer_h + band_h));
        let main_selection = crate::selection::SelectionView {
            models: &project.tracks,
            tracks: selected_tracks,
            owner,
            cell,
            clip: selected,
            scene: selected_scene,
        };
        let mut main_band = iced::widget::Column::new();
        if let Some(build) = &band_cell {
            for &row in open_rows {
                main_band = main_band.push(band::tracks(
                    &main_layout,
                    &main_selection,
                    &|owner| build(row, owner),
                    row,
                ));
            }
        }
        let mut main_footer =
            column![Element::from(status).map(wrap)].width(Length::Fixed(MASTER_W));
        if map_mode {
            main_footer = main_footer.push(super::navigation::view(selected_row).map(wrap));
        }
        let footer =
            row![footer_tracks, output_footer, main_footer.push(main_band)].spacing(REGION_GAP);
        column![rows, footer]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    })
    .height(Length::Fill)
    .into()
}

/// Main's canvas, with the scene's rename field over it while one is open.
fn main_column<'a, M: 'a>(
    canvas: Element<'a, M>,
    editor: Option<iced::widget::Container<'a, M>>,
    height: f32,
) -> Element<'a, M> {
    match editor {
        Some(editor) => stack![canvas, editor.width(MASTER_W).height(height)].into(),
        None => canvas,
    }
}

/// The rename field, placed over the name area of the slot being renamed: right of the
/// launch square, filling the cell. None when the slot's track is folded away.
fn rename_layer<'a, M: 'a>(
    project: &Project,
    layout: &Layout,
    rename: Option<&RenameEditor<'a, M>>,
) -> Option<iced::widget::Container<'a, M>> {
    let (sel, editor) = rename?;
    let track = project.tracks.iter().position(|t| t.id == sel.track)?;
    let ci = layout
        .columns
        .iter()
        .position(|c| c.track() == Some(track))?;
    let top = layout.header_h + sel.slot as f32 * (CELL_H + GAP);
    Some(place(
        editor,
        layout.xs[ci] + CELL_H,
        top,
        layout.columns[ci].width - CELL_H,
    ))
}

/// The header rename field, over the track's title or the group track's title up to its fold button.
/// None for a scene or output, whose editors sit in Main and the output columns.
fn header_editor_layer<'a, M: 'a>(
    layout: &Layout,
    rename: Option<&NameEditor<'a, M>>,
) -> Option<iced::widget::Container<'a, M>> {
    let (RenameTarget::Header(owner), editor) = rename? else {
        return None;
    };
    let ci = layout
        .columns
        .iter()
        .position(|col| super::header::menu(col) == Action::HeaderMenu(*owner))?;
    let col = &layout.columns[ci];
    if !col.caps.renamable {
        return None;
    }
    let name = col.name(layout.xs[ci]);
    Some(place(editor, name.x, name.y, name.width.max(FOLD_W)))
}

/// The scene rename field, over the scene's name in the Main column: right of the launch
/// square, short of the scene number.
fn scene_editor_layer<'a, M: 'a>(
    project: &Project,
    rename: Option<&NameEditor<'a, M>>,
    header_h: f32,
) -> Option<iced::widget::Container<'a, M>> {
    let (RenameTarget::Scene(id), editor) = rename? else {
        return None;
    };
    let row = project.scenes.iter().position(|s| s.id == *id)?;
    let top = header_h + row as f32 * (CELL_H + GAP);
    Some(place(
        editor,
        CELL_H,
        top,
        MASTER_W - CELL_H - SCENE_NUMBER_W,
    ))
}

/// `editor`, `width` wide, with its top-left corner at (`left`, `top`) in the grid canvas.
fn place<'a, M: 'a>(
    editor: &Rc<dyn Fn(f32) -> Element<'a, M> + 'a>,
    left: f32,
    top: f32,
    width: f32,
) -> iced::widget::Container<'a, M> {
    let at = Padding {
        top,
        left,
        ..Padding::ZERO
    };
    container(editor(width)).padding(at)
}

/// The gap after the last track, wide enough for the add-track hint.
const ADD_TRACK_GAP_W: f32 = 230.0;
/// Scene-selectable gutter between column regions, shared with footer spacing.
const REGION_GAP: f32 = 6.0;
/// Room kept clear on Main's right edge for the scene number.
const SCENE_NUMBER_W: f32 = 18.0;
const CLIP_COLUMNS: &str = "session-clip-columns";
const FOOTER_COLUMNS: &str = "session-footer-columns";
const OUTPUT_COLUMNS: &str = "session-output-columns";
const OUTPUT_FOOTER_COLUMNS: &str = "session-output-footer-columns";
const HEADER_COLUMNS: &str = "session-header-columns";
const OUTPUT_HEADER_COLUMNS: &str = "session-output-header-columns";
pub(super) const SCENE_ROWS: &str = "session-scene-rows";

/// Mirror horizontal scrolling between the clips, the pinned header row and the pinned stop row
/// and band. Every one of a region's scrollables takes the offset; the one that moved ignores it.
pub(crate) fn scroll_columns(
    region: qtable_engine::model::Region,
    _footer: bool,
    offset: u32,
) -> iced::Task<crate::messages::Message> {
    iced::Task::batch(scroll_targets(region).iter().map(|target| {
        iced::widget::operation::scroll_to(
            iced::widget::Id::new(target),
            scrollable::AbsoluteOffset {
                x: Some(offset as f32),
                y: None,
            },
        )
    }))
}

/// Scroll the scene rows to this offset in px, below the pinned header row.
pub(crate) fn scroll_scenes(offset: f32) -> iced::Task<crate::messages::Message> {
    iced::widget::operation::scroll_to(
        iced::widget::Id::new(SCENE_ROWS),
        scrollable::AbsoluteOffset {
            x: None,
            y: Some(offset),
        },
    )
}

fn scroll_targets(region: qtable_engine::model::Region) -> &'static [&'static str] {
    use qtable_engine::model::Region;
    match region {
        Region::Tracks => &[CLIP_COLUMNS, FOOTER_COLUMNS, HEADER_COLUMNS],
        Region::Outputs => &[OUTPUT_COLUMNS, OUTPUT_FOOTER_COLUMNS, OUTPUT_HEADER_COLUMNS],
        Region::Main => &[],
    }
}

#[cfg(test)]
mod tests;
