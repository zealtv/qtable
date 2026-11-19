use super::canvas::Hosted;
use super::*;
use crate::control_focus::test_registry::{TestHelp, TestKey, TestMessage};
use crate::control_focus::{ControlOwner, View};
use iced::Size;
use iced::widget::canvas::{self as iced_canvas, Program};
use iced::{Color, mouse};

fn state(lit: Lit, enabled: bool) -> State {
    State {
        enabled,
        inactive: false,
        lit,
        invalid: false,
    }
}

fn toggle() -> Toggle {
    Toggle {
        label: "Loop".into(),
    }
}

#[test]
fn activator_wears_the_toggles_colours_on_a_fixed_circle() {
    use crate::style::detail::DEVICE_ACTIVATOR_SIZE;
    use iced::widget::button::Status;
    use widgets::{Looks, activator_style};
    let t = LIVE_DARK;
    let plain = Toggle {
        label: String::new(),
    };
    for lit in [Lit::Off, Lit::On] {
        for enabled in [true, false] {
            for interaction in [
                Interaction::Idle,
                Interaction::Hovered,
                Interaction::Pressed,
            ] {
                assert_eq!(
                    Activator.look(&t, state(lit, enabled), interaction),
                    plain.look(&t, state(lit, enabled), interaction)
                );
            }
        }
    }
    assert_eq!(
        Activator.role(),
        SizeRole::Fixed {
            width: DEVICE_ACTIVATOR_SIZE,
            height: DEVICE_ACTIVATOR_SIZE
        }
    );
    // A full circle, in the idle look whatever the pointer does.
    let style = activator_style(
        Looks::of(&Activator, &t, state(Lit::On, true)),
        DEVICE_ACTIVATOR_SIZE,
    );
    let idle = style(&iced::Theme::Dark, Status::Active);
    for status in [Status::Hovered, Status::Pressed] {
        let other = style(&iced::Theme::Dark, status);
        assert_eq!(other.background, idle.background);
        assert_eq!(other.border.color, idle.border.color);
    }
    assert_eq!(idle.border.radius, (DEVICE_ACTIVATOR_SIZE / 2.0).into());
    assert_eq!(idle.border.width, 1.0);
    let on = plain.look(&t, state(Lit::On, true), Interaction::Idle);
    assert_eq!(idle.background, Some(on.fill.into()));
    assert_eq!(idle.border.color, on.frame);
}

#[test]
fn toggle_reads_the_documented_tokens_for_each_state() {
    let t = LIVE_DARK;
    let off = toggle().look(&t, state(Lit::Off, true), Interaction::Idle);
    assert_eq!(
        (off.fill, off.ink, off.frame),
        (
            t.control_background,
            t.control_foreground,
            t.control_contrast_frame
        )
    );
    let on = toggle().look(&t, state(Lit::On, true), Interaction::Idle);
    assert_eq!(
        (on.fill, on.ink),
        (t.chosen_default, t.control_on_foreground)
    );
    let hovered = toggle().look(&t, state(Lit::On, true), Interaction::Hovered);
    assert_eq!(hovered.frame, t.control_foreground);
    // Disabled stays subdued even when its saved value is on.
    let disabled = toggle().look(&t, state(Lit::On, false), Interaction::Hovered);
    assert_eq!(
        (disabled.fill, disabled.ink, disabled.frame),
        (
            t.control_background,
            t.text_disabled,
            t.control_contrast_frame
        )
    );
}

#[test]
fn a_view_tab_lifts_the_shown_view_and_keeps_the_rest_flat_on_the_bar() {
    let t = LIVE_DARK;
    let tab = ViewTab {
        label: "Envelopes".into(),
    };
    let shown = tab.look(&t, state(Lit::On, true), Interaction::Idle);
    assert_eq!(
        (shown.fill, shown.ink, shown.frame),
        (
            t.surface_highlight,
            t.control_foreground,
            t.control_contrast_frame
        )
    );
    // Hovering the shown tab leaves it as it is; hovering another lifts it a little.
    let hovered_shown = tab.look(&t, state(Lit::On, true), Interaction::Hovered);
    assert_eq!(hovered_shown.fill, t.surface_highlight);
    let idle = tab.look(&t, state(Lit::Off, true), Interaction::Idle);
    assert_eq!(
        (idle.fill, idle.ink),
        (t.surface_background, t.control_foreground)
    );
    let hovered = tab.look(&t, state(Lit::Off, true), Interaction::Hovered);
    assert_eq!(hovered.fill, t.detail_view_background);
    let disabled = tab.look(&t, state(Lit::Off, false), Interaction::Hovered);
    assert_eq!(
        (disabled.fill, disabled.ink),
        (t.surface_background, t.text_disabled)
    );
    // Every tab has the same width, so the bar never shifts as the shown tab changes.
    assert_eq!(
        tab.role().width(),
        Length::Fixed(crate::style::detail::VIEW_TAB_WIDTH)
    );
}

