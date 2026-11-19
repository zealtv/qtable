use super::*;
use crate::context_menu::{Entry, MenuTarget, current_entries};
use crate::inspector::lane_model::tests::mixed_project;
use crate::messages::Message as Root;
use qtable_engine::model::ArgumentRef;
use std::ops::Range;

fn setup() -> (App, ClipKey) {
    let mut app = App::new(
        qtable_engine::engine::Engine::new(mixed_project()).unwrap(),
        "120".into(),
        None,
    );
    app.selection.clip = Some(crate::grid::Selection {
        track: TrackId(1),
        slot: 0,
    });
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    app.chrome.detail_open = true;
    app.focus = crate::focus::Focus::Detail;
    reconcile(&mut app);
    let key = ClipKey {
        track: TrackId(1),
        slot: 0,
        id: app.project().clip(TrackId(1), 0).unwrap().id,
        epoch: app.clip_view.editor.epoch,
    };
    (app, key)
}

/// The argument of the clip's envelope lane for `parameter` and `field`.
fn argument(app: &App, key: ClipKey, parameter: &str, field: &str) -> ArgumentRef {
    clip_owned(app, key)
        .unwrap()
        .lanes
        .iter()
        .filter_map(|l| l.envelope())
        .find(|e| e.target.parameter.0 == parameter && e.target.field.0 == field)
        .unwrap()
        .target
        .clone()
}
fn envelope(app: &App, key: ClipKey, parameter: &str, field: &str) -> lanes::Identity {
    lanes::Identity::Envelope(argument(app, key, parameter, field))
}
fn kick(app: &App, key: ClipKey) -> lanes::Identity {
    lanes::Identity::Event(clip_owned(app, key).unwrap().events[0].definition.clone())
}
fn points(app: &App, key: ClipKey, lane: &lanes::Identity) -> Vec<Breakpoint> {
    let lanes::Identity::Envelope(target) = lane else {
        panic!("an envelope lane");
    };
    clip_owned(app, key)
        .unwrap()
        .lanes
        .iter()
        .filter_map(|l| l.envelope())
        .find(|e| e.target == *target)
        .map(|e| e.points.clone())
        .unwrap_or_default()
}

/// Right-click as a canvas reports it, through the action the canvas publishes.
fn right_click(
    app: &mut App,
    key: ClipKey,
    lane: &lanes::Identity,
    under: Under,
) -> Option<ContentTarget> {
    app.close_overlay();
    let click = Click {
        key,
        revision: app.project_revision(),
        lane: lane.clone(),
        under,
    };
    let _ = crate::update::update(
        app,
        Root::Inspector(crate::inspector::Message::Timeline(Action::ContentMenu(
            click,
        ))),
    );
    match app.overlay.as_ref()? {
        Overlay::Context(OpenMenu {
            target: MenuTarget::Content(target),
            ..
        }) => Some(target.clone()),
        _ => None,
    }
}
fn choose(app: &mut App, target: &ContentTarget, command: ContentCommand) {
    let _ = crate::update::update(
        app,
        Root::Inspector(crate::inspector::Message::Timeline(Action::ContentCommand(
            target.clone(),
            command,
        ))),
    );
}
fn select(app: &mut App, key: ClipKey, span: Range<u64>, lanes: Vec<lanes::Identity>) {
    content::apply(
        app,
        content::Selection::time(key, BeatTime(span.start)..BeatTime(span.end), lanes),
    );
}
/// The undo steps the history holds; call it last in a test.
fn steps(app: &mut App) -> usize {
    let mut count = 0;
    while app.engine.can_undo() {
        app.engine.execute(Command::Undo).unwrap();
        count += 1;
    }
    for _ in 0..count {
        app.engine.execute(Command::Redo).unwrap();
    }
    count
}
fn target(
    key: ClipKey,
    app: &App,
    lane: lanes::Identity,
    clicked: Clicked,
    at: u64,
) -> ContentTarget {
    ContentTarget {
        key,
        revision: app.project_revision(),
        lane,
        clicked,
        at: BeatTime(at),
    }
}
/// A menu's rows by label; `-` stands for a separator and `#` marks a heading.
fn labels(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| match entry {
            Entry::Command { label, .. } => (*label).to_owned(),
            Entry::Heading(label) => format!("#{label}"),
            Entry::Separator => "-".to_owned(),
            Entry::Palette { .. } => "palette".to_owned(),
        })
        .collect()
}
fn rows_of(app: &App, target: &ContentTarget) -> Vec<String> {
    labels(&current_entries(app, MenuTarget::Content(target.clone())))
}
fn enabled(app: &App, target: &ContentTarget, wanted: &str) -> bool {
    current_entries(app, MenuTarget::Content(target.clone()))
        .iter()
        .any(|entry| matches!(entry, Entry::Command { label, enabled: true, .. } if *label == wanted))
}

