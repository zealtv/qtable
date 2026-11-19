use super::*;
use crate::drafts::MixerField;
use crate::selection::ColumnOwner;
use iced::advanced::{Layout, Shell, layout, widget::Tree};
use iced::{Event, Rectangle, Size, mouse};
use qtable_engine::engine::Engine;
use qtable_engine::model::{BeatTime, NewDevice};

fn app() -> App {
    let mut app = App::new(
        Engine::new(crate::files::new_project()).unwrap(),
        "120".into(),
        None,
    );
    let track = app.project().tracks[0].id;
    send(
        &mut app,
        Message::Grid(crate::grid::Action::SelectTrack(track, false)),
    );
    for _ in 0..3 {
        crate::edit_actions::run_edit(
            &mut app,
            Command::Edit(ProjectEdit::AddDevice {
                track,
                kind: NewDevice::Target,
                before: None,
            }),
        );
    }
    reconcile(&mut app);
    app
}

fn send(app: &mut App, message: Message) {
    let _ = crate::update::update(app, message);
}

fn action(app: &mut App, action: Action) {
    send(app, Message::Device(action));
}

fn ids(app: &App) -> Vec<DeviceId> {
    app.project()
        .track(shown_track(app).unwrap())
        .unwrap()
        .devices
        .iter()
        .map(|d| d.id)
        .collect()
}

/// An empty Target expanded: its 126 px face, a 1 px divider and the shared editor width.
const EXPANDED_EMPTY: f32 = 126.0 + 1.0 + target_face::EDITOR_WIDTH;

#[test]
fn device_selection_uses_ranges_and_toggles_and_clears_on_track_change() {
    let mut app = app();
    let ids = ids(&app);
    action(&mut app, Action::Select(ids[0]));
    app.selection.shift_down = true;
    action(&mut app, Action::Select(ids[2]));
    assert_eq!(app.selection.devices, ids);
    app.selection.shift_down = false;
    app.selection.command_down = true;
    action(&mut app, Action::Select(ids[1]));
    assert_eq!(app.selection.devices, [ids[0], ids[2]]);
    action(&mut app, Action::Select(ids[1]));
    assert!(app.selection.devices.contains(&ids[1]));
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::AddTrack {
            name: "Other".into(),
        }),
    );
    let track = app.project().tracks[1].id;
    send(
        &mut app,
        Message::Grid(crate::grid::Action::SelectTrack(track, false)),
    );
    assert!(app.selection.devices.is_empty());
    assert!(app.selection.device_anchor.is_none());
}

#[test]
fn device_keys_submit_one_edit_and_select_copies_in_chain_order() {
    let mut app = app();
    let original = ids(&app);
    action(&mut app, Action::Select(original[1]));
    let revision = app.project_revision();
    send(&mut app, Message::ClipShortcut(Shortcut::Duplicate));
    assert_eq!(app.project_revision(), revision + 1);
    let duplicate = app.selection.devices[0];
    assert_eq!(
        ids(&app),
        [original[0], original[1], duplicate, original[2]]
    );
    send(&mut app, Message::ClipShortcut(Shortcut::Copy));
    send(&mut app, Message::ClipShortcut(Shortcut::Paste));
    assert_eq!(app.project_revision(), revision + 2);
    let pasted = app.selection.devices[0];
    assert_eq!(
        ids(&app),
        [original[0], original[1], duplicate, pasted, original[2]]
    );
    app.selection.devices = vec![duplicate, pasted];
    send(&mut app, Message::ClipShortcut(Shortcut::Delete));
    assert_eq!(app.project_revision(), revision + 3);
    assert_eq!(ids(&app), original);
    assert!(app.selection.devices.is_empty());
    assert_eq!(app.status.severity(), crate::status::Severity::Notice);
}

