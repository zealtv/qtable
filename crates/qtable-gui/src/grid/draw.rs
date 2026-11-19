//! Drawing for the grid and master canvases, in Live 12's dark theme colours.
use super::{
    CELL_H, CLIP_NAME_INSET, Cell, CellStatus, GAP, Grid, HINT_INSET, HINT_LINE_H, MASTER_W,
    Master, PROGRESS_H, SCENE_NAME_INSET, SCENE_NUMBER_INSET, SCENE_NUMBER_ROOM, SELECTION_OUTLINE,
    THIN_OUTLINE, cache,
};

use crate::control::theme::{DARK_INK, ink};
use crate::control::{self, ControlKind, LIVE_DARK, Look};
pub(super) use crate::outputs::to_colour as colour;
use iced::widget::canvas::{Frame, Path, Stroke, Text as CanvasText};
use iced::widget::text::Alignment;
use iced::{Color, Point, Rectangle, Size, alignment};

pub(super) const INK: Color = LIVE_DARK.control_foreground;
pub(super) const GREEN: Color = LIVE_DARK.chosen_play;
/// Launch area behind a playing or follow-queued clip's icon (Live 12
/// screenshot, 2026-10-11).
pub(super) const LAUNCH_ACTIVE: Color = LIVE_DARK.launch_active;
pub(super) const GAP_COLOUR: Color = LIVE_DARK.desktop;
pub(super) const TRACK_BODY: Color = LIVE_DARK.surface_area;
/// Empty slots remain visible as lighter boxes above the unoccupied track space.
pub(super) const SLOT_BODY: Color = LIVE_DARK.slot_background;
pub(super) const BUTTON: Color = LIVE_DARK.control_background;
/// Stop button background while its track plays (Live's SurfaceHighlight).
pub(super) const LIT: Color = LIVE_DARK.surface_highlight;
pub(super) const DISABLED: Color = LIVE_DARK.text_disabled;
// Neutral lightening, preserving clip colours and readable launch controls.
// Geometry follows Live 12 observations (2026-10-11); tint strength is a qtable choice.
pub(super) const ROW_TINT: Color = LIVE_DARK.row_tint;
pub(crate) const COLUMN_TINT: Color = LIVE_DARK.column_tint;
/// The unfocused selection's grey.
pub(super) const SELECTION_INACTIVE: Color = LIVE_DARK.selection_inactive;
/// A clip's launch area, darker than its colour; the launch icon's ink is derived from it.
const LAUNCH_SHADE: f32 = 0.78;
/// The follow-action icon, darker still.
const FOLLOW_ICON_SHADE: f32 = 0.35;
const TEXT_SIZE: f32 = crate::style::detail::TEXT_SIZE as f32;

#[cfg(test)]
pub(super) fn row_y(slot: usize) -> f32 {
    super::HEADER_H + slot as f32 * (CELL_H + GAP)
}

/// The longest prefix of `text` whose measured width fits in `width` (at least one character),
/// in the grid's canvas face.
pub(super) fn fit(text: &str, width: f32) -> String {
    control::measure::fit(text, width, control::measure::Face::canvas(TEXT_SIZE))
}

/// Live shows only the number for a scene unless it has been given a name of its own.
pub(super) fn scene_is_named(name: &str) -> bool {
    let default = name
        .strip_prefix("Scene ")
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    !name.trim().is_empty() && !default
}

pub(super) fn rect(frame: &mut Frame, area: Rectangle, fill: Color) {
    frame.fill(&Path::rectangle(area.position(), area.size()), fill);
}

pub(super) fn outline(frame: &mut Frame, area: Rectangle, color: Color, width: f32) {
    frame.stroke(
        &Path::rectangle(area.position(), area.size()),
        Stroke::default().with_color(color).with_width(width),
    );
}

fn scaled(c: Color, factor: f32) -> Color {
    Color::from_rgb(c.r * factor, c.g * factor, c.b * factor)
}

