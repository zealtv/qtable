//! The inspector against the design's table (§3.4), read from the real widget tree: which
//! settings are present, which are dimmed (present, disabled) and which are absent, for every
//! source, mode and target class. Each case is also checked against `RemoteMapping::facets`,
//! so the inspector shows exactly what the model decides.
use super::*;
use crate::control_focus::test_registry::{rectangles_in, states_in};
use crate::grid::test_renderer;
use crate::map_mode::tests::{act, app};
use crate::style::detail::{
    DEFAULT_BROWSER_WIDTH, MAPPING_INSPECTOR_MIN_WIDTH, REGION_FRAME, TEXT_SIZE,
};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Layout, layout};
use iced::{Element, Rectangle, Size};
use qtable_engine::edit::{MappingEdit, ProjectEdit};
use qtable_engine::engine::Command;
use qtable_engine::model::*;

/// The window the tests draw the Browser's column in: the default Browser, 700 px tall.
const WINDOW: Size = Size::new(DEFAULT_BROWSER_WIDTH, 700.0);

/// The track name the width tests use, long enough that the Path wraps.
const LONG_NAME: &str = "A track with a long and very descriptive name";

/// Every text widget in `element`, laid out in `size` by the test renderer: its fragment and
/// the bounds layout gave it. The text widget reports itself to an `Operation`, so this needs
/// no hook in the production code.
fn texts_in(
    element: &mut Element<'_, crate::messages::Message>,
    size: Size,
) -> Vec<(String, Rectangle)> {
    struct Texts(Vec<(String, Rectangle)>);
    impl Operation for Texts {
        fn text(&mut self, _: Option<&iced::widget::Id>, bounds: Rectangle, text: &str) {
            self.0.push((text.to_owned(), bounds));
        }
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
    }
    let renderer = test_renderer();
    let mut tree = Tree::new(&*element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, size),
    );
    let mut texts = Texts(Vec::new());
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut texts);
    texts.0
}

/// Renames the first track to `LONG_NAME` and checks how the selected mapping's Path then lays
/// out in the default Browser. The Path of a track's Volume carries the name and wraps to two
/// lines, the tallest identity the inspector's fixed height is sized for: `expect_two_lines`
/// asserts exactly that, since one line would make the fit checks vacuous. Any Path must stay
/// within two lines, since a third is clipped (the known limit). A line is measured from the
/// same renderer, never assumed.
fn rename_and_check_path(app: &mut crate::app::App, expect_two_lines: bool) {
    let track = app.project().tracks[0].id;
    let _ = crate::edit_actions::run_edit(
        app,
        Command::Edit(ProjectEdit::RenameTrack {
            track,
            name: LONG_NAME.into(),
        }),
    );
    let path = labels::path(&selected(app).expect("a selected mapping").1);
    let found = texts_in(&mut crate::map_mode::list_view(app), WINDOW);
    let (_, bounds) = found
        .iter()
        .find(|(fragment, _)| *fragment == path)
        .unwrap_or_else(|| panic!("no text {path:?} in {found:?}"));
    let one = {
        let mut line: Element<'_, crate::messages::Message> =
            iced::widget::text("Path").size(TEXT_SIZE).into();
        texts_in(&mut line, WINDOW)[0].1.height
    };
    let lines = (bounds.height / one).round();
    assert!(
        (bounds.height - lines * one).abs() < 0.5,
        "{path:?} is {} px tall, not a whole number of {one} px lines",
        bounds.height
    );
    assert!(
        lines <= 2.0,
        "{path:?} wraps to {lines} lines; a third is clipped"
    );
    if expect_two_lines {
        assert_eq!(
            lines, 2.0,
            "{path:?} should wrap to two lines ({} px)",
            bounds.height
        );
    }
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    CcAbsolute,
    CcRelative,
    PitchBend,
    NoteLatch,
    NoteGate,
    KeyLatch,
    KeyGate,
}

impl Kind {
    const ALL: [Self; 7] = [
        Self::CcAbsolute,
        Self::CcRelative,
        Self::PitchBend,
        Self::NoteLatch,
        Self::NoteGate,
        Self::KeyLatch,
        Self::KeyGate,
    ];

