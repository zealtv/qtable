//! The one overlay slot: exclusivity, stale targets and the route dropdown,
//! through the public update router.
use super::update;
use crate::app::App;
use crate::grid;
use crate::messages::{EditMessage, Message, OutputMessage};
use crate::output_band::RouteChoice;
use crate::overlay::{DropdownOwner, OpenDropdown, Overlay};
use qtable_engine::engine::Engine;
use qtable_engine::model::{Project, TrackId};

fn send(app: &mut App, message: Message) {
    let _ = update(app, message);
}

fn route_key(app: &mut App, named: iced::keyboard::key::Named) {
    let message = Message::KeyPressed {
        key: iced::keyboard::Key::Named(named),
        modifiers: iced::keyboard::Modifiers::NONE,
        repeat: false,
        ignored: true,
    };
    send(app, message);
}

fn highlight(app: &App) -> Option<usize> {
    match app.overlay {
        Some(Overlay::Dropdown(dropdown)) => dropdown.highlight,
        _ => panic!("route dropdown is closed"),
    }
}

#[test]
fn keyboard_anchor_uses_the_face_bottom_and_ignores_stale_replies() {
    let mut app = app_with(1);
    let owner = first_track(&app);
    let generation = app.project_generation;
    let bounds = iced::Rectangle {
        x: 220.0,
        y: 310.0,
        width: 100.0,
        height: 20.0,
    };
    let _ = app.open_route_dropdown_at_face(owner);
    send(
        &mut app,
        Message::RouteAnchored(owner, generation + 1, Some(bounds)),
    );
    assert!(matches!(app.overlay, Some(Overlay::Dropdown(d)) if d.anchor_pending));
    send(
        &mut app,
        Message::RouteAnchored(owner, generation, Some(bounds)),
    );
    assert!(
        matches!(app.overlay, Some(Overlay::Dropdown(d)) if !d.anchor_pending && d.at == crate::control::menu::Anchor::below(bounds, 2.0))
    );
    app.close_overlay();
    send(
        &mut app,
        Message::RouteAnchored(owner, generation, Some(bounds)),
    );
    assert!(app.overlay.is_none());
    let _ = app.open_route_dropdown_at_face(owner);
    send(&mut app, Message::RouteAnchored(owner, generation, None));
    assert!(
        app.overlay.is_none(),
        "a hidden or missing face cannot anchor a keyboard list"
    );
    // A reply cannot reposition an unrelated click-opened list.
    app.cursor = iced::Point::new(10.0, 20.0);
    app.open_route_dropdown(owner);
    send(
        &mut app,
        Message::RouteAnchored(owner, generation, Some(bounds)),
    );
    assert!(
        matches!(app.overlay, Some(Overlay::Dropdown(d)) if d.at == crate::control::menu::Anchor::point(iced::Point::new(10.0, 20.0)))
    );
}

#[test]
fn keyboard_moves_over_both_dividers_toggles_and_keeps_the_grid_selection() {
    use iced::keyboard::key::Named;
    let mut app = app_with(3);
    let tracks: Vec<_> = app.project().tracks.iter().map(|t| t.id).collect();
    app.engine
        .execute(qtable_engine::engine::Command::Edit(
            qtable_engine::edit::ProjectEdit::CreateGroup {
                tracks: tracks.clone(),
            },
        ))
        .unwrap();
    let owner = tracks[0];
    send(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(owner, false)),
    );
    let selected = app.selection.tracks.clone();
    open_route_list(&mut app, owner);
    // Creating the group now routes its members to Group, so the radio starts selected.
    assert_eq!(highlight(&app), Some(6));
    for expected in [4, 3, 2, 0, 0] {
        route_key(&mut app, Named::ArrowUp);
        assert_eq!(highlight(&app), Some(expected));
    }
    for expected in [2, 3, 4, 6, 6] {
        route_key(&mut app, Named::ArrowDown);
        assert_eq!(highlight(&app), Some(expected));
    }
    route_key(&mut app, Named::Space);
    assert_eq!(
        app.project()[owner].route,
        qtable_engine::model::Route::Group
    );
    assert_eq!(highlight(&app), Some(6));
    route_key(&mut app, Named::ArrowUp);
    assert_eq!(highlight(&app), Some(4));
    let output = app.project().outputs[2].id;
    route_key(&mut app, Named::Enter);
    assert_eq!(
        app.project()[owner].route,
        qtable_engine::model::Route::single(output)
    );
    assert_eq!(highlight(&app), Some(4));
    route_key(&mut app, Named::Space);
    assert_eq!(
        app.project()[owner].route,
        qtable_engine::model::Route::empty()
    );
    assert_eq!(app.selection.tracks, selected);
    assert!(!app.snapshot.playing, "Return must not launch the grid");
    route_key(&mut app, Named::Escape);
    assert!(app.overlay.is_none());
}