/// Text vertically centred on `y`; right-aligned when `right` is set.
pub(super) fn label(frame: &mut Frame, content: String, x: f32, y: f32, color: Color, right: bool) {
    frame.fill_text(CanvasText {
        content,
        position: Point::new(x, y),
        color,
        size: iced::Pixels(TEXT_SIZE),
        align_x: if right {
            Alignment::Right
        } else {
            Alignment::Left
        },
        align_y: alignment::Vertical::Center,
        ..CanvasText::default()
    });
}

/// Launch and stop glyphs are control kinds; the grid supplies playback colours.
pub(super) fn triangle(frame: &mut Frame, area: Rectangle, color: Color) {
    launch_icon(frame, area, color, false);
}

fn launch_icon(frame: &mut Frame, area: Rectangle, color: Color, follow_enabled: bool) {
    let launch = control::Launch {
        follow: follow_enabled,
    };
    launch.paint(frame, area, &Look::glyph(Color::TRANSPARENT, color));
}

/// A 20 px button square holding a small centred stop square.
pub(super) fn stop_button(frame: &mut Frame, area: Rectangle, background: Color, color: Color) {
    control::Stop.paint(frame, area, &Look::glyph(background, color));
}

pub(super) fn scene_icon(m: &Master<'_>, id: qtable_engine::model::SceneId) -> Color {
    if m.queued_follow_scene == Some(id) {
        if m.blink_on { GREEN } else { DARK_INK }
    } else if m.last_triggered_scene == Some(id) {
        GREEN
    } else {
        DISABLED
    }
}

pub(super) fn master(m: &Master<'_>, frame: &mut Frame, names: &cache::Names) {
    let w = MASTER_W - GAP;
    rect(
        frame,
        Rectangle::new(Point::ORIGIN, frame.size()),
        TRACK_BODY,
    );
    let mut col = super::layout::Column::main(m.main.id);
    col.header_h = m.header_h;
    let selection = crate::selection::SelectionView {
        models: &[],
        tracks: &[],
        owner: m.owner,
        cell: m.cell,
        clip: m.clip,
        scene: m.selected_scene,
    };
    let look = super::header::look(&col, &super::header::HeaderFacts::track(m.main), &selection);
    super::header::draw(frame, &col, 0.0, &look, names);
    for (si, scene) in m.scenes.iter().enumerate() {
        let id = scene.id;
        let name = &scene.name;
        let y = m.header_h + si as f32 * (CELL_H + GAP);
        let area = Rectangle::new(Point::new(0.0, y), Size::new(w, CELL_H));
        rect(
            frame,
            area,
            if m.selected_scenes.contains(&id) {
                LIVE_DARK.selection_background
            } else if m.cell.is_some_and(|c| {
                c.owner == crate::selection::ColumnOwner::Track(m.main.id) && c.row == si
            }) {
                if m.cell.is_some_and(|c| c.active) {
                    LIVE_DARK.selection_background
                } else {
                    SELECTION_INACTIVE
                }
            } else {
                SLOT_BODY
            },
        );
        let launch = Rectangle::new(Point::new(0.0, y), Size::new(CELL_H, CELL_H));
        if m.queued_follow_scene == Some(id) {
            // Inferred from clip behaviour, Live check pending (2026-10-11).
            rect(frame, launch, LAUNCH_ACTIVE);
        }
        triangle(frame, launch, scene_icon(m, id));
        let text_ink = if m.selected_scenes.contains(&id) || m.selected_scene == Some(id) {
            DARK_INK
        } else {
            INK
        };
        let centre = y + CELL_H / 2.0;
        if scene_is_named(name) {
            label(
                frame,
                names.fit(name, w - CELL_H - SCENE_NUMBER_ROOM),
                CELL_H + SCENE_NAME_INSET,
                centre,
                text_ink,
                false,
            );
        }
        label(
            frame,
            (si + 1).to_string(),
            w - SCENE_NUMBER_INSET,
            centre,
            text_ink,
            true,
        );
        if m.selected_scenes.contains(&id) || m.selected_scene == Some(id) {
            outline(
                frame,
                area,
                LIVE_DARK.selection_background,
                SELECTION_OUTLINE,
            );
        }
    }
    super::header::tint(
        frame,
        &col,
        0.0,
        (frame.height() - m.header_h).max(0.0),
        &look,
    );
}

