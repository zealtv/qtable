//! Built-in devices and one window-level drag from the Browser to a track.
use crate::control::rule::{Axis, Level, rule};
use crate::style::detail;
use crate::{app::App, focus::Focus, inspector_panel::Tab, messages::Message};
use iced::widget::{Space, column, container, mouse_area, row, scrollable, stack, text};
use iced::{Element, Length, Padding, Point, Rectangle, Size};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::{DeviceCategory, DeviceId, NewDevice, Track, TrackId};
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Instant};

mod region;
#[cfg(test)]
mod tests;

pub(crate) const ITEMS: &[NewDevice] = &[NewDevice::Target];
pub(crate) type Geometry = Rc<RefCell<HashMap<&'static str, Vec<(Rectangle, DropTarget)>>>>;

pub(crate) struct State {
    pub selected: Option<NewDevice>,
    last_press: Option<(NewDevice, Point, Instant)>,
    pub geometry: Geometry,
    reveal: Option<Reveal>,
}

/// Where the Track tab scrolls to: a device, or the Mixer at the end of the chain.
#[derive(Clone, Copy)]
enum Reveal {
    Device(DeviceId),
    Mixer,
}
impl Default for State {
    fn default() -> Self {
        Self {
            selected: Some(NewDevice::Target),
            last_press: None,
            geometry: Default::default(),
            reveal: None,
        }
    }
}

