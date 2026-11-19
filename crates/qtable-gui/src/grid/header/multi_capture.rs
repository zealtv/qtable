//! Native stills through the production selection, gesture, planner and update paths.
use super::*;
use crate::{app::App, messages::Message};

pub(super) fn setup(app: &mut App, scenario: &str) -> bool {
    let mode = scenario
        .trim_end_matches("-drop")
        .trim_end_matches("-before");
    let (selected, target, fraction): (&[u64], u64, f32) = match mode {
        "multi-into" => (&[7, 8], 5, 0.25),
        "multi-out" => (&[4, 5], 8, 0.25),
        "multi-mixed" => (&[3, 6], 8, 0.25),
        "multi-hidden" => (&[4, 8], 6, 0.25),
        "multi-invalid" => (&[3, 6], 4, 0.25),
        _ => return false,
    };
    for (i, id) in selected.iter().enumerate() {
        app.selection.command_down = i != 0;
        let action = if app.project()[TrackId(*id)].is_group() {
            Action::SelectGroup(TrackId(*id), false)
        } else {
            Action::SelectTrack(TrackId(*id), false)
        };
        let _ = crate::update::update(app, Message::Grid(Action::PressHeader(Box::new(action))));
    }
    app.selection.command_down = false;
    if mode == "multi-hidden" {
        let _ = crate::update::update(app, Message::Grid(Action::ToggleGroup(TrackId(3))));
    }
    let layout = app.grid_layout.get(app.project(), app.project_revision());
    let ci = |id| {
        layout
            .columns
            .iter()
            .position(|c| c.key == ColumnKey::Track(TrackId(id)))
            .unwrap()
    };
    // Drag from the final selected header (visible even in the hidden-selection case).
    let from = ci(*selected.last().unwrap());
    let to = ci(target);
    let origin = Point::new(layout.xs[from] + 12., layout.header_h - 4.);
    let at = Point::new(
        layout.xs[to] + layout.columns[to].width * fraction,
        layout.header_h - 4.,
    );
    let (action, drag) = press(
        &layout.columns[from],
        origin,
        layout.xs[from],
        false,
        app.project_revision(),
    );
    app.header_gestures.tracks.set(drag);
    let _ = crate::update::update(app, Message::Grid(Action::PressHeader(Box::new(action))));
    let mut drag = app.header_gestures.tracks.get().unwrap();
    if !scenario.ends_with("-before") {
        drag.cursor = at;
    }
    let preview = drop_selected_at(
        &app.project().tracks,
        &layout.columns,
        &layout.xs,
        &drag.selected,
        drag.key,
        drag.cursor,
    );
    eprintln!(
        "{scenario} selected={:?} preview={preview:?}",
        drag.selected
    );
    if scenario.ends_with("-drop") {
        let bounds = Rectangle::with_size(Size::new(
            layout.end() + super::super::TRACK_W,
            layout.header_h,
        ));
        let action = release_action(
            &app.project().tracks,
            &layout.columns,
            &layout.xs,
            &mut drag,
            at,
            bounds,
        );
        eprintln!("{scenario} release={action:?}");
        if let Some(action) = action {
            let _ = crate::update::update(app, Message::Grid(action));
        }
        app.header_gestures.tracks.set(None);
    } else {
        app.header_gestures.tracks.set(Some(drag));
        app.header_gestures.tracks.capture_hold.set(true);
    }
    true
}