#[test]
fn flat_toggle_preserves_region_switch_colours_across_pointer_states() {
    use iced::widget::button::Status;
    // Distinct tokens catch a style that bypasses the supplied theme.
    let theme = Theme {
        surface_highlight: Color::from_rgb(0.1, 0.2, 0.3),
        chosen_default: Color::from_rgb(0.4, 0.5, 0.6),
        ..LIVE_DARK
    };
    for lit in [Lit::Off, Lit::On] {
        let style = widgets::flat_toggle_style(&theme, state(lit, true));
        for status in [Status::Active, Status::Hovered, Status::Pressed] {
            let drawn = style(&iced::Theme::Dark, status);
            let fill = match (lit, status) {
                (Lit::On, _) => theme.chosen_default,
                (_, Status::Hovered) => theme.surface_highlight,
                _ => theme.control_background,
            };
            assert_eq!(drawn.background, Some(fill.into()));
            assert_eq!(
                drawn.text_color,
                if lit == Lit::On {
                    theme.control_on_foreground
                } else {
                    theme.control_foreground
                }
            );
            assert_eq!(drawn.border, iced::Border::default().rounded(1));
        }
        let drawn = style(&iced::Theme::Dark, Status::Disabled);
        assert_eq!(drawn.background, Some(theme.control_background.into()));
        assert_eq!(drawn.text_color, theme.text_disabled);
    }
}

#[test]
fn fields_show_the_alert_border_over_the_edit_border() {
    let t = LIVE_DARK;
    let kind = Text {
        placeholder: "".into(),
    };
    let editing = kind.look(&t, State::default(), Interaction::Pressed);
    assert_eq!(editing.frame, t.chosen_default);
    let invalid = State {
        invalid: true,
        ..State::default()
    };
    assert_eq!(
        kind.look(&t, invalid, Interaction::Pressed).frame,
        t.chosen_alert
    );
}

#[test]
fn bang_reads_shared_foregrounds_and_stays_dim_on_an_inactive_target() {
    let theme = Theme {
        control_foreground: Color::from_rgb(0.2, 0.3, 0.4),
        control_on_foreground: Color::from_rgb(0.6, 0.7, 0.8),
        text_disabled: Color::from_rgb(0.3, 0.4, 0.5),
        ..LIVE_DARK
    };
    let bang = Bang { active: true };
    for lit in [Lit::Off, Lit::On, Lit::Partial] {
        for interaction in [
            Interaction::Idle,
            Interaction::Hovered,
            Interaction::Pressed,
        ] {
            let enabled = state(lit, true);
            let look = bang.look(&theme, enabled, interaction);
            let firing = lit != Lit::Off || interaction == Interaction::Pressed;
            assert_eq!(
                look.ink,
                if firing {
                    theme.control_on_foreground
                } else {
                    theme.control_foreground
                }
            );
            assert_eq!(
                look.fill,
                Button {
                    face: Face::Text(String::new()),
                    // A bang's feedback uses the same chosen colour as its pointer press.
                    latched: true,
                }
                .look(&theme, enabled, interaction)
                .fill
            );
            let inactive = Bang { active: false }.look(&theme, enabled, interaction);
            let disabled = bang.look(&theme, state(lit, false), interaction);
            assert_eq!(inactive.ink, theme.text_disabled);
            assert_eq!(disabled.ink, theme.text_disabled);
        }
    }
    // Inactive Targets retain press-time commands; the engine gates their sends.
    assert!(press(
        &hosted(Bang { active: false }, Some(1), true),
        Point::new(15.0, 15.0)
    ));
}