#[test]
fn focused_face_opens_on_space_return_or_down_at_first_ticked_row() {
    use crate::control_focus::{ControlOwner, View};
    use crate::drafts::ControlId;
    use iced::keyboard::key::Named;
    for key in [Named::Space, Named::Enter, Named::ArrowDown] {
        let mut app = app_with(3);
        let owner = first_track(&app);
        let output = app.project().outputs[1].id;
        app.engine
            .execute(qtable_engine::engine::Command::Edit(
                qtable_engine::edit::ProjectEdit::SetTrackRoutes {
                    routes: vec![(owner, qtable_engine::model::Route::single(output))],
                },
            ))
            .unwrap();
        app.control_focus.claim(ControlId::action(
            View::Mixer,
            "route",
            ControlOwner::Track(owner),
        ));
        route_key(&mut app, key);
        assert_eq!(app.route_dropdown(), Some(owner));
        assert_eq!(highlight(&app), Some(3));
    }
}

#[test]
fn group_route_highlights_the_radio_and_empty_route_starts_at_main() {
    use qtable_engine::{edit::ProjectEdit, engine::Command, model::Route};
    let mut app = app_with(1);
    let tracks: Vec<_> = app.project().tracks.iter().map(|t| t.id).collect();
    let owner = tracks[0];
    app.engine
        .execute(Command::Edit(ProjectEdit::CreateGroup { tracks }))
        .unwrap();
    for (route, expected) in [(Route::Group, 4), (Route::empty(), 0)] {
        app.engine
            .execute(Command::Edit(ProjectEdit::SetTrackRoutes {
                routes: vec![(owner, route)],
            }))
            .unwrap();
        app.open_route_dropdown(owner);
        assert_eq!(highlight(&app), Some(expected));
    }
}

#[test]
fn dropdown_keys_do_not_take_consumed_or_modified_field_events() {
    use iced::keyboard::{Key, Modifiers, key::Named};
    for named in [
        Named::ArrowUp,
        Named::ArrowDown,
        Named::Space,
        Named::Enter,
        Named::Escape,
    ] {
        assert!(crate::output_band::key(&Key::Named(named), Modifiers::NONE, false).is_none());
        assert!(crate::output_band::key(&Key::Named(named), Modifiers::COMMAND, true).is_none());
    }
    assert_eq!(
        crate::output_band::key(&Key::Character(" ".into()), Modifiers::NONE, true),
        Some(crate::output_band::RouteKey::Space)
    );
}

#[test]
fn arrows_without_route_focus_still_navigate_the_grid() {
    use iced::keyboard::key::Named;
    let mut app = app_with(0);
    send(&mut app, Message::Edit(EditMessage::AddScene));
    let owner = first_track(&app);
    send(
        &mut app,
        Message::Grid(grid::Action::SelectTrack(owner, false)),
    );
    route_key(&mut app, Named::ArrowDown);
    assert!(app.overlay.is_none());
    assert_eq!(app.selection.clip.unwrap().track, owner);
    assert_eq!(app.selection.clip.unwrap().slot, 0);
}

#[test]
fn keyboard_grid_navigation_releases_route_face_focus_before_down_or_launch() {
    use crate::control_focus::{ControlOwner, View};
    use crate::drafts::ControlId;
    use iced::keyboard::key::Named;
    let mut app = app_with(0);
    send(&mut app, Message::Edit(EditMessage::AddScene));
    let owner = first_track(&app);
    send(&mut app, Message::Grid(grid::Action::Select(owner, 0)));
    let face = ControlId::action(View::Mixer, "route", ControlOwner::Track(owner));
    app.control_focus.claim(face);
    route_key(&mut app, Named::ArrowUp);
    assert!(!app.control_focus.has(face));
    assert!(app.selection.clip.is_none());
    route_key(&mut app, Named::ArrowDown);
    assert!(app.overlay.is_none());
    assert_eq!(app.selection.clip.unwrap().slot, 0);
    // Direct grid launch also clears any latched control outline.
    app.control_focus.claim(face);
    send(&mut app, Message::GridKey(crate::focus::GridKey::Launch));
    assert!(!app.control_focus.has(face));
    route_key(&mut app, Named::Enter);
    assert!(app.overlay.is_none());
}

/// Two tracks and `outputs` outputs.
fn app_with(outputs: usize) -> App {
    let mut app = App::new(
        Engine::new(Project::default()).unwrap(),
        String::new(),
        None,
    );
    for _ in 0..2 {
        send(&mut app, Message::Edit(EditMessage::AddTrack));
    }
    for _ in 0..outputs {
        send(&mut app, Message::Outputs(OutputMessage::AddOutput));
    }
    app
}

fn first_track(app: &App) -> TrackId {
    app.project().tracks[0].id
}

fn open_route_list(app: &mut App, track: TrackId) {
    app.cursor = iced::Point::new(120.0, 90.0);
    let owner = track;
    send(
        app,
        Message::Outputs(OutputMessage::OpenRouteDropdown(owner)),
    );
}