pub(crate) struct Drag {
    pub item: NewDevice,
    origin: Point,
    pub cursor: Point,
    pub moved: bool,
    pub over: Option<DropTarget>,
    pub refused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropTarget {
    Column(TrackId),
    Chain {
        track: TrackId,
        before: Option<DeviceId>,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum Action {
    Press(NewDevice),
    Load,
    Release,
    Cancel,
}

fn name(item: NewDevice) -> &'static str {
    item.build(DeviceId(0)).kind.default_name()
}
fn category(item: NewDevice) -> DeviceCategory {
    item.build(DeviceId(0)).kind.capabilities().category
}

pub(crate) fn view(app: &App) -> Element<'_, Message> {
    let theme = app.theme;
    let selected = app.browser.selected;
    let item_rows = ITEMS.iter().map(|&item| {
        let chosen = selected == Some(item);
        mouse_area(
            container(text(format!("▣ {}", name(item))).size(detail::TEXT_SIZE))
                .padding([2, 4])
                .width(Length::Fill)
                .height(20)
                .style(move |_| iced::widget::container::Style {
                    background: chosen.then_some(theme.selection_background.into()),
                    text_color: Some(if chosen {
                        theme.control_on_foreground
                    } else {
                        theme.control_foreground
                    }),
                    ..Default::default()
                }),
        )
        .on_press(Message::Browser(Action::Press(item)))
        .interaction(iced::mouse::Interaction::Pointer)
        .into()
    });
    let sidebar = column![
        container(text("Library").size(detail::TEXT_SIZE))
            .padding([2, 4])
            .height(20),
        container(text("▸ Targets").size(detail::TEXT_SIZE))
            .padding([2, 4])
            .height(20)
            .style(move |_| iced::widget::container::Style {
                background: Some(theme.surface_highlight.into()),
                ..Default::default()
            }),
        Space::new().height(Length::Fill)
    ]
    .width(96);
    let items = column![
        container(text("Name").size(detail::TEXT_SIZE))
            .padding([2, 4])
            .height(20),
        scrollable(column(item_rows)).height(Length::Fill)
    ]
    .width(Length::Fill);
    container(column![
        container(text("Browser").size(detail::HEADER_SIZE))
            .padding([2, 4])
            .height(20),
        row![sidebar, rule(&theme, Axis::Vertical, Level::Deep), items].height(Length::Fill)
    ])
    .width(app.dividers.browser_width())
    .height(Length::Fill)
    .style(move |_| iced::widget::container::Style {
        background: Some(theme.surface_background.into()),
        text_color: Some(theme.control_foreground),
        border: crate::focus::border(app.focus == Focus::Browser),
        ..Default::default()
    })
    .into()
}

pub(crate) fn update(app: &mut App, action: Action) {
    match action {
        Action::Press(item) => {
            let now = crate::gesture_clock::now();
            app.focus = Focus::Browser;
            app.control_focus.clear();
            app.close_overlay();
            app.browser.selected = Some(item);
            let double = app
                .browser
                .last_press
                .is_some_and(|(last, position, time)| {
                    last == item
                        && position.distance(app.cursor) < 6.0
                        && now.saturating_duration_since(time).as_millis() <= 300
                });
            app.browser.last_press = (!double).then_some((item, app.cursor, now));
            if double {
                app.device_drag = None;
                load_selected(app);
            } else {
                app.device_drag = Some(Drag {
                    item,
                    origin: app.cursor,
                    cursor: app.cursor,
                    moved: false,
                    over: None,
                    refused: false,
                });
            }
        }
        Action::Load => {
            app.device_drag = None;
            app.browser.last_press = None;
            load_selected(app);
        }
        Action::Release => {
            if let Some(drag) = app.device_drag.take()
                && drag.moved
            {
                app.browser.last_press = None;
                if let Some(target) = hit(&app.browser.geometry, app.cursor)
                    && accepts(app, drag.item, target)
                {
                    load(app, drag.item, target);
                }
            }
        }
        Action::Cancel => {
            app.info.update(crate::info::Hover::Clear);
            app.device_drag = None;
            app.browser.last_press = None;
        }
    }
}

fn load_selected(app: &mut App) {
    let Some(item) = app.browser.selected else {
        return;
    };
    let track = app.selection.track().or(match app.selection.owner {
        Some(crate::selection::ColumnOwner::Track(id)) => Some(id),
        _ => None,
    });
    if let Some(track) = track {
        load(app, item, DropTarget::Column(track));
    } else {
        app.status.show(
            "Select a track to load a device",
            crate::status::Severity::Notice,
        );
    }
}

fn load(app: &mut App, kind: NewDevice, target: DropTarget) {
    let (track, before) = match target {
        DropTarget::Column(track) => (track, None),
        DropTarget::Chain { track, before } => (track, before),
    };
    let Some(model) = app.project().track(track) else {
        return;
    };
    if !model.capabilities().device_chain {
        return;
    }
    let old: Vec<_> = model.devices.iter().map(|d| d.id).collect();
    if !crate::edit_actions::run_edit(
        app,
        Command::Edit(ProjectEdit::AddDevice {
            track,
            kind,
            before,
        }),
    ) {
        return;
    }
    let model = app.project().track(track).unwrap();
    let device = model
        .devices
        .iter()
        .find(|d| !old.contains(&d.id))
        .unwrap()
        .id;
    let owner = if model.capabilities().foldable {
        crate::selection::ColumnOwner::Group(track)
    } else {
        crate::selection::ColumnOwner::Track(track)
    };
    let tab = match model.capabilities().region {
        qtable_engine::model::Region::Tracks => Tab::Track,
        qtable_engine::model::Region::Outputs => Tab::Output,
        qtable_engine::model::Region::Main => Tab::Main,
    };
    app.selection.clip = None;
    app.selection.scene = None;
    app.selection.clear_cell();
    app.selection.owner = Some(owner);
    app.selection.anchor = Some(track);
    app.selection.tracks = vec![track];
    app.selection.inspector_tab = tab;
    app.chrome.detail_open = true;
    crate::device_chain::reconcile(app);
    app.selection.devices = vec![device];
    app.selection.device_anchor = Some(device);
    request_reveal(app, device);
    app.focus = Focus::Detail;
    app.control_focus.clear();
}

pub(crate) fn keyboard(app: &mut App, key: &iced::keyboard::Key, ignored: bool) -> bool {
    use iced::keyboard::{Key, key::Named};
    if !ignored || app.focus != Focus::Browser || !app.chrome.browser_open {
        return false;
    }
    match key {
        Key::Named(Named::ArrowUp | Named::ArrowDown) => {
            app.browser.selected = ITEMS.first().copied();
            true
        }
        Key::Named(Named::Enter) => {
            update(app, Action::Load);
            true
        }
        _ => false,
    }
}

fn accepts(app: &App, item: NewDevice, target: DropTarget) -> bool {
    let track = match target {
        DropTarget::Column(track) | DropTarget::Chain { track, .. } => track,
    };
    app.project()
        .track(track)
        .is_some_and(|track| track.capabilities().accepts_device(category(item)))
}

pub(crate) fn cursor(app: &mut App, cursor: Point) {
    let target = hit(&app.browser.geometry, cursor);
    let refused = target.is_some_and(|target| {
        app.device_drag
            .as_ref()
            .is_some_and(|drag| !accepts(app, drag.item, target))
    });
    if let Some(drag) = &mut app.device_drag {
        drag.cursor = cursor;
        drag.moved |= cursor.distance(drag.origin) >= 4.0;
        drag.over = target.filter(|_| !refused);
        drag.refused = refused;
        if drag.moved {
            app.browser.last_press = None;
        }
    }
}
fn hit(geometry: &Geometry, cursor: Point) -> Option<DropTarget> {
    geometry
        .borrow()
        .values()
        .flatten()
        .find(|(bounds, _)| bounds.contains(cursor))
        .map(|(_, target)| *target)
}

pub(crate) fn columns<'a, M: 'a>(
    content: Element<'a, M>,
    key: &'static str,
    geometry: Option<Geometry>,
    columns: &[crate::grid::layout::Column],
    xs: &[f32],
    drag: Option<&'a Drag>,
) -> Element<'a, M> {
    let Some(geometry) = geometry else {
        return content;
    };
    let columns: Vec<_> = columns
        .iter()
        .zip(xs)
        .map(|(col, x)| {
            let crate::grid::layout::ColumnKey::Track(track) = col.key;
            (*x, col.width, track, col.title(*x))
        })
        .collect();
    region::region(
        content,
        key,
        geometry,
        move |size| {
            columns
                .iter()
                .map(|&(x, width, track, title)| {
                    (
                        Rectangle::new(Point::new(x, 0.0), Size::new(width, size.height)),
                        DropTarget::Column(track),
                        title,
                    )
                })
                .collect()
        },
        drag,
    )
}