#[test]
fn solo_lights_fully_or_on_its_lower_half() {
    let t = LIVE_DARK;
    let on = Solo.look(&t, state(Lit::On, true), Interaction::Idle);
    assert_eq!(
        (on.fill, on.accent),
        (t.chosen_alternative, t.chosen_alternative)
    );
    let partial = Solo.look(&t, state(Lit::Partial, true), Interaction::Idle);
    assert_eq!(
        (partial.fill, partial.accent),
        (t.control_background, t.chosen_alternative)
    );
    let off = Solo.look(&t, state(Lit::Off, true), Interaction::Idle);
    assert_eq!(off.accent, off.fill);
}

#[test]
fn swapping_the_theme_reskins_every_kind() {
    let custom = Theme {
        chosen_default: Color::from_rgb(0.1, 0.2, 0.9),
        control_background: Color::from_rgb(0.3, 0.0, 0.0),
        chosen_alternative: Color::from_rgb(0.0, 0.4, 0.0),
        display_background: Color::from_rgb(0.0, 0.0, 0.3),
        ..LIVE_DARK
    };
    let lit = state(Lit::On, true);
    let idle = Interaction::Idle;
    assert_eq!(
        toggle().look(&custom, lit, idle).fill,
        custom.chosen_default
    );
    assert_eq!(
        Solo.look(&custom, lit, idle).fill,
        custom.chosen_alternative
    );
    assert_eq!(
        Stop.look(&custom, State::default(), idle).fill,
        custom.control_background
    );
    assert_eq!(
        Dropdown.look(&custom, State::default(), idle).fill,
        custom.display_background
    );
    assert_eq!(
        FollowToggle.look(&custom, lit, idle).fill,
        custom.chosen_default
    );
}

#[test]
fn value_formats_show_two_decimals_and_keep_untouched_text_a_no_op() {
    assert_eq!(ValueFormat::Float.display(120.0), "120.00");
    assert_eq!(ValueFormat::Float.display(120.126), "120.126");
    assert!(ValueFormat::Float.is_untouched_display("120.126", 120.126));
    assert!(!ValueFormat::Float.is_untouched_display("121", 120.126));
    let delay = ValueFormat::Parameter(&qtable_engine::model::TRACK_DELAY);
    assert_eq!(delay.display(0.1234), "0.1234");
    assert!(delay.is_untouched_display("0.1234", 0.1234));
    assert_eq!(delay.display(0.12345678), "0.1235");
    assert!(delay.is_untouched_display("0.1235", 0.12345678));
    assert_eq!(ValueFormat::Float.display(0.73456789), "0.7346");
    assert!(ValueFormat::Float.is_untouched_display("0.7346", 0.73456789));
}

#[test]
fn size_roles_hold_the_ui_language_widths() {
    use iced::Length;
    assert_eq!(SizeRole::Float.width(), Length::Fixed(72.0));
    assert_eq!(SizeRole::Port.width(), Length::Fixed(60.0));
    assert_eq!(SizeRole::Tag.width(), Length::Fixed(44.0));
    assert!(SizeRole::Strip.is_compact() && !SizeRole::Fill.is_compact());
}

fn hosted<K: ControlKind>(kind: K, on_press: Option<u8>, enabled: bool) -> Hosted<K, u8, TestKey> {
    let mut control = Control::new(
        ControlId::action(View::Mixer, "solo", ControlOwner::Song),
        TestHelp::TrackSolo,
        kind,
    );
    control.state.enabled = enabled;
    Hosted {
        control,
        theme: LIVE_DARK,
        on_press,
    }
}

fn press<K: ControlKind>(program: &Hosted<K, u8, TestKey>, at: Point) -> bool {
    let bounds = Rectangle::new(Point::new(10.0, 10.0), Size::new(22.0, 13.0));
    let mut pressed = false;
    let action = program.update(
        &mut pressed,
        &iced_canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        bounds,
        mouse::Cursor::Available(at),
    );
    action.is_some() && pressed
}

#[test]
fn hosted_kinds_publish_only_for_enabled_presses_on_themselves() {
    let inside = Point::new(15.0, 15.0);
    assert!(press(&hosted(Solo, Some(1), true), inside));
    assert!(!press(&hosted(Solo, Some(1), true), Point::new(50.0, 15.0)));
    assert!(!press(&hosted(Solo, Some(1), false), inside));
    assert!(!press(&hosted(Solo, None, true), inside));
}

