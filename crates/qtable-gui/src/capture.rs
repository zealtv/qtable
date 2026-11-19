//! Capture hooks for unattended screenshots (`tools/gui-capture.sh`) and the tests that share
//! their fixtures. Everything here exists only with the `dev-hooks` feature (and in test builds);
//! a release build has none of it and reads none of these variables.
//!
//! One entry point sets the app up at boot ([`apply`]), one runs every tick ([`on_tick`]), and
//! [`window_size`] and [`fixture_project`] choose the window and the starting set. The setup for
//! each area sits beside the code whose private items it needs (`<area>::capture`); this module
//! calls them in order. Code that must consult a hook while drawing or building state asks a named
//! predicate below, which is `false` or `None` without the feature.
#![cfg_attr(not(any(test, feature = "dev-hooks")), allow(dead_code))]

#[cfg(any(test, feature = "dev-hooks"))]
use crate::drafts::MixerField;
#[cfg(any(test, feature = "dev-hooks"))]
use crate::*;

/// The value of a capture variable, if set and valid text.
#[cfg(any(test, feature = "dev-hooks"))]
fn var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Whether a capture variable is set at all.
#[cfg(any(test, feature = "dev-hooks"))]
fn is_set(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

// Named predicates for hooks that sit in drawing or state code.

/// `QTABLE_CAPTURE_ALL_OUTPUTS`: show every output column, not just the capped number.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn all_outputs() -> bool {
    is_set("QTABLE_CAPTURE_ALL_OUTPUTS")
}

/// `QTABLE_CAPTURE_BRACE=drag`: draw the loop brace's body dragged a beat right.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn brace_drag() -> bool {
    var("QTABLE_CAPTURE_BRACE").as_deref() == Some("drag")
}

/// `QTABLE_CAPTURE_LANE_TRAITS=drag`: draw the lane list mid-drag.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn lane_drag() -> bool {
    var("QTABLE_CAPTURE_LANE_TRAITS").as_deref() == Some("drag")
}

/// `QTABLE_CAPTURE_HOVER=segment`: place the pointer on an envelope segment's band.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn hover_segment() -> bool {
    var("QTABLE_CAPTURE_HOVER").as_deref() == Some("segment")
}

/// `QTABLE_CAPTURE_SESSION`: the Session header scenario to replay.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn session() -> Option<String> {
    var("QTABLE_CAPTURE_SESSION")
}

#[cfg(not(any(test, feature = "dev-hooks")))]
pub(crate) fn all_outputs() -> bool {
    false
}

#[cfg(not(any(test, feature = "dev-hooks")))]
pub(crate) fn brace_drag() -> bool {
    false
}

#[cfg(not(any(test, feature = "dev-hooks")))]
pub(crate) fn lane_drag() -> bool {
    false
}

#[cfg(not(any(test, feature = "dev-hooks")))]
pub(crate) fn hover_segment() -> bool {
    false
}

#[cfg(not(any(test, feature = "dev-hooks")))]
pub(crate) fn session() -> Option<String> {
    None
}

/// The window size: `QTABLE_CAPTURE_WINDOW=<width>x<height>` in points, else the layout the
/// capture asks for, else `None` for the ordinary default.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn window_size() -> Option<iced::Size> {
    if let Some(text) = var("QTABLE_CAPTURE_WINDOW")
        && let Some((width, height)) = text.split_once('x')
        && let (Ok(width), Ok(height)) = (width.trim().parse(), height.trim().parse())
    {
        return Some(iced::Size::new(width, height));
    }
    // The hit-argument and lane captures match the maintainer's window (about 900 pt tall) so rows wrap.
    (is_set("QTABLE_CAPTURE_HIT_ARGUMENTS")
        || is_set("QTABLE_CAPTURE_LANES")
        || is_set("QTABLE_CAPTURE_MARKERS"))
    .then(|| iced::Size::new(1100.0, 900.0))
}

#[cfg(not(any(test, feature = "dev-hooks")))]
pub(crate) fn window_size() -> Option<iced::Size> {
    None
}

