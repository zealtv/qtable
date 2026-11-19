//! The Mixer row: numbered Activator and Solo beside a normalized volume fader. Geometry follows the read-only Live
//! capture of 2026-10-12.
//! Group tracks share the control layout and alone show the half-coloured Solo.
use crate::app::App;
use crate::control::{Fader, Lit, SizeRole, Solo, Toggle};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::{Control, ControlId};
use crate::info::Id;
use crate::messages::{Message, OutputMessage};
use crate::style::detail;
use iced::keyboard::{Key, Modifiers, key::Named};
use iced::widget::{Space, column, container, row};
use iced::{Element, Length};
use qtable_engine::model::{ControlRef, SoloLook, TrackId, VOLUME};

const SWITCH_W: f32 = 22.0;
const ACTIVATOR_H: f32 = 22.0;
const SOLO_H: f32 = 13.0;
const CONTROLS_H: f32 = 96.0;
const SWITCH_GAP: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum MixerMessage {
    /// Set the volume and send it: a typed value, Delete, or a committed fader drag.
    SetVolume(TrackId, f64),
    /// One step of a fader drag; the track's send mode decides whether it sends.
    DragVolume(TrackId, f64),
    /// The drag ended (or the window lost focus) at this value.
    ReleaseVolume(TrackId, f64),
    Activator(TrackId),
    Solo(TrackId),
    OutputSolo(TrackId),
    /// S: solo the selected tracks.
    SoloSelected,
    /// F1–F8: toggle the Activator of the track at this position.
    ToggleActivatorAt(usize),
}

/// Live's mixer keys (manual, Keyboard Shortcuts): S solos the selected tracks and F1–F8
/// toggle the first eight tracks' Activators. Only keys no text field took.
pub(crate) fn key(
    key: &Key,
    modifiers: Modifiers,
    repeat: bool,
    ignored: bool,
) -> Option<MixerMessage> {
    if repeat || !ignored || modifiers != Modifiers::NONE {
        return None;
    }
    let function = [
        Named::F1,
        Named::F2,
        Named::F3,
        Named::F4,
        Named::F5,
        Named::F6,
        Named::F7,
        Named::F8,
    ];
    match key.as_ref() {
        Key::Character("s" | "S") => Some(MixerMessage::SoloSelected),
        Key::Named(named) => function
            .iter()
            .position(|f| *f == named)
            .map(MixerMessage::ToggleActivatorAt),
        _ => None,
    }
}

/// Every strip has a volume fader with activity in its well. Tracks, groups and outputs
/// also have Activator and Solo switches; Main has neither mute nor solo.
pub(crate) fn mixer(app: &App, strip: TrackId) -> Element<'_, Message> {
    let project = app.project();
    let Some(facts) = project.track_facts(strip) else {
        return Space::new().into();
    };
    let snapshot = &app.snapshot;
    let (level, caption) = if facts.capabilities.destination {
        (
            activity_level(&snapshot.output_activity, strip),
            output_caption(project, strip),
        )
    } else {
        (
            track_level(project, &snapshot.track_activity, strip),
            facts.number.unwrap_or_default().to_string(),
        )
    };
    let switches = facts.capabilities.switched.then(|| {
        column![
            switch(app, strip, caption, facts.mixer.active),
            Space::new().height(SWITCH_GAP),
            solo(app, strip, facts.solo_look),
        ]
        .into()
    });
    controls_cell(app, switches, level, strip)
}

/// Keep the buttons low beside the full-height fader, leaving the MIDI strip's blank space above.
fn controls_cell<'a>(
    app: &'a App,
    switches: Option<Element<'a, Message>>,
    lit: f32,
    strip: TrackId,
) -> Element<'a, Message> {
    let mut left = column![Space::new().height(48)];
    if let Some(switches) = switches {
        left = left.push(switches);
    }
    let controls = row![
        Space::new().width(7),
        left,
        Space::new().width(Length::Fill),
        match volume_control(app, strip, lit) {
            Some(control) => {
                let value = control.kind.value;
                crate::parameter_gesture::menu(
                    app,
                    app.controls().fader(
                        control,
                        value,
                        app.selection.shift_down,
                        move |value| Message::Mixer(MixerMessage::DragVolume(strip, value)),
                        move |value| Message::Mixer(MixerMessage::ReleaseVolume(strip, value)),
                    ),
                    &crate::info::Subject::Volume(strip),
                )
            }
            None => Space::new().into(),
        },
        Space::new().width(12),
    ]
    .height(CONTROLS_H);
    container(controls)
        .padding(detail::STRIP_INSET)
        .width(Length::Fill)
        .into()
}