#[test]
fn compact_device_numbers_share_a_cell_and_short_choices_keep_their_text() {
    use crate::style::detail;
    assert_eq!(
        2.0 * detail::DEVICE_NUMBER_WIDTH + detail::CONTROL_GAP,
        detail::DEVICE_CONTROL_WIDTH
    );
    let focus = crate::control_focus::ControlFocus::<TestKey>::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    let id = crate::control_focus::ControlId::action(View::Track, "choice", ControlOwner::Song);
    for label in ["Number", "Slider", "Toggle", "String"] {
        let mut compact: iced::Element<'_, TestMessage> = controls.dropdown(
            Control::new(id, TestHelp::TargetFieldControl, Dropdown).role(SizeRole::DeviceChoice),
            vec![label],
            Some(label),
            |_| TestMessage::Focused(id),
        );
        let mut natural: iced::Element<'_, TestMessage> = controls.dropdown(
            Control::new(id, TestHelp::TargetFieldControl, Dropdown).role(SizeRole::Content),
            vec![label],
            Some(label),
            |_| TestMessage::Focused(id),
        );
        let width = |widget: &mut iced::Element<'_, TestMessage>| {
            crate::control_focus::test_registry::rectangles::<_, TestKey>(widget)[0]
                .1
                .width
        };
        assert_eq!(width(&mut compact), detail::DEVICE_CHOICE_WIDTH);
        assert!(
            width(&mut natural) <= width(&mut compact),
            "{label} fits with its arrow"
        );
    }
}

#[test]
fn tooltip_leaves_its_trigger_layout_alone() {
    let mut element: iced::Element<'_, TestMessage> = tooltip(
        iced::widget::Space::new().width(40).height(20),
        "Stop All Clips",
        iced::widget::tooltip::Position::Top,
    );
    let renderer = crate::grid::test_renderer();
    let mut tree = iced::advanced::widget::Tree::new(&element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &iced::advanced::layout::Limits::new(Size::ZERO, Size::new(600.0, 900.0)),
    );
    assert_eq!(node.size(), Size::new(40.0, 20.0));
}

#[test]
fn an_inactive_toggle_greys_its_lit_fill_and_keeps_its_place() {
    let t = LIVE_DARK;
    let inactive = |lit, enabled| State {
        inactive: true,
        ..state(lit, enabled)
    };
    let on = toggle().look(&t, inactive(Lit::On, true), Interaction::Idle);
    assert_eq!(
        (on.fill, on.ink),
        (t.control_on_inactive, t.control_on_foreground)
    );
    let off = toggle().look(&t, inactive(Lit::Off, true), Interaction::Idle);
    assert_eq!((off.fill, off.ink), (t.control_background, t.text_disabled));
    // Disabled wins over inactive.
    let disabled = toggle().look(&t, inactive(Lit::On, false), Interaction::Idle);
    assert_eq!(
        (disabled.fill, disabled.ink),
        (t.control_background, t.text_disabled)
    );
    // Hover and frame do not move.
    assert_eq!(
        toggle()
            .look(&t, inactive(Lit::On, true), Interaction::Hovered)
            .frame,
        t.control_foreground
    );
}

#[test]
fn the_inactive_state_never_reaches_a_label_as_disabled() {
    let theme = LIVE_DARK;
    let focus = crate::control_focus::ControlFocus::<TestKey>::default();
    let controls = Controls::new(theme, &focus);
    let active = State::default();
    let inactive = State {
        inactive: true,
        ..State::default()
    };
    let disabled = State {
        enabled: false,
        ..State::default()
    };
    // An inactive device keeps its labels' ink, as Live does.
    assert_eq!(controls.label_ink(inactive), controls.label_ink(active));
    assert_eq!(controls.label_ink(active), theme.control_foreground);
    // A control that is itself disabled (Live's Root under Chromatic) dims its label.
    assert_eq!(controls.label_ink(disabled), theme.text_disabled);
    // Both at once: the control's own `enabled` decides.
    let both = State {
        enabled: false,
        inactive: true,
        ..State::default()
    };
    assert_eq!(controls.label_ink(both), theme.text_disabled);
}

