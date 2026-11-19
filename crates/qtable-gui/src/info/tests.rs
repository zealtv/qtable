use super::*;
use crate::drafts::MixerField;
use crate::{app::App, drafts::DraftKey, files, grid, update::update};
use qtable_engine::engine::Engine;

#[test]
fn every_registered_control_has_plain_nonempty_help() {
    for &id in Id::ALL {
        let entry = id.entry();
        assert!(!entry.title.trim().is_empty(), "{id:?}");
        assert!(!entry.text.trim().is_empty(), "{id:?}");
        assert!(!entry.text.contains('\n'), "{id:?}");
        assert!(entry.text.len() < 200, "{id:?} help should be short");
    }
}

#[test]
fn exits_cannot_clear_another_widget_even_with_the_same_help_entry() {
    let a = Source::new();
    let b = Source::new();
    let mut state = State::default();
    state.update(Hover::Enter(a, Id::ClipColour, None));
    state.update(Hover::Enter(b, Id::ClipColour, None));
    state.update(Hover::Leave(a));
    assert_eq!(state.current, Some((b, Id::ClipColour, None)));
    state.update(Hover::Leave(b));
    assert_eq!(state.id(), Id::InfoView);
    state.update(Hover::Enter(a, Id::ClipLaunch, None));
    state.update(Hover::Clear);
    assert_eq!(state.id(), Id::InfoView);
}

#[test]
fn widget_and_canvas_hover_leave_pending_fields_and_the_engine_untouched() {
    let project = files::new_project();
    let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
    let revision = app.project_revision();
    let key = DraftKey::Tempo;
    app.pending_field = Some(key);
    app.tempo_input = "not yet a tempo".into();
    let owner = Source::new();
    for message in [
        Message::Info(Hover::Enter(owner, Id::Tempo, None)),
        Message::Grid(grid::Action::Info(Hover::Enter(
            owner,
            Id::ClipLaunch,
            None,
        ))),
        Message::Info(Hover::Leave(owner)),
    ] {
        let _ = update(&mut app, message);
        assert_eq!(app.project_revision(), revision);
        assert_eq!(app.pending_field, Some(key));
        assert_eq!(app.tempo_input, "not yet a tempo");
        assert!(app.drafts.errors.is_empty());
        assert!(app.status.message().is_none());
        assert!(!app.snapshot.playing);
    }
}

fn two_destination_project() -> qtable_engine::model::Project {
    use qtable_engine::model::*;
    let mut p = Project::default();
    p.outputs
        .push(Track::new_output(TrackId(201), "A", "127.0.0.1", 9000).with_path("/bop"));
    let mut off = Track::new_output(TrackId(202), "B", "127.0.0.1", 9001).with_path("/viz");
    off.mixer.active = false;
    p.outputs.push(off);
    let mut track = Track::new(TrackId(1), "Member", 0).with_path("/kick");
    track.route = Route::Outputs(vec![TrackId(201), TrackId(202)]);
    p.tracks = vec![track];
    p
}

#[test]
fn path_subjects_list_one_line_per_destination_and_mark_inactive_ones() {
    let p = two_destination_project();
    let track = qtable_engine::model::TrackId(1);
    assert_eq!(
        addresses(&p, &Subject::Path(track)).unwrap(),
        ["A: /bop/kick", "B: /viz/kick (off)"]
    );
    assert_eq!(
        addresses(&p, &Subject::Volume(track)).unwrap(),
        ["A: /bop/kick/volume", "B: /viz/kick/volume (off)"]
    );
}

#[test]
fn a_subject_without_destinations_says_so_and_a_stale_one_shows_nothing() {
    let mut p = two_destination_project();
    let track = qtable_engine::model::TrackId(1);
    p.tracks[0].route = qtable_engine::model::Route::empty();
    assert_eq!(
        addresses(&p, &Subject::Path(track)).unwrap(),
        ["No destination configured."]
    );
    let gone = qtable_engine::model::TrackId(99);
    assert_eq!(addresses(&p, &Subject::Path(gone)), None);
    assert_eq!(addresses(&p, &Subject::Volume(gone)), None);
    let event = qtable_engine::model::EventRef {
        device: qtable_engine::model::DeviceId(1),
        key: qtable_engine::model::EntryKey("hit".into()),
    };
    let lane = Subject::Lane {
        track,
        scene: qtable_engine::model::SceneId(1),
        event: event.clone(),
    };
    assert_eq!(addresses(&p, &lane), None, "no clip in slot");
    let gone = Subject::Lane {
        track,
        scene: qtable_engine::model::SceneId(99),
        event,
    };
    assert_eq!(addresses(&p, &gone), None, "no such scene");
}

