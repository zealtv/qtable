//! Live-style window regions. Visibility belongs to the GUI session, not the set.
use crate::{
    app::App,
    dividers::{self, Divider},
    focus::Focus,
    grid::Row,
    info::{self, Id},
    messages::Message,
    status, transport,
};
use iced::keyboard::{Key, Modifiers};
use iced::widget::{Space, column, container, mouse_area, row, tooltip};
use iced::{Border, Element, Length, Theme};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Region {
    Browser,
    Detail,
    Info,
    Outputs,
    /// The strip switch, Live's mixer icon (⌘⌥M): shows or hides every checked section.
    Strip,
    /// One strip section's check (Output ⌘⌥I, or Path, Mixer and Throughput from the menu).
    Section(Row),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct State {
    pub(crate) browser_open: bool,
    pub(crate) detail_open: bool,
    pub(crate) info_open: bool,
    pub(crate) outputs_open: bool,
    /// The strip switch: the checked sections show only while it is on.
    pub(crate) strip_open: bool,
    /// The four sections' checks, in `Row` order.
    pub(crate) output_open: bool,
    pub(crate) path_open: bool,
    pub(crate) mixer_open: bool,
    pub(crate) throughput_open: bool,
    /// Startup request (`--sections-menu-open`): open the sections menu once, for screenshots.
    #[cfg(any(test, feature = "dev-hooks"))]
    pub(crate) start_with_sections_menu: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            // Retain the compact startup layout; the Browser opens with its switch or ⌥5.
            browser_open: false,
            detail_open: true,
            info_open: false,
            outputs_open: true,
            strip_open: true,
            output_open: true,
            path_open: true,
            mixer_open: true,
            throughput_open: true,
            #[cfg(any(test, feature = "dev-hooks"))]
            start_with_sections_menu: false,
        }
    }
}

impl State {
    /// A section's check.
    pub(crate) fn section(&self, row: Row) -> bool {
        match row {
            Row::Output => self.output_open,
            Row::Path => self.path_open,
            Row::Mixer => self.mixer_open,
            Row::Throughput => self.throughput_open,
        }
    }

    fn section_mut(&mut self, row: Row) -> &mut bool {
        match row {
            Row::Output => &mut self.output_open,
            Row::Path => &mut self.path_open,
            Row::Mixer => &mut self.mixer_open,
            Row::Throughput => &mut self.throughput_open,
        }
    }

    /// The rows on screen: the checked sections while the strip switch is on.
    pub(crate) fn rows(&self) -> Vec<Row> {
        [Row::Output, Row::Path, Row::Mixer, Row::Throughput]
            .into_iter()
            .filter(|row| self.strip_open && self.section(*row))
            .collect()
    }

    pub(crate) fn toggle(&mut self, region: Region) {
        let open = match region {
            Region::Browser => &mut self.browser_open,
            Region::Detail => &mut self.detail_open,
            Region::Info => &mut self.info_open,
            Region::Outputs => &mut self.outputs_open,
            Region::Strip => &mut self.strip_open,
            Region::Section(row) => {
                let checked = !self.section(row);
                *self.section_mut(row) = checked;
                // Checking a section while the strip is hidden shows it, so the choice is visible.
                self.strip_open |= checked;
                return;
            }
        };
        *open = !*open;
    }
}

/// Match the base key, before Option transforms B/L into other characters.
/// Ignore auto-repeat so holding the shortcut does not flicker the region.
pub(crate) fn shortcut(key: &Key, modifiers: Modifiers, repeat: bool) -> Option<Region> {
    if repeat {
        return None;
    }
    if matches!(key.as_ref(), Key::Character("?"))
        && !modifiers.intersects(Modifiers::COMMAND | Modifiers::ALT | Modifiers::CTRL)
    {
        return Some(Region::Info);
    }
    if modifiers != Modifiers::COMMAND | Modifiers::ALT {
        return None;
    }
    match key.as_ref() {
        Key::Character("b" | "B") => Some(Region::Browser),
        Key::Character("l" | "L") => Some(Region::Detail),
        Key::Character("r" | "R") => Some(Region::Outputs),
        Key::Character("i" | "I") => Some(Region::Section(Row::Output)),
        Key::Character("m" | "M") => Some(Region::Strip),
        _ => None,
    }
}

