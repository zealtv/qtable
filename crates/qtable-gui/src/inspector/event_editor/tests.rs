use super::*;
use crate::drafts::{ClipField, DraftKey};
use crate::messages::Message as RootMessage;

fn setup() -> (App, ClipKey) {
    let engine = qtable_engine::engine::Engine::new(crate::files::new_project()).unwrap();
    let mut app = App::new(engine, "120".into(), None);
    let track = app.project().tracks[0].id;
    run_edit(
        &mut app,
        Command::Edit(ProjectEdit::CreateClip {
            track,
            slot: 0,
            length: BeatTime(3840),
        }),
    );
    app.selection.clip = Some(crate::grid::Selection { track, slot: 0 });
    reconcile(&mut app);
    let key = ClipKey {
        track,
        slot: 0,
        id: clip(app.project(), track, 0).unwrap().id,
        epoch: app.clip_view.editor.epoch,
    };
    (app, key)
}
fn send(app: &mut App, message: Message) {
    let _ = crate::update::update(app, RootMessage::Inspector(message));
}
fn timeline(app: &mut App, action: Action) {
    send(app, Message::Timeline(action));
}
/// The track's event at this relative path, added with a lane when it has none.
fn lane(app: &mut App, key: ClipKey, path: &str) -> EventRef {
    let edit = LaneEdit::AddPath(path.into());
    let path = OscAddress::Relative(path.into());
    let find = |app: &App| app.project().track(key.track).unwrap().find_event(&path);
    if let Some(found) = find(app) {
        return found;
    }
    run_edit(
        app,
        Command::Edit(ProjectEdit::EditLane {
            track: key.track,
            slot: 0,
            edit,
        }),
    );
    find(app).unwrap()
}
fn add(app: &mut App, key: ClipKey, ticks: u64) {
    let x = lane(app, key, "/x");
    timeline(app, Action::Add(key, BeatTime(ticks), x));
}
fn paths(app: &App, key: ClipKey) -> Vec<OscAddress> {
    let track = app.project().track(key.track).unwrap();
    let clip = track.slots[0].clip.as_ref().unwrap();
    clip.lane_refs()
        .iter()
        .map(|r| track.event_def(r).unwrap().1.path.clone())
        .collect()
}

#[test]
fn add_select_move_and_delete_use_engine_edits_and_leave_other_events_unchanged() {
    let (mut app, key) = setup();
    add(&mut app, key, 960);
    add(&mut app, key, 1920);
    let revision = app.project_revision();
    timeline(&mut app, Action::Select(key, Some(0)));
    timeline(&mut app, Action::Grid(Grid::EighthTriplet));
    assert_eq!(app.project_revision(), revision);
    let target = HitTarget {
        key,
        index: 0,
        revision: app.project_revision(),
    };
    timeline(&mut app, Action::Move(target, BeatTime(1440)));
    let events = &clip(app.project(), key.track, 0).unwrap().events;
    assert_eq!(
        (events[0].position, events[1].position),
        (BeatTime(1440), BeatTime(1920))
    );
    shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    assert_eq!(clip(app.project(), key.track, 0).unwrap().events.len(), 1);
    assert_eq!(app.clip_view.editor.selected, None);
    shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    assert_eq!(clip(app.project(), key.track, 0).unwrap().events.len(), 1);
}

#[test]
fn typing_or_selecting_a_scene_removes_delete_ownership_and_load_clears_selection() {
    let (mut app, key) = setup();
    add(&mut app, key, 0);
    let field = DraftKey::clip(key.track, 0, ClipField::Name);
    send(&mut app, Message::Draft(field, "/new".into()));
    assert!(!app.clip_view.editor.focused);
    shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    assert_eq!(clip(app.project(), key.track, 0).unwrap().events.len(), 1);
    timeline(&mut app, Action::Select(key, Some(0)));
    let scene = app.project().scenes[0].id;
    let _ = crate::update::update(
        &mut app,
        RootMessage::Grid(crate::grid::Action::SelectScene(scene)),
    );
    shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    assert_eq!(clip(app.project(), key.track, 0).unwrap().events.len(), 1);
    app.reset_for_project();
    assert_eq!(app.clip_view.editor.selected, None);
    assert!(!app.clip_view.editor.focused);
}

