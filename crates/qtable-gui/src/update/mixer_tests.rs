//! Mixer band clicks through the public update router.
use super::update;
use crate::app::App;
use crate::drafts::MixerField;
use crate::messages::{EditMessage, FileMessage, Message};
use crate::mixer_band::MixerMessage;
use iced::keyboard::{Key, Modifiers};
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{MixerState, Project, SoloLook, Track, TrackId};

/// A new set with three tracks; tracks 2 and 3 form a group.
fn app() -> (App, [TrackId; 3], TrackId) {
    let mut app = App::new(Engine::new(Project::default()).unwrap(), "120".into(), None);
    let _ = update(&mut app, Message::Files(FileMessage::New));
    for _ in 0..2 {
        let _ = update(&mut app, Message::Edit(EditMessage::AddTrack));
    }
    let ids: Vec<_> = app.project().tracks.iter().map(|t| t.id).collect();
    let tracks = [ids[0], ids[1], ids[2]];
    app.engine
        .execute(Command::Edit(ProjectEdit::CreateGroup {
            tracks: vec![tracks[1], tracks[2]],
        }))
        .unwrap();
    let group = app.project().group_tracks().next().unwrap().id;
    (app, tracks, group)
}

fn click(app: &mut App, message: MixerMessage) {
    let _ = update(app, Message::Mixer(message));
}

fn mixer(app: &App, strip: TrackId) -> MixerState {
    app.project().track_facts(strip).unwrap().mixer
}

#[test]
fn the_activator_toggles_its_track_only() {
    let (mut app, [a, b, _], _) = app();
    click(&mut app, MixerMessage::Activator(a));
    assert!(!mixer(&app, a).active);
    assert!(mixer(&app, b).active);
    assert!(!app.project().track_sends(a));
    click(&mut app, MixerMessage::Activator(a));
    assert!(mixer(&app, a).active);
}

#[test]
fn a_plain_solo_click_is_exclusive_and_a_second_click_unsolos() {
    let (mut app, [a, b, _], _) = app();
    click(&mut app, MixerMessage::Solo(a));
    click(&mut app, MixerMessage::Solo(b));
    assert!(!mixer(&app, a).solo);
    assert!(mixer(&app, b).solo);
    click(&mut app, MixerMessage::Solo(b));
    assert!(!app.project().any_solo());
}

#[test]
fn a_member_solo_shows_the_group_half_lit_and_group_switches_apply() {
    let (mut app, [a, b, c], group) = app();
    click(&mut app, MixerMessage::Solo(b));
    assert_eq!(
        app.project().track_facts(group).unwrap().solo_look,
        SoloLook::Partial
    );
    click(&mut app, MixerMessage::Activator(group));
    assert!(!app.project().track_sends(b));
    assert!(!app.project().track_sends(c));
    assert!(!app.project().track_sends(a), "track 1 is soloed out");
}

#[test]
fn command_click_adds_to_the_solo_set_and_removes_from_it() {
    let (mut app, [a, b, c], _) = app();
    click(&mut app, MixerMessage::Solo(a));
    let _ = update(&mut app, Message::ModifiersChanged(Modifiers::COMMAND));
    click(&mut app, MixerMessage::Solo(b));
    assert!(mixer(&app, a).solo && mixer(&app, b).solo);
    click(&mut app, MixerMessage::Solo(a));
    assert!(!mixer(&app, a).solo && mixer(&app, b).solo);
    let _ = update(&mut app, Message::ModifiersChanged(Modifiers::NONE));
    click(&mut app, MixerMessage::Solo(c));
    assert!(!mixer(&app, b).solo && mixer(&app, c).solo);
}

#[test]
fn a_click_inside_a_multi_selection_switches_every_selected_track() {
    let (mut app, [a, b, c], _) = app();
    app.selection.tracks = vec![a, c];
    click(&mut app, MixerMessage::Solo(c));
    assert!(mixer(&app, a).solo && mixer(&app, c).solo);
    click(&mut app, MixerMessage::Activator(a));
    assert!(!mixer(&app, a).active && !mixer(&app, c).active);
    // A click outside the selection acts alone.
    click(&mut app, MixerMessage::Activator(b));
    assert!(!mixer(&app, b).active && !mixer(&app, a).active);
}

#[test]
fn s_solos_the_selected_tracks_and_toggles_back() {
    let (mut app, [a, b, c], _) = app();
    click(&mut app, MixerMessage::Solo(a));
    app.selection.tracks = vec![b, c];
    click(&mut app, MixerMessage::SoloSelected);
    assert!(!mixer(&app, a).solo);
    assert!(mixer(&app, b).solo && mixer(&app, c).solo);
    click(&mut app, MixerMessage::SoloSelected);
    assert!(!app.project().any_solo());
}

