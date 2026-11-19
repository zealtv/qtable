//! Clip-panel UI state and complementary percentage edits.
use super::Message;
use crate::app::App;
use crate::drafts::{ClipField, DraftKey};
use crate::edit_actions::execute_edit;
use crate::field_focus::Report;
use crate::inspector_update::execute_inspector_edit;
use qtable_engine::edit::ProjectEdit;
use qtable_engine::engine::Command;
use qtable_engine::model::{Project, TrackId};

pub(super) fn percent_b(a: u32, b: u32) -> Option<u8> {
    let total = u64::from(a) + u64::from(b);
    (total != 0).then(|| ((u64::from(b) * 100 + total / 2) / total) as u8)
}

pub(crate) fn handle(app: &mut App, message: Message) -> Option<Message> {
    match message {
        Message::Marker(track, slot, field, text) => {
            super::clip_markers::text(app, track, slot, field, text)
        }
        Message::SetMarker(track, slot, field) => super::clip_markers::set(app, track, slot, field),
        Message::Info(hover) => app.info.update(hover),
        Message::ToggleSection(tab, section) => {
            app.toggle_detail_section(tab, section);
        }
        Message::TogglePalette => app.clip_view.palette_open = !app.clip_view.palette_open,
        Message::ClipName(track, slot, name) => execute_inspector_edit(
            app,
            super::field_key(track, slot, ClipField::Name),
            ProjectEdit::RenameClip { track, slot, name },
        ),
        Message::ChanceSplit(track, slot, b) => set_percent(app, track, slot, b.min(100)),
        Message::FollowPercent(track, slot, a, input) => {
            let key = super::field_key(
                track,
                slot,
                if a {
                    ClipField::PercentA
                } else {
                    ClipField::PercentB
                },
            );
            match input.trim().trim_end_matches('%').trim().parse::<u8>() {
                Ok(percent) if percent <= 100 => {
                    set_percent(app, track, slot, if a { 100 - percent } else { percent })
                }
                _ => {
                    app.drafts
                        .errors
                        .insert(key, "Enter a whole percentage from 0 to 100".into());
                }
            }
        }
        message => return Some(message),
    }
    None
}

fn set_percent(app: &mut App, track: TrackId, slot: usize, b: u8) {
    let Some(mut follow_actions) =
        clip(app.project(), track, slot).map(|c| c.follow_actions.clone())
    else {
        return;
    };
    follow_actions.actions.chance_a = u32::from(100 - b);
    follow_actions.actions.chance_b = u32::from(b);
    let revision = app.project_revision();
    execute_edit(
        app,
        Command::Edit(ProjectEdit::SetClipFollowActions {
            track,
            slot,
            follow_actions,
        }),
    );
    if app.project_revision() != revision {
        for field in [ClipField::PercentA, ClipField::PercentB] {
            app.drafts.forget(super::field_key(track, slot, field));
        }
    }
}

pub(crate) fn clip(
    project: &Project,
    track: TrackId,
    slot: usize,
) -> Option<&qtable_engine::model::Clip> {
    project.clip(track, slot)
}

