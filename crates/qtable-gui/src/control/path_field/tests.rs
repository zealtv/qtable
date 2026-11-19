use super::*;
use crate::control::widgets::Controls;
use crate::control::{Control, LIVE_DARK, SizeRole};
use crate::control_focus::test_registry::TestHelp;
use crate::control_focus::test_registry::{TestKey, TestMessage};
use crate::control_focus::{ControlFocus, ControlId, ControlKey, View};
use crate::style::detail::{COMPACT_TEXT_SIZE, TEXT_SIZE};
use iced::advanced::{layout, renderer::Headless, widget::Tree};
use qtable_engine::model::{Path, PathLevel, Project, Route, Track, TrackId};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

fn renderer() -> Renderer {
    let mut future = std::pin::pin!(Renderer::new(
        iced::Font::default(),
        iced::Pixels(TEXT_SIZE as f32),
        Some("tiny-skia")
    ));
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(Some(renderer)) => renderer,
        _ => panic!("software renderer must be available"),
    }
}

/// A member of group `/drums`, routed through it to outputs `/bop` and `/viz`.
fn project(second: &str) -> Project {
    let mut p = Project::default();
    p.outputs
        .push(Track::new_output(TrackId(201), "A", "127.0.0.1", 9000).with_path("/bop"));
    p.outputs
        .push(Track::new_output(TrackId(202), "B", "127.0.0.1", 9001).with_path(second));
    let mut group = Track::new_group(TrackId(10), "Group", 0).with_path("/drums");
    group.route = Route::Outputs(vec![TrackId(201), TrackId(202)]);
    let mut member = Track::new(TrackId(1), "Member", 0);
    member.group = Some(group.id);
    member.route = Route::Group;
    member.path = Path::new("/kick").unwrap();
    p.tracks = vec![group, member];
    p
}

#[test]
fn prefix_counts_distinct_paths_and_goes_empty_when_absolute() {
    let different = project("/viz");
    let prefix = Prefix::of(&different, TrackId(1), PathLevel::Under, false);
    assert_eq!(
        (prefix.text().as_str(), prefix.more),
        ("/bop/drums/kick", 1)
    );
    let track_level = Prefix::of(&different, TrackId(1), PathLevel::Track, false);
    assert_eq!(track_level.text(), "/bop/drums");
    let same = project("/bop");
    let prefix = Prefix::of(&same, TrackId(1), PathLevel::Under, false);
    assert_eq!(
        (prefix.text().as_str(), prefix.more),
        ("/bop/drums/kick", 0)
    );
    assert_eq!(
        Prefix::of(&different, TrackId(1), PathLevel::Under, true),
        Prefix::default()
    );
    assert_eq!(
        Prefix::of(&different, TrackId(999), PathLevel::Under, false),
        Prefix::default()
    );
}

#[test]
fn shortening_drops_whole_leading_segments() {
    let width = |s: &str| s.chars().count() as f32 * 6.0;
    let full = "/bop/drums/kick";
    assert_eq!(shorten(full, 200.0, width), full);
    assert_eq!(shorten(full, 15.0 * 6.0 - 1.0, width), ".../drums/kick");
    assert_eq!(shorten(full, 10.0 * 6.0, width), ".../kick");
    assert_eq!(shorten(full, 5.0 * 6.0, width), "...");
    assert_eq!(shorten(full, 0.0, width), "...");
    assert_eq!(shorten("", 0.0, width), "");
    assert_eq!(shorten("/bop", 6.0, width), "...");
}

#[test]
fn shortening_measures_the_real_font() {
    let size = Pixels(TEXT_SIZE as f32);
    let full = "/bop/drums/kick";
    let whole = measure(full, size);
    let shown = |budget| shorten(full, budget, |s| measure(s, size));
    assert_eq!(shown(whole), full);
    let one_off = shown(whole - 1.0);
    assert!(
        one_off.starts_with("...") && one_off.ends_with("/kick"),
        "{one_off}"
    );
    assert!(measure(&one_off, size) <= whole - 1.0);
    assert_eq!(shown(measure("...", size)), "...");
}

