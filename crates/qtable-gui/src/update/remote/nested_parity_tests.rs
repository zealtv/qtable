//! Acceptance §10.5: real GUI messages and mapped MIDI/key inputs share nested scope.
use super::*;
use crate::{Message, grid, mixer_band::MixerMessage, selection::ColumnOwner};
use qtable_engine::{edit::ProjectEdit, engine::Engine, model::*};
use qtable_remote::{Endpoint, Input};
use std::time::{Duration, Instant};

fn project(midi: bool, folded: bool) -> Project {
    // Outer(Inner(a,b),c),d. Every leaf has clips on rows 0 and 2, with a gap.
    let mut p = Project {
        scenes: (1..=3)
            .map(|id| Scene {
                id: SceneId(id),
                name: String::new(),
                follow_actions: Default::default(),
            })
            .collect(),
        global_quantisation: GlobalQuantisation::None,
        ..Project::default()
    };
    for (id, parent, group) in [
        (10, None, true),
        (20, Some(10), true),
        (1, Some(20), false),
        (2, Some(20), false),
        (3, Some(10), false),
        (4, None, false),
    ] {
        let mut track = if group {
            Track::new_group(TrackId(id), format!("G{id}"), 3)
        } else {
            Track::new(TrackId(id), format!("T{id}"), 3)
        };
        track.group = parent.map(TrackId);
        track.route = if parent.is_some() {
            Route::Group
        } else {
            Route::single(p.main.id)
        };
        track.fold_state = group && folded;
        if !group {
            for row in [0, 2] {
                track.slots[row].clip = Some(
                    Clip::new(
                        ClipId(id * 10 + row as u64),
                        "Clip",
                        BeatTime(3840),
                        track.colour,
                    )
                    .unwrap(),
                );
            }
        }
        p.tracks.push(track);
    }
    for group in [TrackId(10), TrackId(20)] {
        for control in [
            ControlRef::ClipSlot {
                track: group,
                scene: SceneId(1),
            },
            ControlRef::TrackStop(group),
            ControlRef::TrackSelect(group),
            ControlRef::TrackActivator(group),
            ControlRef::TrackSolo(group),
        ] {
            let index = p.mappings.len() as u8;
            let source = if midi {
                Source::Midi(MidiSource {
                    channel: 1,
                    message: MidiMessage::Note(60 + index),
                })
            } else {
                Source::Key(KeySource {
                    key: KeyName(((b'a' + index) as char).to_string()),
                })
            };
            p.mappings.push(RemoteMapping::new(
                MappingId(index as u64 + 1),
                source,
                control,
                0.0,
                1.0,
            ));
        }
    }
    p.validate().unwrap();
    p
}