/// Two lines, so the hint fits the blank space beside a few tracks.
const ADD_TRACK_HINT: [&str; 2] = ["Double-click or ⌘T", "to add a track"];

pub(super) fn grid_base(g: &Grid<'_>, frame: &mut Frame, names: &cache::Names) {
    rect(
        frame,
        Rectangle::new(Point::ORIGIN, frame.size()),
        GAP_COLOUR,
    );
    let body_h = (frame.height() - g.layout.header_h).max(0.0);
    for (ci, col) in g.layout.columns.iter().enumerate() {
        let x = g.layout.xs[ci];
        let width = col.width;
        rect(
            frame,
            Rectangle::new(Point::new(x, g.layout.header_h), Size::new(width, body_h)),
            TRACK_BODY,
        );
        let Some(track) = col.index.and_then(|index| g.tracks.get(index)) else {
            continue;
        };
        let look = super::header::look(
            col,
            &super::header::HeaderFacts::track(track),
            &g.selection_view(),
        );
        super::header::draw(frame, col, x, &look, names);
        let Some(ti) = col.track() else { continue };
        for (si, cell) in g.cells[ti].iter().enumerate() {
            clip_slot(
                frame,
                cell,
                Point::new(x, g.layout.header_h + si as f32 * (CELL_H + GAP)),
                width,
                names,
            );
        }
    }
    if !g.status_row {
        // Like Live's "Drop Files and Devices Here": a hint in the gap after the last track.
        for (i, line) in ADD_TRACK_HINT.iter().enumerate() {
            let y = g.layout.header_h + CELL_H / 2.0 + i as f32 * HINT_LINE_H;
            label(
                frame,
                line.to_string(),
                g.layout.end() + HINT_INSET,
                y,
                DISABLED,
                false,
            );
        }
    }
    if let Some(row) =
        super::scene_area::selected_row(g.selected, g.selected_scene, g.cell, g.scenes)
    {
        rect(
            frame,
            Rectangle::new(
                Point::new(0.0, g.layout.header_h + row as f32 * (CELL_H + GAP)),
                Size::new(frame.width(), CELL_H),
            ),
            ROW_TINT,
        );
    }
    for (ci, col) in g.layout.columns.iter().enumerate() {
        let Some(track) = col.index.and_then(|index| g.tracks.get(index)) else {
            continue;
        };
        let look = super::header::look(
            col,
            &super::header::HeaderFacts::track(track),
            &g.selection_view(),
        );
        super::header::tint(frame, col, g.layout.xs[ci], body_h, &look);
    }
}

pub(super) fn grid_overlay(g: &Grid<'_>, frame: &mut Frame) {
    for (ci, col) in g.layout.columns.iter().enumerate() {
        let x = g.layout.xs[ci];
        if let Some(gi) = col.group_track() {
            let folded = g.tracks.get(gi).is_some_and(|group| group.fold_state);
            super::group_column::slots(frame, g, col, x, folded);
            continue;
        }
        let Some(ti) = col.track() else { continue };
        for (si, cell) in g.cells[ti].iter().enumerate() {
            clip_overlay(
                frame,
                cell,
                Point::new(x, g.layout.header_h + si as f32 * (CELL_H + GAP)),
                col.width,
                g.blink_on,
                g.stop_feedback(&[ti], si),
            );
        }
    }
}