/// The ids of the text inputs in an element, in tree order.
fn input_ids(element: &mut iced::Element<'_, TestMessage>) -> Vec<String> {
    use iced::advanced::widget::{Operation, operation::Outcome};
    struct Collect(Vec<String>);
    impl Operation for Collect {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
        fn text_input(
            &mut self,
            id: Option<&iced::widget::Id>,
            _: Rectangle,
            _: &mut dyn iced::advanced::widget::operation::TextInput,
        ) {
            self.0.push(format!("{id:?}"));
        }
        fn finish(&self) -> Outcome<()> {
            Outcome::None
        }
    }
    let renderer = crate::grid::test_renderer();
    let mut tree = iced::advanced::widget::Tree::new(&*element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &iced::advanced::layout::Limits::new(Size::ZERO, Size::new(200.0, 40.0)),
    );
    let mut collect = Collect(Vec::new());
    element.as_widget_mut().operate(
        &mut tree,
        iced::advanced::Layout::new(&node),
        &renderer,
        &mut collect,
    );
    collect.0
}

#[test]
fn number_and_text_take_an_input_id_override_else_the_one_their_control_id_names() {
    let focus = crate::control_focus::ControlFocus::<TestKey>::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    let key = TestKey(1);
    let id = ControlId::field(View::Track, key);
    let exact = iced::widget::Id::new("target-face-exact");
    let number = |input: Option<iced::widget::Id>| {
        let control = Control::new(id, TestHelp::TargetNumber, Number::new("")).input_id(input);
        controls.number(
            control,
            "1",
            |_| TestMessage::Focused(id),
            TestMessage::Focused(id),
            None,
        )
    };
    let text = |input: Option<iced::widget::Id>| {
        let control = Control::new(
            id,
            TestHelp::TargetText,
            Text {
                placeholder: "".into(),
            },
        )
        .input_id(input);
        controls.text(
            control,
            "a",
            |_| TestMessage::Focused(id),
            TestMessage::Focused(id),
        )
    };
    let named = format!("{:?}", Some(key.input_id()));
    let overridden = format!("{:?}", Some(exact.clone()));
    assert_eq!(input_ids(&mut number(None)), std::slice::from_ref(&named));
    assert_eq!(
        input_ids(&mut number(Some(exact.clone()))),
        std::slice::from_ref(&overridden)
    );
    assert_eq!(input_ids(&mut text(None)), [named]);
    assert_eq!(input_ids(&mut text(Some(exact))), [overridden]);
}

#[test]
fn label_ink_follows_the_control_state_and_matches_the_old_fixed_ink_when_enabled() {
    let text = Text {
        placeholder: "".into(),
    };
    let enabled = text.look(&LIVE_DARK, state(Lit::Off, true), Interaction::Idle);
    let disabled = text.look(&LIVE_DARK, state(Lit::Off, false), Interaction::Idle);
    assert_eq!(enabled.ink, crate::style::TEXT);
    assert_eq!(disabled.ink, crate::style::DISABLED);
}