/// The starting set a capture wants when no project path was given.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn fixture_project() -> Option<qtable_engine::model::Project> {
    if var("QTABLE_CAPTURE_CLIP").as_deref() == Some("route-dropdown") {
        Some(output_band::capture_project())
    } else if track_clipboard::capture_wants_fixture() {
        Some(track_clipboard::capture_project())
    } else {
        None
    }
}

#[cfg(not(any(test, feature = "dev-hooks")))]
pub(crate) fn fixture_project() -> Option<qtable_engine::model::Project> {
    None
}

/// Every tick: hold the illustrative state some captures keep through native window activation.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn on_tick(app: &mut crate::app::App) {
    throughput::capture(app);
    device_chain::hold_drag(app);
    browser::hold_capture(app);
    inspector::event_editor::content::hold_marker_capture(app);
}

#[cfg(not(any(test, feature = "dev-hooks")))]
pub(crate) fn on_tick(_app: &mut crate::app::App) {}

/// The one boot entry: set the freshly built app up as the capture environment asks, and return
/// the startup task (focus and scroll operations) that goes with it.
#[cfg(not(any(test, feature = "dev-hooks")))]
pub(crate) fn apply(
    app: crate::app::App,
) -> (crate::app::App, iced::Task<crate::messages::Message>) {
    (app, iced::Task::none())
}