#[test]
fn rename_and_pending_text_keep_precedence_and_escape_cancels() {
    let mut app = app();
    let id = ids(&app)[0];
    action(&mut app, Action::Select(id));
    send(&mut app, Message::ClipShortcut(Shortcut::Rename));
    assert_eq!(app.device_chain.renaming, Some(id));
    action(&mut app, Action::Draft("Receiver".into()));
    let revision = app.project_revision();
    for shortcut in [
        Shortcut::Delete,
        Shortcut::Copy,
        Shortcut::Duplicate,
        Shortcut::Paste,
    ] {
        send(&mut app, Message::ClipShortcut(shortcut));
    }
    assert_eq!(app.project_revision(), revision);
    send(&mut app, Message::CancelField);
    assert_eq!(app.project().device(id).unwrap().1.name, "Target");
    assert!(app.device_chain.renaming.is_none());
    send(&mut app, Message::ClipShortcut(Shortcut::Rename));
    action(&mut app, Action::Draft("Receiver".into()));
    action(&mut app, Action::CommitRename);
    assert_eq!(app.project().device(id).unwrap().1.name, "Receiver");
    assert_eq!(app.project_revision(), revision + 1);
    app.pending_field = Some(crate::drafts::DraftKey::Mixer(
        app.project().tracks[0].id,
        MixerField::Volume,
    ));
    send(&mut app, Message::ClipShortcut(Shortcut::Delete));
    assert!(app.project().device(id).is_some());
}

#[test]
fn device_clipboard_survives_replacement_and_pastes_on_grid_columns() {
    let mut app = app();
    let id = ids(&app)[0];
    action(&mut app, Action::Select(id));
    send(&mut app, Message::ClipShortcut(Shortcut::Copy));
    app.mark_saved();
    send(&mut app, Message::Files(crate::messages::FileMessage::New));
    assert!(matches!(
        app.clip_actions.clipboard,
        Some(GridClipboard::Devices(_))
    ));
    let track = app.project().tracks[0].id;
    send(
        &mut app,
        Message::Grid(crate::grid::Action::SelectTrack(track, false)),
    );
    send(&mut app, Message::ClipShortcut(Shortcut::Paste));
    assert_eq!(app.project().track(track).unwrap().devices.len(), 1);
    let main = app.project().main.id;
    send(
        &mut app,
        Message::Grid(crate::grid::Action::SelectColumn(
            ColumnOwner::Track(main),
            None,
        )),
    );
    assert!(can_paste_grid(&app));
    send(&mut app, Message::ClipShortcut(Shortcut::Paste));
    assert!(app.project().main.devices.is_empty());
    assert!(
        app.status
            .message()
            .unwrap()
            .contains("Targets require a Track")
    );
    send(
        &mut app,
        Message::Outputs(crate::messages::OutputMessage::AddOutput),
    );
    let output = app.project().outputs[0].id;
    send(
        &mut app,
        Message::Grid(crate::grid::Action::SelectColumn(
            ColumnOwner::Track(output),
            None,
        )),
    );
    send(&mut app, Message::ClipShortcut(Shortcut::Paste));
    assert!(app.project().track(output).unwrap().devices.is_empty());
    assert!(
        app.status
            .message()
            .unwrap()
            .contains("Targets require a Track")
    );
}

#[test]
fn menu_uses_one_overlay_and_closes_when_device_disappears() {
    let mut app = app();
    let id = ids(&app)[0];
    action(&mut app, Action::Menu(id, Point::new(50.0, 70.0)));
    assert_eq!(
        app.menu_target(),
        Some(crate::context_menu::MenuTarget::Device(id))
    );
    let labels: Vec<_> = menu_entries(&app)
        .iter()
        .filter_map(|e| match e {
            crate::context_menu::Entry::Command { label, .. } => Some(*label),
            _ => None,
        })
        .collect();
    assert_eq!(labels, ["Rename", "Copy", "Paste", "Duplicate", "Delete"]);
    app.engine
        .execute(Command::Edit(ProjectEdit::RemoveDevices {
            devices: vec![id],
        }))
        .unwrap();
    send(&mut app, Message::Tick);
    assert!(app.overlay.is_none());
    assert!(app.selection.devices.is_empty());
}

#[test]
fn delete_notice_uses_the_engine_report_for_dependent_lanes_and_hits() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::CreateClip {
            track,
            slot: 0,
            length: BeatTime(3840),
        }),
    );
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::EditLane {
            track,
            slot: 0,
            edit: qtable_engine::edit::LaneEdit::AddPath("/cue".into()),
        }),
    );
    let id = ids(&app)[0];
    action(&mut app, Action::Select(id));
    send(&mut app, Message::ClipShortcut(Shortcut::Delete));
    assert_eq!(
        app.status.message(),
        Some("Removed Target and 1 lane in 1 clip")
    );
    assert_eq!(app.status.severity(), crate::status::Severity::Notice);
}

