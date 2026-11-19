//! One dispatch for the closed set of stored device kinds.
use super::target_face;
use crate::{app::App, control_focus::View, messages::Message};
use iced::Element;
use qtable_engine::model::{Device, DeviceKind};

pub(super) fn view<'a>(app: &'a App, device: &'a Device, location: View) -> Element<'a, Message> {
    match &device.kind {
        DeviceKind::Target(_) => target_face::view(app, device, location),
    }
}

pub(crate) fn width(device: &Device) -> f32 {
    match device.kind {
        // Expanding adds the entry editor left of the face and argument inspector right.
        DeviceKind::Target(ref target) if device.expanded => {
            target_face::width(target) + 1.0 + target_face::EDITOR_WIDTH
        }
        DeviceKind::Target(ref target) => target_face::width(target),
    }
}