/// ⌘⌥T inserts an output, as it inserts a return track in Live.
pub(crate) fn insert_output_shortcut(key: &Key, modifiers: Modifiers, repeat: bool) -> bool {
    !repeat
        && modifiers == Modifiers::COMMAND | Modifiers::ALT
        && matches!(key.as_ref(), Key::Character("t" | "T"))
}

/// Use the typed character for ?, but the base key for Command-Option shortcuts.
/// A field which consumed ? keeps it as text instead of toggling the panel.
pub(crate) fn event_shortcut(
    key: &Key,
    modified_key: &Key,
    modifiers: Modifiers,
    repeat: bool,
    status: iced::event::Status,
) -> Option<Region> {
    if shortcut(modified_key, modifiers, repeat) == Some(Region::Info) {
        return (status == iced::event::Status::Ignored).then_some(Region::Info);
    }
    shortcut(key, modifiers, repeat).filter(|region| *region != Region::Info)
}

/// The command line: at most one project path. With the `dev-hooks` feature, layout switches
/// also permit unattended screenshots without synthetic keyboard input.
pub(crate) fn startup(
    args: impl IntoIterator<Item = OsString>,
) -> Result<(Option<PathBuf>, State), String> {
    #[cfg_attr(not(any(test, feature = "dev-hooks")), allow(unused_mut))]
    let mut state = State::default();
    let mut path = None;
    for arg in args {
        match arg.to_str() {
            #[cfg(any(test, feature = "dev-hooks"))]
            Some(switch) if layout_switch(&mut state, switch) => {}
            Some(switch) if switch.starts_with('-') => {
                return Err(format!("unknown option: {switch}"));
            }
            _ if path.is_none() => path = Some(PathBuf::from(arg)),
            _ => return Err("expected at most one project path".into()),
        }
    }
    Ok((path, state))
}

/// One layout switch, for screenshots: whether `switch` named one (and was applied).
#[cfg(any(test, feature = "dev-hooks"))]
fn layout_switch(state: &mut State, switch: &str) -> bool {
    match switch {
        "--collapse-browser" => state.browser_open = false,
        "--collapse-detail" => state.detail_open = false,
        "--show-info" => state.info_open = true,
        "--collapse-outputs" => state.outputs_open = false,
        "--collapse-output" => state.output_open = false,
        "--collapse-path" => state.path_open = false,
        "--collapse-mixer" => state.mixer_open = false,
        "--collapse-throughput" | "--collapse-timing" => state.throughput_open = false,
        "--hide-strip" => state.strip_open = false,
        "--sections-menu-open" => state.start_with_sections_menu = true,
        _ => return false,
    }
    true
}