#[test]
fn reorder_drag_commits_once_and_cancel_or_click_commits_nothing() {
    let mut app = app();
    let ids = ids(&app);
    let revision = app.project_revision();
    action(&mut app, Action::Begin(ids[0], Point::ORIGIN));
    action(&mut app, Action::Over(Some(None)));
    action(&mut app, Action::Finish);
    assert_eq!(app.project_revision(), revision);
    action(&mut app, Action::Begin(ids[0], Point::ORIGIN));
    send(&mut app, Message::CursorMoved(Point::new(1000.0, 0.0)));
    action(&mut app, Action::Over(Some(None)));
    action(&mut app, Action::Finish);
    assert_eq!(app.project_revision(), revision + 1);
    assert_eq!(self::ids(&app), [ids[1], ids[2], ids[0]]);
    action(&mut app, Action::Begin(ids[1], Point::ORIGIN));
    send(&mut app, Message::CursorMoved(Point::new(30.0, 0.0)));
    action(&mut app, Action::Over(Some(None)));
    send(&mut app, Message::CancelField);
    action(&mut app, Action::Finish);
    assert_eq!(app.project_revision(), revision + 1);
    action(&mut app, Action::Begin(ids[1], Point::ORIGIN));
    send(
        &mut app,
        Message::ToggleRegion(crate::chrome::Region::Detail),
    );
    assert!(app.device_chain.drag.is_none());
    assert_eq!(app.project_revision(), revision + 1);
}

#[test]
fn chain_pointer_host_hit_tests_titles_bodies_and_clipped_devices() {
    let app = app();
    let track = &app.project().tracks[0];
    let renderer = crate::grid::test_renderer();
    let mut widget = host::region(
        row(track
            .devices
            .iter()
            .map(|d| frame::device(&app, d, crate::control_focus::View::Track)))
        .spacing(4)
        .height(Length::Fill),
        track,
        &app.device_chain,
        Point::new(20.0, 30.0),
    );
    let mut tree = Tree::new(&widget);
    let node = widget.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(1800.0, 200.0)),
    );
    for (point, title) in [
        (Point::new(30.0, 10.0), true),
        (Point::new(600.0, 100.0), false),
    ] {
        let mut messages = vec![];
        widget.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(Size::new(1800.0, 200.0)),
        );
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, Message::Device(Action::Begin(..))) == title
                    && matches!(m, Message::Device(Action::Begin(..) | Action::Select(_))))
        );
    }
    let mut messages = vec![];
    widget.as_widget_mut().update(
        &mut tree,
        &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
        Layout::new(&node),
        mouse::Cursor::Available(Point::new(600.0, 10.0)),
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut Shell::new(&mut messages),
        &Rectangle::with_size(Size::new(300.0, 200.0)),
    );
    assert!(messages.is_empty());
    assert_eq!(host::insertion_index(track, track.devices[0].id, None), 3);
}

#[test]
fn rename_blur_checks_ignore_stale_replies_and_commit_current_drafts() {
    let mut app = app();
    let id = ids(&app)[0];
    action(&mut app, Action::Select(id));
    send(&mut app, Message::ClipShortcut(Shortcut::Rename));
    action(&mut app, Action::Draft("Receiver".into()));
    let generation = app.project_generation;
    let revision = app.project_revision();
    action(
        &mut app,
        Action::RenameFocusChecked(id, "Target".into(), generation, false),
    );
    action(
        &mut app,
        Action::RenameFocusChecked(id, "Receiver".into(), generation + 1, false),
    );
    action(
        &mut app,
        Action::RenameFocusChecked(id, "Receiver".into(), generation, true),
    );
    assert_eq!(app.project_revision(), revision);
    assert_eq!(app.device_chain.renaming, Some(id));
    action(
        &mut app,
        Action::RenameFocusChecked(id, "Receiver".into(), generation, false),
    );
    assert_eq!(app.project_revision(), revision + 1);
    assert_eq!(app.project().device(id).unwrap().1.name, "Receiver");
    assert!(app.device_chain.renaming.is_none());
}

#[test]
fn a_rename_committed_by_losing_focus_is_a_step_apart_from_the_click_that_follows() {
    // No input boundary lies between the blur commit and the next click's
    // edit, so only the commit's own token keeps them two steps.
    let mut app = app();
    let id = ids(&app)[0];
    let track = shown_track(&app).unwrap();
    action(&mut app, Action::Select(id));
    send(&mut app, Message::ClipShortcut(Shortcut::Rename));
    action(&mut app, Action::Draft("Receiver".into()));
    let generation = app.project_generation;
    action(
        &mut app,
        Action::RenameFocusChecked(id, "Receiver".into(), generation, false),
    );
    assert_eq!(app.project().device(id).unwrap().1.name, "Receiver");
    send(
        &mut app,
        Message::Mixer(crate::mixer_band::MixerMessage::Activator(track)),
    );
    let active = |app: &App| app.project().track_facts(track).unwrap().mixer.active;
    let toggled = active(&app);
    app.engine.execute(Command::Undo).unwrap();
    assert_ne!(active(&app), toggled, "the toggle is undone");
    assert_eq!(
        app.project().device(id).unwrap().1.name,
        "Receiver",
        "the rename is a step of its own"
    );
}