const SHAPES: [&str; 7] = [
    "Sine", "Triangle", "Saw Down", "Saw Up", "Square", "Ramp In", "Ramp Out",
];
fn with_shapes(before: &[&str], after: &[&str]) -> Vec<String> {
    let mut before = before.to_vec();
    before.push("#Insert Shape");
    before.extend(SHAPES);
    before.extend(after);
    before.into_iter().map(str::to_owned).collect()
}

// ---- Opening: what a right-click settles ----

#[test]
fn a_right_click_on_an_unpicked_point_picks_it_alone_and_opens_the_point_menu() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    select(&mut app, key, 0..600, vec![opacity.clone()]);
    let menu = right_click(&mut app, key, &opacity, Under::Point(2)).unwrap();
    assert_eq!((menu.clicked, menu.at), (Clicked::Point, BeatTime(2400)));
    let selection = app.clip_view.editor.content.clone().unwrap();
    assert_eq!(selection.points.len(), 1);
    assert_eq!(selection.points[0].1, 2);
    // Its fields show in the footer: the single-point adapter holds it.
    assert_eq!(app.clip_view.editor.point.as_ref().unwrap().index, 2);
}

#[test]
fn a_right_click_on_one_of_several_picked_points_keeps_them_all() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let clip = clip_owned(&app, key).unwrap();
    for index in [1, 2] {
        let item = content::Selection::items(&clip)
            .into_iter()
            .find(|(item, i)| item.lane == opacity && *i == index)
            .unwrap()
            .0;
        content::pick(&mut app, key, item, true);
    }
    let before = app.clip_view.editor.content.clone();
    assert_eq!(before.as_ref().unwrap().points.len(), 2);
    let menu = right_click(&mut app, key, &opacity, Under::Point(1)).unwrap();
    assert_eq!(menu.clicked, Clicked::Point);
    assert_eq!(app.clip_view.editor.content, before);
}

#[test]
fn a_right_click_inside_a_time_selection_keeps_it_and_opens_its_menu() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let events = kick(&app, key);
    select(
        &mut app,
        key,
        960..2000,
        vec![opacity.clone(), events.clone()],
    );
    let before = app.clip_view.editor.content.clone();
    // A point, lane space and a hit inside it.
    for (lane, under) in [
        (&opacity, Under::Point(1)),
        (&opacity, Under::Space(BeatTime(1500))),
        (&events, Under::Hit(0)),
        (&events, Under::Hit(1)),
        (&events, Under::Space(BeatTime(1000))),
    ] {
        let menu = right_click(&mut app, key, lane, under).unwrap();
        assert_eq!(menu.clicked, Clicked::TimeSelection, "{under:?}");
        assert_eq!(menu.at, BeatTime(960));
        assert_eq!(app.clip_view.editor.content, before);
    }
    // Outside the range, or on a lane the selection leaves out, it is a fresh click.
    let menu = right_click(&mut app, key, &opacity, Under::Point(3)).unwrap();
    assert_eq!(menu.clicked, Clicked::Point);
    assert_ne!(app.clip_view.editor.content, before);
}

#[test]
fn a_right_click_on_other_lane_space_places_the_insert_marker_there() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let menu = right_click(&mut app, key, &opacity, Under::Space(BeatTime(480))).unwrap();
    assert_eq!((menu.clicked, menu.at), (Clicked::Space, BeatTime(480)));
    let selection = app.clip_view.editor.content.clone().unwrap();
    assert_eq!(selection.insert_mark(), Some(BeatTime(480)));
    assert_eq!(selection.lanes, vec![opacity]);
    assert_eq!(app.status.message(), Some("Insert Mark 1.1.3"));
    // The same on an event row.
    let events = kick(&app, key);
    let menu = right_click(&mut app, key, &events, Under::Space(BeatTime(1440))).unwrap();
    assert_eq!(menu.clicked, Clicked::Space);
    assert_eq!(
        app.clip_view.editor.content.as_ref().unwrap().insert_mark(),
        Some(BeatTime(1440))
    );
}

