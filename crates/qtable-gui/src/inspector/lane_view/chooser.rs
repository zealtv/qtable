//! The Device and Control choosers, Live's pair beside a clip's envelope editor on the
//! Envelopes tab. Each is a dropdown face in the bottom bar; its list opens through the app's one
//! overlay slot, with red LEDs marking existing envelopes.
//!
//! Device chooser: the track's devices in chain order (Targets, then Mixer); the Overview tab
//! took over v3.4's *All lanes* row. Control chooser: *Only show adjusted envelopes*, then the
//! device's parameter arguments; events live in the Events tab, so they aren't listed. An entry
//! with a lane in this clip carries the red dot, and the closed faces carry it too (`leds`), as
//! Live's do. An argument's envelope starts with the first point.
pub(crate) mod entries;
use super::Choice;
use crate::app::App;
use crate::control::automation_mark::Mark;
use crate::control::menu::{self, Anchor};
use crate::control::multi_select::{FaceLed, MultiFace};
use crate::control::{Control, SizeRole};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::{ControlId, Controls};
use crate::info::Id;
use crate::inspector::Message as Inspect;
use crate::messages::Message;
use crate::parameter_gesture::Effective;
use crate::style::detail;
use entries::{Item, RowSpec, row_button};
use iced::{Element, Length, Point, Size};
use qtable_engine::model::{DeviceRef, Project, TrackId};

const PANEL_W: f32 = 190.0;
/// Rows shown before a list scrolls.
const VISIBLE_ROWS: usize = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Which {
    Device,
    Control,
}

/// Where the open list sits, and which chooser it belongs to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpenChooser {
    pub at: Point,
    pub track: TrackId,
    pub slot: usize,
    pub which: Which,
}

impl OpenChooser {
    pub(crate) fn still_valid(&self, app: &App) -> bool {
        app.selection
            .clip
            .is_some_and(|s| (s.track, s.slot) == (self.track, self.slot))
            && app.project().clip(self.track, self.slot).is_some()
    }
}

/// The LEDs the two faces show before their names, as Live's red dots: Device's while any of
/// its arguments has a lane in this clip (the list's LED), Control's while the chosen argument
/// has one, grey once a manual change overrides it (`live`, the argument's value now).
pub(crate) fn leds(
    project: &Project,
    (track, slot): (TrackId, usize),
    choice: &Choice,
    live: Option<Effective>,
) -> [FaceLed; 2] {
    let all = entries::entries(project, track, slot);
    let entry = all.iter().find(|d| Some(d.device) == choice.device);
    let lit = |on: bool, mark: Mark| {
        if on {
            FaceLed::Lit(mark)
        } else {
            FaceLed::Unlit
        }
    };
    let device = entry.is_some_and(|d| d.items.iter().any(|item| item.laned));
    let control = entry.is_some_and(|d| {
        d.items
            .iter()
            .any(|item| item.laned && Some(&item.target) == choice.argument.as_ref())
    });
    let overridden = if live.is_some_and(|l| l.overridden()) {
        Mark::Overridden
    } else {
        Mark::Following
    };
    [lit(device, Mark::Following), lit(control, overridden)]
}

/// The two closed faces for the Envelopes tab's bottom bar, each with its LED (`leds`). Control
/// is disabled (face "—") while no device is chosen.
pub(crate) fn faces<'a>(
    project: &Project,
    controls: &Controls<'_>,
    (track, slot): (TrackId, usize),
    (choice, live): (&Choice, Option<Effective>),
) -> [Element<'a, Inspect>; 2] {
    let [device_led, control_led] = leds(project, (track, slot), choice, live);
    let owner = project.track(track);
    let device_text = match choice.device {
        None => "—".to_owned(),
        Some(DeviceRef::Mixer) => "Mixer".to_owned(),
        Some(DeviceRef::Chain(id)) => owner
            .and_then(|t| t.device(id))
            .map(|d| d.name.clone())
            .unwrap_or_default(),
    };
    let control_text = choice
        .argument
        .as_ref()
        .and_then(|a| owner.map(|t| crate::inspector::lane_model::argument_names(t, a).1))
        .unwrap_or_else(|| "—".into());
    let face = |name: &'static str, help: Id, (text, led): (String, FaceLed), enabled, which| {
        controls.multi_face(
            Control::new(
                ControlId::action(View::Clip, name, ControlOwner::ClipSlot(track, slot)),
                help,
                MultiFace {
                    led,
                    pips: vec![],
                    text,
                    text_colour: None,
                },
            )
            .role(SizeRole::Fixed {
                width: detail::DEVICE_CONTROL_WIDTH,
                height: detail::CONTROL_HEIGHT,
            })
            .enabled(enabled),
            Inspect::OpenChooser(track, slot, which),
        )
    };
    [
        face(
            "device-chooser",
            Id::LaneDeviceChooser,
            (device_text, device_led),
            true,
            Which::Device,
        ),
        face(
            "control-chooser",
            Id::LaneControlChooser,
            (control_text, control_led),
            choice.device.is_some(),
            Which::Control,
        ),
    ]
}