fn app(p: Project) -> App {
    App::new(Engine::new(p).unwrap(), "120".into(), None)
}
fn gui(app: &mut App, action: grid::Action) {
    let _ = crate::update::update(app, Message::Grid(action));
}
fn enqueue(app: &App, midi: bool, index: u8) {
    let inputs = if midi {
        [
            Input::Midi(vec![0x90, 60 + index, 100].into()),
            Input::Midi(vec![0x80, 60 + index, 0].into()),
        ]
    } else {
        let key = ((b'a' + index) as char).to_string();
        [
            Input::Key {
                key: key.clone(),
                pressed: true,
                repeat: false,
            },
            Input::Key {
                key,
                pressed: false,
                repeat: false,
            },
        ]
    };
    for input in inputs {
        app.remote.ingress.push(Endpoint::Keyboard, input);
    }
}
fn mapped(app: &mut App, midi: bool, index: u8) {
    enqueue(app, midi, index);
    let _ = crate::update::update(app, Message::Remote(Action::Wake));
}
fn playing(app: &App, expected: &[u64]) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let snapshot = app.engine.snapshot();
        let actual: Vec<_> = snapshot
            .tracks
            .iter()
            .filter(|t| t.playing_slot == Some(0))
            .map(|t| t.id.0)
            .collect();
        if actual == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "expected {expected:?}, got {actual:?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn nested_gui_and_mapped_launch_stop_share_commands_and_leaf_scope_across_folds() {
    for midi in [false, true] {
        for folded in [false, true] {
            for (group, offset, leaves, remaining) in [
                (TrackId(10), 0, vec![1, 2, 3], vec![]),
                (TrackId(20), 5, vec![1, 2], vec![3]),
            ] {
                let p = project(midi, folded);
                let mut click = app(p.clone());
                let mut remote = app(p);
                // Assert the actual mapped command, before exercising the ordinary host drain.
                for (index, expected) in [
                    (offset, Command::LaunchGroupSlot { group, slot: 0 }),
                    (offset + 1, Command::StopGroup { group }),
                ] {
                    enqueue(&remote, midi, index);
                    let (_, actions) = remote.remote.router.drain_actions(
                        &remote.remote.ingress,
                        remote.engine.project(),
                        &remote.engine.snapshot(),
                        None,
                    );
                    assert_eq!(actions.len(), 1);
                    assert_eq!(
                        format!("{:?}", actions[0].value),
                        format!("{:?}", qtable_remote::Action::Command(expected))
                    );
                }
                gui(
                    &mut click,
                    grid::Action::SelectColumn(ColumnOwner::Group(group), Some(0)),
                );
                gui(&mut click, grid::Action::LaunchGroupSlot(group, 0));
                mapped(&mut remote, midi, offset);
                playing(&click, &leaves);
                playing(&remote, &leaves);
                assert_eq!(click.selection.tracks, remote.selection.tracks);
                assert_eq!(click.selection.owner, remote.selection.owner);
                // Seed every Outer leaf so Inner Stop must preserve c, and d stays untouched.
                gui(&mut click, grid::Action::LaunchGroupSlot(TrackId(10), 0));
                gui(&mut remote, grid::Action::LaunchGroupSlot(TrackId(10), 0));
                playing(&click, &[1, 2, 3]);
                playing(&remote, &[1, 2, 3]);
                gui(&mut click, grid::Action::StopGroup(group));
                mapped(&mut remote, midi, offset + 1);
                playing(&click, &remaining);
                playing(&remote, &remaining);
                assert_eq!(click.project(), remote.project());
                assert_eq!(click.project_revision(), 0);
                assert_eq!(remote.project_revision(), 0);
            }
        }
    }
}

#[test]
fn nested_gui_and_mapped_select_mute_and_additive_solo_preserve_identity_and_scope() {
    for midi in [false, true] {
        for folded in [false, true] {
            for (group, offset, leaves) in [
                (TrackId(10), 0, vec![1, 2, 3]),
                (TrackId(20), 5, vec![1, 2]),
            ] {
                let p = project(midi, folded);
                let mut click = app(p.clone());
                let mut remote = app(p.clone());
                let mut expected = Engine::new(p).unwrap();
                gui(&mut click, grid::Action::SelectGroup(group, false));
                mapped(&mut remote, midi, offset + 2);
                assert_eq!(click.selection.tracks, [group]);
                assert_eq!(click.selection.tracks, remote.selection.tracks);
                assert_eq!(click.selection.owner, remote.selection.owner);
                assert_eq!(click.project_revision(), 0);
                assert_eq!(remote.project_revision(), 0);
                for host in [&mut click, &mut remote] {
                    gui(host, grid::Action::LaunchGroupSlot(TrackId(10), 0));
                    gui(host, grid::Action::LaunchClip(TrackId(4), 0));
                    playing(host, &[1, 2, 3, 4]);
                }
                // Mapped Solo is additive by existing policy; compare the GUI modifier gesture.
                click.selection.command_down = true;
                for (index, message, command, sending) in [
                    (
                        offset + 3,
                        MixerMessage::Activator(group),
                        Command::Edit(ProjectEdit::SetActive {
                            strips: vec![group],
                            active: false,
                        }),
                        vec![4],
                    ),
                    (
                        offset + 3,
                        MixerMessage::Activator(group),
                        Command::Edit(ProjectEdit::SetActive {
                            strips: vec![group],
                            active: true,
                        }),
                        vec![1, 2, 3, 4],
                    ),
                    (
                        offset + 4,
                        MixerMessage::Solo(group),
                        Command::Edit(ProjectEdit::SetSolo {
                            strips: vec![group],
                            solo: true,
                            exclusive: false,
                        }),
                        leaves,
                    ),
                ] {
                    if index == offset + 4 {
                        // An unrelated existing Solo distinguishes additive from exclusive.
                        let seed = Command::Edit(ProjectEdit::SetSolo {
                            strips: vec![TrackId(4)],
                            solo: true,
                            exclusive: false,
                        });
                        for engine in [&mut click.engine, &mut remote.engine, &mut expected] {
                            engine.execute(seed.clone()).unwrap();
                        }
                    }
                    let _ = crate::update::update(&mut click, Message::Mixer(message));
                    mapped(&mut remote, midi, index);
                    expected.execute(command).unwrap();
                    assert_eq!(click.project(), expected.project());
                    assert_eq!(remote.project(), expected.project());
                    let actual: Vec<_> = remote
                        .project()
                        .clip_tracks()
                        .filter(|t| remote.project().track_sends(t.id))
                        .map(|t| t.id.0)
                        .collect();
                    // Inner mute leaves its Outer sibling audible.
                    let mut sending =
                        if index == offset + 3 && sending == [4] && group == TrackId(20) {
                            vec![3, 4]
                        } else {
                            sending
                        };
                    if index == offset + 4 {
                        sending.push(4);
                    }
                    assert_eq!(actual, sending);
                    assert_eq!(click.project_revision(), remote.project_revision());
                    playing(&click, &[1, 2, 3, 4]);
                    playing(&remote, &[1, 2, 3, 4]);
                }
                // GUI Fold is an engine edit; it must preserve every mapped stable control.
                let mappings = click.project().mappings.clone();
                gui(&mut click, grid::Action::ToggleGroup(group));
                expected
                    .execute(Command::Edit(ProjectEdit::SetGroupFolded {
                        group,
                        folded: !folded,
                    }))
                    .unwrap();
                assert_eq!(click.project(), expected.project());
                assert_eq!(click.project().mappings, mappings);
            }
        }
    }
}

