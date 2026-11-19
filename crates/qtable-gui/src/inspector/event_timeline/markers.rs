//! Distinct event, copy-ghost and insert-marker painting.
use super::*;
pub(super) fn hit(
    frame: &mut Frame,
    layout: &Layout,
    position: BeatTime,
    top: f32,
    colour: iced::Color,
    selected: bool,
    ghost: bool,
) {
    if !layout.axis.shows(position, MARKER_HALF) {
        return;
    }
    let marker = Path::rectangle(
        Point::new(layout.x(position) - MARKER_HALF, RULER_HEIGHT + top + 6.0),
        Size::new(MARKER_HALF * 2.0, 12.0),
    );
    let fill = if selected { style::ACCENT } else { colour };
    frame.fill(
        &marker,
        if ghost {
            iced::Color {
                a: crate::style::detail::DIM_ALPHA,
                ..fill
            }
        } else {
            fill
        },
    );
    frame.stroke(
        &marker,
        Stroke::default()
            .with_color(if selected {
                style::TEXT
            } else {
                style::CONTROL
            })
            .with_width(1.0),
    );
}