    fn source(self) -> Source {
        let midi = |message| {
            Source::Midi(MidiSource {
                channel: 1,
                message,
            })
        };
        match self {
            Self::CcAbsolute | Self::CcRelative => midi(MidiMessage::Cc(21)),
            Self::PitchBend => midi(MidiMessage::PitchBend),
            Self::NoteLatch | Self::NoteGate => midi(MidiMessage::Note(60)),
            Self::KeyLatch | Self::KeyGate => Source::Key(KeySource {
                key: KeyName("a".into()),
            }),
        }
    }

    fn mode(self) -> SourceMode {
        match self {
            Self::CcRelative => SourceMode::Relative(RelativeEncoding::TwosComplement),
            Self::NoteGate | Self::KeyGate => SourceMode::Gate,
            _ => SourceMode::Absolute,
        }
    }

    fn key(self) -> bool {
        matches!(self, Self::KeyLatch | Self::KeyGate)
    }
}

/// The design's six target classes, each by one control of a new project.
fn targets() -> [(&'static str, ControlRef); 6] {
    [
        ("Number", ControlRef::Argument(FieldRef::volume(TrackId(1)))),
        (
            "Choice",
            ControlRef::Transport(TransportControl::Quantisation),
        ),
        ("Switch", ControlRef::TrackSolo(TrackId(1))),
        ("Activator", ControlRef::TrackActivator(TrackId(1))),
        (
            "Scene Select",
            ControlRef::SceneNavigation(SceneNavigation::Select),
        ),
        (
            "Trigger",
            ControlRef::ClipSlot {
                track: TrackId(1),
                scene: SceneId(1),
            },
        ),
    ]
}

/// An app in the mode `kind` belongs to, with one mapping of `kind` on `control`, selected.
/// `None` when the model refuses the mapping (Gate on a trigger or Scene Select).
fn mapped(kind: Kind, control: &ControlRef) -> Option<crate::app::App> {
    let mut app = app();
    act(&mut app, crate::map_mode::Action::Toggle);
    if kind.key() {
        act(&mut app, crate::map_mode::Action::ToggleKey);
    }
    let descriptor = describe_control(app.project(), control).unwrap();
    let (min, max) = crate::map_mode::labels::learn_range(control, &descriptor);
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::EditMapping(MappingEdit::Add {
            source: kind.source(),
            control: control.clone(),
            min,
            max,
        })),
    )
    .then_some(())?;
    let id = app.project().mappings[0].id;
    if kind.mode() != SourceMode::Absolute {
        crate::edit_actions::run_edit(
            &mut app,
            Command::Edit(ProjectEdit::EditMapping(MappingEdit::SetMode {
                mapping: id,
                mode: kind.mode(),
            })),
        )
        .then_some(())?;
    }
    act(&mut app, crate::map_mode::Action::SelectRow(id));
    Some(app)
}

const CELLS: [(Cell, &str); 6] = [
    (Cell::Mode, "mode"),
    (Cell::Takeover, "takeover"),
    (Cell::Sensitivity, "sensitivity"),
    (Cell::Invert, "invert"),
    (Cell::Min, "min"),
    (Cell::Max, "max"),
];

/// Whether a registered control is `cell` of mapping `id`, whatever widget it takes.
fn is_cell(control: ControlId, id: MappingId, cell: Cell) -> bool {
    let key = DraftKey::Mapping(id, cell);
    [
        ControlId::field(View::Mappings, key),
        ControlId::field_action(View::Mappings, key, "mode"),
        ControlId::field_action(View::Mappings, key, "takeover"),
        ControlId::field_action(View::Mappings, key, "invert"),
        ControlId::field_action(View::Mappings, key, "limit"),
    ]
    .contains(&control)
}

/// The inspector's settings as the widgets show them: present ones by name, a dimmed one
/// marked `~`, absent ones left out.
fn shown(app: &crate::app::App) -> String {
    let id = app.map_mode.selected_row.unwrap();
    let states: Vec<(ControlId, bool)> = states_in(&mut crate::map_mode::list_view(app), WINDOW);
    let mut words = Vec::new();
    for (cell, name) in CELLS {
        let found: Vec<bool> = states
            .iter()
            .filter(|(control, _)| is_cell(*control, id, cell))
            .map(|(_, enabled)| *enabled)
            .collect();
        assert!(found.len() <= 1, "{name} twice");
        if let Some(&enabled) = found.first() {
            words.push(if enabled {
                name.to_owned()
            } else {
                format!("{name}~")
            });
        }
    }
    words.join(" ")
}