#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn apply(
    mut app: crate::app::App,
) -> (crate::app::App, iced::Task<crate::messages::Message>) {
    let mut capture_task = iced::Task::none();
    if std::env::var_os("QTABLE_CAPTURE_SETTINGS").is_some() {
        app.open_overlay(overlay::Overlay::Settings(settings_pane::Tab::Midi));
    }
    if std::env::var_os("QTABLE_CAPTURE_HIT_ARGUMENTS").is_some() {
        app.dividers.capture_hit_arguments();
    }
    if std::env::var_os("QTABLE_CAPTURE_LANES").is_some()
        || std::env::var_os("QTABLE_CAPTURE_MARKERS").is_some()
    {
        app.dividers.capture_lanes();
    }
    if let Some(width) = std::env::var("QTABLE_CAPTURE_BROWSER_WIDTH")
        .ok()
        .and_then(|width| width.parse().ok())
    {
        app.dividers.capture_browser_width(width);
    }
    if app.chrome.start_with_sections_menu {
        app.open_overlay(overlay::Overlay::SectionsMenu);
    }
    // The unattended capture script can reveal a clip without desktop input.
    if std::env::var_os("QTABLE_CAPTURE_CLIP").is_some() {
        let selected = app.project().tracks.iter().find_map(|track| {
            track
                .slots
                .iter()
                .position(|slot| slot.clip.is_some())
                .map(|slot| grid::Selection {
                    track: track.id,
                    slot,
                })
        });
        if let Some(selected) = selected {
            let _ = update(
                &mut app,
                Message::Grid(grid::Action::Select(selected.track, selected.slot)),
            );
        }
        inspector::event_editor::reconcile(&mut app);
        if let Some(selected) = selected
            && let Some(clip) =
                inspector::clip_panel_update::clip(app.project(), selected.track, selected.slot)
            && !clip.events.is_empty()
        {
            let key = inspector::event_editor::ClipKey {
                track: selected.track,
                slot: selected.slot,
                id: clip.id,
                epoch: app.clip_view.editor.epoch,
            };
            let clip = clip.clone();
            // The second hit sits on the absolute lane of the paths fixture.
            let hit =
                usize::from(std::env::var("QTABLE_CAPTURE_CLIP").as_deref() == Ok("absolute-lane"));
            app.clip_view.editor.select_hit(&clip, key, Some(hit));
            // Capture-only: show the Info View on that hit's lane.
            if std::env::var("QTABLE_CAPTURE_INFO").as_deref() == Ok("lane")
                && let Some(scene) = app.project().scenes.get(selected.slot).map(|s| s.id)
            {
                let subject = info::Subject::Lane {
                    track: selected.track,
                    scene,
                    event: clip.events[hit].definition.clone(),
                };
                app.info.show(info::Id::TargetEntryPath, subject);
            }
        }
        if std::env::var("QTABLE_CAPTURE_CLIP").as_deref() == Ok("launch") {
            app.detail_layout
                .toggle(inspector_panel::Tab::Clip, detail_layout::CLIP);
        }
        context_menu::capture(&mut app);
        capture_task = output_band::capture(&mut app);
    }
    // Capture-only: give a track, a group, an output and Main a path; "error" also
    // leaves a rejected draft on the first track.
    if let Ok(mode) = std::env::var("QTABLE_CAPTURE_PATHS") {
        if app.project().outputs.is_empty() {
            let _ = update(
                &mut app,
                Message::Outputs(crate::messages::OutputMessage::AddOutput),
            );
        }
        let ids = [
            (app.project().clip_tracks().next().map(|t| t.id), "/kick"),
            (app.project().group_tracks().next().map(|t| t.id), "/rhythm"),
            (app.project().outputs.first().map(|o| o.id), "/show"),
            (Some(app.project().main.id), "/main"),
        ];
        for (id, path) in ids {
            if let Some(id) = id {
                let _ = update(
                    &mut app,
                    Message::Edit(crate::messages::EditMessage::PathSubmitted(id, path.into())),
                );
            }
        }
        let first = app.project().clip_tracks().next().map(|t| t.id);
        if mode == "error"
            && let Some(id) = first
        {
            use crate::messages::EditMessage;
            for message in [
                EditMessage::PathChanged(id, "/bad/".into()),
                EditMessage::PathSubmitted(id, "/bad/".into()),
            ] {
                let _ = update(&mut app, Message::Edit(message));
            }
        }
    }
    if std::env::var_os("QTABLE_CAPTURE_SCENE").is_some() {
        app.selection.clip = None;
        app.selection.scene = app.project().scenes.first().map(|scene| scene.id);
        app.selection.inspector_tab = inspector_panel::Tab::Scene;
    }
    // The lanes capture plays the clip it selected even when QTABLE_CAPTURE_TRACK then
    // selects the track, so the device chain shows automation marks.
    let lanes_clip = app.selection.clip;
    if std::env::var_os("QTABLE_CAPTURE_TRACK").is_some() {
        app.selection.clip = None;
        app.selection.scene = None;
        let first = if std::env::var("QTABLE_CAPTURE_TRACK").as_deref() == Ok("group") {
            app.project().group_tracks().next().map(|t| t.id)
        } else {
            app.project().clip_tracks().next().map(|t| t.id)
        };
        if let Some(track) = first {
            let action = if std::env::var("QTABLE_CAPTURE_TRACK").as_deref() == Ok("group") {
                grid::Action::SelectGroup(track, false)
            } else {
                grid::Action::SelectTrack(track, false)
            };
            let _ = update(&mut app, Message::Grid(action));
            // Capture the group's own Detail View, rather than its first member.
            // SelectGroup keeps the explicit header selected for the Session grid.
            app.selection.anchor = Some(track);
        }
        app.selection.inspector_tab = inspector_panel::Tab::Track;
        if std::env::var("QTABLE_CAPTURE_TRACK").as_deref() == Ok("single-group")
            || std::env::var("QTABLE_CAPTURE_TRACK").as_deref() == Ok("single-member")
        {
            let _ = update(
                &mut app,
                Message::Outputs(crate::messages::OutputMessage::CreateGroupAroundSelected),
            );
            if std::env::var("QTABLE_CAPTURE_TRACK").as_deref() == Ok("single-member")
                && let Some(member) = first
            {
                let _ = update(
                    &mut app,
                    Message::Grid(grid::Action::SelectTrack(member, false)),
                );
            }
        }
    }
    if std::env::var_os("QTABLE_CAPTURE_OUTPUT").is_some() {
        app.selection.clip = None;
        app.selection.scene = None;
        let first = app.project().outputs.first().map(|o| o.id);
        app.selection.tracks = first.into_iter().collect();
        app.selection.anchor = first;
        app.selection.inspector_tab = inspector_panel::Tab::Output;
    }
    track_clipboard::capture(&mut app);
    if std::env::var_os("QTABLE_CAPTURE_MAIN").is_some() {
        let main = app.project().main.id;
        let _ = update(
            &mut app,
            Message::Grid(grid::Action::SelectColumn(
                selection::ColumnOwner::Track(main),
                None,
            )),
        );
    }
    // Exercise the ordinary cell-selection messages without desktop input.
    if let Ok(kind) = std::env::var("QTABLE_CAPTURE_COLUMN_CELL") {
        let row = 1;
        let action = match kind.as_str() {
            "track" => app
                .project()
                .clip_tracks()
                .next()
                .map(|t| grid::Action::Select(t.id, row)),
            "group" => app.project().group_tracks().next().map(|t| {
                grid::Action::SelectColumn(selection::ColumnOwner::Group(t.id), Some(row))
            }),
            "output" => app.project().outputs.first().map(|t| {
                grid::Action::SelectColumn(selection::ColumnOwner::Track(t.id), Some(row))
            }),
            "main" | "middle" => app
                .project()
                .scenes
                .get(row)
                .map(|s| grid::Action::SelectScene(s.id)),
            _ => None,
        };
        if let Some(action) = action {
            let _ = update(&mut app, Message::Grid(action));
        }
    }
    if std::env::var_os("QTABLE_CAPTURE_MIXER").is_some() {
        for tab in [
            inspector_panel::Tab::Track,
            inspector_panel::Tab::Output,
            inspector_panel::Tab::Main,
        ] {
            app.detail_layout.toggle(tab, detail_layout::OUTPUT);
        }
    }
    // Capture the real routing reminder without desktop input or a transient timer.
    if std::env::var("QTABLE_CAPTURE_STATUS").as_deref() == Ok("notice") {
        use crate::messages::OutputMessage;
        if app.project().outputs.is_empty() {
            let _ = update(&mut app, Message::Outputs(OutputMessage::AddOutput));
        }
        if let Some(output) = app.project().outputs.first()
            && output.mixer.active
        {
            let id = output.id;
            let _ = update(
                &mut app,
                Message::Outputs(OutputMessage::ToggleOutputEnabled(id)),
            );
        }
    }
    // Capture ⌘Z and ⇧⌘Z through the ordinary key message, with no desktop input: rename
    // the first track, undo it, and for `redo` redo it. The status bar names the step.
    let first_track = app.project().clip_tracks().next().map(|t| t.id);
    if let Ok(kind) = std::env::var("QTABLE_CAPTURE_UNDO")
        && let Some(track) = first_track
    {
        let _ = crate::edit_actions::run_edit(
            &mut app,
            qtable_engine::engine::Command::Edit(qtable_engine::edit::ProjectEdit::RenameTrack {
                track,
                name: "Edited".into(),
            }),
        );
        let key = |modifiers| Message::KeyPressed {
            key: iced::keyboard::Key::Character("z".into()),
            modifiers,
            repeat: false,
            ignored: true,
        };
        let _ = update(&mut app, key(iced::keyboard::Modifiers::COMMAND));
        if kind == "redo" {
            let shifted = iced::keyboard::Modifiers::COMMAND | iced::keyboard::Modifiers::SHIFT;
            let _ = update(&mut app, key(shifted));
        }
    }
    // Capture ⌘, ⌘S and ⇧⌘S through the ordinary key message, with no desktop input.
    // `settings` opens the pane; `settings-typing` first types a tempo, which the key
    // commits; `save-rejected` types a tempo the engine refuses, then presses ⌘S (the
    // status bar shows the error and nothing saves); `save-as` opens the save dialog.
    // There is no plain `save`: ⌘S on a project with a path writes that file.
    if let Ok(kind) = std::env::var("QTABLE_CAPTURE_SHORTCUT") {
        let chord = |character: &str, modifiers| Message::KeyPressed {
            key: iced::keyboard::Key::Character(character.into()),
            modifiers,
            repeat: false,
            ignored: true,
        };
        let command = iced::keyboard::Modifiers::COMMAND;
        let message = match kind.as_str() {
            "settings" | "settings-typing" => chord(",", command),
            "save-rejected" => chord("s", command),
            "save-as" => chord("S", command | iced::keyboard::Modifiers::SHIFT),
            _ => Message::Tick,
        };
        match kind.as_str() {
            "settings-typing" => {
                let _ = update(&mut app, Message::TempoChanged("93".into()));
            }
            "save-rejected" => {
                let _ = update(&mut app, Message::TempoChanged("fast".into()));
            }
            _ => {}
        }
        capture_task = capture_task.chain(update(&mut app, message));
    }
    // Capture-only: show the Info View on the Volume Path of the first track.
    let info_track = app.project().clip_tracks().next().map(|t| t.id);
    if std::env::var("QTABLE_CAPTURE_INFO").as_deref() == Ok("volume-path")
        && let Some(track) = info_track
    {
        app.info
            .show(info::Id::VolumePath, info::Subject::Volume(track));
    }
    if std::env::var_os("QTABLE_CAPTURE_FOCUS_PATH").is_some()
        && let Some(track) = info_track
    {
        capture_task = capture_task.chain(iced::widget::operation::focus(
            drafts::DraftKey::Mixer(track, MixerField::Address).input_id(),
        ));
    }
    // Capture-only: focus Mixer controls without taking desktop input from the user.
    if let Ok(face) = std::env::var("QTABLE_CAPTURE_FOCUS_VOLUME")
        && let Some(track) = info_track
    {
        if !app
            .project()
            .track(track)
            .unwrap()
            .mixer_device
            .volume
            .output
            .enabled
        {
            let _ = update(
                &mut app,
                Message::Edit(messages::EditMessage::MixerDevice(
                    mixer_device::MixerMessage::Toggle(track),
                )),
            );
        }
        if face == "device" {
            capture_task = capture_task.chain(iced::widget::operation::focus(
                drafts::DraftKey::Mixer(track, MixerField::Volume).input_id(),
            ));
        } else if face == "strip" {
            let _ = update(
                &mut app,
                Message::ControlFocused(drafts::ControlId::action(
                    control_focus::View::Mixer,
                    "volume",
                    control_focus::ControlOwner::Track(track),
                )),
            );
        }
    }
    if std::env::var_os("QTABLE_CAPTURE_PLAY").is_some() {
        let _ = update(&mut app, Message::Play);
    }
    // Capture-only: play the mixed-lanes fixture's clip and drag its Mixer Volume, so
    // the engine reports that argument overridden and its lane shows a grey dot.
    if std::env::var_os("QTABLE_CAPTURE_LANES").is_some()
        && let Some(selected) = lanes_clip
    {
        let _ = update(&mut app, Message::Play);
        let _ = update(
            &mut app,
            Message::Grid(grid::Action::LaunchClip(selected.track, selected.slot)),
        );
        // The launch waits for its quantisation; once Volume follows its envelope,
        // change it by hand so that lane shows the overridden (grey) look.
        let volume = mixer_device::volume_field(selected.track);
        for _ in 0..60 {
            app.snapshot = app.engine.snapshot();
            if app.snapshot.automation.iter().any(|a| {
                a.field == volume && a.state == qtable_engine::scheduler::AutomationState::Following
            }) {
                edit_actions::execute(
                    &mut app,
                    qtable_engine::engine::Command::Edit(
                        qtable_engine::edit::ProjectEdit::SetParameter {
                            target: volume.parameter.clone(),
                            values: vec![(volume.field.clone(), 0.4)],
                        },
                    ),
                );
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    if let Ok(scenario) = std::env::var("QTABLE_CAPTURE_VIEW") {
        // The track capture cleared the selection; this menu shows it kept.
        if scenario == "menu-enabled" {
            app.selection.clip = lanes_clip;
        }
        inspector::lane_view::capture(&mut app, &scenario);
    }
    if let Ok(window) = std::env::var("QTABLE_CAPTURE_ZOOM") {
        inspector::event_editor::capture_zoom(&mut app, &window);
    }
    if let Ok(scenario) = std::env::var("QTABLE_CAPTURE_POINTS") {
        inspector::event_editor::capture_points(&mut app, &scenario);
    }
    if let Ok(scenario) = std::env::var("QTABLE_CAPTURE_LANE_TRAITS") {
        inspector::event_editor::lanes::capture(&mut app, &scenario);
    }
    if let Ok(scenario) = std::env::var("QTABLE_CAPTURE_DRAW") {
        inspector::event_editor::drawing::capture(&mut app, &scenario);
    }
    if std::env::var_os("QTABLE_CAPTURE_CONTENT").is_some() {
        inspector::event_editor::content::capture(&mut app);
    }
    if let Ok(scenario) = std::env::var("QTABLE_CAPTURE_MARKERS") {
        inspector::event_editor::content::capture_markers(&mut app, &scenario);
    }
    if let Ok(scenario) = std::env::var("QTABLE_CAPTURE_MAP") {
        map_mode::capture(&mut app, &scenario);
    }
    throughput::capture(&mut app);
    // Unattended musical-field captures use the ordinary focus operation.
    if let Ok(field) = std::env::var("QTABLE_CAPTURE_FOCUS_TIME") {
        let key = if field == "scene" {
            app.project()
                .scenes
                .first()
                .map(|s| drafts::DraftKey::scene(s.id, drafts::SceneField::FollowTime))
        } else {
            app.selection.clip.map(|clip| {
                drafts::DraftKey::clip(
                    clip.track,
                    clip.slot,
                    match field.as_str() {
                        "point" => drafts::ClipField::PointPosition,
                        "region" => drafts::ClipField::LaneMarker(
                            inspector::lane_view::region::Field::Start,
                        ),
                        "start" => drafts::ClipField::Start,
                        _ => drafts::ClipField::Length,
                    },
                )
            })
        };
        if let Some(key) = key {
            app.control_focus.claim(drafts::ControlId::field(
                if field == "scene" {
                    control_focus::View::Scene
                } else {
                    control_focus::View::Clip
                },
                key,
            ));
            capture_task = capture_task.chain(iced::widget::operation::focus(key.input_id()));
        }
    }
    // QTABLE_CAPTURE_SCROLL=<points> scrolls the Events/Overview lane list down that far
    // (the ordinary scroll operation; the list clamps it to its end).
    if let Some(y) = std::env::var("QTABLE_CAPTURE_SCROLL")
        .ok()
        .and_then(|y| y.parse::<f32>().ok())
    {
        capture_task = capture_task.chain(iced::widget::operation::scroll_to(
            iced::widget::Id::new(inspector::LANE_SCROLL),
            iced::widget::scrollable::AbsoluteOffset {
                x: None,
                y: Some(y),
            },
        ));
    }
    device_chain::capture(&mut app);
    browser::capture(&mut app);
    // Native Session header captures use the ordinary update and scroll paths.
    if let Ok(scenario) = std::env::var("QTABLE_CAPTURE_SESSION") {
        grid::header::capture_session(&mut app, &scenario);
        let folds = match scenario.as_str() {
            "outer-fold" => 1,
            "outer-reopen" => 2,
            _ => 0,
        };
        let group = app.project().group_tracks().next().map(|group| group.id);
        if let Some(group) = group {
            if scenario == "group-slot" {
                let _ = update(
                    &mut app,
                    Message::Grid(grid::Action::SelectColumn(
                        selection::ColumnOwner::Group(group),
                        Some(1),
                    )),
                );
            }
            for _ in 0..folds {
                let _ = update(&mut app, Message::Grid(grid::Action::ToggleGroup(group)));
            }
        }
        if scenario == "scene-scroll-control" {
            if let Some(scene) = app.project().scenes.get(1).map(|s| s.id) {
                let _ = update(&mut app, Message::Grid(grid::Action::SelectScene(scene)));
            }
            capture_task = capture_task.chain(grid::scroll_scenes(189.));
        }
        if scenario == "scroll" {
            capture_task = capture_task.chain(grid::scroll_columns(
                qtable_engine::model::Region::Tracks,
                false,
                315,
            ));
        }
    }
    (app, capture_task)
}