#[test]
fn selection_changes_and_replacement_clip_ids_reject_stale_actions() {
    let (mut app, key) = setup();
    add(&mut app, key, 0);
    app.selection.clip = Some(crate::grid::Selection {
        track: key.track,
        slot: 1,
    });
    reconcile(&mut app);
    assert_eq!(app.clip_view.editor.selected, None);
    let target = HitTarget {
        key,
        index: 0,
        revision: app.project_revision(),
    };
    timeline(&mut app, Action::Move(target, BeatTime(960)));
    assert_eq!(
        clip(app.project(), key.track, 0).unwrap().events[0].position,
        BeatTime::ZERO
    );
    app.selection.clip = Some(crate::grid::Selection {
        track: key.track,
        slot: 0,
    });
    reconcile(&mut app);
    run_edit(
        &mut app,
        Command::Edit(ProjectEdit::DeleteClip {
            track: key.track,
            slot: 0,
        }),
    );
    reconcile(&mut app);
    run_edit(
        &mut app,
        Command::Edit(ProjectEdit::CreateClip {
            track: key.track,
            slot: 0,
            length: BeatTime(3840),
        }),
    );
    reconcile(&mut app);
    add(&mut app, key, 960);
    assert!(clip(app.project(), key.track, 0).unwrap().events.is_empty());
}

#[test]
fn removal_shifts_selection_and_clears_indexed_drafts_only_after_success() {
    let (mut app, key) = setup();
    add(&mut app, key, 0);
    add(&mut app, key, 960);
    let field = DraftKey::clip(
        key.track,
        0,
        ClipField::EventArgument {
            event: 1,
            argument: 0,
        },
    );
    app.drafts.values.insert(field, "unfinished".into());
    remove(&mut app, key.track, 0, 8);
    assert_eq!(app.clip_view.editor.selected, Some((key, 1)));
    assert!(app.drafts.values.contains_key(&field));
    remove(&mut app, key.track, 0, 0);
    assert_eq!(app.clip_view.editor.selected, Some((key, 0)));
    assert!(!app.drafts.values.contains_key(&field));
}

#[test]
fn move_beyond_markers_keeps_event_and_selected_strip_supports_all_argument_types() {
    let (mut app, key) = setup();
    add(&mut app, key, 0);
    let target = HitTarget {
        key,
        index: 0,
        revision: app.project_revision(),
    };
    timeline(&mut app, Action::Move(target, BeatTime(3840)));
    assert_eq!(
        clip(app.project(), key.track, 0).unwrap().events[0].position,
        BeatTime(3840)
    );
    use qtable_engine::model::FieldType;
    let signature = [
        FieldType::Int,
        FieldType::Float,
        FieldType::String,
        FieldType::Bool,
    ];
    super::sign_lane(&mut app, key.track, 0, 0, &signature);
    for (arg, (kind, raw)) in [
        (super::super::ArgKind::Int, "7"),
        (super::super::ArgKind::Float, "0.75"),
        (super::super::ArgKind::String, "warm"),
        (super::super::ArgKind::Bool, "true"),
    ]
    .into_iter()
    .enumerate()
    {
        send(
            &mut app,
            Message::Argument(key.track, 0, 0, arg, kind.name().into(), raw.into()),
        );
    }
    let event = &clip(app.project(), key.track, 0).unwrap().events[0];
    assert_eq!(event.arguments.len(), 4);
    assert_eq!(
        event.arguments[0],
        qtable_engine::model::OscArgument::Int(7)
    );
    let field = DraftKey::clip(
        key.track,
        0,
        ClipField::EventArgument {
            event: 0,
            argument: 1,
        },
    );
    send(&mut app, Message::Draft(field, "NaN".into()));
    send(
        &mut app,
        Message::Argument(key.track, 0, 0, 1, "float".into(), "NaN".into()),
    );
    assert!(app.drafts.errors.contains_key(&field));
}