fn clip_slot(frame: &mut Frame, cell: &Cell<'_>, at: Point, width: f32, names: &cache::Names) {
    let area = Rectangle::new(at, Size::new(width, CELL_H));
    let launch = Rectangle::new(at, Size::new(CELL_H, CELL_H));
    if let Some(clip) = &cell.clip {
        let base = colour(clip.colour);
        rect(frame, area, base);
        rect(frame, launch, scaled(base, LAUNCH_SHADE));
        let follow_enabled = clip.follow_actions.actions.enabled;
        launch_icon(
            frame,
            launch,
            if follow_enabled {
                scaled(base, FOLLOW_ICON_SHADE)
            } else {
                ink(scaled(base, LAUNCH_SHADE))
            },
            follow_enabled,
        );
        label(
            frame,
            names.fit(&clip.name, width - CELL_H - 2.0 * CLIP_NAME_INSET),
            at.x + CELL_H + CLIP_NAME_INSET,
            at.y + CELL_H / 2.0,
            ink(base),
            false,
        );
    } else {
        rect(frame, area, SLOT_BODY);
        if cell.stop_button {
            stop_button(frame, launch, BUTTON, DISABLED);
        }
    }
}

/// The play icon of a follow-queued slot pulses between Live's play green and
/// the dark icon colour; other slots keep their usual icon.
pub(super) fn pulsing_icon(cell: &Cell<'_>, blink_on: bool) -> Option<Color> {
    cell.follow_queued
        .then_some(if blink_on { GREEN } else { DARK_INK })
}

fn clip_overlay(
    frame: &mut Frame,
    cell: &Cell<'_>,
    at: Point,
    width: f32,
    blink_on: bool,
    stopping: Option<bool>,
) {
    let area = Rectangle::new(at, Size::new(width, CELL_H));
    if let Some(clip) = &cell.clip {
        let launch = Rectangle::new(at, Size::new(CELL_H, CELL_H));
        let pulse = pulsing_icon(cell, blink_on);
        match cell.status {
            CellStatus::Triggered if blink_on => {
                outline(frame, area, LIVE_DARK.clip_text_light, SELECTION_OUTLINE)
            }
            CellStatus::Playing(progress) => {
                rect(frame, launch, LAUNCH_ACTIVE);
                triangle(frame, launch, pulse.unwrap_or(GREEN));
                rect(
                    frame,
                    Rectangle::new(
                        Point::new(at.x, at.y + CELL_H - PROGRESS_H),
                        Size::new(width * progress, PROGRESS_H),
                    ),
                    LIVE_DARK.clip_text_light,
                );
            }
            CellStatus::Stopped => {
                if let Some(icon) = pulse {
                    rect(frame, launch, LAUNCH_ACTIVE);
                    triangle(frame, launch, icon);
                } else if cell.selected {
                    rect(frame, launch, LIVE_DARK.selection_background);
                    launch_icon(frame, launch, DARK_INK, clip.follow_actions.actions.enabled);
                }
            }
            CellStatus::Triggered => {}
        }
    }
    if cell.ghost {
        if cell.clip.is_none() {
            rect(frame, area, SELECTION_INACTIVE);
        }
        rect(
            frame,
            Rectangle::new(at, Size::new(CELL_H, CELL_H)),
            SELECTION_INACTIVE,
        );
        outline(frame, area, SELECTION_INACTIVE, THIN_OUTLINE);
    } else if cell.selected {
        if cell.clip.is_none() {
            rect(frame, area, LIVE_DARK.selection_background);
            if cell.stop_button {
                stop_button(
                    frame,
                    Rectangle::new(at, Size::new(CELL_H, CELL_H)),
                    LIVE_DARK.selection_background,
                    DARK_INK,
                );
            }
        }
        outline(
            frame,
            area,
            LIVE_DARK.selection_background,
            SELECTION_OUTLINE,
        );
    }
    // Paint after selection so the fired square remains visible in a selected scene.
    if let Some(on) = stopping
        && cell.clip.is_none()
        && cell.stop_button
    {
        stop_button(
            frame,
            Rectangle::new(at, Size::new(CELL_H, CELL_H)),
            if cell.selected {
                LIVE_DARK.selection_background
            } else {
                BUTTON
            },
            if on { GREEN } else { DARK_INK },
        );
    }
}