/// Commits a draft of one of the clip's fields through its Enter message. The name reports
/// its own rejection, as the grid's rename does.
pub(crate) fn commit(
    app: &mut App,
    track: TrackId,
    slot: usize,
    field: ClipField,
    value: String,
) -> Report {
    let key = DraftKey::clip(track, slot, field);
    let Some(clip) = app.project().clip(track, slot) else {
        return Report::Settled;
    };
    let message = match field {
        ClipField::Name => {
            crate::edit_actions::submit_name(
                app,
                key,
                ProjectEdit::RenameClip {
                    track,
                    slot,
                    name: value,
                },
            );
            if app.drafts.values.contains_key(&key)
                && let Some(error) = app.status.message().map(str::to_owned)
            {
                app.drafts.errors.insert(key, error);
            }
            return Report::Settled;
        }
        ClipField::LaneMarker(field) => {
            super::lane_view::region::commit(app, track, slot, field, value);
            return Report::Check;
        }
        ClipField::Length => Message::Length(track, slot, value),
        ClipField::Start | ClipField::End | ClipField::Position => {
            Message::Marker(track, slot, field, value)
        }
        ClipField::FollowTime => Message::FollowTime(track, slot, value),
        ClipField::PercentA => Message::FollowPercent(track, slot, true, value),
        ClipField::PercentB => Message::FollowPercent(track, slot, false, value),
        ClipField::ChanceA => Message::FollowChance(track, slot, true, value),
        ClipField::ChanceB => Message::FollowChance(track, slot, false, value),
        ClipField::Multiplier => Message::Multiplier(track, slot, value),
        ClipField::PointPosition | ClipField::PointValue | ClipField::PointCurve => {
            Message::PointText(track, slot, field, value)
        }
        ClipField::EventArgument { event, argument } => {
            let Some(arg) = clip
                .events
                .get(event)
                .and_then(|e| e.arguments.get(argument))
            else {
                return Report::Settled;
            };
            Message::Argument(
                track,
                slot,
                event,
                argument,
                super::ArgKind::of(arg).name().into(),
                value,
            )
        }
    };
    crate::inspector_update::handle_inspector(app, message);
    Report::Check
}