#[test]
fn hits_select_their_lane_and_copy_the_nearest_earlier_hit_arguments() {
    let (mut app, key) = setup();
    add(&mut app, key, 960);
    let int = [qtable_engine::model::FieldType::Int];
    super::sign_lane(&mut app, key.track, 0, 0, &int);
    send(
        &mut app,
        Message::Argument(key.track, 0, 0, 0, "int".into(), "5".into()),
    );
    let y = lane(&mut app, key, "/y");
    timeline(&mut app, Action::Add(key, BeatTime(0), y));
    assert_eq!(
        app.clip_view
            .editor
            .lane_for(key, app.project().clip(key.track, key.slot).unwrap()),
        Some(1)
    );
    add(&mut app, key, 1920);
    add(&mut app, key, 0);
    let events = &clip(app.project(), key.track, 0).unwrap().events;
    assert!(events[1].arguments.is_empty(), "another lane starts bare");
    assert_eq!(
        events[2].arguments, events[0].arguments,
        "copies the earlier hit"
    );
    assert_eq!(
        events[3].arguments,
        vec![qtable_engine::model::OscArgument::Int(0)],
        "else the signature's defaults"
    );
    assert_eq!(
        app.clip_view
            .editor
            .lane_for(key, app.project().clip(key.track, key.slot).unwrap()),
        Some(0)
    );
    timeline(&mut app, Action::SelectLane(key, 1));
    assert_eq!(app.clip_view.editor.selected, None);
    assert_eq!(
        app.clip_view
            .editor
            .lane_for(key, app.project().clip(key.track, key.slot).unwrap()),
        Some(1)
    );
}

#[test]
fn readdressing_a_lane_moves_its_hits_and_removing_it_drops_them() {
    let (mut app, key) = setup();
    add(&mut app, key, 0);
    add(&mut app, key, 960);
    let y = lane(&mut app, key, "/y");
    timeline(&mut app, Action::Add(key, BeatTime(0), y));
    run_edit(
        &mut app,
        Command::Edit(ProjectEdit::EditLane {
            track: key.track,
            slot: 0,
            edit: LaneEdit::SetPath {
                lane: 0,
                path: "/z".into(),
            },
        }),
    );
    assert_eq!(paths(&app, key)[0], OscAddress::Relative("/z".into()));
    let z = app
        .project()
        .track(key.track)
        .unwrap()
        .find_event(&OscAddress::Relative("/z".into()));
    let clip_now = clip(app.project(), key.track, 0).unwrap();
    assert_eq!(
        clip_now
            .events
            .iter()
            .filter(|e| Some(&e.definition) == z.as_ref())
            .count(),
        2
    );
    crate::edit_actions::execute_structural_edit(
        &mut app,
        Command::Edit(ProjectEdit::EditLane {
            track: key.track,
            slot: 0,
            edit: LaneEdit::Remove { lane: 0 },
        }),
    );
    reconcile(&mut app);
    let clip_now = clip(app.project(), key.track, 0).unwrap();
    assert_eq!(clip_now.events.len(), 1);
    assert_eq!(paths(&app, key), vec![OscAddress::Relative("/y".into())]);
    assert_eq!(app.clip_view.editor.lanes, None);
    // Delete with only a lane selected removes nothing.
    timeline(&mut app, Action::SelectLane(key, 0));
    shortcut(&mut app, crate::clip_shortcuts::Shortcut::Delete);
    assert_eq!(clip(app.project(), key.track, 0).unwrap().events.len(), 1);
}

#[test]
fn unlinked_lanes_keep_their_own_playheads_and_linked_lanes_the_clip_s() {
    let (mut app, key) = setup();
    let target = |field: &str| ArgumentRef {
        device: qtable_engine::model::DeviceRef::Mixer,
        parameter: qtable_engine::model::ParameterKey("volume".into()),
        field: qtable_engine::model::FieldKey(field.into()),
    };
    app.snapshot
        .clip_positions
        .push(qtable_engine::scheduler::ClipPosition {
            track: key.track,
            clip: key.id,
            position: BeatTime(1440),
        });
    let lane = |clip, position| qtable_engine::scheduler::LanePosition {
        track: key.track,
        clip,
        target: target("value"),
        position,
    };
    app.snapshot.lane_positions = vec![
        lane(key.id, BeatTime(5760)),
        lane(qtable_engine::model::ClipId(key.id.0 + 1000), BeatTime(10)),
    ];
    reconcile(&mut app);
    let editor = &app.clip_view.editor;
    assert_eq!(
        editor.lane_playheads,
        vec![(target("value"), BeatTime(5760))]
    );
    assert_eq!(
        editor.playhead_for(&target("value"), true),
        Some(BeatTime(5760))
    );
    assert_eq!(
        editor.playhead_for(&target("value"), false),
        Some(BeatTime(1440))
    );
    assert_eq!(editor.playhead_for(&target("other"), true), None);
    // A stop keeps the last position, as the clip's does; another clip forgets it.
    app.snapshot.lane_positions.clear();
    reconcile(&mut app);
    assert_eq!(
        app.clip_view.editor.playhead_for(&target("value"), true),
        Some(BeatTime(5760))
    );
    app.selection.clip = None;
    reconcile(&mut app);
    assert!(app.clip_view.editor.lane_playheads.is_empty());
}

