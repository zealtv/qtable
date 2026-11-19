//! The menus of the clip editor's content: a point, a hit, empty lane space and a time
//! selection, in both canvases. One table says which rows each shows and in what order, so the
//! six menus cannot drift apart. What was clicked (`Clicked`) and the kind of lane pick the
//! rows; passing state (an empty clipboard, no envelope) only dims. Every row runs the path its
//! key or button already uses (`event_editor::menu`).
use super::{Entry, command};
use crate::app::App;
use crate::clip_shortcuts::Shortcut;
use crate::control::Glyph;
use crate::info::Id;
use crate::inspector::event_editor::lanes::Identity;
use crate::inspector::event_editor::menu::{Clicked, ContentCommand, ContentTarget};
use crate::inspector::event_editor::{Action, shapes};
use crate::inspector::lane_model::EnvelopeRow;
use crate::messages::Message;
use qtable_engine::model::{Axis, Shape};

/// A row that runs `command` on this menu's target.
fn row(
    target: &ContentTarget,
    label: &'static str,
    shortcut: &'static str,
    run: ContentCommand,
    info: Id,
    enabled: bool,
) -> Entry {
    command(
        label,
        shortcut,
        Message::Inspector(crate::inspector::Message::Timeline(Action::ContentCommand(
            target.clone(),
            run,
        ))),
        info,
        enabled,
    )
}

fn glyph(shape: Shape) -> Glyph {
    match shape {
        Shape::Sine => Glyph::ShapeSine,
        Shape::Triangle => Glyph::ShapeTriangle,
        Shape::SawDown => Glyph::ShapeSawDown,
        Shape::SawUp => Glyph::ShapeSawUp,
        Shape::Square => Glyph::ShapeSquare,
        Shape::RampIn => Glyph::ShapeRampIn,
        Shape::RampOut => Glyph::ShapeRampOut,
    }
}

/// The rows for `target`, in Live's order: editing, Clear Envelope, Draw Mode, Insert Shape
/// with Simplify, then qtable's way back to the device. A group with no rows brings no
/// separator.
pub(crate) fn content_entries(app: &App, target: &ContentTarget) -> Vec<Entry> {
    let clicked = target.clicked;
    let selection = clicked == Clicked::TimeSelection;
    let item = matches!(clicked, Clicked::Point | Clicked::Hit);
    let editor = &app.clip_view.editor;
    let track = target.key.track;

    let mut editing = Vec::new();
    let mut keyed = |label, shortcut, key, info, enabled| {
        editing.push(row(
            target,
            label,
            shortcut,
            ContentCommand::Key(key),
            info,
            enabled,
        ));
    };
    if selection {
        keyed("Cut", "⌘X", Shortcut::Cut, Id::CutContent, true);
    }
    if selection || item {
        keyed("Copy", "⌘C", Shortcut::Copy, Id::CopyContent, true);
    }
    keyed(
        "Paste",
        "⌘V",
        Shortcut::Paste,
        Id::PasteContent,
        editor.clipboard.is_some(),
    );
    if selection {
        keyed(
            "Duplicate",
            "⌘D",
            Shortcut::Duplicate,
            Id::DuplicateContent,
            true,
        );
    }
    if selection || item {
        keyed("Delete", "Del", Shortcut::Delete, Id::DeleteContent, true);
    }

    let mut clear = Vec::new();
    let mut shapes_group = Vec::new();
    if let Identity::Envelope(argument) = &target.lane {
        let has_envelope = app
            .project()
            .track(track)
            .and_then(|t| {
                t.slots
                    .get(target.key.slot)?
                    .clip
                    .as_ref()?
                    .lanes
                    .iter()
                    .filter_map(|l| l.envelope())
                    .find(|e| e.target == *argument)
                    .map(|_| ())
            })
            .is_some();
        clear.push(row(
            target,
            "Clear Envelope",
            "",
            ContentCommand::ClearEnvelope,
            Id::ClearEnvelope,
            has_envelope,
        ));
        // Choice and switch lanes hold a value: Simplify works on them, shapes don't.
        let axis = EnvelopeRow::of_argument(app.project(), track, argument).axis;
        if axis != Axis::Discrete {
            shapes_group.push(Entry::Heading("Insert Shape"));
            for shape in Shape::ALL {
                let mut entry = row(
                    target,
                    shape.name(),
                    "",
                    ContentCommand::Shape(shape),
                    Id::InsertShape,
                    true,
                );
                if let Entry::Command { glyph: lead, .. } = &mut entry {
                    *lead = Some(glyph(shape));
                }
                shapes_group.push(entry);
            }
        }
        if matches!(clicked, Clicked::Space | Clicked::TimeSelection) {
            shapes_group.push(row(
                target,
                "Simplify Envelope",
                "",
                ContentCommand::Simplify,
                Id::SimplifyEnvelope,
                shapes::can_simplify(app),
            ));
        }
    }

    let mut draw = command(
        "Draw Mode",
        "B",
        Message::ToggleDrawMode,
        Id::DrawMode,
        true,
    );
    if let Entry::Command { checked, .. } = &mut draw {
        *checked = editor.draw.on;
    }

    let device = match &target.lane {
        Identity::Event(definition) => {
            let defined = app
                .project()
                .track(track)
                .is_some_and(|t| t.event_def(definition).is_some());
            super::go_to_device_entry(track, Some(definition.clone()), defined, false)
        }
        Identity::Envelope(argument) => {
            vec![super::go_to_parameter_entry(app, track, argument.clone())]
        }
    };

    let mut entries = Vec::new();
    for group in [editing, clear, vec![draw], shapes_group, device] {
        if group.is_empty() {
            continue;
        }
        if !entries.is_empty() {
            entries.push(Entry::Separator);
        }
        entries.extend(group);
    }
    entries
}