pub(crate) fn chain<'a>(
    app: &'a App,
    track: &'a Track,
    content: Element<'a, Message>,
) -> Element<'a, Message> {
    let item = app
        .device_drag
        .as_ref()
        .map_or(NewDevice::Target, |drag| drag.item);
    region::region(
        content,
        "chain",
        app.browser.geometry.clone(),
        move |size| {
            let mut boundaries = vec![0.0];
            let mut offset = 0.0;
            for device in &track.devices {
                let width = crate::device_chain::width(device);
                boundaries.push(offset + width / 2.0);
                offset += width + crate::style::detail::CHAIN_GAP;
            }
            boundaries.push(size.width);
            (0..=track.devices.len())
                .filter_map(|index| {
                    let start = boundaries[index];
                    let end = boundaries[index + 1];
                    if end <= start {
                        return None;
                    }
                    let before = track.devices.get(index).map(|d| d.id);
                    let clamped = track.insert_index(category(item), before);
                    let before = track.devices.get(clamped).map(|d| d.id);
                    let x: f32 = track.devices[..clamped]
                        .iter()
                        .map(|d| crate::device_chain::width(d) + crate::style::detail::CHAIN_GAP)
                        .sum();
                    Some((
                        Rectangle::new(Point::new(start, 0.0), Size::new(end - start, size.height)),
                        DropTarget::Chain {
                            track: track.id,
                            before,
                        },
                        Rectangle::new(
                            Point::new((x - 2.0).max(0.0), 0.0),
                            Size::new(2.0, size.height),
                        ),
                    ))
                })
                .collect()
        },
        app.device_drag.as_ref(),
    )
}