#[test]
fn a_right_click_on_a_hit_picks_it_unless_it_is_already_picked() {
    let (mut app, key) = setup();
    let events = kick(&app, key);
    let menu = right_click(&mut app, key, &events, Under::Hit(1)).unwrap();
    assert_eq!((menu.clicked, menu.at), (Clicked::Hit, BeatTime(1920)));
    assert_eq!(app.clip_view.editor.content.as_ref().unwrap().hits, vec![1]);
    assert_eq!(app.clip_view.editor.selected, Some((key, 1)));
}

#[test]
fn a_right_click_against_a_stale_clip_revision_or_hidden_editor_opens_nothing() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let stale = Click {
        key,
        revision: app.project_revision() + 1,
        lane: opacity.clone(),
        under: Under::Point(0),
    };
    handle(&mut app, Action::ContentMenu(stale));
    assert!(app.overlay.is_none());
    let other_epoch = ClipKey {
        epoch: key.epoch + 1,
        ..key
    };
    assert!(right_click(&mut app, other_epoch, &opacity, Under::Point(0)).is_none());
    app.chrome.detail_open = false;
    assert!(right_click(&mut app, key, &opacity, Under::Point(0)).is_none());
    app.chrome.detail_open = true;
    // Draw Mode changes what a left press does, not what a right-click opens.
    app.clip_view.editor.draw.toggle();
    assert!(right_click(&mut app, key, &opacity, Under::Point(0)).is_some());
}

// ---- The rows of the six menus ----