pub(crate) fn view<'a>(
    app: &'a App,
    grid: Element<'a, Message>,
    detail: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let control = container(
        row![
            toggle(
                if app.chrome.browser_open {
                    "◀"
                } else {
                    "▶"
                },
                "Browser (⌘⌥B)",
                Region::Browser,
                app.chrome.browser_open,
            ),
            transport::view(app)
        ]
        .spacing(6),
    )
    .padding(6)
    .width(Length::Fill)
    .style(surface);

    let devices = crate::device_chain::shown(app);
    let detail_height = app.dividers.shown_detail_height(devices);
    let mut main = column![focus_area(
        container(grid)
            .padding(4)
            .width(Length::Fill)
            .height(if detail_height.is_some() {
                Length::Fill
            } else {
                Length::FillPortion(3)
            })
            .style(region_style(surface, app.focus == Focus::Grid)),
        Focus::Grid,
    )]
    .width(Length::Fill)
    .height(Length::Fill);
    if let Some(detail) = detail {
        main = main.push(dividers::handle(Divider::Detail, !devices));
        let band_height = detail_height.map_or(Length::FillPortion(2), Length::Fixed);
        let detail = focus_area(
            container(detail)
                .padding(crate::style::detail::DETAIL_INSET)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(region_style(detail_surface, app.focus == Focus::Detail)),
            Focus::Detail,
        );
        // Live puts the Info View at the left of the bottom band, top-aligned with the Detail View.
        let mut band = row![].spacing(4).height(band_height);
        if app.chrome.info_open {
            band = band.push(info::view(&app.info, app.project()));
        }
        main = main.push(band.push(detail));
    }
    let mut workspace = row![].height(Length::Fill);
    // Map Mode puts the mapping list in the Browser's place, as Live does, and gives it back
    // when the mode ends.
    if app.map_mode.on {
        workspace = workspace.push(focus_area(crate::map_mode::list_view(app), Focus::Browser));
        workspace = workspace.push(dividers::handle(Divider::Browser, true));
    } else if app.chrome.browser_open {
        workspace = workspace.push(focus_area(crate::browser::view(app), Focus::Browser));
        workspace = workspace.push(dividers::handle(Divider::Browser, true));
    }
    workspace = workspace.push(main);

    let mut content = column![control, workspace].spacing(4).height(Length::Fill);
    if app.chrome.info_open && !app.chrome.detail_open {
        // With the Detail View collapsed the Info View keeps its own strip.
        content = content.push(
            row![
                container(info::view(&app.info, app.project())).height(Length::Fixed(120.0)),
                Space::new().width(Length::Fill)
            ]
            .spacing(4),
        );
    }
    column![
        container(content)
            .padding([0, 4])
            .width(Length::Fill)
            .height(Length::Fill),
        status::view(
            &app.status,
            app.chrome,
            app.project(),
            app.controls(),
            app.theme
        )
    ]
    .spacing(4)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Clicking anywhere in a region, even on empty space, makes it the active region.
fn focus_area<'a>(content: impl Into<Element<'a, Message>>, focus: Focus) -> Element<'a, Message> {
    mouse_area(content)
        .on_press(Message::SetFocus(focus))
        .into()
}

pub(crate) fn toggle<'a>(
    glyph: &'static str,
    hint: &'static str,
    region: Region,
    open: bool,
) -> Element<'a, Message> {
    info::help(
        crate::control::tooltip(
            crate::style::flat(glyph)
                .on_press(Message::ToggleRegion(region))
                .style(crate::control::widgets::flat_toggle_style(
                    &crate::control::LIVE_DARK,
                    crate::control::State {
                        lit: if open {
                            crate::control::Lit::On
                        } else {
                            crate::control::Lit::Off
                        },
                        ..Default::default()
                    },
                )),
            hint,
            // The Browser switch sits at the window's top edge. Iced clamps a Top
            // tooltip into the viewport, covering the switch instead of flipping it.
            if region == Region::Browser {
                tooltip::Position::Bottom
            } else {
                tooltip::Position::Top
            },
        ),
        match region {
            Region::Browser => Id::Browser,
            Region::Detail => Id::Detail,
            Region::Info => Id::InfoView,
            Region::Outputs => Id::OutputArea,
            Region::Strip => Id::StripSwitch,
            Region::Section(Row::Output) => Id::OutputBand,
            Region::Section(Row::Path) => Id::PathBand,
            Region::Section(Row::Mixer) => Id::MixerBand,
            Region::Section(Row::Throughput) => Id::ThroughputBand,
        },
    )
}

// Live 12.1 Default Dark Neutral Medium.ask, captured 2026-10-10:
// SurfaceArea #242424, SurfaceHighlight #464646, DetailViewBackground #3e3e3e.
fn surface(_: &Theme) -> container::Style {
    container::Style {
        background: Some(crate::control::LIVE_DARK.surface_area.into()),
        border: Border {
            color: crate::control::LIVE_DARK.surface_highlight,
            width: 1.0,
            radius: crate::control::LIVE_DARK.corner_control.into(),
        },
        ..Default::default()
    }
}

/// The region's usual surface, with the focus outline when it is the active region.
fn region_style(
    base: fn(&Theme) -> container::Style,
    active: bool,
) -> impl Fn(&Theme) -> container::Style {
    move |theme| container::Style {
        border: crate::focus::border(active),
        ..base(theme)
    }
}

fn detail_surface(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(crate::control::LIVE_DARK.detail_view_background.into()),
        ..surface(theme)
    }
}

#[cfg(test)]
mod tests;
