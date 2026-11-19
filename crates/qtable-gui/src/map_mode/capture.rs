//! Unattended screenshots of Map Mode (`tools/gui-capture.sh --show-map-mode`, `--show-map-learn`,
//! the inspector's `--show-map-inspect-*` and `--show-key-inspect`). They go through the
//! ordinary edits and messages, as the other capture hooks do.
use super::{Action, labels};
use crate::app::App;
use qtable_engine::edit::{MappingEdit, ProjectEdit};
use qtable_engine::engine::Command;
use qtable_engine::model::*;

fn cc(channel: u8, number: u8) -> Source {
    Source::Midi(MidiSource {
        channel,
        message: MidiMessage::Cc(number),
    })
}

/// `list`: Map Mode on, a spread of mappings (every kind of limit, a relative mode, a note and a
/// pitch bend) with one selected, and Volume waiting to learn. `learn`: Map Mode on and nothing
/// mapped yet, Volume waiting for a controller.
pub(crate) fn capture(app: &mut App, scenario: &str) {
    let Some(track) = app.project().tracks.first().map(|track| track.id) else {
        return;
    };
    let scene = app.project().scenes.first().map(|scene| scene.id);
    let volume = ControlRef::Argument(FieldRef::volume(track));
    let _ = crate::update::update(app, crate::messages::Message::Map(Action::Toggle));
    if scenario.starts_with("key") {
        super::update(app, Action::ToggleKey);
        if scenario == "key-list" {
            let mut controls = vec![
                ("s", ControlRef::TrackSolo(track)),
                ("a", ControlRef::TrackActivator(track)),
                ("p", ControlRef::Transport(TransportControl::Play)),
            ];
            if let Some(scene) = scene {
                controls.push(("1", ControlRef::ClipSlot { track, scene }));
            }
            for (key, control) in controls {
                super::update(app, Action::Pick(control));
                let key = iced::keyboard::Key::Character(key.into());
                super::keys::input(
                    app,
                    &key,
                    &key,
                    iced::keyboard::Modifiers::empty(),
                    true,
                    false,
                );
                super::keys::release_all(app);
            }
        }
    }
    if scenario == "list" {
        let mut rows = vec![
            (cc(1, 21), volume.clone()),
            (cc(1, 22), ControlRef::TrackSolo(track)),
            (cc(1, 23), ControlRef::TrackActivator(track)),
            (cc(1, 24), ControlRef::Transport(TransportControl::Tempo)),
            (
                cc(1, 25),
                ControlRef::Transport(TransportControl::Quantisation),
            ),
            (
                Source::Midi(MidiSource {
                    channel: 1,
                    message: MidiMessage::Note(60),
                }),
                ControlRef::TrackStop(track),
            ),
            (
                Source::Midi(MidiSource {
                    channel: 2,
                    message: MidiMessage::PitchBend,
                }),
                ControlRef::Transport(TransportControl::Play),
            ),
            (cc(1, 22), ControlRef::TrackStop(track)),
        ];
        if let Some(scene) = scene {
            rows.insert(
                5,
                (
                    Source::Midi(MidiSource {
                        channel: 1,
                        message: MidiMessage::Note(62),
                    }),
                    ControlRef::ClipSlot { track, scene },
                ),
            );
        }
        for (source, control) in rows {
            let Some(descriptor) = describe_control(app.project(), &control) else {
                continue;
            };
            let (min, max) = labels::learn_range(&control, &descriptor);
            let _ = crate::edit_actions::run_edit(
                app,
                Command::Edit(ProjectEdit::EditMapping(MappingEdit::Add {
                    source,
                    control,
                    min,
                    max,
                })),
            );
        }
        let ids: Vec<MappingId> = app.project().mappings.iter().map(|row| row.id).collect();
        if let Some(&tempo) = ids.get(3) {
            super::update(
                app,
                Action::SetMode(
                    tempo,
                    SourceMode::Relative(RelativeEncoding::TwosComplement),
                ),
            );
        }
        if let Some(&solo) = ids.get(1) {
            super::update(app, Action::SetTakeover(solo, Takeover::None));
        }
        app.map_mode.selected_row = ids.first().copied();
    }
    if let Some(case) = scenario.strip_prefix("inspect-") {
        inspect(app, case);
        return;
    }
    if scenario == "key-inspect" {
        inspect_keys(app);
        return;
    }
    app.map_mode.selected_control = Some(volume);
}

fn note(number: u8) -> Source {
    Source::Midi(MidiSource {
        channel: 1,
        message: MidiMessage::Note(number),
    })
}

