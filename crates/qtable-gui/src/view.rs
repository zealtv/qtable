//! Session grid and Detail View content inside the window regions.
use crate::app::App;
use crate::drafts::{ClipField, DraftKey};
use crate::messages::Message;
use crate::{chrome, grid, inspector, inspector_panel};
use iced::Element;
use qtable_engine::model::TrackId;

pub(crate) fn view(app: &App) -> Element<'_, Message> {
    app.browser.geometry.borrow_mut().clear();
    let grid = grid::view_with_outputs(
        app.project(),
        &app.snapshot,
        grid::ViewState {
            selected: app.selection.clip,
            ghost: app.selection.ghost_clip,
            owner: app.selection.owner,
            cell: app.selection.cell,
            selected_scene: app.selection.scene,
            selected_scenes: &app.selection.scenes.members,
            selected_tracks: &app.selection.tracks,
            shift_down: app.selection.shift_down,
            scene_stop: app.scene_stop_feedback.rows(
                app.project(),
                &app.snapshot,
                std::time::Instant::now(),
            ),
            blink_on: grid::blink_phase(app.tick_count),
            revision: app.project_revision(),
        },
        &app.grid_layout,
        crate::outputs::width(app),
        grid::Extras {
            header_gestures: app.header_gestures.clone(),
            map_mode: app.map_mode.on,
            rows: app.chrome.rows(),
            device_geometry: Some(app.browser.geometry.clone()),
            device_drag: app.device_drag.as_ref(),
            band_cell: Some(crate::strip_rows::cells(app)),
            rename: crate::clip_actions::rename_editor(app),
            name_editor: crate::header_actions::rename_editor(app),
            focused: app.control_focus.current(),
            scene_viewport: Some(app.scene_follow.viewport()),
        },
    );
    let detail = app.chrome.detail_open.then(|| detail_view(app));
    let window = crate::control::map_layer::map_layer(
        chrome::view(app, grid, detail),
        app.map_mode.on,
        app.theme,
        crate::map_mode::ink(app),
        crate::map_mode::marks(app),
        |control| Message::Map(crate::map_mode::Action::Pick(control)),
    );
    let window = crate::browser::ghost(app, crate::overlay::view(app, window));
    crate::control::key_gate::key_gate(
        window,
        |key, modified_key, modifiers, pressed| {
            crate::map_mode::keys::captures(app, key, modified_key, modifiers, pressed)
        },
        Message::MappedKey,
    )
}

pub(crate) fn detail_view(app: &App) -> Element<'_, Message> {
    let layout = match app.selection.inspector_tab {
        inspector_panel::Tab::Clip => inspector::view(
            app.project(),
            app.selection.clip.map(|s| (s.track, s.slot)),
            &app.drafts.values,
            &app.drafts.errors,
            (&app.clip_view, &app.detail_layout),
            app.controls(),
        )
        .map(Message::Inspector),
        inspector_panel::Tab::Scene => inspector::scene_view(
            app.project(),
            app.selection.scene,
            &app.drafts.values,
            &app.drafts.errors,
            &app.detail_layout,
            app.controls(),
        )
        .map(Message::Inspector),
        inspector_panel::Tab::Track => crate::track_detail::view(app),
        inspector_panel::Tab::Output => crate::output_detail::view(app),
        inspector_panel::Tab::Main => crate::main_detail::view(app),
    };
    layout.view(Message::Info)
}

/// Draft key for the Clip name field of one slot.
pub(crate) fn clip_name_key(track: TrackId, slot: usize) -> DraftKey {
    inspector::field_key(track, slot, ClipField::Name)
}

/// What a name field shows: the typed draft when one exists (even an empty one, so the
/// field can be cleared and retyped), otherwise the model's current name.
pub(crate) fn name_field_value(
    drafts: &std::collections::HashMap<DraftKey, String>,
    key: &DraftKey,
    model_name: &str,
) -> String {
    drafts
        .get(key)
        .cloned()
        .unwrap_or_else(|| model_name.to_owned())
}