/// The device chooser's rows: each device in chain order. The current one is highlighted.
fn device_rows(project: &Project, track: TrackId, slot: usize, choice: &Choice) -> Vec<RowSpec> {
    self::entries::entries(project, track, slot)
        .into_iter()
        .map(|entry| RowSpec {
            label: entry.name,
            led: false,
            checked: false,
            press: Some(Inspect::ChooseDevice(track, slot, entry.device)),
            open: choice.device == Some(entry.device),
        })
        .collect()
}

/// The control chooser's rows for the chosen device: its arguments. An argument without a lane
/// can always be picked (its view invites the first point).
fn control_rows(project: &Project, track: TrackId, slot: usize, choice: &Choice) -> Vec<RowSpec> {
    let mut rows = vec![RowSpec {
        label: "Only show adjusted envelopes".into(),
        led: false,
        checked: choice.only_adjusted,
        press: Some(Inspect::OnlyAdjusted(track, slot)),
        open: false,
    }];
    let Some(device) = choice.device else {
        return rows;
    };
    let Some(entry) = self::entries::entries(project, track, slot)
        .into_iter()
        .find(|d| d.device == device)
    else {
        return rows;
    };
    for item in entry.items {
        let Item {
            target,
            label,
            laned,
        } = item;
        if choice.only_adjusted && !laned {
            continue;
        }
        rows.push(RowSpec {
            open: choice.argument.as_ref() == Some(&target),
            label,
            led: laned,
            checked: false,
            press: Some(Inspect::ChooseControl(track, slot, target)),
        });
    }
    rows
}

/// The rows an open chooser shows (also what the tests read).
pub(crate) fn rows(app: &App, open: OpenChooser) -> Vec<RowSpec> {
    let choice = super::current(app, open.track, open.slot);
    match open.which {
        Which::Device => device_rows(app.project(), open.track, open.slot, &choice),
        Which::Control => control_rows(app.project(), open.track, open.slot, &choice),
    }
}

/// The rows the keyboard reaches (`menu_keys.rs`): those that take a press, with the chosen
/// entry as the place the first arrow key starts from.
pub(crate) fn key_rows(app: &App, open: OpenChooser) -> crate::menu_keys::Rows {
    let rows: Vec<RowSpec> = rows(app, open)
        .into_iter()
        .filter(|row| row.press.is_some())
        .collect();
    crate::menu_keys::Rows {
        current: rows.iter().position(|row| row.open),
        presses: rows
            .into_iter()
            .filter_map(|row| row.press.map(Message::Inspector))
            .collect(),
    }
}

/// The open list, on the side of the click with room (`menu::Anchor`). Once the keyboard has
/// moved, its row takes the highlight from the chosen entry.
pub(crate) fn view<'a>(app: &App, open: OpenChooser) -> Element<'a, Message> {
    let mut reachable = 0;
    let rows: Vec<RowSpec> = rows(app, open)
        .into_iter()
        .map(|mut row| {
            if row.press.is_some() {
                row.open = crate::menu_keys::highlighted(app, reachable, row.open);
                reachable += 1;
            } else if app.menu_highlight.is_some() {
                row.open = false;
            }
            row
        })
        .collect();
    let content = menu::capped_height(rows.iter().map(|_| true), VISIBLE_ROWS);
    let size = Size::new(PANEL_W, menu::panel_height(content));
    let theme = app.theme;
    menu::layer(Anchor::point(open.at), Message::CloseOverlay, move || {
        let list = menu::scrolling(
            rows.iter()
                .cloned()
                .map(|row| row_button(&theme, row, Id::LaneChooserEntry)),
            content,
        );
        let panel = menu::panel(list, size.width, &theme).height(Length::Fixed(size.height));
        (panel.into(), size)
    })
}