fn add(app: &mut App, source: Source, control: ControlRef) -> Option<MappingId> {
    let descriptor = describe_control(app.project(), &control)?;
    let (min, max) = labels::learn_range(&control, &descriptor);
    let id = app.project().next_mapping_id().ok()?;
    crate::edit_actions::run_edit(
        app,
        Command::Edit(ProjectEdit::EditMapping(MappingEdit::Add {
            source,
            control,
            min,
            max,
        })),
    )
    .then_some(id)
}

/// The first event of `track`'s Target with an argument, and that argument.
fn event_argument(app: &App, track: TrackId) -> Option<(EventRef, FieldKey)> {
    app.project()
        .track(track)?
        .devices
        .iter()
        .find_map(|device| {
            device.target()?.events.iter().find_map(|event| {
                let field = event.fields.first()?;
                Some((
                    EventRef {
                        device: device.id,
                        key: event.key.clone(),
                    },
                    field.key.clone(),
                ))
            })
        })
}

/// The design's five inspector cases (§3.3) in MIDI Map Mode, over a list like its mock-up:
/// `cc` an absolute CC on a ranged Float with Value Scaling, `note` a note on a switch in Gate,
/// `encoder` a relative encoder at 250 % on an event's argument, `trigger` a note on a Bang
/// (nothing to set), and `none` with no row selected.
fn inspect(app: &mut App, case: &str) {
    // The first track whose Target has an event (grid-demo's Kick), and the track after it.
    let tracks: Vec<TrackId> = app.project().tracks.iter().map(|t| t.id).collect();
    let Some(at) = tracks
        .iter()
        .position(|&track| event_argument(app, track).is_some())
    else {
        return;
    };
    let first = tracks[at];
    let second = tracks.get(at + 1).copied().unwrap_or(first);
    let scenes: Vec<SceneId> = app.project().scenes.iter().map(|s| s.id).collect();
    let volume = add(
        app,
        cc(1, 21),
        ControlRef::Argument(FieldRef::volume(first)),
    );
    let _ = add(
        app,
        cc(1, 22),
        ControlRef::Argument(FieldRef::volume(second)),
    );
    let encoder = event_argument(app, first).and_then(|(event, field)| {
        add(
            app,
            cc(1, 30),
            ControlRef::EventArgument {
                track: first,
                event,
                field,
            },
        )
    });
    if let Some(&scene) = scenes.get(2).or(scenes.first()) {
        let _ = add(
            app,
            note(60),
            ControlRef::ClipSlot {
                track: first,
                scene,
            },
        );
    }
    let solo = add(app, note(62), ControlRef::TrackSolo(second));
    let bang = event_argument(app, first).and_then(|(event, _)| {
        add(
            app,
            note(64),
            ControlRef::Event {
                track: first,
                event,
            },
        )
    });
    let mapping = |edit| Command::Edit(ProjectEdit::EditMapping(edit));
    let selected = match case {
        "cc" => {
            if let Some(id) = volume {
                let _ = crate::edit_actions::run_edit(
                    app,
                    mapping(MappingEdit::SetTakeover {
                        mapping: id,
                        takeover: Takeover::ValueScaling,
                    }),
                );
            }
            volume
        }
        "note" => {
            if let Some(id) = solo {
                let _ = crate::edit_actions::run_edit(
                    app,
                    mapping(MappingEdit::SetMode {
                        mapping: id,
                        mode: SourceMode::Gate,
                    }),
                );
            }
            solo
        }
        "encoder" => {
            if let Some(id) = encoder {
                for edit in [
                    MappingEdit::SetMode {
                        mapping: id,
                        mode: SourceMode::Relative(RelativeEncoding::TwosComplement),
                    },
                    MappingEdit::SetSensitivity {
                        mapping: id,
                        sensitivity: 2.5,
                    },
                ] {
                    let _ = crate::edit_actions::run_edit(app, mapping(edit));
                }
            }
            encoder
        }
        "trigger" => bang,
        _ => None,
    };
    app.map_mode.selected_row = selected;
}

/// Key Map Mode's case: a key on a Bang selected, among a key on a switch and on a slot.
fn inspect_keys(app: &mut App) {
    let Some(track) = app
        .project()
        .tracks
        .iter()
        .map(|track| track.id)
        .find(|&track| event_argument(app, track).is_some())
    else {
        return;
    };
    let key = |text: &str| {
        Source::Key(KeySource {
            key: KeyName(text.into()),
        })
    };
    let bang = event_argument(app, track)
        .and_then(|(event, _)| add(app, key("a"), ControlRef::Event { track, event }));
    let _ = add(app, key("s"), ControlRef::TrackSolo(track));
    if let Some(scene) = app.project().scenes.first().map(|scene| scene.id) {
        let _ = add(app, key("1"), ControlRef::ClipSlot { track, scene });
    }
    app.map_mode.selected_row = bang;
}
