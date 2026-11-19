//! Parameter choices for the Envelopes tab, in track device order.
use crate::control::menu::{self, Check, Lead};
use crate::info::{Help, Id};
use crate::inspector::lane_model::argument_names;
use crate::messages::Message;
use iced::Element;
use qtable_engine::model::{ArgumentRef, DeviceRef, ParameterKey, Project, TrackId};

/// One entry of a device's list.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub target: ArgumentRef,
    pub label: String,
    pub laned: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DeviceItems {
    pub device: DeviceRef,
    pub name: String,
    pub items: Vec<Item>,
}

/// Every device of the track with its parameter arguments, as the choosers list them:
/// Target devices in chain order, then Mixer.
pub(crate) fn entries(project: &Project, track: TrackId, slot: usize) -> Vec<DeviceItems> {
    let (Some(owner), Some(clip)) = (project.track(track), project.clip(track, slot)) else {
        return vec![];
    };
    let laned = |target: &ArgumentRef| {
        clip.lanes
            .iter()
            .any(|l| l.envelope().is_some_and(|e| e.target == *target))
    };
    let mut out: Vec<DeviceItems> = owner
        .targets()
        .map(|(device, target)| {
            let mut items = Vec::new();
            for parameter in &target.parameters {
                for field in &parameter.fields {
                    let argument = ArgumentRef {
                        device: DeviceRef::Chain(device.id),
                        parameter: ParameterKey(parameter.key.0.clone()),
                        field: field.key.clone(),
                    };
                    items.push(Item {
                        label: argument_names(owner, &argument).1,
                        laned: laned(&argument),
                        target: argument,
                    });
                }
            }
            DeviceItems {
                device: DeviceRef::Chain(device.id),
                name: device.name.clone(),
                items,
            }
        })
        .collect();
    let volume = ArgumentRef::volume();
    out.push(DeviceItems {
        device: DeviceRef::Mixer,
        name: "Mixer".into(),
        items: vec![Item {
            label: "Volume".into(),
            laned: laned(&volume),
            target: volume,
        }],
    });
    out
}

/// One row, as data, so the responsive layer can rebuild it for each size.
#[derive(Clone)]
pub(crate) struct RowSpec {
    pub(crate) label: String,
    pub(crate) led: bool,
    /// A ✓ in the check column (*Only show adjusted envelopes* while on).
    pub(crate) checked: bool,
    pub(crate) press: Option<crate::inspector::Message>,
    pub(crate) open: bool,
}

/// One chooser row in the shared menu row: the LED in the lead column (its room kept while
/// unlit, so names line up), the current entry highlighted.
pub(crate) fn row_button<'a>(
    theme: &crate::control::Theme,
    spec: RowSpec,
    help: Id,
) -> Element<'a, Message> {
    let RowSpec {
        label,
        led,
        checked,
        press,
        open,
    } = spec;
    menu::item(
        theme,
        menu::Item::new(label, help)
            .check(if checked { Check::Tick } else { Check::None })
            .lead(Lead::Dot(led.then_some(theme.automation_color)))
            .highlighted(open)
            .on_press_maybe(press.map(Message::Inspector)),
        |row, help| row.info(help, Message::Info),
    )
}