#[test]
fn playhead_uses_clip_identity_and_keeps_a_stopped_position() {
    let (mut app, key) = setup();
    app.snapshot
        .clip_positions
        .push(qtable_engine::scheduler::ClipPosition {
            track: key.track,
            clip: key.id,
            position: BeatTime(1440),
        });
    app.snapshot.playing = false;
    reconcile(&mut app);
    assert_eq!(app.clip_view.editor.playhead, Some(BeatTime(1440)));
    // Both a transport stop and a track stop can remove the voice from publication.
    for playing in [false, true] {
        app.snapshot.playing = playing;
        app.snapshot.clip_positions.clear();
        reconcile(&mut app);
        assert_eq!(app.clip_view.editor.playhead, Some(BeatTime(1440)));
    }
    // A click after stop moves only the content selection.
    timeline(&mut app, Action::Cursor(key, BeatTime(480), Some(0)));
    assert_eq!(app.clip_view.editor.playhead, Some(BeatTime(1440)));
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().insert_mark(),
        Some(BeatTime(480))
    );
    // A fresh publication (including launch at a non-zero Start) replaces the held position.
    app.snapshot
        .clip_positions
        .push(qtable_engine::scheduler::ClipPosition {
            track: key.track,
            clip: key.id,
            position: BeatTime(9600),
        });
    reconcile(&mut app);
    assert_eq!(app.clip_view.editor.playhead, Some(BeatTime(9600)));
    app.snapshot.clip_positions.clear();
    app.selection.clip = None;
    reconcile(&mut app);
    assert_eq!(app.clip_view.editor.playhead, None);
    app.selection.clip = Some(crate::grid::Selection {
        track: key.track,
        slot: key.slot,
    });
    reconcile(&mut app);
    assert_eq!(app.clip_view.editor.playhead, None);
    let replacement = clip(app.project(), key.track, key.slot).unwrap().clone();
    run_edit(
        &mut app,
        Command::Edit(ProjectEdit::PasteClip {
            track: key.track,
            slot: key.slot,
            clip: Box::new(replacement),
            carried: Vec::new(),
        }),
    );
    app.snapshot.clip_positions = vec![qtable_engine::scheduler::ClipPosition {
        track: key.track,
        clip: key.id,
        position: BeatTime(1440),
    }];
    reconcile(&mut app);
    assert_eq!(app.clip_view.editor.playhead, None);
}

#[test]
fn marker_actions_reject_stale_epochs_and_alt_arrows_move_start_end_together() {
    use iced::keyboard::{Key, Modifiers, key::Named};
    let (mut app, key) = setup();
    let mut r = clip_owned(&app, key).unwrap().region();
    r.start = BeatTime(480);
    r.end = BeatTime(5760);
    r.loop_start = BeatTime(960);
    r.loop_end = BeatTime(2880);
    let revision = app.project_revision();
    timeline(
        &mut app,
        Action::Markers(
            ClipKey {
                epoch: key.epoch + 1,
                ..key
            },
            revision,
            r,
        ),
    );
    assert_eq!(clip_owned(&app, key).unwrap().start, BeatTime::ZERO);
    timeline(&mut app, Action::Markers(key, revision, r));
    assert_eq!(clip_owned(&app, key).unwrap().start, BeatTime(480));
    timeline(&mut app, Action::Focus(key));
    app.focus = crate::focus::Focus::Detail;
    assert!(content::keyboard(
        &mut app,
        &Key::Named(Named::ArrowRight),
        Modifiers::ALT,
        false,
        true
    ));
    let c = clip_owned(&app, key).unwrap();
    assert_eq!(
        (c.start.0, c.end.0, c.loop_start.0, c.loop_end.0),
        (720, 6000, 960, 2880)
    );
    // A delayed release may not overwrite a newer command.
    timeline(&mut app, Action::Markers(key, revision, r));
    assert_eq!(clip_owned(&app, key).unwrap().start, BeatTime(720));
    app.pending_field = Some(DraftKey::clip(key.track, key.slot, ClipField::Start));
    assert!(!content::keyboard(
        &mut app,
        &Key::Named(Named::ArrowLeft),
        Modifiers::ALT,
        false,
        true
    ));
    assert_eq!(clip_owned(&app, key).unwrap().start, BeatTime(720));
}