#[test]
fn each_menu_holds_exactly_its_column_of_the_table_in_order() {
    let (app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let events = kick(&app, key);
    let on = |lane: &lanes::Identity, clicked| target(key, &app, lane.clone(), clicked, 960);
    let envelope_rows = |clicked, head: &[&str], tail: &[&str]| {
        rows_of(&app, &on(&opacity, clicked)).eq(&with_shapes(head, tail))
    };
    assert!(envelope_rows(
        Clicked::Point,
        &[
            "Copy",
            "Paste",
            "Delete",
            "-",
            "Clear Envelope",
            "-",
            "Draw Mode",
            "-"
        ],
        &["-", "Go to Device"],
    ));
    assert!(envelope_rows(
        Clicked::Space,
        &["Paste", "-", "Clear Envelope", "-", "Draw Mode", "-"],
        &["Simplify Envelope", "-", "Go to Device"],
    ));
    assert!(envelope_rows(
        Clicked::TimeSelection,
        &[
            "Cut",
            "Copy",
            "Paste",
            "Duplicate",
            "Delete",
            "-",
            "Clear Envelope",
            "-",
            "Draw Mode",
            "-"
        ],
        &["Simplify Envelope", "-", "Go to Device"],
    ));
    let event_rows = |clicked| rows_of(&app, &on(&events, clicked));
    assert_eq!(
        event_rows(Clicked::Hit),
        [
            "Copy",
            "Paste",
            "Delete",
            "-",
            "Draw Mode",
            "-",
            "Go to Device"
        ]
    );
    assert_eq!(
        event_rows(Clicked::Space),
        ["Paste", "-", "Draw Mode", "-", "Go to Device"]
    );
    assert_eq!(
        event_rows(Clicked::TimeSelection),
        [
            "Cut",
            "Copy",
            "Paste",
            "Duplicate",
            "Delete",
            "-",
            "Draw Mode",
            "-",
            "Go to Device"
        ]
    );
}

#[test]
fn choice_and_switch_lanes_have_no_shape_rows_but_keep_simplify() {
    let (app, key) = setup();
    for (parameter, rows) in [("mode", "Enum"), ("strobe", "Toggle")] {
        let lane = envelope(&app, key, parameter, "value");
        for clicked in [Clicked::Point, Clicked::Space, Clicked::TimeSelection] {
            let all = rows_of(&app, &target(key, &app, lane.clone(), clicked, 960));
            assert!(
                all.iter()
                    .all(|l| !l.starts_with('#') && !SHAPES.contains(&l.as_str())),
                "{rows} {clicked:?}: {all:?}"
            );
            assert_eq!(
                all.contains(&"Simplify Envelope".to_owned()),
                clicked != Clicked::Point,
                "{rows} {clicked:?}"
            );
        }
    }
}

#[test]
fn no_content_menu_shows_edit_value_or_a_grid_row() {
    let (app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let events = kick(&app, key);
    for lane in [&opacity, &events] {
        for clicked in [
            Clicked::Point,
            Clicked::Hit,
            Clicked::Space,
            Clicked::TimeSelection,
        ] {
            for label in rows_of(&app, &target(key, &app, lane.clone(), clicked, 0)) {
                for banned in [
                    "Edit Value",
                    "Snap to Grid",
                    "Narrow Grid",
                    "Widen Grid",
                    "Triplet Grid",
                    "Clear All Envelopes",
                    "Re-Enable Automation",
                ] {
                    assert!(!label.contains(banned), "{label}");
                }
            }
        }
    }
}

#[test]
fn passing_state_only_dims_rows() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let point = right_click(&mut app, key, &opacity, Under::Point(0)).unwrap();
    // Paste dims while the content clipboard is empty, and lights once something is copied.
    assert!(!enabled(&app, &point, "Paste"));
    assert!(enabled(&app, &point, "Copy"));
    choose(&mut app, &point, ContentCommand::Key(Shortcut::Copy));
    assert!(app.clip_view.editor.clipboard.is_some());
    assert!(enabled(&app, &point, "Paste"));
    // Clear Envelope dims for an argument the clip has no envelope for.
    assert!(enabled(&app, &point, "Clear Envelope"));
    let mut missing = argument(&app, key, "position", "y");
    missing.field = qtable_engine::model::FieldKey("x".into());
    let none = target(
        key,
        &app,
        lanes::Identity::Envelope(missing),
        Clicked::Space,
        0,
    );
    assert!(!enabled(&app, &none, "Clear Envelope"));
    // Simplify dims below three points.
    select(&mut app, key, 0..0, vec![opacity.clone()]);
    let space = target(key, &app, opacity, Clicked::Space, 0);
    assert!(enabled(&app, &space, "Simplify Envelope"));
    let empty = envelope(&app, key, "position", "y");
    select(&mut app, key, 0..0, vec![empty.clone()]);
    let space = target(key, &app, empty, Clicked::Space, 0);
    assert!(!enabled(&app, &space, "Simplify Envelope"));
}

#[test]
fn draw_mode_shows_a_tick_only_while_on_and_the_row_flips_the_mode_and_closes_the_menu() {
    let (mut app, key) = setup();
    let events = kick(&app, key);
    let menu = right_click(&mut app, key, &events, Under::Hit(0)).unwrap();
    let ticked = |app: &App| {
        current_entries(app, MenuTarget::Content(menu.clone()))
            .into_iter()
            .find_map(|entry| match entry {
                Entry::Command {
                    label: "Draw Mode",
                    shortcut,
                    checked,
                    message,
                    ..
                } => {
                    assert_eq!(shortcut, "B");
                    Some((checked, message))
                }
                _ => None,
            })
            .unwrap()
    };
    let (checked, message) = ticked(&app);
    assert!(!checked && !app.clip_view.editor.draw.on);
    // The row sends the very message the Control Bar button sends.
    assert!(matches!(message, Root::ToggleDrawMode));
    let _ = crate::update::update(&mut app, message);
    assert!(app.clip_view.editor.draw.on);
    assert!(app.overlay.is_none());
    let menu = right_click(&mut app, key, &events, Under::Hit(0)).unwrap();
    assert!(ticked(&app).0, "{menu:?}");
}

// ---- What the rows do ----

#[test]
fn delete_from_a_point_menu_removes_the_picked_point_in_one_step() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let menu = right_click(&mut app, key, &opacity, Under::Point(2)).unwrap();
    let before = points(&app, key, &opacity);
    choose(&mut app, &menu, ContentCommand::Key(Shortcut::Delete));
    let after = points(&app, key, &opacity);
    assert_eq!(after.len(), before.len() - 1);
    assert!(after.iter().all(|p| p.time != BeatTime(2400)));
    assert_eq!(app.project_revision(), 1);
    assert_eq!(steps(&mut app), 1);
    assert!(app.overlay.is_none());
}