/// The focus ids a Number box publishes for each of `presses` left presses on it; `on_reset`
/// is a focus message for `reset_id`, so its arrival shows the reset fired.
fn number_presses(
    reset: Option<ControlId<TestKey>>,
    enabled: bool,
    presses: usize,
) -> Vec<Vec<ControlId<TestKey>>> {
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    let focus = crate::control_focus::ControlFocus::<TestKey>::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    let id = ControlId::action(
        View::Track,
        "number",
        ControlOwner::Track(qtable_engine::model::TrackId(1)),
    );
    let control = Control::new(id, TestHelp::TargetNumber, Number::new("")).enabled(enabled);
    let mut element = controls.number(
        control,
        "1",
        |_| TestMessage::Focused(id),
        TestMessage::Focused(id),
        reset.map(TestMessage::Focused),
    );
    let renderer = crate::grid::test_renderer();
    let mut tree = Tree::new(&element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(100.0, 40.0)),
    );
    let viewport = Rectangle::with_size(Size::new(100.0, 40.0));
    let clock = crate::gesture_clock::test::Clock::new();
    (0..presses)
        .map(|_| {
            clock.advance(std::time::Duration::from_millis(100));
            let mut messages = vec![];
            element.as_widget_mut().update(
                &mut tree,
                &iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Layout::new(&node),
                mouse::Cursor::Available(Point::new(5.0, 5.0)),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &viewport,
            );
            messages
                .into_iter()
                .filter_map(|m| match m {
                    TestMessage::Focused(id) => Some(id),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

fn reset_id() -> ControlId<TestKey> {
    ControlId::action(
        View::Track,
        "reset",
        ControlOwner::Track(qtable_engine::model::TrackId(2)),
    )
}

#[test]
fn a_double_click_on_a_number_box_publishes_its_reset_before_the_text_input_sees_it() {
    let reset = reset_id();
    let presses = number_presses(Some(reset), true, 3);
    assert!(!presses[0].contains(&reset));
    // The focus claim follows the reset, as for any click on the box.
    assert_eq!(presses[1].first(), Some(&reset));
    // The pair is spent, so the third press behaves as a first one.
    assert!(!presses[2].contains(&reset));
}

#[test]
fn a_number_box_without_a_reset_or_disabled_ignores_a_double_click() {
    let reset = reset_id();
    for presses in [
        number_presses(None, true, 2),
        number_presses(Some(reset), false, 2),
    ] {
        assert!(presses.iter().flatten().all(|id| *id != reset));
    }
}

#[test]
fn toggle_enter_activates_only_a_selected_enabled_control_once_per_press() {
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    use iced::{Event, keyboard, mouse};
    let renderer = crate::grid::test_renderer();
    let id = crate::control_focus::ControlId::action(View::Track, "toggle", ControlOwner::Song);
    for kind in ["boxed", "solo", "follow", "activator"] {
        for (selected, enabled, repeat, modifiers, activated, captured) in [
            (true, true, false, keyboard::Modifiers::NONE, true, true),
            (false, true, false, keyboard::Modifiers::NONE, false, false),
            (true, false, false, keyboard::Modifiers::NONE, false, false),
            (true, true, true, keyboard::Modifiers::NONE, false, true),
            (
                true,
                true,
                false,
                keyboard::Modifiers::COMMAND,
                false,
                false,
            ),
        ] {
            let mut focus = crate::control_focus::ControlFocus::<TestKey>::default();
            if selected {
                focus.claim(id);
            }
            let controls = Controls::new(LIVE_DARK, &focus);
            let mut widget = match kind {
                "boxed" => controls.toggle(
                    Control::new(id, TestHelp::TargetToggle, toggle()).enabled(enabled),
                    TestMessage::Tick,
                ),
                "solo" => controls.canvas(
                    Control::new(id, TestHelp::TargetToggle, Solo).enabled(enabled),
                    Some(TestMessage::Tick),
                ),
                "follow" => controls.canvas(
                    Control::new(id, TestHelp::TargetToggle, FollowToggle).enabled(enabled),
                    Some(TestMessage::Tick),
                ),
                _ => controls.canvas(
                    Control::new(id, TestHelp::TargetToggle, StripIcon).enabled(enabled),
                    Some(TestMessage::Tick),
                ),
            };
            let mut tree = Tree::new(&widget);
            let node = widget.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(200.0, 100.0)),
            );
            let event = Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Enter),
                modified_key: keyboard::Key::Named(keyboard::key::Named::Enter),
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                modifiers,
                text: None,
                repeat,
            });
            let mut messages = vec![];
            let mut shell = Shell::new(&mut messages);
            widget.as_widget_mut().update(
                &mut tree,
                &event,
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &Rectangle::with_size(Size::new(200.0, 100.0)),
            );
            assert_eq!(shell.is_event_captured(), captured, "{kind}");
            assert_eq!(
                messages,
                if activated {
                    vec![TestMessage::Tick]
                } else {
                    vec![]
                },
                "{kind}"
            );
        }
    }
}

#[test]
fn readout_reads_the_recessed_face_in_the_darkest_frame() {
    let t = LIVE_DARK;
    let on = Readout::right().look(&t, state(Lit::Off, true), Interaction::Idle);
    assert_eq!(
        (on.fill, on.ink, on.frame),
        (
            t.display_background,
            t.control_foreground,
            t.control_contrast_frame
        )
    );
    let off = Readout::left().look(&t, state(Lit::Off, false), Interaction::Idle);
    assert_eq!(
        (off.fill, off.ink, off.frame),
        (
            t.display_background,
            t.text_disabled,
            t.control_contrast_frame
        )
    );
    // Nothing presses a readout: pointer state changes nothing.
    assert_eq!(
        Readout::right().look(&t, state(Lit::Off, true), Interaction::Hovered),
        on
    );
}