#[test]
fn lane_subjects_follow_the_lane_and_absolute_lanes_show_one_line() {
    use qtable_engine::model::*;
    let mut p = crate::files::new_project();
    p.outputs
        .push(Track::new_output(TrackId(201), "A", "127.0.0.1", 9000).with_path("/bop"));
    let track = p.tracks[0].id;
    p.tracks[0].route = Route::single(TrackId(201));
    let mut clip = Clip::new(ClipId(1), "C", BeatTime(960), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.lanes = [
        OscAddress::Relative("/hit".into()),
        OscAddress::Absolute("/abs/x".into()),
    ]
    .into_iter()
    .map(|path| {
        qtable_engine::model::Lane::Events(EventLane {
            definition: p.ensure_event(track, path, &[]).unwrap(),
        })
    })
    .collect();
    let events: Vec<_> = clip.lane_refs();
    p.tracks[0].slots[0].clip = Some(clip);
    let scene = p.scenes[0].id;
    let lane = |event: &EventRef| Subject::Lane {
        track,
        scene,
        event: event.clone(),
    };
    let name = p.tracks[0].path.as_str().to_owned();
    assert_eq!(
        addresses(&p, &lane(&events[0])).unwrap(),
        [format!("A: /bop{name}/hit")]
    );
    assert_eq!(addresses(&p, &lane(&events[1])).unwrap(), ["/abs/x"]);
    let other = EventRef {
        device: events[0].device,
        key: EntryKey("gone".into()),
    };
    assert_eq!(addresses(&p, &lane(&other)), None, "unknown event");
}

#[test]
fn a_hovered_lane_shows_nothing_once_its_lane_leaves_the_list() {
    use qtable_engine::model::*;
    let mut p = crate::files::new_project();
    p.outputs
        .push(Track::new_output(TrackId(201), "A", "127.0.0.1", 9000).with_path("/bop"));
    let track = p.tracks[0].id;
    p.tracks[0].route = Route::single(TrackId(201));
    let mut clip = Clip::new(ClipId(1), "C", BeatTime(960), LIVE_CLIP_PALETTE[0]).unwrap();
    clip.lanes = ["/first", "/second"]
        .into_iter()
        .map(|path| {
            Lane::Events(EventLane {
                definition: p
                    .ensure_event(track, OscAddress::Relative(path.into()), &[])
                    .unwrap(),
            })
        })
        .collect();
    let events = clip.lane_refs();
    p.tracks[0].slots[0].clip = Some(clip);
    let scene = p.scenes[0].id;
    let hovered = Subject::Lane {
        track,
        scene,
        event: events[1].clone(),
    };
    assert_eq!(label(&p, &hovered).as_deref(), Some("/second"));
    // The first lane goes: the hovered event moves up to position 0. A positional ref would
    // now name another lane or none; the stable ref keeps naming its own event.
    p.tracks[0].slots[0].clip.as_mut().unwrap().lanes.remove(0);
    assert_eq!(label(&p, &hovered).as_deref(), Some("/second"));
    // The hovered lane goes too: nothing resolves, and nothing else stands in for it.
    p.tracks[0].slots[0].clip.as_mut().unwrap().lanes.clear();
    assert_eq!(label(&p, &hovered), None);
    assert_eq!(addresses(&p, &hovered), None);
    // A different clip shown under the same position does not borrow the old lane.
    p.tracks[0].slots[0].clip = None;
    assert_eq!(label(&p, &hovered), None);
}

#[test]
fn a_lane_subject_names_its_event_by_label() {
    use qtable_engine::model::TrackId;
    let p = qtable_engine::file::load("../qtable-engine/tests/fixtures/v6-lanes.ron").unwrap();
    let refs = p.tracks[0].clip(0).unwrap().lane_refs();
    let scene = p.scenes[0].id;
    let lane = |event: &EventRef| Subject::Lane {
        track: TrackId(1),
        scene,
        event: event.clone(),
    };
    assert_eq!(label(&p, &lane(&refs[2])).as_deref(), Some("/kick (f)"));
    assert_eq!(label(&p, &lane(&refs[3])).as_deref(), Some("abs /kick"));
    let stale = EventRef {
        device: refs[0].device,
        key: EntryKey("gone".into()),
    };
    assert_eq!(label(&p, &lane(&stale)), None, "stale event");
    assert_eq!(label(&p, &Subject::Path(TrackId(1))), None);
}

#[test]
fn hovering_a_path_field_records_its_subject_and_the_view_builds() {
    use crate::control::{PathField, SizeRole, path_field::Prefix};
    use crate::drafts::{Control, Controls};
    use crate::drafts::{ControlFocus, ControlId};
    use iced::advanced::{layout, renderer::Headless, widget::Tree};
    use iced::{Event, Point, Rectangle, Renderer, Size, mouse};
    let mut future = std::pin::pin!(Renderer::new(
        iced::Font::default(),
        iced::Pixels(crate::style::detail::TEXT_SIZE as f32),
        Some("tiny-skia")
    ));
    let std::task::Poll::Ready(Some(renderer)) = std::future::Future::poll(
        future.as_mut(),
        &mut std::task::Context::from_waker(std::task::Waker::noop()),
    ) else {
        panic!("software renderer must be available");
    };
    let track = qtable_engine::model::TrackId(1);
    let key = DraftKey::Mixer(track, MixerField::Address);
    let focus = ControlFocus::default();
    let controls = Controls::new(crate::control::LIVE_DARK, &focus);
    let mut field = controls.path(
        Control::new(
            ControlId::field(crate::control_focus::View::Track, key),
            Id::VolumePath,
            PathField {
                placeholder: "/volume",
                prefix: Prefix::default(),
                absolute: None,
                subject: Subject::Volume(track),
            },
        )
        .role(SizeRole::Fill),
        "/volume",
        |_| Message::Tick,
        Message::Tick,
        Message::Tick,
        None,
    );
    let mut tree = Tree::new(&field);
    let size = Size::new(200.0, 40.0);
    let node =
        field
            .as_widget_mut()
            .layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
    let point = Point::new(100.0, 8.0);
    let mut messages = vec![];
    field.as_widget_mut().update(
        &mut tree,
        &Event::Mouse(mouse::Event::CursorMoved { position: point }),
        iced::advanced::Layout::new(&node),
        mouse::Cursor::Available(point),
        &renderer,
        &mut iced::advanced::clipboard::Null,
        &mut iced::advanced::Shell::new(&mut messages),
        &Rectangle::with_size(size),
    );
    let mut state = State::default();
    for message in messages {
        if let Message::Info(hover) = message {
            state.update(hover);
        }
    }
    assert_eq!(state.id(), Id::VolumePath);
    assert_eq!(state.subject(), Some(&Subject::Volume(track)));
    let project = two_destination_project();
    let _ = view(&state, &project);
}

#[test]
fn breakdowns_share_ordered_levels_and_absolute_entries_skip_the_ancestors() {
    use qtable_engine::model::*;
    let mut project = two_destination_project();
    let mut group = Track::new_group(TrackId(10), "Drums", 0).with_path("/drums");
    group.route = project.tracks[0].route.clone();
    project.tracks[0].group = Some(group.id);
    project.tracks[0].route = Route::Group;
    project.tracks.insert(0, group);
    let track = TrackId(1);
    let reference = project
        .ensure_event(track, OscAddress::Relative("/hit".into()), &[])
        .unwrap();
    let device = project.device_mut(reference.device).unwrap();
    let target = device.target_mut().unwrap();
    target.path = Path::new("/mypath").unwrap();
    let subject = Subject::TargetEntry(EntryRef::Event {
        track,
        event: reference.clone(),
    });
    let lines = address_details(&project, &subject).unwrap();
    assert_eq!(lines[0].address, "A: /bop/drums/kick/mypath/hit");
    assert_eq!(
        lines[0].breakdown,
        "Output /bop · Group /drums · Track /kick · Target /mypath · Event /hit"
    );
    assert_eq!(
        lines[1].breakdown,
        "Output /viz · Group /drums · Track /kick · Target /mypath · Event /hit"
    );
    assert!(lines[1].address.ends_with(" (off)"));
    let strip = address_details(&project, &Subject::Path(track)).unwrap();
    assert_eq!(
        strip[0].breakdown,
        "Output /bop · Group /drums · Track /kick"
    );
    let volume = address_details(&project, &Subject::Volume(track)).unwrap();
    assert_eq!(
        volume[0].breakdown,
        "Output /bop · Group /drums · Track /kick · Volume /volume"
    );
    let device = project.device_mut(reference.device).unwrap();
    let target = device.target_mut().unwrap();
    target.events[0].path = OscAddress::Absolute("/direct".into());
    let absolute = address_details(&project, &subject).unwrap();
    assert_eq!(absolute.len(), 1);
    assert_eq!(absolute[0].address, "/direct");
    assert_eq!(absolute[0].breakdown, "Event /direct (Absolute)");
}

#[test]
fn breakdowns_omit_empty_pieces_and_do_not_repeat_a_destinations_own_path() {
    use qtable_engine::model::*;
    let project = two_destination_project();
    let lines = address_details(&project, &Subject::Path(TrackId(201))).unwrap();
    assert_eq!(lines[0].breakdown, "Output /bop");
    let mut project = project;
    project.tracks[0].path = Path::default();
    let lines = address_details(&project, &Subject::Volume(TrackId(1))).unwrap();
    assert_eq!(lines[0].breakdown, "Output /bop · Volume /volume");
}
