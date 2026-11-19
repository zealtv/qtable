//! The strip sections menu above the status bar's mixer icon, after Live 12's: a check per strip
//! section with its shortcut, then Outputs (our Return Tracks). Choosing a line toggles it and closes the menu; a click outside closes it.
use crate::app::App;
use crate::chrome::Region;
use crate::control::menu::{self, Anchor, Check, Item};
use crate::grid::Row;
use crate::info::{Help, Id};
use crate::messages::Message;
use iced::widget::column;
use iced::{Element, Size};

const MENU_W: f32 = 190.0;
/// Clear of the status strip and the icon that opens the menu: the menu's bottom right corner
/// sits this far from the window's.
const BOTTOM_GAP: f32 = 30.0;
const RIGHT_GAP: f32 = 34.0;

/// One menu line: its label, shortcut, what it toggles and its Info View entry.
struct Line {
    label: &'static str,
    shortcut: &'static str,
    region: Region,
    help: Id,
}

const SECTIONS: [Line; 4] = [
    Line {
        label: "Output",
        shortcut: "⌥⌘I",
        region: Region::Section(Row::Output),
        help: Id::OutputBand,
    },
    Line {
        label: "Path",
        shortcut: "",
        region: Region::Section(Row::Path),
        help: Id::PathBand,
    },
    Line {
        label: "Mixer",
        shortcut: "",
        region: Region::Section(Row::Mixer),
        help: Id::MixerBand,
    },
    Line {
        label: "Throughput",
        shortcut: "",
        region: Region::Section(Row::Throughput),
        help: Id::ThroughputBand,
    },
];

const OUTPUTS: Line = Line {
    label: "Outputs",
    shortcut: "⌥⌘R",
    region: Region::Outputs,
    help: Id::OutputArea,
};

fn checked(app: &App, region: Region) -> bool {
    match region {
        Region::Section(row) => app.chrome.section(row),
        Region::Outputs => app.chrome.outputs_open,
        _ => false,
    }
}

fn line_view<'a>(
    theme: &crate::control::Theme,
    line: &Line,
    checked: bool,
    highlighted: bool,
) -> Element<'a, Message> {
    menu::item(
        theme,
        Item::new(line.label, line.help)
            .shortcut(line.shortcut)
            .check(if checked { Check::Tick } else { Check::None })
            .highlighted(highlighted)
            .on_press_maybe(Some(Message::ToggleRegion(line.region))),
        |row, help| row.info(help, Message::Info),
    )
}

/// The rows the keyboard reaches (`menu_keys.rs`): every line, top to bottom.
pub(crate) fn rows() -> crate::menu_keys::Rows {
    crate::menu_keys::Rows {
        presses: SECTIONS
            .iter()
            .chain([&OUTPUTS])
            .map(|line| Message::ToggleRegion(line.region))
            .collect(),
        current: None,
    }
}

/// The menu layer over the window, anchored above the icon at the bottom right: the menu opens
/// up and to the left of that corner, the side with room.
pub(crate) fn view<'a>(app: &App) -> Element<'a, Message> {
    let theme = app.theme;
    let checks = SECTIONS.map(|line| checked(app, line.region));
    let outputs = checked(app, OUTPUTS.region);
    let highlight = app.menu_highlight;
    let size = Size::new(
        MENU_W,
        menu::panel_height(menu::content_height(SECTIONS.len() + 1, 1)),
    );
    // The corner is only known per window size, so the anchor is the window's own corner, inset.
    menu::layer(
        Anchor::corner(RIGHT_GAP, BOTTOM_GAP),
        Message::CloseOverlay,
        move || {
            let lines = SECTIONS
                .iter()
                .zip(checks)
                .enumerate()
                .map(|(index, (line, checked))| {
                    line_view(&theme, line, checked, highlight == Some(index))
                })
                .chain([
                    menu::separator(&theme),
                    line_view(&theme, &OUTPUTS, outputs, highlight == Some(SECTIONS.len())),
                ]);
            (menu::panel(column(lines), size.width, &theme).into(), size)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_follow_the_status_bar_menu_in_live_order() {
        let labels: Vec<_> = SECTIONS.iter().map(|l| l.label).collect();
        assert_eq!(labels, ["Output", "Path", "Mixer", "Throughput"]);
        assert_eq!(OUTPUTS.region, Region::Outputs);
        // Shortcuts in the menu match the real ones.
        let keys: Vec<_> = SECTIONS.iter().map(|l| l.shortcut).collect();
        assert_eq!(keys, ["⌥⌘I", "", "", ""]);
    }
}