#[test]
fn readout_box_takes_no_focus_in_any_role_and_numbers_read_from_the_right() {
    let focus = crate::control_focus::ControlFocus::<TestKey>::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    let id = crate::control_focus::ControlId::action(View::Track, "readout", ControlOwner::Song);
    for role in [SizeRole::Fill, SizeRole::Strip] {
        let mut element: iced::Element<'_, TestMessage> = controls.readout_box(
            Control::new(id, TestHelp::OutputHost, Readout::right()).role(role),
            "127.0.0.1:9000",
        );
        assert!(
            crate::control_focus::test_registry::rectangles::<_, TestKey>(&mut element).is_empty(),
            "{role:?} readout registers no focus target"
        );
    }
    assert_eq!(Readout::right().role(), SizeRole::Fill);
    assert_eq!(Readout::right().align, iced::alignment::Horizontal::Right);
    assert_eq!(Readout::left().align, iced::alignment::Horizontal::Left);
}

#[test]
fn argument_text_is_the_one_reading_of_an_osc_argument() {
    use qtable_engine::model::OscArgument::*;
    assert_eq!(argument_text(&Int(-3)), "-3");
    assert_eq!(argument_text(&Float(1.0)), "1.00");
    assert_eq!(argument_text(&Float(0.125)), "0.125");
    assert_eq!(argument_text(&Float(0.123)), "0.123");
    assert_eq!(argument_text(&Bool(true)), "true");
    assert_eq!(argument_text(&String("kick".into())), "kick");
}

#[test]
fn field_formats_keep_each_callers_rounding() {
    use qtable_engine::model::FieldType;
    // The face (collapsed.rs `display`): Int rounds to whole, the rest to at most four decimals.
    assert_eq!(ValueFormat::Field(FieldType::Int).display(2.6), "3");
    assert_eq!(ValueFormat::Field(FieldType::Int).display(-12.0), "-12");
    assert_eq!(ValueFormat::Field(FieldType::Float).display(0.5), "0.50");
    assert_eq!(
        ValueFormat::Field(FieldType::Float).display(0.123456),
        "0.1235"
    );
    // The definition editor's Min and Max (definitions.rs `range_text`): Int as stored, Float
    // in its shortest decimals.
    assert_eq!(ValueFormat::FieldBound(FieldType::Int).display(2.0), "2");
    assert_eq!(ValueFormat::FieldBound(FieldType::Int).display(2.6), "2.6");
    assert_eq!(
        ValueFormat::FieldBound(FieldType::Float).display(1.0),
        "1.00"
    );
    assert_eq!(
        ValueFormat::FieldBound(FieldType::Float).display(0.123456),
        "0.123456"
    );
}

#[test]
fn an_untouched_field_display_submits_as_no_change() {
    use qtable_engine::model::FieldType;
    let format = ValueFormat::Field(FieldType::Float);
    assert!(format.is_untouched_display(" 0.50 ", 0.5));
    assert!(!format.is_untouched_display("0.51", 0.5));
}

#[test]
fn focused_fader_brackets_frame_handle_and_meter_without_joining_edges() {
    use iced::advanced::renderer::Headless;
    use iced::advanced::{Layout, layout, widget::Tree};
    for value in [0.0, 0.5, 1.0] {
        for (selected, enabled) in [(true, true), (false, true), (true, false)] {
            let id =
                crate::control_focus::ControlId::action(View::Mixer, "volume", ControlOwner::Song);
            let mut focus = crate::control_focus::ControlFocus::<TestKey>::default();
            if selected {
                focus.claim(id);
            }
            let controls = Controls::new(LIVE_DARK, &focus);
            let mut widget = controls.fader(
                Control::new(id, TestHelp::Volume, Fader { value, level: 0.4 }).enabled(enabled),
                value,
                false,
                |_| TestMessage::Tick,
                |_| TestMessage::Tick,
            );
            let mut renderer = crate::grid::test_renderer();
            let mut tree = Tree::new(&widget);
            let size = Size::new(27.0, 96.0);
            let node = widget.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, size),
            );
            widget.as_widget().draw(
                &tree,
                &mut renderer,
                &iced::Theme::Dark,
                &iced::advanced::renderer::Style::default(),
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &iced::Rectangle::with_size(size),
            );
            let pixels = renderer.screenshot(Size::new(27, 96), 1.0, Color::BLACK);
            let pixel = |x: usize, y: usize| {
                let offset = (y * 27 + x) * 4;
                &pixels[offset..offset + 4]
            };
            let stroke = LIVE_DARK.focus_stroke.into_rgba8();
            for (x, y) in [(0, 2), (26, 2), (0, 93), (26, 93)] {
                assert_eq!(pixel(x, y) == stroke, selected && enabled);
            }
            for (x, y) in [(13, 0), (13, 95), (0, 48), (26, 48)] {
                assert_ne!(pixel(x, y), stroke, "the middle edges stay open");
            }
        }
    }
}