#[test]
fn function_keys_toggle_activators_by_position() {
    let (mut app, [_, b, _], group) = app();
    // Live's order counts the group track: 1 track, 2 group, 3 and 4 its members.
    click(&mut app, MixerMessage::ToggleActivatorAt(1));
    assert!(!mixer(&app, group).active);
    click(&mut app, MixerMessage::ToggleActivatorAt(1));
    click(&mut app, MixerMessage::ToggleActivatorAt(2));
    assert!(!mixer(&app, b).active);
    click(&mut app, MixerMessage::ToggleActivatorAt(7));
    assert_eq!(
        app.project()
            .tracks
            .iter()
            .filter(|t| !t.mixer.active)
            .count(),
        1
    );
}

#[test]
fn mixer_keys_map_only_unmodified_unclaimed_presses() {
    use crate::mixer_band::key;
    use iced::keyboard::key::Named;
    let s = Key::Character("s".into());
    assert_eq!(
        key(&s, Modifiers::NONE, false, true),
        Some(MixerMessage::SoloSelected)
    );
    assert_eq!(
        key(&s, Modifiers::NONE, false, false),
        None,
        "a text field took it"
    );
    assert_eq!(key(&s, Modifiers::COMMAND, false, true), None);
    assert_eq!(key(&s, Modifiers::NONE, true, true), None);
    let f3 = Key::Named(Named::F3);
    assert_eq!(
        key(&f3, Modifiers::NONE, false, true),
        Some(MixerMessage::ToggleActivatorAt(2))
    );
    assert_eq!(
        key(&Key::Named(Named::F9), Modifiers::NONE, false, true),
        None
    );
}

#[test]
fn output_solo_clicks_share_track_exclusivity_and_command_addition() {
    use qtable_engine::model::{Route, TrackId};
    let (mut app, [a, b, _], _) = app();
    let mut p = app.project().clone();
    p.outputs = vec![
        Track::new_output(TrackId(201), "A", "127.0.0.1", 9000),
        Track::new_output(TrackId(202), "B", "127.0.0.1", 9001),
    ];
    p.tracks.iter_mut().find(|t| t.id == a).unwrap().route = Route::single(TrackId(201));
    p.tracks.iter_mut().find(|t| t.id == b).unwrap().route = Route::single(TrackId(202));
    app.engine.execute(Command::LoadProject(p)).unwrap();
    click(&mut app, MixerMessage::Solo(b));
    click(&mut app, MixerMessage::OutputSolo(TrackId(201)));
    assert!(!mixer(&app, b).solo);
    assert!(app.project().track_sends(a) && !app.project().track_sends(b));
    let _ = update(&mut app, Message::ModifiersChanged(Modifiers::COMMAND));
    click(&mut app, MixerMessage::OutputSolo(TrackId(202)));
    assert!(app.project().outputs.iter().all(|o| o.mixer.solo));
    click(&mut app, MixerMessage::OutputSolo(TrackId(201)));
    assert!(!app.project().outputs[0].mixer.solo && app.project().outputs[1].mixer.solo);
    let _ = update(&mut app, Message::ModifiersChanged(Modifiers::NONE));
    click(&mut app, MixerMessage::OutputSolo(TrackId(201)));
    assert!(app.project().outputs[0].mixer.solo && !app.project().outputs[1].mixer.solo);
}

#[test]
fn strip_volume_edits_only_its_owner_and_refreshes_detail_drafts_after_acceptance() {
    use crate::drafts::DraftKey;
    let (mut app, [a, b, _], group) = app();
    let output = TrackId(201);
    let mut project = app.project().clone();
    project
        .outputs
        .push(Track::new_output(output, "A", "127.0.0.1", 9000));
    app.engine.execute(Command::LoadProject(project)).unwrap();
    app.selection.tracks = vec![a, b];
    for track in [a, group, output, app.project().main.id] {
        let key = DraftKey::Mixer(track, MixerField::Volume);
        app.drafts.values.insert(key, "stale".into());
        app.drafts.errors.insert(key, "old error".into());
        click(&mut app, MixerMessage::SetVolume(track, 0.25));
        assert_eq!(
            app.project()
                .track(track)
                .unwrap()
                .mixer_device
                .volume
                .value,
            0.25
        );
        assert!(!app.drafts.values.contains_key(&key));
        assert!(!app.drafts.errors.contains_key(&key));
        assert_eq!(
            app.project().track(b).unwrap().mixer_device.volume.value,
            1.0
        );
        click(&mut app, MixerMessage::SetVolume(track, 1.0));
        assert_eq!(
            app.project()
                .track(track)
                .unwrap()
                .mixer_device
                .volume
                .value,
            1.0
        );
        app.drafts.values.insert(key, "invalid draft".into());
        click(&mut app, MixerMessage::SetVolume(track, f64::NAN));
        assert_eq!(
            app.project()
                .track(track)
                .unwrap()
                .mixer_device
                .volume
                .value,
            1.0
        );
        assert_eq!(app.drafts.values[&key], "invalid draft");
    }
}