mod zoom {
    use super::*;
    use crate::control::time_axis::Window;
    use crate::inspector::lane_view::EditorTab;

    fn window(start: f64, visible: f64) -> Option<Window> {
        Some(Window { start, visible })
    }
    fn zoom_of(app: &App, key: ClipKey) -> Option<Window> {
        app.clip_view
            .editor
            .zooms
            .window(key.track, key.id, BeatTime(3840))
    }
    fn select_range(app: &mut App, key: ClipKey, range: std::ops::Range<u64>) {
        let lane = lane(app, key, "/x");
        timeline(
            app,
            Action::Content(content::Selection::time(
                key,
                BeatTime(range.start)..BeatTime(range.end),
                vec![lanes::Identity::Event(lane)],
            )),
        );
    }

    /// How many undo steps the history holds, leaving the project as it was.
    fn steps(app: &mut App) -> usize {
        let mut count = 0;
        while app.engine.can_undo() {
            app.engine.execute(Command::Undo).unwrap();
            count += 1;
        }
        for _ in 0..count {
            app.engine.execute(Command::Redo).unwrap();
        }
        count
    }

    #[test]
    fn zooming_sends_no_command_bumps_no_revision_or_epoch_and_makes_no_undo_step() {
        let (mut app, key) = setup();
        add(&mut app, key, 960);
        timeline(&mut app, Action::Select(key, Some(0)));
        let before = steps(&mut app);
        let (revision, epoch) = (app.project_revision(), app.clip_view.editor.epoch);
        let label = app.engine.undo_label();
        let selected = app.clip_view.editor.selected;
        timeline(&mut app, Action::Zoom(key, window(960.0, 1920.0)));
        timeline(&mut app, Action::Zoom(key, window(1200.0, 960.0)));
        timeline(&mut app, Action::ZoomTo(key));
        timeline(&mut app, Action::Zoom(key, window(960.0, 1920.0)));
        assert_eq!(zoom_of(&app, key), window(960.0, 1920.0));
        assert_eq!(app.project_revision(), revision, "no engine command ran");
        assert_eq!(app.clip_view.editor.epoch, epoch);
        assert_eq!(
            app.clip_view.editor.selected, selected,
            "the selection stays"
        );
        assert_eq!(app.engine.undo_label(), label, "no undo step was made");
        assert_eq!(steps(&mut app), before);
    }

    #[test]
    fn undo_and_redo_keep_the_zoom() {
        let (mut app, key) = setup();
        timeline(&mut app, Action::Zoom(key, window(960.0, 1920.0)));
        // A real edit after the zoom, in a step of its own.
        app.engine.close_gestures();
        let mut region = clip(app.project(), key.track, 0).unwrap().region();
        region.end = BeatTime(2880);
        region.loop_end = BeatTime(2880);
        let revision = app.project_revision();
        timeline(&mut app, Action::Markers(key, revision, region));
        assert!(
            steps(&mut app) >= 2,
            "the markers edit is a step beyond the clip's"
        );
        let end = |app: &App| clip(app.project(), key.track, 0).map(|c| c.end);
        assert_eq!(end(&app), Some(BeatTime(2880)));
        app.engine.execute(Command::Undo).unwrap();
        reconcile(&mut app);
        assert_eq!(
            end(&app),
            Some(BeatTime(3840)),
            "the undo took back the markers"
        );
        assert_eq!(zoom_of(&app, key), window(960.0, 1920.0));
        app.engine.execute(Command::Redo).unwrap();
        reconcile(&mut app);
        assert_eq!(end(&app), Some(BeatTime(2880)));
        assert_eq!(zoom_of(&app, key), window(960.0, 1920.0));
    }