#[test]
fn delete_from_one_of_several_picked_points_removes_them_all_in_one_step() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let clip = clip_owned(&app, key).unwrap();
    for index in [1, 2] {
        let item = content::Selection::items(&clip)
            .into_iter()
            .find(|(item, i)| item.lane == opacity && *i == index)
            .unwrap()
            .0;
        content::pick(&mut app, key, item, true);
    }
    let menu = right_click(&mut app, key, &opacity, Under::Point(2)).unwrap();
    choose(&mut app, &menu, ContentCommand::Key(Shortcut::Delete));
    let times: Vec<_> = points(&app, key, &opacity).iter().map(|p| p.time).collect();
    assert_eq!(times, [BeatTime(0), BeatTime(3840)]);
    assert_eq!(app.project_revision(), 1);
    assert_eq!(steps(&mut app), 1);
}

#[test]
fn copy_then_paste_from_the_menus_round_trips_a_point() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let menu = right_click(&mut app, key, &opacity, Under::Point(2)).unwrap();
    choose(&mut app, &menu, ContentCommand::Key(Shortcut::Copy));
    assert_eq!(app.project_revision(), 0);
    let copied = points(&app, key, &opacity)[2].clone();
    let menu = right_click(&mut app, key, &opacity, Under::Space(BeatTime(480))).unwrap();
    choose(&mut app, &menu, ContentCommand::Key(Shortcut::Paste));
    let pasted = points(&app, key, &opacity)
        .into_iter()
        .find(|p| p.time == BeatTime(480))
        .expect("the point lands at the insert marker");
    assert_eq!((pasted.value, pasted.curve), (copied.value, copied.curve));
}

#[test]
fn a_shape_from_a_points_menu_fills_one_grid_step_from_the_points_time() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let menu = right_click(&mut app, key, &opacity, Under::Point(1)).unwrap();
    assert_eq!(app.clip_view.editor.grid, Grid::Sixteenth);
    choose(&mut app, &menu, ContentCommand::Shape(Shape::Triangle));
    let all = points(&app, key, &opacity);
    // The clicked point stays, as the value the step is entered from, and the triangle fills
    // the sixteenth that starts at its time.
    assert!(
        all.iter()
            .any(|p| p.time == BeatTime(1200) && (p.value - 0.9).abs() < 1e-9)
    );
    let peak = all
        .iter()
        .find(|p| p.time == BeatTime(1320))
        .expect("the triangle's peak sits mid-step");
    assert!((peak.value - 1.0).abs() < 1e-9);
    let selected = app.clip_view.editor.content.clone().unwrap();
    assert_eq!(selected.range, BeatTime(1200)..BeatTime(1440));
    assert_eq!(app.project_revision(), 1);
    assert_eq!(steps(&mut app), 1);
}

#[test]
fn with_grid_off_a_shape_from_a_point_fills_a_sixteenth() {
    let (mut app, key) = setup();
    app.clip_view.editor.grid = Grid::Off;
    let opacity = envelope(&app, key, "opacity", "value");
    let menu = right_click(&mut app, key, &opacity, Under::Point(1)).unwrap();
    choose(&mut app, &menu, ContentCommand::Shape(Shape::SawUp));
    let selected = app.clip_view.editor.content.clone().unwrap();
    assert_eq!(selected.range, BeatTime(1200)..BeatTime(1440));
}

#[test]
fn a_shape_from_empty_space_fills_one_grid_step_from_the_insert_marker() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let menu = right_click(&mut app, key, &opacity, Under::Space(BeatTime(480))).unwrap();
    choose(&mut app, &menu, ContentCommand::Shape(Shape::Square));
    let selected = app.clip_view.editor.content.clone().unwrap();
    assert_eq!(selected.range, BeatTime(480)..BeatTime(720));
    assert_eq!(app.project_revision(), 1);
    assert!(
        points(&app, key, &opacity)
            .iter()
            .any(|p| p.time == BeatTime(600))
    );
}