pub(crate) fn ghost<'a>(app: &'a App, content: Element<'a, Message>) -> Element<'a, Message> {
    let Some(drag) = &app.device_drag else {
        return content;
    };
    if !drag.moved {
        return content;
    }
    let refused = drag.refused;
    let label = container(
        text(if refused {
            "Targets require a Track with clips"
        } else {
            name(drag.item)
        })
        .size(detail::TEXT_SIZE)
        .color(if refused {
            app.theme.text_disabled
        } else {
            app.theme.control_foreground
        }),
    )
    .padding([2, 6])
    .style(crate::style::tooltip);
    stack![
        content,
        container(label).padding(Padding {
            left: (drag.cursor.x + 12.0).max(0.0),
            top: (drag.cursor.y + 12.0).max(0.0),
            ..Padding::ZERO
        })
    ]
    .into()
}

/// Unattended Browser capture; loading uses the same action as Return.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture(app: &mut App) {
    let Ok(mode) = std::env::var("QTABLE_CAPTURE_BROWSER") else {
        return;
    };
    app.chrome.browser_open = true;
    app.focus = Focus::Browser;
    if mode == "loaded" {
        update(app, Action::Load);
    }
}

/// Hold illustrative cross-region feedback through native window activation for captures.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn hold_capture(app: &mut App) {
    let Ok(mode) = std::env::var("QTABLE_CAPTURE_BROWSER") else {
        return;
    };
    let key = match mode.as_str() {
        "grid-drag" => "tracks",
        "chain-drag" => "chain",
        _ => return,
    };
    let zone = app
        .browser
        .geometry
        .borrow()
        .get(key)
        .and_then(|zones| {
            if mode == "grid-drag" {
                zones.get(1).or_else(|| zones.first())
            } else {
                zones.first()
            }
        })
        .copied();
    if let Some((bounds, target)) = zone {
        let cursor = Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + 30.0_f32.min(bounds.height / 2.0),
        );
        app.device_drag = Some(Drag {
            item: NewDevice::Target,
            origin: Point::ORIGIN,
            cursor,
            moved: true,
            over: Some(target),
            refused: false,
        });
    }
}

pub(crate) fn request_reveal(app: &mut App, device: DeviceId) {
    app.browser.reveal = Some(Reveal::Device(device));
}

#[cfg(test)]
pub(crate) fn reveal_is_mixer(app: &App) -> bool {
    matches!(app.browser.reveal, Some(Reveal::Mixer))
}

pub(crate) fn request_reveal_mixer(app: &mut App) {
    app.browser.reveal = Some(Reveal::Mixer);
}

pub(crate) fn reveal(app: &mut App) -> iced::Task<Message> {
    let Some(reveal) = app.browser.reveal.take() else {
        return iced::Task::none();
    };
    let track = match reveal {
        Reveal::Device(device) => app.project().device(device).map(|(track, _)| track),
        Reveal::Mixer => {
            crate::device_chain::shown_track(app).and_then(|id| app.project().track(id))
        }
    };
    let Some(track) = track else {
        return iced::Task::none();
    };
    // The Mixer follows the last device, so it starts where every device has ended.
    let x = track
        .devices
        .iter()
        .take_while(|d| match reveal {
            Reveal::Device(device) => d.id != device,
            Reveal::Mixer => true,
        })
        .map(|d| crate::device_chain::width(d) + crate::style::detail::CHAIN_GAP)
        .sum();
    iced::widget::operation::scroll_to(
        iced::widget::Id::new(crate::device_chain::SCROLL),
        iced::widget::scrollable::AbsoluteOffset {
            x: Some(x),
            y: None,
        },
    )
}
