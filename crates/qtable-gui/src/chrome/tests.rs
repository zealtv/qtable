//! Region toggles, shortcuts, startup switches and the strip sections.
use super::*;
use crate::drafts::TrackField;
use crate::{drafts::DraftKey, fields::format_tempo, messages::FileMessage, update::update};
use qtable_engine::engine::Engine;

#[test]
fn toggles_survive_project_replacement_without_dirtying_the_set() {
    let project = crate::files::new_project();
    let tempo = format_tempo(project.tempo.bpm());
    let mut app = App::new(Engine::new(project).unwrap(), tempo, None);
    for region in [Region::Browser, Region::Detail, Region::Info] {
        let _ = update(&mut app, Message::ToggleRegion(region));
    }
    let collapsed = app.chrome;
    assert!(collapsed.browser_open && !collapsed.detail_open && collapsed.info_open);
    assert!(!app.is_dirty());
    let _ = update(&mut app, Message::Files(FileMessage::New));
    assert_eq!(app.project_generation, 1);
    assert_eq!(app.chrome, collapsed);
    for region in [Region::Browser, Region::Detail, Region::Info] {
        let _ = update(&mut app, Message::ToggleRegion(region));
    }
    assert_eq!(app.chrome, State::default());
}

#[test]
fn collapsing_detail_commits_the_field_before_removing_its_widget() {
    let project = crate::files::new_project();
    let tempo = format_tempo(project.tempo.bpm());
    let mut app = App::new(Engine::new(project).unwrap(), tempo, None);
    let track = app.project().tracks[0].id;
    let key = DraftKey::Track(track, TrackField::Name);
    app.drafts.values.insert(key, "Renamed".into());
    app.pending_field = Some(key);
    let _ = update(&mut app, Message::ToggleRegion(Region::Detail));
    assert!(!app.chrome.detail_open);
    assert_eq!(app.project().tracks[0].name, "Renamed");
    assert!(app.pending_field.is_none());
}

#[test]
fn opening_detail_keeps_the_browser_visibility_and_sizes() {
    for browser_open in [false, true] {
        let project = crate::files::new_project();
        let mut app = App::new(Engine::new(project).unwrap(), String::new(), None);
        app.chrome.browser_open = browser_open;
        app.chrome.detail_open = false;
        let sizes = app.dividers;
        for _ in 0..3 {
            let _ = update(&mut app, Message::ToggleRegion(Region::Detail));
            assert!(app.chrome.detail_open);
            assert_eq!(app.chrome.browser_open, browser_open);
            assert_eq!(app.dividers, sizes);
            let _ = update(&mut app, Message::ToggleRegion(Region::Detail));
            assert!(!app.chrome.detail_open);
            assert_eq!(app.chrome.browser_open, browser_open);
        }
        assert!(!app.is_dirty());
    }
}

#[test]
fn shortcuts_require_command_option_and_do_not_repeat() {
    let both = Modifiers::COMMAND | Modifiers::ALT;
    let b = Key::Character("b".into());
    let l = Key::Character("l".into());
    assert_eq!(shortcut(&b, both, false), Some(Region::Browser));
    assert_eq!(shortcut(&l, both, false), Some(Region::Detail));
    for modifiers in [
        Modifiers::NONE,
        Modifiers::COMMAND,
        Modifiers::ALT,
        both | Modifiers::SHIFT,
    ] {
        assert_eq!(shortcut(&b, modifiers, false), None);
    }
    assert_eq!(shortcut(&b, both, true), None);
    assert_eq!(shortcut(&Key::Character("x".into()), both, false), None);
}

#[test]
fn outputs_toggle_and_insert_shortcuts_use_command_option() {
    let both = Modifiers::COMMAND | Modifiers::ALT;
    let r = Key::Character("r".into());
    let t = Key::Character("t".into());
    assert_eq!(shortcut(&r, both, false), Some(Region::Outputs));
    assert!(insert_output_shortcut(&t, both, false));
    assert!(!insert_output_shortcut(&t, both, true));
    assert!(!insert_output_shortcut(&t, Modifiers::COMMAND, false));
    let (_, state) = startup([OsString::from("--collapse-outputs")]).unwrap();
    assert!(!state.outputs_open);
    let i = Key::Character("i".into());
    assert_eq!(
        shortcut(&i, both, false),
        Some(Region::Section(Row::Output))
    );
    let s = Key::Character("s".into());
    assert_eq!(shortcut(&s, both, false), None);
    // Command-S alone stays Save.
    assert_eq!(shortcut(&s, Modifiers::COMMAND, false), None);
    let m = Key::Character("m".into());
    assert_eq!(shortcut(&m, both, false), Some(Region::Strip));
    assert_eq!(shortcut(&m, Modifiers::COMMAND, false), None);
}