#[test]
fn frame_look_follows_invalid_then_focused_then_idle() {
    let theme = LIVE_DARK;
    let kind = PathField {
        placeholder: "",
        prefix: Prefix::default(),
        absolute: None,
        subject: TrackId(1),
    };
    let looks = |invalid| {
        Looks::of(
            &kind,
            &theme,
            State {
                invalid,
                ..State::default()
            },
        )
    };
    assert_eq!(
        frame_look(&looks(false), false).frame,
        theme.surface_highlight
    );
    assert_eq!(frame_look(&looks(false), true).frame, theme.chosen_default);
    assert_eq!(frame_look(&looks(true), false).frame, theme.chosen_alert);
    assert_eq!(frame_look(&looks(true), true).frame, theme.chosen_alert);
}

fn field<'a>(
    controls: &Controls<'a, TestKey>,
    prefix: Prefix,
    absolute: Option<bool>,
    role: SizeRole,
) -> Element<'a, TestMessage> {
    let key = TestKey(1);
    controls.path(
        Control::new(
            ControlId::field(View::Mixer, key),
            TestHelp::Path,
            PathField {
                placeholder: "no path",
                prefix,
                absolute,
                subject: TrackId(1),
            },
        )
        .role(role),
        "/kick",
        |_| TestMessage::Tick,
        TestMessage::Tick,
        TestMessage::Tick,
        absolute.map(|_| TestMessage::Tick),
    )
}

/// The widths of the row's parts: [lead space, prefix, input, +N?, Absolute?].
fn layout_parts(
    width: f32,
    role: SizeRole,
    prefix: Prefix,
    absolute: Option<bool>,
) -> (f32, Vec<f32>) {
    let focus = ControlFocus::<TestKey>::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    let mut element = field(&controls, prefix, absolute, role);
    let mut tree = Tree::new(&element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer(),
        &layout::Limits::new(Size::ZERO, Size::new(width, 200.0)),
    );
    fn finite(node: &layout::Node) {
        assert!(node.bounds().width.is_finite() && node.bounds().height.is_finite());
        node.children().iter().for_each(finite);
    }
    finite(&node);
    // The focus wrapper and info area each hold the frame; descend to the row.
    let mut row = &node;
    while row.children().len() == 1 {
        row = &row.children()[0];
    }
    (
        node.size().width,
        row.children().iter().map(|c| c.size().width).collect(),
    )
}

#[test]
fn strip_field_reserves_measured_input_and_gives_prefix_the_remainder() {
    let prefix = Prefix {
        segments: vec![Segment::new("Group", "/bop/drums", Ownership::Parent)],
        more: 1,
    };
    let (frame, parts) = layout_parts(96.0, SizeRole::Strip, prefix.clone(), None);
    assert_eq!(frame, 96.0);
    let (lead, shown, input, plus) = (parts[0], parts[1], parts[2], parts[3]);
    assert!(
        shown >= measure(".../drums", Pixels(COMPACT_TEXT_SIZE as f32)),
        "prefix {shown}"
    );
    let inner = frame - 2.0;
    assert_eq!(lead + shown + input + plus, inner);
    assert!(
        input >= measure("/kick", Pixels(COMPACT_TEXT_SIZE as f32)) + 4.0,
        "input {input}"
    );
    // Without "+N", the prefix can use more room.
    let (_, parts) = parts_without_plus();
    assert!(parts[2] >= CURSOR_FLOOR, "input {}", parts[2]);
}

fn parts_without_plus() -> (f32, Vec<f32>) {
    let prefix = Prefix {
        segments: vec![Segment::new("Group", "/bop/drums", Ownership::Parent)],
        more: 0,
    };
    layout_parts(96.0, SizeRole::Strip, prefix, None)
}