/// The same descriptor and drag host serve Main and the other volume-capable strips.
fn volume_control(app: &App, strip: TrackId, level: f32) -> Option<Control<Fader>> {
    let track = app.project().track(strip)?;
    track.capabilities().has_volume.then(|| {
        Control::new(
            ControlId::action(View::Mixer, "volume", ControlOwner::Track(strip)),
            Id::Volume,
            Fader {
                value: crate::mixer_device::shown_volume(app, track),
                level,
            },
        )
        .drives(ControlRef::Argument(crate::mixer_device::volume_field(
            strip,
        )))
        .enabled(track.mixer_device.volume.output.enabled)
        .default_to(VOLUME.default)
    })
}

fn is_group(app: &App, strip: TrackId) -> bool {
    app.project()
        .track_capabilities(strip)
        .is_some_and(|c| c.foldable)
}

fn is_output(app: &App, strip: TrackId) -> bool {
    app.project()
        .track_capabilities(strip)
        .is_some_and(|c| c.destination)
}

/// Live's Track Activator: lit orange with the track number (or the output's positional letter) while the
/// track plays out, grey when it is muted.
fn switch(app: &App, strip: TrackId, caption: String, active: bool) -> Element<'_, Message> {
    let (message, help) = if is_output(app, strip) {
        (
            Message::Outputs(OutputMessage::ToggleOutputEnabled(strip)),
            Id::OutputEnable,
        )
    } else {
        (
            Message::Mixer(MixerMessage::Activator(strip)),
            if is_group(app, strip) {
                Id::GroupActivator
            } else {
                Id::TrackActivator
            },
        )
    };
    app.controls().toggle(
        Control::new(
            ControlId::action(View::Mixer, "activator", ControlOwner::Track(strip)),
            help,
            Toggle { label: caption },
        )
        .drives(ControlRef::TrackActivator(strip))
        .role(SizeRole::Fixed {
            width: SWITCH_W,
            height: ACTIVATOR_H,
        })
        .lit(active),
        message,
    )
}

/// Solo, drawn so a group with only some soloed members can show Live's half-coloured
/// button (manual 18.3).
fn solo(app: &App, strip: TrackId, look: SoloLook) -> Element<'_, Message> {
    let (message, help) = if is_output(app, strip) {
        (
            Message::Mixer(MixerMessage::OutputSolo(strip)),
            Id::OutputSolo,
        )
    } else {
        (
            Message::Mixer(MixerMessage::Solo(strip)),
            if is_group(app, strip) {
                Id::GroupSolo
            } else {
                Id::TrackSolo
            },
        )
    };
    let lit = match look {
        SoloLook::On => Lit::On,
        SoloLook::Partial => Lit::Partial,
        SoloLook::Off => Lit::Off,
    };
    app.controls().canvas(
        Control::new(
            ControlId::action(View::Mixer, "solo", ControlOwner::Track(strip)),
            help,
            Solo,
        )
        .drives(ControlRef::TrackSolo(strip))
        .role(SizeRole::Fixed {
            width: SWITCH_W,
            height: SOLO_H,
        })
        .lit_as(lit),
        Some(message),
    )
}

/// A group's meter follows its busiest descendant clip track, including folded leaves.
fn track_level(
    project: &qtable_engine::model::Project,
    levels: &[(TrackId, f32)],
    strip: TrackId,
) -> f32 {
    if project.track(strip).is_some_and(|track| track.is_group()) {
        project
            .descendant_clip_track_ids(strip)
            .unwrap_or_default()
            .into_iter()
            .fold(0.0, |level, id| f32::max(level, activity_level(levels, id)))
    } else {
        activity_level(levels, strip)
    }
}

fn activity_level<K: PartialEq>(levels: &[(K, f32)], id: K) -> f32 {
    levels
        .iter()
        .find(|(key, _)| *key == id)
        .map_or(0.0, |(_, value)| *value)
}

fn output_caption(project: &qtable_engine::model::Project, output: TrackId) -> String {
    project
        .outputs
        .iter()
        .position(|track| track.id == output)
        .map(output_letter)
        .unwrap_or_default()
}