    #[test]
    fn the_window_belongs_to_the_clip_and_every_tab_shares_it() {
        let (mut app, key) = setup();
        timeline(&mut app, Action::Zoom(key, window(960.0, 1920.0)));
        for tab in [EditorTab::Envelopes, EditorTab::Events, EditorTab::Overview] {
            send(&mut app, Message::ChooseTab(key.track, 0, tab));
            assert_eq!(zoom_of(&app, key), window(960.0, 1920.0), "{tab:?}");
        }
        // Another clip is fitted, and this one is as it was when it comes back.
        run_edit(
            &mut app,
            Command::Edit(ProjectEdit::CreateClip {
                track: key.track,
                slot: 1,
                length: BeatTime(3840),
            }),
        );
        let other = clip(app.project(), key.track, 1).unwrap().id;
        assert_ne!(other, key.id);
        assert_eq!(
            app.clip_view
                .editor
                .zooms
                .window(key.track, other, BeatTime(3840)),
            None
        );
        app.selection.clip = Some(crate::grid::Selection {
            track: key.track,
            slot: 1,
        });
        reconcile(&mut app);
        app.selection.clip = Some(crate::grid::Selection {
            track: key.track,
            slot: 0,
        });
        reconcile(&mut app);
        assert_eq!(zoom_of(&app, key), window(960.0, 1920.0));
    }

    #[test]
    fn deleting_the_clip_forgets_its_zoom_and_a_stale_key_does_nothing() {
        let (mut app, key) = setup();
        timeline(&mut app, Action::Zoom(key, window(960.0, 1920.0)));
        let stale = ClipKey {
            epoch: key.epoch + 1,
            ..key
        };
        timeline(&mut app, Action::Zoom(stale, window(0.0, 480.0)));
        assert_eq!(zoom_of(&app, key), window(960.0, 1920.0));
        run_edit(
            &mut app,
            Command::Edit(ProjectEdit::DeleteClip {
                track: key.track,
                slot: 0,
            }),
        );
        reconcile(&mut app);
        assert_eq!(
            app.clip_view
                .editor
                .zooms
                .window(key.track, key.id, BeatTime(3840)),
            None
        );
    }

    #[test]
    fn a_window_beyond_a_shrunken_clip_is_held_inside_it() {
        let (mut app, key) = setup();
        timeline(&mut app, Action::Zoom(key, window(2400.0, 1440.0)));
        let zooms = &app.clip_view.editor.zooms;
        // The clip now ends at 3000: the window slides back to fit.
        assert_eq!(
            zooms.window(key.track, key.id, BeatTime(3000)),
            window(1560.0, 1440.0)
        );
        // A clip no longer than the window shows all of it: fitted.
        assert_eq!(zooms.window(key.track, key.id, BeatTime(1440)), None);
    }

    #[test]
    fn zooming_all_the_way_out_forgets_the_window() {
        let (mut app, key) = setup();
        timeline(&mut app, Action::Zoom(key, window(960.0, 1920.0)));
        timeline(&mut app, Action::Zoom(key, window(0.0, 3840.0)));
        assert_eq!(zoom_of(&app, key), None);
    }

    #[test]
    fn zoom_to_shows_the_time_selection_exactly_then_the_whole_clip() {
        let (mut app, key) = setup();
        select_range(&mut app, key, 960..2400);
        let selection = app.clip_view.editor.content.clone();
        timeline(&mut app, Action::ZoomTo(key));
        assert_eq!(zoom_of(&app, key), window(960.0, 1440.0));
        assert_eq!(
            app.clip_view.editor.content, selection,
            "the selection stays"
        );
        // Already showing it exactly: the second double-click fits the clip.
        timeline(&mut app, Action::ZoomTo(key));
        assert_eq!(zoom_of(&app, key), None);
        // With nothing selected it also fits.
        timeline(&mut app, Action::Zoom(key, window(960.0, 1920.0)));
        timeline(&mut app, Action::ClearContent);
        timeline(&mut app, Action::ZoomTo(key));
        assert_eq!(zoom_of(&app, key), None);
        // An insert marker alone is no range.
        timeline(&mut app, Action::Zoom(key, window(960.0, 1920.0)));
        select_range(&mut app, key, 1200..1200);
        timeline(&mut app, Action::ZoomTo(key));
        assert_eq!(zoom_of(&app, key), None);
    }
}