#[test]
fn startup_switches_uncheck_sections_and_hide_the_strip() {
    let rows = |arg: &str| startup([OsString::from(arg)]).unwrap().1.rows();
    assert_eq!(
        rows("--collapse-output"),
        [Row::Path, Row::Mixer, Row::Throughput]
    );
    assert_eq!(
        rows("--collapse-path"),
        [Row::Output, Row::Mixer, Row::Throughput]
    );
    assert_eq!(
        rows("--collapse-mixer"),
        [Row::Output, Row::Path, Row::Throughput]
    );
    assert_eq!(
        rows("--collapse-timing"),
        [Row::Output, Row::Path, Row::Mixer]
    );
    assert!(rows("--hide-strip").is_empty());
    let (_, state) = startup([OsString::from("--sections-menu-open")]).unwrap();
    assert!(state.start_with_sections_menu);
}

#[test]
fn rows_follow_the_checks_while_the_strip_is_on() {
    let mut state = State::default();
    assert_eq!(
        state.rows(),
        [Row::Output, Row::Path, Row::Mixer, Row::Throughput]
    );
    state.toggle(Region::Section(Row::Output));
    assert_eq!(state.rows(), [Row::Path, Row::Mixer, Row::Throughput]);
    // The switch hides every row but forgets no check.
    state.toggle(Region::Strip);
    assert!(state.rows().is_empty());
    state.toggle(Region::Strip);
    assert_eq!(state.rows(), [Row::Path, Row::Mixer, Row::Throughput]);
}

#[test]
fn checking_a_section_shows_a_hidden_strip_but_unchecking_does_not() {
    let mut state = State::default();
    state.toggle(Region::Strip);
    state.toggle(Region::Section(Row::Output));
    assert!(!state.strip_open && !state.output_open);
    state.toggle(Region::Section(Row::Output));
    assert!(state.strip_open && state.output_open);
}

#[test]
fn sections_toggle_independently() {
    let project = crate::files::new_project();
    let tempo = format_tempo(project.tempo.bpm());
    let mut app = App::new(Engine::new(project).unwrap(), tempo, None);
    let _ = update(&mut app, Message::ToggleRegion(Region::Section(Row::Mixer)));
    assert!(app.chrome.output_open && !app.chrome.mixer_open);
    let _ = update(
        &mut app,
        Message::ToggleRegion(Region::Section(Row::Output)),
    );
    assert!(!app.chrome.output_open && !app.chrome.mixer_open);
    assert!(!app.is_dirty());
}

#[test]
fn screenshot_switches_work_before_or_after_the_project_path() {
    for args in [
        vec!["--collapse-browser", "set.ron", "--collapse-detail"],
        vec!["set.ron", "--collapse-detail", "--collapse-browser"],
    ] {
        let (path, state) = startup(args.into_iter().map(OsString::from)).unwrap();
        assert_eq!(path, Some(PathBuf::from("set.ron")));
        assert!(!state.browser_open && !state.detail_open);
    }
    assert_eq!(startup([]).unwrap(), (None, State::default()));
}

#[test]
fn question_mark_uses_the_typed_character_and_respects_text_fields() {
    use iced::event::Status;
    let slash = Key::Character("/".into());
    let question = Key::Character("?".into());
    assert_eq!(
        event_shortcut(&slash, &question, Modifiers::SHIFT, false, Status::Ignored),
        Some(Region::Info)
    );
    assert_eq!(
        event_shortcut(&slash, &question, Modifiers::SHIFT, false, Status::Captured),
        None
    );
    assert_eq!(
        event_shortcut(&slash, &slash, Modifiers::NONE, false, Status::Ignored),
        None
    );
    assert_eq!(
        event_shortcut(&slash, &question, Modifiers::SHIFT, true, Status::Ignored),
        None
    );
    for modifier in [Modifiers::COMMAND, Modifiers::ALT, Modifiers::CTRL] {
        assert_eq!(shortcut(&question, modifier, false), None);
    }
    let (path, chrome) = startup([OsString::from("--show-info")]).unwrap();
    assert_eq!(path, None);
    assert!(chrome.info_open);
}