/// A return-style caption derived from the zero-based position in the outputs region.
fn output_letter(mut position: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push((b'A' + (position % 26) as u8) as char);
        if position < 26 {
            break;
        }
        position = position / 26 - 1;
    }
    letters.into_iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_group_meter_includes_nested_sending_leaves() {
        use qtable_engine::model::{Project, Route, Track};
        let mut project = Project::default();
        let outer = Track::new_group(TrackId(1), "Outer", 0);
        let mut inner = Track::new_group(TrackId(2), "Inner", 0);
        inner.group = Some(outer.id);
        inner.route = Route::Group;
        let mut leaf = Track::new(TrackId(3), "Sending leaf", 0);
        leaf.group = Some(inner.id);
        leaf.route = Route::Group;
        project.tracks = vec![outer, inner, leaf];
        project.validate().unwrap();
        let activity = [(TrackId(3), 0.8)];
        for folded in [false, true] {
            project.tracks[0].fold_state = folded;
            assert_eq!(track_level(&project, &activity, TrackId(1)), 0.8);
            assert_eq!(track_level(&project, &activity, TrackId(2)), 0.8);
            assert_eq!(track_level(&project, &activity, TrackId(3)), 0.8);
            assert_eq!(track_level(&project, &[], TrackId(1)), 0.0);
        }
    }

    #[test]
    fn main_and_other_strips_share_the_volume_control() {
        use iced::advanced::{
            Layout, layout,
            widget::{Operation, Tree},
        };
        use iced::{Rectangle, Size};
        use qtable_engine::{
            engine::Engine,
            model::{Project, Track},
        };
        use std::any::Any;
        let mut project = Project::default();
        project.main.mixer_device.volume.value = 0.375;
        let mut member = Track::new(TrackId(1), "Track", 0);
        member.group = Some(TrackId(2));
        member.route = qtable_engine::model::Route::Group;
        project
            .tracks
            .push(Track::new_group(TrackId(2), "Group", 0));
        project.tracks.push(member);
        project
            .outputs
            .push(Track::new_output(TrackId(3), "Output", "127.0.0.1", 9001));
        let mut app = App::new(Engine::new(project).unwrap(), "120".into(), None);
        // Focus wrappers expose their actual layout bounds through widget operations.
        #[derive(Default)]
        struct FaderBounds(Vec<Rectangle>);
        impl Operation for FaderBounds {
            fn custom(
                &mut self,
                _: Option<&iced::advanced::widget::Id>,
                bounds: Rectangle,
                state: &mut dyn Any,
            ) {
                // The help wrapper's declaration shares the focus wrapper's bounds; count the
                // focus wrapper alone.
                if state.is::<crate::control::help::Declared>() {
                    return;
                }
                if bounds.width == 27.0 && bounds.height == CONTROLS_H {
                    self.0.push(bounds);
                }
            }
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
        }
        let renderer = crate::grid::test_renderer();
        for strip in [app.project().main.id, TrackId(1), TrackId(2), TrackId(3)] {
            let control = volume_control(&app, strip, 0.6).unwrap();
            assert_eq!(
                control.id,
                ControlId::action(View::Mixer, "volume", ControlOwner::Track(strip))
            );
            assert_eq!(control.identity.help, Id::Volume);
            assert_eq!(
                control.kind.value,
                app.project()
                    .track(strip)
                    .unwrap()
                    .mixer_device
                    .volume
                    .value
            );
            assert_eq!(control.kind.level, 0.6);
            assert!(!control.state.enabled);
            use crate::control::{ControlKind, Interaction, LIVE_DARK};
            assert_eq!(
                control
                    .kind
                    .look(&LIVE_DARK, control.state, Interaction::Idle)
                    .ink,
                LIVE_DARK.text_disabled,
            );
            for focused in [false, true] {
                if focused {
                    app.control_focus.claim(control.id);
                } else {
                    app.control_focus.clear();
                }
                for width in [crate::grid::COLUMN_W, 160.] {
                    let mut widget = mixer(&app, strip);
                    let mut tree = Tree::new(&widget);
                    let node = widget.as_widget_mut().layout(
                        &mut tree,
                        &renderer,
                        &layout::Limits::new(
                            Size::ZERO,
                            Size::new(width, CONTROLS_H + 2.0 * detail::STRIP_INSET),
                        ),
                    );
                    let mut operation = FaderBounds::default();
                    widget.as_widget_mut().operate(
                        &mut tree,
                        Layout::new(&node),
                        &renderer,
                        &mut operation,
                    );
                    assert_eq!(operation.0.len(), 1, "one fader on {strip:?}");
                    let bounds = operation.0[0];
                    let geometry = crate::control::fader::Geometry::new(bounds);
                    assert_eq!(
                        geometry.well.x - 10.0 - bounds.x,
                        bounds.x + bounds.width - (geometry.well.x + geometry.well.width),
                        "centred artwork in the focus bounds on {strip:?}",
                    );
                    assert_eq!(bounds.y, detail::STRIP_INSET);
                    assert!(
                        bounds.x >= detail::STRIP_INSET
                            && bounds.x + bounds.width <= width - detail::STRIP_INSET
                    );
                    assert_eq!(
                        widget.as_widget().mouse_interaction(
                            &tree,
                            Layout::new(&node),
                            iced::mouse::Cursor::Available(bounds.center()),
                            &Rectangle::with_size(node.size()),
                            &renderer,
                        ),
                        iced::mouse::Interaction::None,
                        "normal arrow through the focus and help wrappers on {strip:?}",
                    );
                }
            }
        }
        assert!(!app.project().main.capabilities().switched);
        assert!(volume_control(&app, TrackId(999), 0.0).is_none());
    }

    #[test]
    fn enabling_volume_restores_fader_state_and_shared_ink() {
        use crate::control::{ControlKind, Interaction, LIVE_DARK};
        use qtable_engine::{edit::ProjectEdit, engine::Engine, model::Project};
        let mut app = App::new(Engine::new(Project::default()).unwrap(), "120".into(), None);
        let id = app.project().main.id;
        for enabled in [false, true, false] {
            let mut output = app.project().main.mixer_device.volume.output.clone();
            output.enabled = enabled;
            crate::edit_actions::execute_edit(
                &mut app,
                qtable_engine::engine::Command::Edit(ProjectEdit::SetParameterOutput {
                    target: qtable_engine::model::ParameterRef::volume(id),
                    output,
                }),
            );
            let control = volume_control(&app, id, 0.6).unwrap();
            assert_eq!(control.state.enabled, enabled);
            let look = control
                .kind
                .look(&LIVE_DARK, control.state, Interaction::Idle);
            assert_eq!(
                look.ink,
                if enabled {
                    LIVE_DARK.fader_handle
                } else {
                    LIVE_DARK.text_disabled
                }
            );
            // OSC activity still reports clip sends independently of volume emission.
            assert_eq!(look.accent, LIVE_DARK.orange_vu_meter);
            assert_eq!(control.kind.level, 0.6);
        }
    }

    #[test]
    fn captions_follow_order_after_reorder_delete_and_rename() {
        use qtable_engine::{
            edit::{self, ProjectEdit},
            model::{Project, Track},
        };
        let mut project = Project {
            outputs: (0..3)
                .map(|i| Track::new_output(TrackId(201 + i), "Lights", "localhost", 9000))
                .collect(),
            ..Project::default()
        };
        let a = TrackId(201);
        let b = TrackId(202);
        assert_eq!(output_caption(&project, a), "A");
        edit::apply(
            &mut project,
            ProjectEdit::PlaceTrack {
                track: a,
                before: None,
                group: None,
            },
        )
        .unwrap();
        assert_eq!(output_caption(&project, a), "C");
        assert_eq!(output_caption(&project, b), "A");
        edit::apply(&mut project, ProjectEdit::RemoveTracks { tracks: vec![b] }).unwrap();
        assert_eq!(output_caption(&project, a), "B");
        edit::apply(
            &mut project,
            ProjectEdit::RenameTrack {
                track: a,
                name: "Video".into(),
            },
        )
        .unwrap();
        assert_eq!(output_caption(&project, a), "B");
    }

    #[test]
    fn output_letters_continue_after_z() {
        for (position, expected) in [
            (0, "A"),
            (1, "B"),
            (25, "Z"),
            (26, "AA"),
            (27, "AB"),
            (51, "AZ"),
            (52, "BA"),
            (701, "ZZ"),
            (702, "AAA"),
        ] {
            assert_eq!(output_letter(position), expected);
        }
    }

    #[test]
    fn each_strip_declares_the_controls_it_drives_and_they_describe() {
        use qtable_engine::{
            engine::Engine,
            model::{Project, Track, describe_control},
        };
        let mut project = Project::default();
        let mut member = Track::new(TrackId(1), "Track", 0);
        member.group = Some(TrackId(2));
        member.route = qtable_engine::model::Route::Group;
        project
            .tracks
            .push(Track::new_group(TrackId(2), "Group", 0));
        project.tracks.push(member);
        project
            .outputs
            .push(Track::new_output(TrackId(3), "Output", "127.0.0.1", 9001));
        let app = App::new(Engine::new(project).unwrap(), "120".into(), None);
        let main = app.project().main.id;
        for strip in [TrackId(1), TrackId(2), TrackId(3)] {
            let mut element = mixer(&app, strip);
            let declared = crate::control_focus::test_registry::declared(
                &mut element,
                iced::Size::new(200.0, 700.0),
            );
            let expect = [
                ControlRef::Argument(crate::mixer_device::volume_field(strip)),
                ControlRef::TrackActivator(strip),
                ControlRef::TrackSolo(strip),
            ];
            for control in &expect {
                assert!(declared.contains(control), "{strip:?}: {control:?}");
                assert!(describe_control(app.project(), control).is_some());
            }
        }
        // Main has a Volume fader and no Activator or Solo.
        let mut element = mixer(&app, main);
        let declared = crate::control_focus::test_registry::declared(
            &mut element,
            iced::Size::new(200.0, 700.0),
        );
        assert_eq!(
            declared,
            [ControlRef::Argument(crate::mixer_device::volume_field(
                main
            ))]
        );
    }
}