#[test]
fn wide_fields_show_the_whole_prefix_and_the_absolute_toggle() {
    let prefix = Prefix {
        segments: vec![Segment::new("Group", "/bop/drums", Ownership::Parent)],
        more: 0,
    };
    let role = SizeRole::Fixed {
        width: 320.0,
        height: 20.0,
    };
    let (frame, parts) = layout_parts(320.0, role, prefix, Some(false));
    assert_eq!(frame, 320.0);
    assert_eq!(parts.len(), 5);
    assert!(parts[1] > 0.0 && parts[1] <= 320.0 - CURSOR_FLOOR);
    assert_eq!(parts[3], 1.0);
    assert_eq!(parts[4], 54.0);
    let (_, absolute) = layout_parts(320.0, role, Prefix::default(), Some(true));
    assert_eq!(absolute[1], 0.0, "an absolute field shows no prefix");
}

#[test]
fn the_paths_fixture_gives_the_member_two_prefixes_and_loads_without_errors() {
    let project = qtable_engine::file::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/paths.ron"
    ))
    .unwrap();
    let member = TrackId(1);
    let strip = Prefix::of(&project, member, PathLevel::Track, false);
    assert_eq!((strip.text().as_str(), strip.more), ("/bop/drums", 1));
    let volume = Prefix::of(&project, member, PathLevel::Under, false);
    assert_eq!(
        (volume.text().as_str(), volume.more),
        ("/bop/drums/kick", 1)
    );
    assert_eq!(
        Prefix::of(&project, TrackId(2), PathLevel::Track, false).text(),
        "/bop"
    );
    assert_eq!(project.outputs[0].path.as_str(), "/bop");
    assert!(project[member].mixer_device.volume.output.enabled);
}

#[test]
fn placeholder_is_hidden_only_when_a_prefix_exists() {
    let mut kind = PathField {
        placeholder: "no path",
        prefix: Prefix::default(),
        absolute: None,
        subject: TrackId(1),
    };
    assert_eq!(kind.input_placeholder(), "no path");
    kind.prefix.segments = vec![Segment::new("Output", "/bop", Ownership::Inherited)];
    assert_eq!(kind.input_placeholder(), "");
    kind.placeholder = "/volume";
    assert_eq!(kind.input_placeholder(), "");
    kind.prefix = Prefix::default();
    assert_eq!(kind.input_placeholder(), "/volume");
}

fn interactive_field<'a>(
    controls: &Controls<'a, TestKey>,
    enabled: bool,
) -> Element<'a, TestMessage> {
    let key = TestKey(2);
    controls.path(
        Control::new(
            ControlId::field(View::Mixer, key),
            TestHelp::VolumePath,
            PathField {
                placeholder: "/volume",
                prefix: Prefix {
                    segments: vec![Segment::new("Group", "/bop/drums", Ownership::Parent)],
                    more: 1,
                },
                absolute: Some(false),
                subject: TrackId(1),
            },
        )
        .enabled(enabled),
        "/volume",
        |_| TestMessage::Tick,
        TestMessage::Tick,
        TestMessage::Commit,
        Some(TestMessage::Cancel),
    )
}

/// Events reach only the headless widget tree, never the desktop.
fn press_part(enabled: bool, part: usize) -> (Vec<TestMessage>, bool) {
    let focus = ControlFocus::<TestKey>::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    let mut element = interactive_field(&controls, enabled);
    let renderer = renderer();
    let mut tree = Tree::new(&element);
    let size = Size::new(320.0, 100.0);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, size),
    );
    let mut row = Layout::new(&node);
    while row.children().count() == 1 {
        row = row.children().next().unwrap();
    }
    let point = row.children().nth(part).unwrap().bounds().center();
    if enabled {
        element.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &mut iced::advanced::widget::operation::focusable::focus::<()>(TestKey(2).input_id()),
        );
    }
    let mut messages = vec![];
    for event in [
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
    ] {
        element.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(size),
        );
    }
    let mut row_tree = &tree;
    while row_tree.children.len() == 1 {
        row_tree = &row_tree.children[0];
    }
    let focused = row_tree.children[INPUT_PART]
        .state
        .downcast_ref::<iced::widget::text_input::State<Para>>()
        .is_focused();
    messages.retain(|m| !matches!(m, TestMessage::Info(_) | TestMessage::Focused(_)));
    (messages, focused)
}