/// The same reading of the model's `facets`.
fn decided(app: &crate::app::App) -> String {
    let row = &app.project().mappings[0];
    let value = describe_value(app.project(), &row.control).unwrap();
    let facets = row.facets(&row.control, &value);
    let mut words = Vec::new();
    for ((_, name), facet) in CELLS.into_iter().zip([
        facets.mode,
        facets.takeover,
        facets.sensitivity,
        facets.invert,
        facets.min,
        facets.max,
    ]) {
        match facet {
            Facet::Hidden => {}
            Facet::Dimmed => words.push(format!("{name}~")),
            Facet::Enabled => words.push(name.to_owned()),
        }
    }
    words.join(" ")
}

/// The design's §3.4, cell by cell: "(abs)", "(rel)" and "(Gate)" become `~` in the other mode.
fn expected(kind: Kind, target: &str) -> &'static str {
    use Kind::*;
    match (kind, target) {
        (CcAbsolute, "Number" | "Choice") => "mode takeover sensitivity~ invert min max",
        (CcRelative, "Number" | "Choice") => "mode takeover~ sensitivity invert min max",
        (CcAbsolute, "Switch") => "mode takeover invert min max",
        (CcRelative, "Switch") => "mode takeover~ invert~ min~ max~",
        (CcAbsolute, "Activator") => "mode min max",
        (CcRelative, "Activator") => "mode min~ max~",
        (CcAbsolute, "Scene Select") => "mode sensitivity~",
        (CcRelative, "Scene Select") => "mode sensitivity",
        (CcAbsolute | CcRelative, "Trigger") => "mode",
        (PitchBend, "Number" | "Choice" | "Switch") => "takeover invert min max",
        (PitchBend, "Activator") => "min max",
        (NoteLatch | NoteGate | KeyLatch | KeyGate, "Number") => "mode invert min max",
        (NoteLatch | KeyLatch, "Choice") => "mode invert~ min max",
        (NoteGate | KeyGate, "Choice") => "mode invert min max",
        (NoteLatch | KeyLatch, "Switch" | "Activator") => "mode invert~",
        (NoteGate | KeyGate, "Switch" | "Activator") => "mode invert",
        (_, "Scene Select" | "Trigger") => "",
        _ => unreachable!("{kind:?} {target}"),
    }
}

#[test]
fn every_cell_of_the_design_table_is_present_dimmed_or_absent_as_facets_decides() {
    let mut checked = 0;
    for kind in Kind::ALL {
        for (target, control) in targets() {
            // A stored Gate on a trigger or Scene Select is valid (it acts as Latch); the
            // inspector offers no Mode there.
            let app = mapped(kind, &control).unwrap_or_else(|| panic!("{kind:?} {target} refused"));
            let shown = shown(&app);
            assert_eq!(shown, expected(kind, target), "{kind:?} {target}");
            assert_eq!(shown, decided(&app), "{kind:?} {target} against facets");
            checked += 1;
        }
    }
    assert_eq!(checked, 7 * 6);
}

#[test]
fn the_inspector_is_the_default_browser_less_its_two_frames() {
    assert_eq!(
        MAPPING_INSPECTOR_MIN_WIDTH,
        DEFAULT_BROWSER_WIDTH - 2.0 * REGION_FRAME
    );
}

