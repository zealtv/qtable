//! Output columns: one return-style column per output, right of the tracks and left of Main.
use crate::app::App;
use crate::grid::COLUMN_W;
use crate::info::Id;
use crate::messages::{Message, OutputMessage};
use crate::style::detail;
use iced::widget::text;
use iced::{Color, Element};
use qtable_engine::model::{Colour, Track, UdpMode};

const GAP: f32 = 1.0;
/// Columns shown before the area scrolls sideways, so Main always stays visible.
const MAX_VISIBLE: usize = 3;

/// Width the output area takes in the grid row; zero while collapsed.
pub(crate) fn width(app: &App) -> f32 {
    let outputs = &app.project().outputs;
    if !app.chrome.outputs_open {
        0.0
    } else if outputs.is_empty() {
        COLUMN_W
    } else {
        let visible = if crate::capture::all_outputs() {
            outputs.len()
        } else {
            outputs.len().min(MAX_VISIBLE)
        };
        visible as f32 * (COLUMN_W + GAP)
    }
}

/// Broadcast and enabled toggles in the Detail View only.
pub(crate) fn mode_toggles<'a>(
    app: &'a App,
    output: &Track,
    view: crate::control_focus::View,
) -> Element<'a, Message> {
    use crate::control::{SizeRole, Toggle};
    use crate::control_focus::ControlOwner;
    use crate::drafts::Control;
    use crate::drafts::ControlId;
    let id = output.id;
    let controls = app.controls();
    let toggle = |name: &'static str, help: Id, label: &str, on: bool, message| {
        controls.toggle(
            Control::new(
                ControlId::action(view, name, ControlOwner::Track(id)),
                help,
                Toggle {
                    label: label.into(),
                },
            )
            .role(SizeRole::Fill)
            .lit(on),
            Message::Outputs(message),
        )
    };
    let broadcast = toggle(
        "broadcast",
        Id::OutputMode,
        "Bcast",
        output
            .destination()
            .or(app.dormant_destinations.get(&id).map(|o| &o.destination))
            .is_some_and(|d| d.mode == UdpMode::Broadcast),
        OutputMessage::ToggleOutputMode(id),
    );
    if !output.capabilities().switched {
        return broadcast;
    }
    detail::pair(
        broadcast,
        toggle(
            "enabled",
            Id::OutputEnable,
            "On",
            output.mixer.active,
            OutputMessage::ToggleOutputEnabled(id),
        ),
    )
    .into()
}

/// Why the output could not resolve its host, when the engine reported one.
pub(crate) fn unresolved_note<'a>(app: &App, output: &Track) -> Option<Element<'a, Message>> {
    let item = app
        .snapshot
        .unresolved_outputs
        .iter()
        .find(|u| u.id == output.id)?;
    Some(
        text(format!("unresolved: {}", item.reason))
            .size(detail::TEXT_SIZE)
            .color(app.theme.chosen_alert)
            .into(),
    )
}

pub(crate) fn to_colour(colour: Colour) -> Color {
    Color::from_rgb8(colour.red, colour.green, colour.blue)
}