#[test]
fn prefix_and_more_refocus_without_blurring_and_absolute_only_toggles() {
    for part in [0, 1, 3, 4] {
        let (messages, focused) = press_part(true, part);
        assert!(
            matches!(messages.as_slice(), [TestMessage::Commit]),
            "{messages:?}"
        );
        assert!(focused, "part {part} blurred the input");
    }
    let (messages, focused) = press_part(true, 2);
    assert!(
        messages.is_empty(),
        "input refocused through the frame: {messages:?}"
    );
    assert!(focused);
    let (messages, _) = press_part(true, 5);
    assert!(
        matches!(messages.as_slice(), [TestMessage::Cancel]),
        "{messages:?}"
    );
}

#[test]
fn disabled_path_field_publishes_no_actions_and_uses_disabled_ink() {
    for part in 0..6 {
        let (messages, _) = press_part(false, part);
        assert!(messages.is_empty(), "disabled part {part}: {messages:?}");
    }
    let kind = PathField {
        placeholder: "/volume",
        prefix: Prefix::default(),
        absolute: None,
        subject: TrackId(1),
    };
    let looks = Looks::of(
        &kind,
        &LIVE_DARK,
        State {
            enabled: false,
            ..State::default()
        },
    );
    for focused in [false, true] {
        assert_eq!(frame_look(&looks, focused).ink, LIVE_DARK.text_disabled);
    }
}

#[test]
fn ownership_inks_follow_the_theme_and_disabled_state() {
    let theme = LIVE_DARK;
    assert_eq!(Ownership::Inherited.ink(&theme, true), theme.text_disabled);
    assert_eq!(Ownership::Parent.ink(&theme, true), theme.path_parent);
    assert_eq!(Ownership::Own.ink(&theme, true), theme.control_foreground);
    assert!(theme.text_disabled.r < theme.path_parent.r);
    assert!(theme.path_parent.r < theme.control_foreground.r);
    for ownership in [Ownership::Inherited, Ownership::Parent, Ownership::Own] {
        assert_eq!(ownership.ink(&theme, false), theme.text_disabled);
    }
    let changed = Theme {
        path_parent: Color::WHITE,
        ..theme
    };
    assert_eq!(Ownership::Parent.ink(&changed, true), Color::WHITE);
}

#[test]
fn shortening_preserves_ownership_across_and_within_pieces() {
    let prefix = PrefixText::new(
        vec![
            Segment::new("Output", "/synth", Ownership::Inherited),
            Segment::new("Parent", "/long/target", Ownership::Parent),
        ],
        0.0,
        11,
        LIVE_DARK,
        true,
    );
    assert_eq!(
        prefix.shown_spans(1000.0),
        vec![
            ("/synth".into(), Ownership::Inherited),
            ("/long/target".into(), Ownership::Parent),
        ]
    );
    assert_eq!(
        prefix.shown_spans(measure(".../target", Pixels(TEXT_SIZE as f32))),
        vec![
            ("...".into(), Ownership::Inherited),
            ("/target".into(), Ownership::Parent),
        ]
    );
    assert_eq!(
        prefix.shown_spans(0.0),
        vec![("...".into(), Ownership::Inherited)]
    );
    let tiny = PrefixText::new(
        vec![Segment::new("", "/é", Ownership::Parent)],
        0.0,
        11,
        LIVE_DARK,
        true,
    );
    assert_eq!(
        tiny.shown_spans(0.0),
        vec![("...".into(), Ownership::Inherited)]
    );
}
