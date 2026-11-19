use super::*;
use crate::messages::Message as Root;
use qtable_engine::model::{ClipId, LIVE_CLIP_PALETTE, OscAddress, OscArgument};

fn setup() -> (App, ClipKey) {
    let mut p = crate::files::new_project();
    let track = p.tracks[0].id;
    let mut c = Clip::new(ClipId(1), "Hits", BeatTime(3840), LIVE_CLIP_PALETTE[0]).unwrap();
    let path = OscAddress::Absolute("/hit".into());
    c.events.push(
        p.hit(track, BeatTime(960), path, vec![OscArgument::Int(3)])
            .unwrap(),
    );
    p.tracks[0].slots[0].clip = Some(c);
    let mut app = App::new(
        qtable_engine::engine::Engine::new(p).unwrap(),
        "120".into(),
        None,
    );
    app.selection.clip = Some(crate::grid::Selection { track, slot: 0 });
    app.selection.inspector_tab = crate::inspector_panel::Tab::Clip;
    app.chrome.detail_open = true;
    app.focus = crate::focus::Focus::Detail;
    reconcile(&mut app);
    let key = ClipKey {
        track,
        slot: 0,
        id: ClipId(1),
        epoch: app.clip_view.editor.epoch,
    };
    (app, key)
}
fn send(app: &mut App, draw: Draw) {
    let _ = crate::update::update(
        app,
        Root::Inspector(crate::inspector::Message::Timeline(Action::Draw(draw))),
    );
}
fn lane(app: &App, key: ClipKey) -> EventRef {
    clip_owned(app, key).unwrap().events[0].definition.clone()
}
fn positions(app: &App, key: ClipKey) -> Vec<u64> {
    clip_owned(app, key)
        .unwrap()
        .events
        .iter()
        .map(|e| e.position.ticks())
        .collect()
}
fn hits(app: &App, key: ClipKey, span: Range<u64>, at: &[u64]) -> Draw {
    Draw::Hits {
        key,
        revision: app.project_revision(),
        lane: lane(app, key),
        span: BeatTime(span.start)..BeatTime(span.end),
        positions: at.iter().map(|&t| BeatTime(t)).collect(),
    }
}

#[test]
fn painted_hits_commit_as_one_edit_and_keep_the_existing_hit() {
    let (mut app, key) = setup();
    let before = app.project_revision();
    let draw = hits(&app, key, 0..960, &[0, 240, 480, 720]);
    send(&mut app, draw);
    assert_eq!(app.project_revision(), before + 1, "one edit per gesture");
    assert_eq!(positions(&app, key), vec![0, 240, 480, 720, 960]);
}

#[test]
fn new_hits_copy_the_nearest_earlier_hits_arguments() {
    let (mut app, key) = setup();
    let draw = hits(&app, key, 1200..1680, &[1200, 1440]);
    send(&mut app, draw);
    let events = clip_owned(&app, key).unwrap().events;
    assert_eq!(events.len(), 3);
    assert!(
        events
            .iter()
            .all(|e| e.arguments == vec![OscArgument::Int(3)])
    );
}

#[test]
fn a_click_on_a_hit_deletes_it_in_one_edit() {
    let (mut app, key) = setup();
    let before = app.project_revision();
    let target = HitTarget {
        key,
        index: 0,
        revision: before,
    };
    send(&mut app, Draw::Erase(target));
    assert!(positions(&app, key).is_empty());
    assert_eq!(app.project_revision(), before + 1);
}

#[test]
fn a_stroke_from_an_older_revision_changes_nothing() {
    let (mut app, key) = setup();
    let stale = HitTarget {
        key,
        index: 0,
        revision: app.project_revision() + 1,
    };
    send(&mut app, Draw::Erase(stale));
    let mut draw = hits(&app, key, 0..240, &[0]);
    if let Draw::Hits { revision, .. } = &mut draw {
        *revision += 1;
    }
    send(&mut app, draw);
    assert_eq!(positions(&app, key), vec![960]);
}

#[test]
fn painted_steps_create_the_envelope_lane_and_replace_inside_the_range() {
    let (mut app, key) = setup();
    let target = ArgumentRef::volume();
    let lane = PointTarget {
        key,
        revision: app.project_revision(),
        target: target.clone(),
    };
    let point = |t, v| Breakpoint {
        time: BeatTime(t),
        value: v,
        curve: 0.0,
    };
    let before = app.project_revision();
    send(
        &mut app,
        Draw::Points {
            lane,
            replacement: crate::inspector::draw::Replacement {
                range: BeatTime(0)..BeatTime(481),
                points: vec![
                    point(0, 0.5),
                    point(240, 0.5),
                    point(240, 0.25),
                    point(480, 0.25),
                ],
            },
        },
    );
    assert_eq!(app.project_revision(), before + 1);
    let clip = clip_owned(&app, key).unwrap();
    let points = &clip
        .lanes
        .iter()
        .find_map(|l| l.envelope().filter(|e| e.target == target))
        .unwrap()
        .points;
    assert_eq!(points.len(), 4);
    assert_eq!(
        points[2].time, points[1].time,
        "a jump pair at the cell edge"
    );
}

fn b(app: &mut App, modifiers: iced::keyboard::Modifiers, ignored: bool) {
    let _ = crate::update::update(
        app,
        Root::KeyPressed {
            key: iced::keyboard::Key::Character("b".into()),
            modifiers,
            repeat: false,
            ignored,
        },
    );
}

#[test]
fn b_toggles_draw_mode_but_not_in_a_field_or_with_a_modifier() {
    let (mut app, _) = setup();
    assert!(!app.clip_view.editor.draw.on);
    b(&mut app, iced::keyboard::Modifiers::empty(), true);
    assert!(app.clip_view.editor.draw.on);
    let _ = crate::update::update(&mut app, Root::DrawKeyReleased);
    assert!(app.clip_view.editor.draw.on, "a tap leaves it on");
    b(&mut app, iced::keyboard::Modifiers::empty(), false);
    b(&mut app, iced::keyboard::Modifiers::COMMAND, true);
    assert!(
        app.clip_view.editor.draw.on,
        "a captured or modified B is not ours"
    );
    let _ = crate::update::update(&mut app, Root::ToggleDrawMode);
    assert!(!app.clip_view.editor.draw.on);
}