/// The clip field an inspector message submits.
pub(crate) fn submitted(message: &Message) -> Option<DraftKey> {
    if let Message::Timeline(super::event_editor::Action::LaneRegionText(target, field, _)) =
        message
    {
        return Some(DraftKey::clip(
            target.key.track,
            target.key.slot,
            ClipField::LaneMarker(*field),
        ));
    }
    let (track, slot, field) = match message {
        Message::ClipName(t, s, _) => (t, s, ClipField::Name),
        Message::FollowPercent(t, s, a, _) => (
            t,
            s,
            if *a {
                ClipField::PercentA
            } else {
                ClipField::PercentB
            },
        ),
        Message::Marker(t, s, field, _) => (t, s, *field),
        Message::Length(t, s, _) => (t, s, ClipField::Length),
        Message::FollowTime(t, s, _) => (t, s, ClipField::FollowTime),
        Message::FollowChance(t, s, a, _) => (
            t,
            s,
            if *a {
                ClipField::ChanceA
            } else {
                ClipField::ChanceB
            },
        ),
        Message::Multiplier(t, s, _) => (t, s, ClipField::Multiplier),
        Message::PointText(t, s, field, _) => (t, s, *field),
        Message::Argument(t, s, e, a, _, _) => (
            t,
            s,
            ClipField::EventArgument {
                event: *e,
                argument: *a,
            },
        ),
        _ => return None,
    };
    Some(DraftKey::clip(*track, *slot, field))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> (App, TrackId) {
        let engine = qtable_engine::engine::Engine::new(crate::files::new_project()).unwrap();
        let mut app = App::new(engine, "120".into(), None);
        let track = app.project().tracks[0].id;
        execute_edit(
            &mut app,
            Command::Edit(ProjectEdit::CreateClip {
                track,
                slot: 0,
                length: qtable_engine::model::BeatTime(3840),
            }),
        );
        (app, track)
    }
    fn send(app: &mut App, message: Message) {
        let _ = crate::update::update(app, crate::messages::Message::Inspector(message));
    }

    #[test]
    fn slider_and_keyboard_edits_commit_complementary_weights_and_keep_rejected_drafts() {
        let (mut app, track) = app();
        send(&mut app, Message::ChanceSplit(track, 0, 25));
        let actions = &clip(app.project(), track, 0)
            .unwrap()
            .follow_actions
            .actions;
        assert_eq!((actions.chance_a, actions.chance_b), (75, 25));
        let key = super::super::field_key(track, 0, ClipField::PercentA);
        send(&mut app, Message::Draft(key, "101".into()));
        send(
            &mut app,
            Message::FollowPercent(track, 0, true, "101".into()),
        );
        assert_eq!(app.drafts.values[&key], "101");
        assert!(app.drafts.errors.contains_key(&key));
        send(&mut app, Message::Draft(key, "60 %".into()));
        let _ = crate::update::update(&mut app, crate::messages::Message::CommitField);
        let actions = &clip(app.project(), track, 0)
            .unwrap()
            .follow_actions
            .actions;
        assert_eq!((actions.chance_a, actions.chance_b), (60, 40));
        assert!(!app.drafts.values.contains_key(&key));
    }

    #[test]
    fn disclosure_palette_and_hover_are_ui_state_and_hover_does_not_commit_a_draft() {
        let (mut app, track) = app();
        let revision = app.project_revision();
        send(
            &mut app,
            Message::ToggleSection(
                crate::inspector_panel::Tab::Clip,
                crate::detail_layout::CLIP,
            ),
        );
        send(
            &mut app,
            Message::ToggleSection(
                crate::inspector_panel::Tab::Clip,
                crate::detail_layout::LAUNCH,
            ),
        );
        send(&mut app, Message::TogglePalette);
        assert!(
            !app.detail_layout.expanded(
                crate::inspector_panel::Tab::Clip,
                crate::detail_layout::CLIP
            ) && !app.detail_layout.expanded(
                crate::inspector_panel::Tab::Clip,
                crate::detail_layout::LAUNCH
            ) && app.clip_view.palette_open
        );
        assert_eq!(app.project_revision(), revision);
        let key = super::super::field_key(track, 0, ClipField::Name);
        send(&mut app, Message::Draft(key, "Pending".into()));
        send(&mut app, Message::Info(crate::info::Hover::Clear));
        assert_eq!(app.pending_field, Some(key));
        assert_ne!(clip(app.project(), track, 0).unwrap().name, "Pending");
        assert_eq!(app.project_revision(), revision);
    }

    #[test]
    fn musical_edits_use_the_meter_and_explicit_follow_time_preserves_the_multiplier() {
        let (mut app, track) = app();
        send(&mut app, Message::Length(track, 0, "2. 0. 0".into()));
        let before = clip(app.project(), track, 0).unwrap();
        assert_eq!(before.length().ticks(), 7680);
        assert!(before.follow_actions.linked);
        let multiplier = before.follow_actions.multiplier;
        send(&mut app, Message::FollowTime(track, 0, "1. 2. 0".into()));
        let after = clip(app.project(), track, 0).unwrap();
        assert_eq!(after.follow_actions.time.ticks(), 5760);
        assert!(
            after.follow_actions.linked,
            "typing a time does not switch mode"
        );
        assert_eq!(after.follow_actions.multiplier, multiplier);
    }

    #[test]
    fn linked_toggle_and_multiplier_keep_the_other_setting() {
        let (mut app, track) = app();
        send(&mut app, Message::Multiplier(track, 0, "1/2x".into()));
        send(&mut app, Message::FollowTime(track, 0, "1. 0. 0".into()));
        send(&mut app, Message::FollowLinked(track, 0, false));
        let unlinked = clip(app.project(), track, 0)
            .unwrap()
            .follow_actions
            .clone();
        assert!(!unlinked.linked);
        assert_eq!(unlinked.multiplier.numerator(), 1);
        assert_eq!(unlinked.multiplier.denominator(), 2);
        assert_eq!(unlinked.time.ticks(), 3840);
        send(&mut app, Message::FollowLinked(track, 0, true));
        assert!(clip(app.project(), track, 0).unwrap().follow_actions.linked);
        send(&mut app, Message::Multiplier(track, 0, "0".into()));
        let key = super::super::field_key(track, 0, ClipField::Multiplier);
        assert!(app.drafts.errors.contains_key(&key));
    }

    #[test]
    fn weights_map_to_complementary_percentages_without_overflow_or_changing_zero_zero() {
        assert_eq!(percent_b(0, 0), None);
        assert_eq!(percent_b(1, 0), Some(0));
        assert_eq!(percent_b(1, 1), Some(50));
        assert_eq!(percent_b(0, 1), Some(100));
        assert_eq!(percent_b(u32::MAX, u32::MAX), Some(50));
        assert_eq!(percent_b(2, 1), Some(33));
    }
}

