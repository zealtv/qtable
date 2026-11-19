//! A control's canonical address and back: one leaf per control, built from stable ids. The Info View shows it, remote control
//! will answer on it and an OSCQuery server would publish the same leaves.
use super::*;

const ROOT: &str = "/qtable";

/// The canonical leaf of `control`, without the value argument it takes.
pub fn control_address(control: &ControlRef) -> String {
    match control {
        ControlRef::Argument(field) => {
            let p = &field.parameter;
            format!(
                "{ROOT}/tracks/by-id/{}/devices/{}/parameters/{}/arguments/{}/value",
                p.track.0,
                device(p.device),
                p.key.0,
                field.field.0
            )
        }
        ControlRef::Event { track, event } => format!(
            "{ROOT}/tracks/by-id/{}/devices/by-id/{}/events/{}/fire",
            track.0, event.device.0, event.key.0
        ),
        ControlRef::EventArgument {
            track,
            event,
            field,
        } => format!(
            "{ROOT}/tracks/by-id/{}/devices/by-id/{}/events/{}/arguments/{}/value",
            track.0, event.device.0, event.key.0, field.0
        ),
        ControlRef::ClipSlot { track, scene } => format!(
            "{ROOT}/tracks/by-id/{}/clip_slots/by-scene/{}/fire",
            track.0, scene.0
        ),
        ControlRef::Scene(scene) => format!("{ROOT}/scenes/by-id/{}/fire", scene.0),
        ControlRef::TrackSelect(track) => format!("{ROOT}/tracks/by-id/{}/select", track.0),
        ControlRef::SceneNavigation(control) => {
            format!("{ROOT}/view/scenes/{}", scene_leaf(*control))
        }
        ControlRef::TrackStop(track) => format!("{ROOT}/tracks/by-id/{}/stop", track.0),
        ControlRef::TrackActivator(track) => format!("{ROOT}/tracks/by-id/{}/activator", track.0),
        ControlRef::TrackSolo(track) => format!("{ROOT}/tracks/by-id/{}/solo", track.0),
        ControlRef::Transport(control) => format!("{ROOT}/transport/{}", transport_leaf(*control)),
    }
}

fn scene_leaf(control: SceneNavigation) -> &'static str {
    match control {
        SceneNavigation::Launch => "fire",
        SceneNavigation::CancelLaunch => "cancel",
        SceneNavigation::Up => "up",
        SceneNavigation::Down => "down",
        SceneNavigation::Select => "select",
    }
}

fn device(device: DeviceRef) -> String {
    match device {
        DeviceRef::Mixer => "mixer".to_owned(),
        DeviceRef::Chain(id) => format!("by-id/{}", id.0),
    }
}

fn transport_leaf(control: TransportControl) -> &'static str {
    match control {
        TransportControl::Play => "start",
        TransportControl::Stop => "stop",
        TransportControl::StopAll => "stop_all",
        TransportControl::Tempo => "tempo/value",
        TransportControl::Quantisation => "quantisation/value",
        TransportControl::FollowActions => "follow_actions/value",
        TransportControl::ReEnableAutomation => "re_enable_automation",
    }
}

/// The control a canonical leaf names; `None` for anything else, including a positional
/// alias or a key outside `[a-z0-9_-]+`. It checks the form, not that the ids exist: ask
/// `describe_control` for that.
pub fn parse_control_address(text: &str) -> Option<ControlRef> {
    let rest = text.strip_prefix(ROOT)?.strip_prefix('/')?;
    let parts: Vec<&str> = rest.split('/').collect();
    let id = |part: &str| part.parse::<u64>().ok();
    let key = |part: &str| {
        (!part.is_empty()
            && part
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-'))
        .then(|| part.to_owned())
    };
    let control = match parts.as_slice() {
        ["transport", leaf @ ..] => {
            let leaf = leaf.join("/");
            ControlRef::Transport(
                TransportControl::ALL
                    .into_iter()
                    .find(|c| transport_leaf(*c) == leaf)?,
            )
        }
        ["view", "scenes", leaf] => ControlRef::SceneNavigation(
            SceneNavigation::ALL
                .into_iter()
                .find(|c| scene_leaf(*c) == *leaf)?,
        ),
        ["scenes", "by-id", scene, "fire"] => ControlRef::Scene(SceneId(id(scene)?)),
        ["tracks", "by-id", track, tail @ ..] => {
            let track = TrackId(id(track)?);
            match tail {
                ["select"] => ControlRef::TrackSelect(track),
                ["stop"] => ControlRef::TrackStop(track),
                ["activator"] => ControlRef::TrackActivator(track),
                ["solo"] => ControlRef::TrackSolo(track),
                ["clip_slots", "by-scene", scene, "fire"] => ControlRef::ClipSlot {
                    track,
                    scene: SceneId(id(scene)?),
                },
                ["devices", "by-id", device, "events", event, "fire"] => ControlRef::Event {
                    track,
                    event: EventRef {
                        device: DeviceId(id(device)?),
                        key: EntryKey(key(event)?),
                    },
                },
                [
                    "devices",
                    "by-id",
                    device,
                    "events",
                    event,
                    "arguments",
                    field,
                    "value",
                ] => ControlRef::EventArgument {
                    track,
                    event: EventRef {
                        device: DeviceId(id(device)?),
                        key: EntryKey(key(event)?),
                    },
                    field: FieldKey(key(field)?),
                },
                [
                    "devices",
                    "mixer",
                    "parameters",
                    parameter,
                    "arguments",
                    argument,
                    "value",
                ] => argument_ref(track, DeviceRef::Mixer, key(parameter)?, key(argument)?),
                [
                    "devices",
                    "by-id",
                    device,
                    "parameters",
                    parameter,
                    "arguments",
                    argument,
                    "value",
                ] => argument_ref(
                    track,
                    DeviceRef::Chain(DeviceId(id(device)?)),
                    key(parameter)?,
                    key(argument)?,
                ),
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(control)
}

fn argument_ref(
    track: TrackId,
    device: DeviceRef,
    parameter: String,
    argument: String,
) -> ControlRef {
    ControlRef::Argument(FieldRef {
        parameter: ParameterRef {
            track,
            device,
            key: ParameterKey(parameter),
        },
        field: FieldKey(argument),
    })
}