#[test]
fn nested_mappings_do_not_retarget_after_reorder_ungroup_and_removal() {
    for midi in [false, true] {
        let mut p = project(midi, true);
        for (index, control) in [
            (10, ControlRef::TrackSelect(TrackId(1))),
            (11, ControlRef::TrackSolo(TrackId(1))),
        ] {
            let source = if midi {
                Source::Midi(MidiSource {
                    channel: 1,
                    message: MidiMessage::Note(60 + index),
                })
            } else {
                Source::Key(KeySource {
                    key: KeyName(((b'a' + index) as char).to_string()),
                })
            };
            p.mappings.push(RemoteMapping::new(
                MappingId(index as u64 + 1),
                source,
                control,
                0.0,
                1.0,
            ));
        }
        let survivor_bindings = p.mappings[10..].to_vec();
        let mut app = app(p);
        mapped(&mut app, midi, 10);
        assert_eq!(app.selection.tracks, [TrackId(1)]);
        for edit in [
            ProjectEdit::MoveTrack { from: 2, to: 3 },
            ProjectEdit::Ungroup { group: TrackId(20) },
            ProjectEdit::RemoveTracks {
                tracks: vec![TrackId(2)],
            },
        ] {
            assert!(crate::edit_actions::execute_structural_edit(
                &mut app,
                Command::Edit(edit)
            ));
            mapped(&mut app, midi, 10);
            assert_eq!(app.selection.tracks, [TrackId(1)]);
            assert_eq!(
                &app.project().mappings[app.project().mappings.len() - 2..],
                survivor_bindings
            );
        }
        mapped(&mut app, midi, 11);
        assert!(app.project()[TrackId(1)].mixer.solo);
        assert!(!app.project()[TrackId(3)].mixer.solo);
        assert_eq!(
            app.project().mappings.len(),
            7,
            "only the removed header's five controls disappear"
        );
        gui(&mut app, grid::Action::SelectTrack(TrackId(4), false));
        let before = app.project().clone();
        let revision = app.project_revision();
        for index in 5..10 {
            mapped(&mut app, midi, index);
        }
        assert_eq!(
            app.selection.tracks,
            [TrackId(4)],
            "removed bindings never name a replacement column"
        );
        assert_eq!(app.project(), &before);
        assert_eq!(app.project_revision(), revision);
    }
}