#[cfg(test)]
mod marker_tests {
    use super::*;
    use qtable_engine::model::{BeatTime, GlobalQuantisation};
    fn setup() -> (App, TrackId) {
        let engine = qtable_engine::engine::Engine::new(crate::files::new_project()).unwrap();
        let mut app = App::new(engine, "120".into(), None);
        let track = app.project().tracks[0].id;
        execute_edit(
            &mut app,
            Command::Edit(ProjectEdit::CreateClip {
                track,
                slot: 0,
                length: BeatTime(11520),
            }),
        );
        app.selection.clip = Some(crate::grid::Selection { track, slot: 0 });
        super::super::event_editor::reconcile(&mut app);
        (app, track)
    }
    fn send(app: &mut App, msg: Message) {
        crate::inspector_update::handle_inspector(app, msg);
    }
    #[test]
    fn box_fields_are_independent_and_rejected_drafts_keep_the_model() {
        let (mut app, t) = setup();
        send(
            &mut app,
            Message::Marker(t, 0, ClipField::End, "5. 1. 1".into()),
        );
        send(
            &mut app,
            Message::Marker(t, 0, ClipField::Position, "2. 1. 1".into()),
        );
        send(&mut app, Message::Length(t, 0, "1. 0. 0".into()));
        let c = app.project().clip(t, 0).unwrap();
        assert_eq!(
            (c.start.0, c.end.0, c.loop_start.0, c.loop_end.0),
            (0, 15360, 3840, 7680)
        );
        send(&mut app, Message::ToggleLoop(t, 0));
        send(&mut app, Message::Length(t, 0, "0. 2. 0".into()));
        let c = app.project().clip(t, 0).unwrap();
        assert!(!c.looping);
        assert_eq!((c.end.0, c.loop_end.0), (15360, 5760));
        let key = super::super::field_key(t, 0, ClipField::End);
        app.drafts.values.insert(key, "1. 1. 1".into());
        send(
            &mut app,
            Message::Marker(t, 0, ClipField::End, "1. 1. 1".into()),
        );
        assert_eq!(app.project().clip(t, 0).unwrap().end.0, 15360);
        assert!(app.drafts.errors.contains_key(&key));
        assert!(app.status.message().is_some());
    }
    #[test]
    fn sets_use_only_the_playing_clip_and_global_quantisation_and_length_sets_loop_end() {
        let (mut app, t) = setup();
        let id = app.project().clip(t, 0).unwrap().id;
        app.clip_view.editor.playhead = Some(BeatTime(5000));
        let revision = app.project_revision();
        send(&mut app, Message::SetMarker(t, 0, ClipField::Start));
        assert_eq!(app.project_revision(), revision, "held playhead cannot Set");
        app.snapshot.clip_positions = vec![qtable_engine::scheduler::ClipPosition {
            track: t,
            clip: id,
            position: BeatTime(5000),
        }];
        super::super::event_editor::reconcile(&mut app);
        assert!(app.clip_view.editor.playing);
        send(&mut app, Message::SetMarker(t, 0, ClipField::Position));
        let c = app.project().clip(t, 0).unwrap();
        assert_eq!((c.loop_start.0, c.loop_end.0), (3840, 15360));
        app.snapshot.clip_positions = vec![qtable_engine::scheduler::ClipPosition {
            track: t,
            clip: id,
            position: BeatTime(10000),
        }];
        send(&mut app, Message::SetMarker(t, 0, ClipField::Length));
        assert_eq!(app.project().clip(t, 0).unwrap().loop_end.0, 11520);
        execute_edit(
            &mut app,
            Command::SetGlobalQuantisation(GlobalQuantisation::None),
        );
        app.snapshot.clip_positions = vec![qtable_engine::scheduler::ClipPosition {
            track: t,
            clip: id,
            position: BeatTime(5001),
        }];
        send(&mut app, Message::SetMarker(t, 0, ClipField::Start));
        assert_eq!(app.project().clip(t, 0).unwrap().start.0, 5001);
        app.snapshot.clip_positions.clear();
        super::super::event_editor::reconcile(&mut app);
        assert!(!app.clip_view.editor.playing);
    }
}
