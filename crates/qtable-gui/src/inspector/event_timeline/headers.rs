//! Lane headers and envelope curves. The curves come from `EnvelopeRow::samples`, the
//! engine's own evaluation; nothing here knows curve maths.
use super::*;
use crate::control::{LIVE_DARK, measure};
use qtable_engine::model::Breakpoint;

/// Where a header's automation dot sits, and how far from it a click still counts.
const DOT_X: f32 = 14.0;
const DOT_RADIUS: f32 = 3.5;
const DOT_HIT: f32 = 8.0;
const POINT_RADIUS: f32 = 2.75;
/// The selected point's ring, and the grabbed point's growth while dragging.
const RING_RADIUS: f32 = 5.0;
/// Where an event lane's label, and an envelope lane's device and name, start.
const EVENT_LABEL_X: f32 = 8.0;
const ENVELOPE_LABEL_X: f32 = 24.0;
/// The most of an envelope header's run its device may take; the name keeps the rest.
const DEVICE_SHARE: f32 = 0.5;

fn text(frame: &mut Frame, content: String, at: Point, color: iced::Color) {
    frame.fill_text(Text {
        content,
        position: at,
        color,
        size: iced::Pixels(crate::style::detail::TEXT_SIZE as f32),
        ..Default::default()
    });
}

/// The canvas face lane headers draw in: the default font at the shared text size.
fn face() -> measure::Face {
    measure::Face::canvas(crate::style::detail::TEXT_SIZE as f32)
}

/// An envelope's ink: the lane's red while it follows the clip, grey once overridden.
fn ink(live: Option<Effective>) -> iced::Color {
    if live.is_some_and(|l| l.overridden()) {
        LIVE_DARK.automation_disabled
    } else {
        LIVE_DARK.automation_color
    }
}

/// The header column of one row. Event lanes keep their label; envelope lanes add the
/// automation dot, the device in parent ink beside the argument name, and the live value.
pub(super) fn header(frame: &mut Frame, timeline: &Timeline, row: usize, chosen: bool) {
    let lane = &timeline.model.rows[row];
    let y = RULER_HEIGHT + lane.top;
    let ink_text = if chosen {
        LIVE_DARK.clip_text_light
    } else {
        style::TEXT
    };
    // Lane labels stay within their own column; the full address is in the event strip.
    let clip = Rectangle::new(
        Point::new(0.0, y),
        Size::new(
            LABEL_WIDTH
                - if timeline.envelope(row).is_some_and(|e| e.region.is_some()) {
                    24.0
                } else {
                    6.0
                },
            lane.height,
        ),
    );
    let Some(envelope) = timeline.envelope(row) else {
        frame.with_clip(clip, |frame| {
            text(
                frame,
                measure::fit_ellipsis(&lane.name, clip.width - EVENT_LABEL_X, face()),
                Point::new(EVENT_LABEL_X, y + 5.0),
                ink_text,
            );
        });
        return;
    };
    if envelope.region.is_some() {
        crate::control::Glyph::Unlinked.paint(
            frame,
            Rectangle::new(
                Point::new(LABEL_WIDTH - 20.0, y + 5.0),
                Size::new(12.0, 12.0),
            ),
            LIVE_DARK.chosen_alternative,
        );
    }
    let live = timeline.live.get(row).copied().flatten();
    frame.fill(
        &Path::circle(Point::new(DOT_X, y + 11.0), DOT_RADIUS),
        ink(live),
    );
    frame.with_clip(clip, |frame| {
        // Device and name read as one run: the device takes at most half of it, and the name
        // starts where the measured device text ends and fills the rest, each clipped by width.
        let room = clip.width - ENVELOPE_LABEL_X;
        let device = if lane.device.is_empty() {
            String::new()
        } else {
            let shown = measure::fit_ellipsis(&lane.device, room * DEVICE_SHARE, face());
            format!("{shown} · ")
        };
        let name_x = ENVELOPE_LABEL_X + measure::width(&device, face());
        text(
            frame,
            device,
            Point::new(ENVELOPE_LABEL_X, y + 5.0),
            LIVE_DARK.path_parent,
        );
        text(
            frame,
            measure::fit_ellipsis(&lane.name, clip.width - name_x, face()),
            Point::new(name_x, y + 5.0),
            ink_text,
        );
    });
    if let Some(live) = live {
        let content = envelope.value_text(live.value);
        let (width, height) = (LABEL_WIDTH - 24.0 - 8.0, 18.0);
        let origin = Point::new(LABEL_WIDTH - 8.0 - width, y + lane.height - height - 6.0);
        frame.fill(
            &Path::rounded_rectangle(
                origin,
                Size::new(width, height),
                LIVE_DARK.corner_control.into(),
            ),
            LIVE_DARK.display_background,
        );
        frame.with_clip(Rectangle::new(origin, Size::new(width, height)), |frame| {
            frame.fill_text(Text {
                content,
                position: Point::new(origin.x + width - 6.0, origin.y + 3.0),
                color: style::TEXT,
                size: iced::Pixels(crate::style::detail::TEXT_SIZE as f32),
                align_x: iced::alignment::Horizontal::Right.into(),
                ..Default::default()
            });
        });
    }
}

/// The dot's click target in header coordinates, for row `row`.
pub(super) fn dot_hit(timeline: &Timeline, row: usize, point: Point) -> bool {
    let Some(lane) = timeline.model.rows.get(row) else {
        return false;
    };
    let centre = Point::new(DOT_X, RULER_HEIGHT + lane.top + 11.0);
    (point.x - centre.x).abs() <= DOT_HIT && (point.y - centre.y).abs() <= DOT_HIT
}