#[test]
fn device_selection_survives_a_clip_tab_on_the_same_track() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let id = ids(&app)[0];
    action(&mut app, Action::Select(id));
    send(
        &mut app,
        Message::Grid(crate::grid::Action::Select(track, 0)),
    );
    assert_eq!(app.selection.inspector_tab, Tab::Clip);
    assert_eq!(app.selection.devices, [id]);
    assert!(!shown(&app));
    app.selection.inspector_tab = Tab::Track;
    reconcile(&mut app);
    assert_eq!(app.selection.devices, [id]);
}

#[test]
fn group_header_refuses_targets_and_slots_or_ranges_reject_device_paste() {
    let mut app = app();
    let id = ids(&app)[0];
    action(&mut app, Action::Select(id));
    send(&mut app, Message::ClipShortcut(Shortcut::Copy));
    let first = app.project().tracks[0].id;
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::AddTrack {
            name: "Member".into(),
        }),
    );
    let second = app.project().tracks[1].id;
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::CreateGroup {
            tracks: vec![first, second],
        }),
    );
    let group = app.project().group_tracks().next().unwrap().id;
    send(
        &mut app,
        Message::Grid(crate::grid::Action::SelectGroup(group, false)),
    );
    assert!(can_paste_grid(&app));
    send(&mut app, Message::ClipShortcut(Shortcut::Paste));
    assert!(app.project().track(group).unwrap().devices.is_empty());
    assert!(
        app.status
            .message()
            .unwrap()
            .contains("Targets require a Track")
    );
    send(
        &mut app,
        Message::Grid(crate::grid::Action::Select(first, 0)),
    );
    assert!(!can_paste_grid(&app));
    let revision = app.project_revision();
    send(&mut app, Message::ClipShortcut(Shortcut::Paste));
    assert_eq!(app.project_revision(), revision);
    send(
        &mut app,
        Message::Grid(crate::grid::Action::SelectTrack(first, false)),
    );
    app.selection.tracks = vec![first, second];
    assert!(!can_paste_grid(&app));
}

/// The Mixer frame: the chain's last node, whatever the device count.
fn mixer_frame(node: &layout::Node, devices: usize) -> &layout::Node {
    // Padding container -> (empty: row | scroll -> row) of [devices, Mixer].
    let chain = &node.children()[0].children()[0];
    let row = if devices == 0 {
        chain
    } else {
        &chain.children()[0]
    };
    row.children().last().unwrap()
}

#[test]
fn mixer_is_the_last_device_and_follows_the_chain_in_every_count() {
    let mut app = app();
    let track = app.project().tracks[0].id;
    let renderer = crate::grid::test_renderer();
    for count in [3, 0] {
        if count == 0 {
            let devices = ids(&app);
            crate::edit_actions::run_edit(
                &mut app,
                Command::Edit(ProjectEdit::RemoveDevices { devices }),
            );
        }
        let mut widget = view(&app, app.project().track(track).unwrap(), View::Track);
        let mut tree = Tree::new(&widget);
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(3000.0, 238.0)),
        );
        let chain = &node.children()[0].children()[0];
        let row = if count == 0 {
            chain
        } else {
            &chain.children()[0]
        };
        let mixer = mixer_frame(&node, count);
        assert_eq!(
            mixer.bounds().width,
            crate::mixer_device::width(app.project().track(track).unwrap())
        );
        let before = &row.children()[row.children().len() - 2];
        assert_eq!(
            before.bounds().x + before.bounds().width + detail::CHAIN_GAP,
            mixer.bounds().x,
            "the Mixer follows the last device by one chain gap"
        );
        if count == 3 {
            // Three expanded empty frames and two gaps, with no spacer.
            assert_eq!(
                before.bounds().width,
                3.0 * EXPANDED_EMPTY + 2.0 * detail::CHAIN_GAP
            );
        }
    }
}