#[test]
fn a_shape_from_a_time_selections_menu_reaches_every_selected_lane_that_takes_shapes() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let steps_lane = envelope(&app, key, "steps", "value");
    let mode = envelope(&app, key, "mode", "value");
    let events = kick(&app, key);
    select(
        &mut app,
        key,
        960..1920,
        vec![opacity.clone(), steps_lane.clone(), mode.clone(), events],
    );
    let before = clip_owned(&app, key).unwrap();
    let menu = right_click(&mut app, key, &opacity, Under::Space(BeatTime(1000))).unwrap();
    assert_eq!(menu.clicked, Clicked::TimeSelection);
    choose(&mut app, &menu, ContentCommand::Shape(Shape::Sine));
    let after = clip_owned(&app, key).unwrap();
    assert_eq!(app.project_revision(), 1);
    for lane in [&opacity, &steps_lane] {
        assert_ne!(points(&app, key, lane), {
            let lanes::Identity::Envelope(t) = lane else {
                unreachable!()
            };
            before
                .lanes
                .iter()
                .filter_map(|l| l.envelope())
                .find(|e| e.target == *t)
                .unwrap()
                .points
                .clone()
        });
    }
    // A choice lane and the hits stay as they were.
    let mode_points = |clip: &Clip| {
        let lanes::Identity::Envelope(t) = &mode else {
            unreachable!()
        };
        clip.lanes
            .iter()
            .filter_map(|l| l.envelope())
            .find(|e| e.target == *t)
            .unwrap()
            .points
            .clone()
    };
    assert_eq!(mode_points(&after), mode_points(&before));
    assert_eq!(after.events, before.events);
}

#[test]
fn simplify_from_a_menu_thins_the_lane_clicked() {
    let (mut app, key) = setup();
    let steps_lane = envelope(&app, key, "steps", "value");
    // Collinear points in the middle of a lane.
    let steps_target = argument(&app, key, "steps", "value");
    let dense = {
        let mut project = app.project().clone();
        let clip = project.tracks[0].slots[0].clip.as_mut().unwrap();
        for lane in &mut clip.lanes {
            if let qtable_engine::model::Lane::Envelope(e) = lane
                && e.target == steps_target
            {
                e.points = (0..=8)
                    .map(|i| Breakpoint {
                        time: BeatTime(i * 480),
                        value: i as f64,
                        curve: 0.0,
                    })
                    .collect();
            }
        }
        project
    };
    assert!(crate::edit_actions::run_edit(
        &mut app,
        Command::LoadProject(dense)
    ));
    reconcile(&mut app);
    let key = ClipKey {
        epoch: app.clip_view.editor.epoch,
        ..key
    };
    let menu = right_click(&mut app, key, &steps_lane, Under::Space(BeatTime(480))).unwrap();
    assert!(enabled(&app, &menu, "Simplify Envelope"));
    let revision = app.project_revision();
    choose(&mut app, &menu, ContentCommand::Simplify);
    assert_eq!(app.project_revision(), revision + 1);
    assert_eq!(points(&app, key, &steps_lane).len(), 2);
}

#[test]
fn clear_envelope_removes_the_clicked_envelope_in_one_step_and_undo_restores_it() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let before = clip_owned(&app, key).unwrap();
    let menu = right_click(&mut app, key, &opacity, Under::Point(1)).unwrap();
    choose(&mut app, &menu, ContentCommand::ClearEnvelope);
    let after = clip_owned(&app, key).unwrap();
    assert_eq!(after.lanes.len(), before.lanes.len() - 1);
    assert!(points(&app, key, &opacity).is_empty());
    assert_eq!(app.project_revision(), 1);
    assert!(app.clip_view.editor.content.is_none());
    app.engine.execute(Command::Undo).unwrap();
    assert_eq!(clip_owned(&app, key).unwrap(), before);
}