/// One envelope lane's curve and breakpoints from `points`, which are the committed ones or a
/// gesture's preview.
fn curve(
    frame: &mut Frame,
    timeline: &Timeline,
    layout: &Layout,
    row: usize,
    points: &[Breakpoint],
) {
    let (Some(envelope), Some(lane)) = (timeline.envelope(row), timeline.lane(row, layout)) else {
        return;
    };
    if points.is_empty() {
        return;
    }
    let colour = ink(timeline.live.get(row).copied().flatten());
    if envelope.region.is_some() {
        let path = envelope.overview_polyline(timeline.clip.start, &lane);
        frame.stroke(
            &path,
            Stroke::default()
                .with_color(iced::Color {
                    a: crate::style::detail::NON_EDITABLE_ALPHA,
                    ..colour
                })
                .with_width(1.5),
        );
        return;
    }
    let path = envelope.polyline(
        points,
        &lane,
        None,
        (
            0.0,
            crate::inspector::clip_markers::extent(&timeline.clip, timeline.signature).ticks()
                as f64,
        ),
    );
    frame.stroke(&path, Stroke::default().with_color(colour).with_width(1.5));
    for point in points
        .iter()
        .filter(|p| lane.time.shows(p.time, POINT_RADIUS))
    {
        frame.fill(
            &Path::circle(
                Point::new(lane.x(point.time), lane.y(point.value)),
                POINT_RADIUS,
            ),
            colour,
        );
    }
}

/// Each envelope lane's committed curve and breakpoints, except the lane `previewing` stands
/// in for with its gesture's list.
pub(super) fn curves(
    frame: &mut Frame,
    timeline: &Timeline,
    layout: &Layout,
    previewing: Option<usize>,
) {
    for row in 0..timeline.model.rows.len() {
        if previewing == Some(row) {
            continue;
        }
        if let Some(envelope) = timeline.envelope(row) {
            curve(frame, timeline, layout, row, &envelope.points);
        }
    }
}

/// The point being dragged, over the cached curves: its lane drawn from the gesture's list
/// (the list a release would commit), and a ring on the dragged or selected point.
pub(super) fn points(frame: &mut Frame, timeline: &Timeline, state: &State, layout: &Layout) {
    let ring = |frame: &mut Frame,
                row: usize,
                lane: &crate::inspector::point_gesture::Lane,
                point: &Breakpoint| {
        let colour = ink(timeline.live.get(row).copied().flatten());
        let at = Point::new(lane.x(point.time), lane.y(point.value));
        frame.stroke(
            &Path::circle(at, RING_RADIUS),
            Stroke::default().with_color(style::TEXT).with_width(1.5),
        );
        frame.fill(&Path::circle(at, POINT_RADIUS), colour);
    };
    if let Some(paint) = &state.paint
        && let Some(preview) = paint.painting.preview()
    {
        curve(frame, timeline, layout, paint.row, &preview);
        return;
    }
    if let Some(drag) = timeline.point_drag(state)
        && let Some(preview) = drag.gesture.preview()
        && let Some(lane) = timeline.lane(drag.row, layout)
    {
        curve(frame, timeline, layout, drag.row, preview);
        if let Some(grabbed) = drag.gesture.grabbed().and_then(|i| preview.get(i)) {
            ring(frame, drag.row, &lane, grabbed);
        }
        return;
    }
    if let Some(selection) = &timeline.context.content {
        for (target, index) in &selection.points {
            if let Some(row) = timeline
                .model
                .rows
                .iter()
                .position(|r| matches!(&r.kind, RowKind::Envelope(e) if e.target == *target))
                && let Some(lane) = timeline.lane(row, layout)
                && let Some(point) = lane.points().get(*index)
            {
                ring(frame, row, &lane, point);
            }
        }
        return;
    }
    // The selected point of the selected envelope lane.
    if let Some(index) = timeline.point
        && let Some(row) = timeline
            .model
            .rows
            .iter()
            .position(|r| matches!(&r.kind, RowKind::Envelope(e) if Some(&e.target) == timeline.envelope.as_ref()))
        && let Some(lane) = timeline.lane(row, layout)
        && let Some(point) = lane.points().get(index)
    {
        ring(frame, row, &lane, point);
    }
}

/// A hovered segment's two points, in the hover ink.
pub(super) fn hovered_points(
    frame: &mut Frame,
    lane: &crate::inspector::point_gesture::Lane,
    points: &[Breakpoint],
    colour: iced::Color,
) {
    for point in points {
        let at = Point::new(lane.x(point.time), lane.y(point.value));
        frame.fill(&Path::circle(at, POINT_RADIUS), colour);
    }
}

/// An envelope lane with no points yet shows its argument's value as a faint dotted guide.
pub(super) fn guides(frame: &mut Frame, timeline: &Timeline, layout: &Layout) {
    for row in 0..timeline.model.rows.len() {
        let Some(envelope) = timeline.envelope(row) else {
            continue;
        };
        let Some(live) = timeline.live.get(row).copied().flatten() else {
            continue;
        };
        if !envelope.points.is_empty() {
            continue;
        }
        let y = timeline.lane(row, layout).map_or(0.0, |l| l.y(live.value));
        frame.stroke(
            &Path::line(
                Point::new(LABEL_WIDTH, y),
                Point::new(LABEL_WIDTH + layout.span(), y),
            ),
            Stroke {
                line_dash: canvas::LineDash {
                    segments: &[2.0, 4.0],
                    offset: 0,
                },
                ..Stroke::default()
                    .with_color(iced::Color {
                        a: crate::style::detail::DIM_ALPHA,
                        ..ink(Some(live))
                    })
                    .with_width(1.0)
            },
        );
    }
}