#[test]
fn every_setting_fits_the_default_browser_and_the_fixed_inspector() {
    for kind in Kind::ALL {
        for (target, control) in targets() {
            let Some(mut app) = mapped(kind, &control) else {
                continue;
            };
            // A long name and path, so the Path wraps: the tallest identity.
            rename_and_check_path(&mut app, target == "Number");
            assert_eq!(app.dividers.browser_width(), DEFAULT_BROWSER_WIDTH);
            let id = app.map_mode.selected_row.unwrap();
            let found: Vec<(ControlId, Rectangle)> =
                rectangles_in(&mut crate::map_mode::list_view(&app), WINDOW);
            let top = WINDOW.height - MAPPING_INSPECTOR_HEIGHT;
            for (cell, name) in CELLS {
                for (_, bounds) in found.iter().filter(|(c, _)| is_cell(*c, id, cell)) {
                    assert!(
                        bounds.x >= 0.0
                            && bounds.x + bounds.width <= DEFAULT_BROWSER_WIDTH
                            && bounds.y >= top
                            && bounds.y + bounds.height <= WINDOW.height
                            && bounds.width > 20.0,
                        "{kind:?} {target}: {name} at {bounds:?}"
                    );
                }
            }
        }
    }
}

/// The padding (2026-11-16: "a little bit" more room): every setting keeps
/// 8 px from the panel's sides and bottom, a two-line Path included.
#[test]
fn every_setting_keeps_the_inspector_padding_from_the_panel_edges() {
    for kind in Kind::ALL {
        for (target, control) in targets() {
            let Some(mut app) = mapped(kind, &control) else {
                continue;
            };
            rename_and_check_path(&mut app, target == "Number");
            let id = app.map_mode.selected_row.unwrap();
            let found: Vec<(ControlId, Rectangle)> =
                rectangles_in(&mut crate::map_mode::list_view(&app), WINDOW);
            // Pinned to the 8 px the tour asked for, not read from the token it checks.
            let inset = 8.0;
            // The Browser's column sits inside the 1 px frame; the panel is the inspector's least width.
            let (left, right) = (REGION_FRAME + inset, WINDOW.width - REGION_FRAME - inset);
            let bottom = WINDOW.height - REGION_FRAME - inset;
            for (cell, name) in CELLS {
                for (_, b) in found.iter().filter(|(c, _)| is_cell(*c, id, cell)) {
                    assert!(
                        b.x >= left && b.x + b.width <= right && b.y + b.height <= bottom,
                        "{kind:?} {target}: {name} at {b:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn with_nothing_selected_the_inspector_offers_no_setting() {
    let mut app = mapped(Kind::CcAbsolute, &targets()[0].1).unwrap();
    app.map_mode.selected_row = None;
    let found: Vec<(ControlId, bool)> = states_in(&mut crate::map_mode::list_view(&app), WINDOW);
    assert!(
        found
            .iter()
            .all(|(control, _)| control.view() != View::Mappings),
        "{found:?}"
    );
}

#[test]
fn a_mode_change_moves_no_setting() {
    // Switching a CC between Absolute and a relative mode dims Takeover or Sensitivity in place.
    let mut app = mapped(Kind::CcAbsolute, &targets()[0].1).unwrap();
    let id = app.map_mode.selected_row.unwrap();
    let place = |app: &crate::app::App| {
        let found: Vec<(ControlId, Rectangle)> =
            rectangles_in(&mut crate::map_mode::list_view(app), WINDOW);
        CELLS
            .iter()
            .map(|(cell, _)| {
                found
                    .iter()
                    .find(|(c, _)| is_cell(*c, id, *cell))
                    .map(|(_, bounds)| *bounds)
            })
            .collect::<Vec<_>>()
    };
    let absolute = place(&app);
    act(
        &mut app,
        crate::map_mode::Action::SetMode(id, SourceMode::Relative(RelativeEncoding::SignedBit)),
    );
    assert_eq!(place(&app), absolute);
    assert!(absolute.iter().all(Option::is_some));
}

#[test]
fn a_note_or_key_mode_chooser_takes_the_left_of_two_columns_and_a_cc_s_the_full_width() {
    let width = |kind| {
        let app = mapped(kind, &targets()[0].1).unwrap();
        let id = app.map_mode.selected_row.unwrap();
        let found: Vec<(ControlId, Rectangle)> =
            rectangles_in(&mut crate::map_mode::list_view(&app), WINDOW);
        found
            .iter()
            .find(|(c, _)| is_cell(*c, id, Cell::Mode))
            .unwrap()
            .1
            .width
    };
    let cc = width(Kind::CcAbsolute);
    let note = width(Kind::NoteLatch);
    assert!(note < cc * 0.6, "note {note}, cc {cc}");
    assert_eq!(width(Kind::KeyGate), note);
}