#[test]
fn the_focused_activator_is_outlined_by_a_circle_not_a_square() {
    use crate::style::detail::DEVICE_ACTIVATOR_SIZE;
    use iced::advanced::renderer::Headless;
    use iced::advanced::{Layout, layout, widget::Tree};
    let side = DEVICE_ACTIVATOR_SIZE as usize;
    let render = |selected: bool| {
        let id = crate::control_focus::ControlId::action(
            View::Track,
            "device-on",
            ControlOwner::Device(qtable_engine::model::DeviceId(1)),
        );
        let mut focus = crate::control_focus::ControlFocus::<TestKey>::default();
        if selected {
            focus.claim(id);
        }
        let controls = Controls::new(LIVE_DARK, &focus);
        let mut widget = controls.activator(
            Control::new(id, TestHelp::Volume, Activator).lit(true),
            TestMessage::Tick,
        );
        let mut renderer = crate::grid::test_renderer();
        let mut tree = Tree::new(&widget);
        let size = Size::new(DEVICE_ACTIVATOR_SIZE, DEVICE_ACTIVATOR_SIZE);
        let node = widget.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        widget.as_widget().draw(
            &tree,
            &mut renderer,
            &iced::Theme::Dark,
            &iced::advanced::renderer::Style::default(),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &iced::Rectangle::with_size(size),
        );
        renderer.screenshot(Size::new(side as u32, side as u32), 1.0, Color::BLACK)
    };
    let (plain, focused) = (render(false), render(true));
    let at = |pixels: &[u8], x: usize, y: usize| pixels[(y * side + x) * 4..][..4].to_vec();
    // The stroke reaches the middle of each edge, as the circle's own frame does...
    for (x, y) in [
        (0, side / 2),
        (side - 1, side / 2),
        (side / 2, 0),
        (side / 2, side - 1),
    ] {
        assert_ne!(at(&focused, x, y), at(&plain, x, y), "edge ({x}, {y})");
    }
    // ...and leaves the corners clear, where a square stroke would paint them.
    let stroke = LIVE_DARK.focus_stroke.into_rgba8().to_vec();
    for (x, y) in [(0, 0), (side - 1, 0), (0, side - 1), (side - 1, side - 1)] {
        assert_eq!(at(&focused, x, y), at(&plain, x, y), "corner ({x}, {y})");
        assert_ne!(at(&focused, x, y), stroke, "corner ({x}, {y})");
    }
}

#[test]
fn a_control_declares_what_it_drives_to_widget_operations() {
    use qtable_engine::model::{ControlRef, TrackId};
    let focus = crate::control_focus::ControlFocus::default();
    let controls = Controls::new(LIVE_DARK, &focus);
    let id = ControlId::action(View::Mixer, "solo", ControlOwner::Track(TrackId(1)));
    let mapped =
        Control::new(id, TestHelp::TrackSolo, Solo).drives(ControlRef::TrackSolo(TrackId(1)));
    let mut element = controls.canvas(mapped, Some(TestMessage::Tick));
    let size = Size::new(100.0, 40.0);
    assert_eq!(
        crate::control_focus::test_registry::declared(&mut element, size),
        [ControlRef::TrackSolo(TrackId(1))]
    );
    let plain = Control::new(id, TestHelp::TrackSolo, Solo);
    let mut element = controls.canvas(plain, Some(TestMessage::Tick));
    assert!(crate::control_focus::test_registry::declared(&mut element, size).is_empty());
}