#[test]
fn opening_one_popover_replaces_the_other() {
    let mut app = app_with(5);
    let track = first_track(&app);
    send(&mut app, Message::Grid(grid::Action::HeaderMenu(track)));
    assert!(app.context_menu().is_some());
    send(&mut app, Message::ToggleSectionsMenu);
    assert_eq!(app.overlay, Some(Overlay::SectionsMenu));
    let output = app.project().outputs[0].id;
    send(
        &mut app,
        Message::Grid(crate::grid::Action::HeaderMenu(output)),
    );
    assert_eq!(
        app.menu_target(),
        Some(crate::context_menu::MenuTarget::Header(output))
    );
    open_route_list(&mut app, track);
    assert!(app.route_dropdown().is_some() && app.menu_target().is_none());
    // Right-clicking a track while a list is open leaves only the track's menu.
    send(&mut app, Message::Grid(grid::Action::HeaderMenu(track)));
    assert!(app.context_menu().is_some() && app.route_dropdown().is_none());
}

#[test]
fn the_sections_menu_toggles_and_closes_on_a_region_toggle() {
    let mut app = app_with(0);
    send(&mut app, Message::ToggleSectionsMenu);
    send(&mut app, Message::ToggleSectionsMenu);
    assert_eq!(app.overlay, None);
    send(&mut app, Message::ToggleSectionsMenu);
    send(
        &mut app,
        Message::ToggleRegion(crate::chrome::Region::Section(grid::Row::Mixer)),
    );
    assert_eq!(app.overlay, None);
}

#[test]
fn deleting_an_outputs_target_closes_its_menu_and_picker() {
    let mut app = app_with(6);
    let track = first_track(&app);
    let output = app.project().outputs[0].id;
    send(
        &mut app,
        Message::Grid(crate::grid::Action::HeaderMenu(output)),
    );
    // A removal arriving from elsewhere (undo, another client) still closes the menu.
    app.engine
        .execute(qtable_engine::engine::Command::Edit(
            qtable_engine::edit::ProjectEdit::RemoveTracks {
                tracks: vec![output],
            },
        ))
        .unwrap();
    app.validate_overlay();
    assert_eq!(app.overlay, None);

    open_route_list(&mut app, track);
    send(&mut app, Message::Edit(EditMessage::DeleteTrack(track)));
    assert_eq!(app.overlay, None, "the picker's track is gone");
}

#[test]
fn a_context_menu_closes_with_its_track_and_survives_an_unrelated_deletion() {
    use qtable_engine::edit::ProjectEdit;
    use qtable_engine::engine::Command;
    let mut app = app_with(0);
    send(&mut app, Message::Edit(EditMessage::AddTrack));
    let (a, b) = (app.project().tracks[0].id, app.project().tracks[1].id);
    send(&mut app, Message::Grid(grid::Action::HeaderMenu(a)));
    app.engine
        .execute(Command::Edit(ProjectEdit::RemoveTracks { tracks: vec![b] }))
        .unwrap();
    app.validate_overlay();
    assert!(app.context_menu().is_some());
    app.engine
        .execute(Command::Edit(ProjectEdit::RemoveTracks { tracks: vec![a] }))
        .unwrap();
    app.validate_overlay();
    assert_eq!(app.overlay, None);
}

#[test]
fn the_picker_opens_at_the_click_toggles_without_closing_and_closes_away() {
    let mut app = app_with(6);
    let track = first_track(&app);
    open_route_list(&mut app, track);
    assert_eq!(
        app.overlay,
        Some(Overlay::Dropdown(OpenDropdown {
            at: crate::control::menu::Anchor::point(iced::Point::new(120.0, 90.0)),
            owner: DropdownOwner::Route(track),
            highlight: Some(0),
            anchor_pending: false,
        }))
    );
    let (first, second) = (app.project().outputs[0].id, app.project().outputs[1].id);
    for output in [first, second] {
        send(
            &mut app,
            Message::Outputs(OutputMessage::ChooseRoute(
                track,
                RouteChoice::Destination(output),
            )),
        );
    }
    assert!(
        app.route_dropdown().is_some(),
        "the list stays open while choosing"
    );
    let lit = app.project().resolved_route(track).to_vec();
    assert!(lit.contains(&first) && lit.contains(&second));
    send(&mut app, Message::CloseOverlay);
    assert_eq!(app.overlay, None);
    open_route_list(&mut app, track);
    send(&mut app, Message::CancelField);
    assert_eq!(app.overlay, None);
}

#[test]
fn a_toggle_outside_an_open_list_dismisses_the_other_popover() {
    let mut app = app_with(2);
    let track = first_track(&app);
    send(&mut app, Message::ToggleSectionsMenu);
    let output = app.project().outputs[0].id;
    send(
        &mut app,
        Message::Outputs(OutputMessage::ChooseRoute(
            track,
            RouteChoice::Destination(output),
        )),
    );
    assert_eq!(app.overlay, None);
}

#[test]
fn replacing_a_project_clears_the_slot() {
    let mut app = app_with(1);
    send(&mut app, Message::ToggleSectionsMenu);
    app.reset_for_project();
    assert_eq!(app.overlay, None);
}