#[test]
fn go_to_device_opens_the_device_that_defines_each_kind_of_lane() {
    let (mut app, key) = setup();
    let events = kick(&app, key);
    let opacity = envelope(&app, key, "opacity", "value");
    for lane in [events, opacity] {
        let clicked = if matches!(lane, lanes::Identity::Event(_)) {
            Clicked::Hit
        } else {
            Clicked::Point
        };
        let menu = target(key, &app, lane, clicked, 0);
        let go = current_entries(&app, MenuTarget::Content(menu))
            .into_iter()
            .find_map(|entry| match entry {
                Entry::Command {
                    label: "Go to Device",
                    message,
                    enabled: true,
                    ..
                } => Some(message),
                _ => None,
            })
            .expect("an enabled Go to Device row");
        app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
        let _ = crate::update::update(&mut app, go);
        assert_eq!(
            app.selection.inspector_tab,
            crate::inspector_panel::Tab::Track
        );
    }
}

// ---- Keyboard and lifetime ----

#[test]
fn the_keyboard_reaches_each_enabled_row_and_skips_headings_and_dimmed_rows() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    let menu = right_click(&mut app, key, &opacity, Under::Point(1)).unwrap();
    let Some(Overlay::Context(open)) = app.overlay.clone() else {
        panic!("open");
    };
    let rows = crate::context_menu::rows(&app, &open);
    let entries = current_entries(&app, MenuTarget::Content(menu));
    let enabled_commands = entries
        .iter()
        .filter(|e| matches!(e, Entry::Command { enabled: true, .. }))
        .count();
    // Paste is dimmed (nothing copied): the rest of the commands are reachable.
    assert_eq!(rows.presses.len(), enabled_commands);
    assert!(entries.iter().any(|e| matches!(
        e,
        Entry::Command {
            label: "Paste",
            enabled: false,
            ..
        }
    )));
    assert!(entries.iter().any(|e| matches!(e, Entry::Heading(_))));
    assert_eq!(enabled_commands, entries.len() - 1 - 4 - 1);
    // Down reaches the first row, Enter chooses it (Copy), and the menu closes.
    let press = |app: &mut App, named| {
        let _ = crate::update::update(
            app,
            Root::KeyPressed {
                key: iced::keyboard::Key::Named(named),
                modifiers: iced::keyboard::Modifiers::NONE,
                repeat: false,
                ignored: true,
            },
        );
    };
    use iced::keyboard::key::Named;
    press(&mut app, Named::ArrowDown);
    press(&mut app, Named::Enter);
    assert!(app.overlay.is_none());
    assert!(app.clip_view.editor.clipboard.is_some(), "Copy ran");
    // Escape closes.
    right_click(&mut app, key, &opacity, Under::Point(1)).unwrap();
    press(&mut app, Named::Escape);
    assert!(app.overlay.is_none());
}

#[test]
fn the_menu_closes_when_its_clip_epoch_or_revision_goes() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    right_click(&mut app, key, &opacity, Under::Point(1)).unwrap();
    app.validate_overlay();
    assert!(app.overlay.is_some());
    // The project moves on under it.
    assert!(crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::RenameClip {
            track: key.track,
            slot: 0,
            name: "new".into(),
        })
    ));
    app.validate_overlay();
    assert!(app.overlay.is_none());
    // So does a new editor epoch (another clip or a load).
    let menu = right_click(&mut app, key, &opacity, Under::Point(1)).unwrap();
    app.reset_for_project();
    app.validate_overlay();
    assert!(app.overlay.is_none());
    // A row of a closed, stale menu does nothing.
    let before = app.project().clone();
    choose(&mut app, &menu, ContentCommand::Key(Shortcut::Delete));
    assert_eq!(app.project(), &before);
}

#[test]
fn no_value_readout_shows_while_a_content_menu_is_open() {
    let (mut app, key) = setup();
    let opacity = envelope(&app, key, "opacity", "value");
    // The pointer rests on a point, reading its value.
    handle(&mut app, Action::Hover(Some("0.90".into())));
    assert_eq!(app.clip_view.editor.hover.as_deref(), Some("0.90"));
    right_click(&mut app, key, &opacity, Under::Point(1)).unwrap();
    assert_eq!(app.clip_view.editor.hover, None);
    // A readout published while the menu is open (a canvas that has not noticed it) is dropped.
    handle(&mut app, Action::Hover(Some("5".into())));
    assert_eq!(app.clip_view.editor.hover, None);
    // Once the menu has closed the readouts come back.
    app.close_overlay();
    handle(&mut app, Action::Hover(Some("0.50".into())));
    assert_eq!(app.clip_view.editor.hover.as_deref(), Some("0.50"));
}