#[test]
fn detail_band_uses_fixed_device_height_and_only_clip_scene_offer_a_drag() {
    use crate::inspector_panel::Tab;
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    use iced::{Event, Point, Rectangle, Size, mouse};
    let renderer = crate::grid::test_renderer();
    let project = crate::files::new_project();
    let mut app = App::new(Engine::new(project).unwrap(), String::new(), None);
    for tab in [Tab::Track, Tab::Output, Tab::Main, Tab::Clip, Tab::Scene] {
        app.selection.inspector_tab = tab;
        for height in [768.0, 400.0] {
            let size = Size::new(1200.0, height);
            app.dividers.resize_window(size);
            let mut widget = view(
                &app,
                Space::new().height(Length::Fill).into(),
                Some(crate::view::detail_view(&app)),
            );
            let mut tree = Tree::new(widget.as_widget());
            let node = widget.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, size),
            );
            let main = Layout::new(&node)
                .children()
                .next()
                .unwrap()
                .children()
                .next()
                .unwrap()
                .children()
                .nth(1)
                .unwrap()
                .children()
                .next()
                .unwrap();
            let grid = main.children().next().unwrap().bounds();
            let divider = main.children().nth(1).unwrap().bounds();
            let band = main.children().nth(2).unwrap().bounds();
            let devices = crate::device_chain::shown(&app);
            if devices {
                assert_eq!(band.height, if height == 768.0 { 238.0 } else { 100.0 });
                assert!(
                    grid.height >= crate::grid::band::least_height(),
                    "grid keeps its minimum: {grid:?}"
                );
            }
            assert!(grid.y + grid.height <= divider.y);
            assert!(divider.y + divider.height <= band.y);
            assert!(band.y + band.height <= height);
            let cursor = mouse::Cursor::Available(Point::new(600.0, divider.center_y()));
            widget.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(mouse::Event::CursorMoved {
                    position: cursor.position().unwrap(),
                }),
                Layout::new(&node),
                cursor,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut vec![]),
                &Rectangle::with_size(size),
            );
            assert_eq!(
                widget.as_widget().mouse_interaction(
                    &tree,
                    Layout::new(&node),
                    cursor,
                    &Rectangle::with_size(size),
                    &renderer,
                ),
                if devices {
                    mouse::Interaction::None
                } else {
                    mouse::Interaction::ResizingVertically
                }
            );
            let mut messages = vec![];
            widget.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Layout::new(&node),
                cursor,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(size),
            );
            assert_eq!(
                messages
                    .iter()
                    .any(|m| matches!(m, Message::BeginDrag(Divider::Detail))),
                !devices
            );
        }
    }
}

#[test]
fn browser_tooltip_clears_its_switch_at_the_top_window_edge() {
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    use iced::{Point, Rectangle, Size, Vector, mouse};
    for open in [false, true] {
        let mut element = toggle("▶", "Browser (⌘⌥B)", Region::Browser, open);
        let renderer = crate::grid::test_renderer();
        let mut tree = Tree::new(&element);
        let viewport = Rectangle::with_size(Size::new(896.0, 672.0));
        let node = element
            .as_widget_mut()
            .layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, viewport.size()),
            )
            .move_to(Point::new(6.0, 6.0));
        let trigger = node.bounds();
        let cursor = mouse::Cursor::Available(trigger.center());
        element.as_widget_mut().update(
            &mut tree,
            &iced::Event::Mouse(mouse::Event::CursorMoved {
                position: trigger.center(),
            }),
            Layout::new(&node),
            cursor,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut vec![]),
            &viewport,
        );
        let mut overlay = element
            .as_widget_mut()
            .overlay(
                &mut tree,
                Layout::new(&node),
                &renderer,
                &viewport,
                Vector::ZERO,
            )
            .expect("hover opens the shared tooltip");
        let bubble = overlay.as_overlay_mut().layout(&renderer, viewport.size());
        let bounds = bubble.children()[0].bounds();
        assert!(bounds.y >= trigger.y + trigger.height);
        assert!(viewport.contains(bounds.position()));
        assert!(viewport.contains(Point::new(
            bounds.x + bounds.width,
            bounds.y + bounds.height
        )));
    }
}