#[test]
fn mixer_scrolls_with_a_long_chain() {
    let app = app();
    let track = app.project().track(shown_track(&app).unwrap()).unwrap();
    let renderer = crate::grid::test_renderer();
    let mut widget = view(&app, track, View::Track);
    let mut tree = Tree::new(&widget);
    let node = widget.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(800.0, 238.0)),
    );
    let mixer = mixer_frame(&node, 3);
    assert!(
        mixer.bounds().x > 800.0,
        "the Mixer sits after the devices, in the horizontal scroll area"
    );
}

#[test]
fn mixer_title_is_plain_with_no_power_button() {
    let app = app();
    let track = app.project().track(shown_track(&app).unwrap()).unwrap();
    let renderer = crate::grid::test_renderer();
    let mut widget = view(&app, track, View::Track);
    let mut tree = Tree::new(&widget);
    let node = widget.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(3000.0, 238.0)),
    );
    let frame = &mixer_frame(&node, 3).children()[0];
    let title = &frame.children()[0];
    assert_eq!(title.bounds().height, detail::DEVICE_TITLE_HEIGHT);
    // A device title holds Activator, name and fold; the Mixer's holds the name text alone.
    let name = &title.children()[0];
    assert!(
        name.children().is_empty(),
        "no Activator or fold in the title"
    );
    assert_eq!(name.bounds().x, title.bounds().x + 6.0);
}

#[test]
fn device_and_mixer_bodies_are_169_pixels_or_shrink_to_the_available_height() {
    let app = app();
    let track = app.project().track(shown_track(&app).unwrap()).unwrap();
    let renderer = crate::grid::test_renderer();
    for height in [204.0, 226.0, 110.0] {
        let mut widget = view(&app, track, View::Track);
        let mut tree = Tree::new(&widget);
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, Size::new(1200.0, height)),
        );
        let frame = &mixer_frame(&node, 3).children()[0];
        assert_eq!(frame.children()[0].bounds().height, 20.0);
        let body = &frame.children()[1];
        // Like the devices, the Mixer gives up the chain scrollbar's gutter.
        let gutter = crate::style::detail::SCROLLBAR_GUTTER;
        assert_eq!(body.bounds().height, (height - 28.0 - gutter).min(169.0));
        if height < 169.0 {
            assert!(
                body.children()[0].bounds().height > body.bounds().height,
                "Mixer controls remain in a vertical scroll area"
            );
        } else {
            assert!(
                body.children()[0].bounds().height <= body.bounds().height,
                "the Mixer's columns fit the standard body"
            );
        }
        // Horizontal chain scroll -> Browser host -> reorder host -> device row.
        let mut device_frames = vec![];
        fn frames<'a>(node: &'a layout::Node, found: &mut Vec<&'a layout::Node>) {
            if node.bounds().width == EXPANDED_EMPTY && node.children().len() == 1 {
                let column = &node.children()[0];
                if column.children().len() == 2 && column.children()[0].bounds().height == 20.0 {
                    found.push(column);
                }
            }
            for child in node.children() {
                frames(child, found);
            }
        }
        frames(&node.children()[0].children()[0], &mut device_frames);
        assert_eq!(device_frames.len(), 3);
        for frame in device_frames {
            assert_eq!(
                frame.children()[1].bounds().height,
                (height - 28.0 - crate::style::detail::SCROLLBAR_GUTTER).min(169.0)
            );
        }
    }
}

#[test]
fn devices_and_mixer_sit_chain_gap_apart_on_the_chain_background() {
    let app = app();
    let track = app.project().track(shown_track(&app).unwrap()).unwrap();
    let renderer = crate::grid::test_renderer();
    let mut widget = view(&app, track, View::Track);
    let mut tree = Tree::new(&widget);
    let node = widget.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(2400.0, 238.0)),
    );
    fn frames<'a>(node: &'a layout::Node, found: &mut Vec<&'a layout::Node>) {
        if node.bounds().width == EXPANDED_EMPTY && node.children().len() == 1 {
            found.push(node);
            return;
        }
        for child in node.children() {
            frames(child, found);
        }
    }
    let mut found = vec![];
    frames(&node.children()[0].children()[0], &mut found);
    assert_eq!(found.len(), 3);
    for pair in found.windows(2) {
        let (a, b) = (pair[0].bounds(), pair[1].bounds());
        assert_eq!(b.x - (a.x + a.width), detail::CHAIN_GAP);
    }
}
