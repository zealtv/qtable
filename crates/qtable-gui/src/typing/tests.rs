//! Every kind of draft is on the one path: `unsaved` sees it, `commit` commits it, and each of
//! the four callers (a click elsewhere, `CommitField`, the save and settings shortcuts, undo)
//! reaches it. `Draft::ALL` is declared with `Draft` itself, and `open`, `landed` and `rejects`
//! below match on every kind with no catch-all arm, so a new kind fails to compile here until
//! the tests open it, and then fails unless the path commits it.
use super::*;
use crate::device_chain::target_face::{Action as Face, FaceField};
use crate::drafts::DraftKey;
use crate::messages::{EditMessage, Message};
use crate::status::Severity;
use qtable_engine::edit::{ProjectEdit, TargetEdit};
use qtable_engine::engine::{Command, Engine};
use qtable_engine::model::{DeviceId, FieldType, NewDevice, TrackId};
use std::time::Instant;

const NEW_NAME: &str = "Renamed";

struct Fixture {
    app: App,
    track: TrackId,
    device: DeviceId,
    face: FaceField,
}

/// A set with one track holding a clip in slot 0 and a Target device with one parameter, whose
/// face shows its number field.
fn fixture() -> Fixture {
    let engine = Engine::new(crate::files::new_project()).expect("engine starts");
    let mut app = App::new(engine, String::new(), None);
    let track = app.project().tracks[0].id;
    let update = crate::update::update;
    let _ = update(
        &mut app,
        Message::Grid(crate::grid::Action::Select(track, 0)),
    );
    let _ = update(&mut app, Message::Edit(EditMessage::CreateSelectedClip));
    let _ = update(
        &mut app,
        Message::Grid(crate::grid::Action::SelectTrack(track, false)),
    );
    crate::edit_actions::run_edit(
        &mut app,
        Command::Edit(ProjectEdit::AddDevice {
            track,
            kind: NewDevice::Target,
            before: None,
        }),
    );
    crate::device_chain::reconcile(&mut app);
    let device = app.project().track(track).unwrap().devices[0].id;
    let device_action = |app: &mut App, action: Face| {
        let _ = update(
            app,
            Message::Device(crate::device_chain::Action::Target(action)),
        );
    };
    device_action(&mut app, Face::Add(device, true));
    let entry = app
        .project()
        .device(device)
        .unwrap()
        .1
        .target()
        .unwrap()
        .parameters[0]
        .key
        .clone();
    device_action(
        &mut app,
        Face::Definition(
            device,
            TargetEdit::AddField {
                entry,
                value_type: FieldType::Float,
            },
        ),
    );
    device_action(&mut app, Face::Fold(device));
    let parameter = &app
        .project()
        .device(device)
        .unwrap()
        .1
        .target()
        .unwrap()
        .parameters[0];
    let face = FaceField {
        device,
        entry: parameter.key.clone(),
        field: parameter.fields.last().unwrap().key.clone(),
        parameter: true,
    };
    Fixture {
        app,
        track,
        device,
        face,
    }
}

fn parameter_value(fixture: &Fixture) -> Option<f64> {
    let (_, device) = fixture.app.project().device(fixture.device)?;
    device.target()?.parameters[0].fields.last()?.value
}

/// Starts a draft of this kind that the engine accepts, or one it rejects if `rejected`. A kind
/// the engine cannot reject says so in `rejects`.
fn open(fixture: &mut Fixture, kind: Draft, rejected: bool) {
    let app = &mut fixture.app;
    match kind {
        Draft::DeviceRename => {
            app.device_chain.renaming = Some(fixture.device);
            app.device_chain.draft = NEW_NAME.into();
        }
        Draft::TargetFace => {
            let text = if rejected { "2.0" } else { "0.5" };
            let _ = crate::update::update(
                app,
                Message::Device(crate::device_chain::Action::Target(Face::Exact(
                    fixture.face.clone(),
                    text.into(),
                ))),
            );
        }
        Draft::Field => {
            let text = if rejected { "fast" } else { "93" };
            let _ = crate::update::update(app, Message::TempoChanged(text.into()));
            assert_eq!(app.pending_field, Some(DraftKey::Tempo));
        }
        Draft::ClipRename => {
            app.clip_actions.renaming = Some(crate::grid::Selection {
                track: fixture.track,
                slot: 0,
            });
            app.clip_actions.draft = NEW_NAME.into();
        }
        Draft::HeaderRename => {
            // Main's name is fixed, so the engine refuses to rename it. (Its editor never opens
            // from the UI, so set the editor as it would be.)
            let track = if rejected {
                app.project().main.id
            } else {
                fixture.track
            };
            app.header_actions.renaming = Some(crate::grid::RenameTarget::Header(track));
            app.header_actions.draft = NEW_NAME.into();
        }
    }
    assert!(kind.is_open(app), "{kind:?} did not open");
}

/// Whether the engine can reject a draft of this kind.
fn rejects(kind: Draft) -> bool {
    match kind {
        Draft::DeviceRename | Draft::ClipRename => false,
        Draft::TargetFace | Draft::Field | Draft::HeaderRename => true,
    }
}

/// Whether the accepted draft of this kind is in the set.
fn landed(fixture: &Fixture, kind: Draft) -> bool {
    let app = &fixture.app;
    match kind {
        Draft::DeviceRename => app
            .project()
            .device(fixture.device)
            .is_some_and(|(_, device)| device.name == NEW_NAME),
        Draft::TargetFace => parameter_value(fixture) == Some(0.5),
        Draft::Field => app.project().tempo.bpm() == 93.0,
        Draft::ClipRename => {
            app.project().tracks[0].slots[0]
                .clip
                .as_ref()
                .map(|clip| clip.name.as_str())
                == Some(NEW_NAME)
        }
        Draft::HeaderRename => app
            .project()
            .track(fixture.track)
            .is_some_and(|track| track.name == NEW_NAME),
    }
}

fn with_scheduler_failure(app: &mut App) {
    let status = qtable_engine::status::EngineStatus {
        scheduler_failure: Some("boom".into()),
        ..Default::default()
    };
    app.status.poll(&status, Instant::now());
    assert_eq!(app.status.message(), Some("Scheduler stopped: boom"));
}

#[test]
fn nothing_is_unsaved_until_a_draft_opens_and_each_kind_opens_alone() {
    let mut fixture = fixture();
    assert!(!unsaved(&fixture.app));
    for kind in Draft::ALL {
        let mut fixture = self::fixture();
        open(&mut fixture, *kind, false);
        assert!(unsaved(&fixture.app), "{kind:?} is unsaved typing");
        for other in Draft::ALL.iter().filter(|other| *other != kind) {
            assert!(!other.is_open(&fixture.app), "{kind:?} opened {other:?}");
        }
    }
    assert!(commit_all(&mut fixture.app).all_committed());
}

#[test]
fn commit_all_commits_every_kind_of_draft() {
    for kind in Draft::ALL {
        let mut fixture = fixture();
        open(&mut fixture, *kind, false);
        let outcome = commit_all(&mut fixture.app);
        assert!(outcome.all_committed(), "{kind:?}");
        assert_eq!(outcome.rejected_field(), None, "{kind:?}");
        assert!(!unsaved(&fixture.app), "{kind:?} is still open");
        assert!(landed(&fixture, *kind), "{kind:?} did not reach the set");
    }
}

#[test]
fn commit_all_commits_every_kind_at_once() {
    let mut fixture = fixture();
    // Typing in one place commits what is open elsewhere (a click elsewhere), so open the
    // kinds that go through `update` first and set the others as the editors would leave them.
    open(&mut fixture, Draft::TargetFace, false);
    open(&mut fixture, Draft::DeviceRename, false);
    open(&mut fixture, Draft::ClipRename, false);
    let track = fixture.track;
    let app = &mut fixture.app;
    app.header_actions.renaming = Some(crate::grid::RenameTarget::Header(track));
    app.header_actions.draft = NEW_NAME.into();
    app.pending_field = Some(DraftKey::Tempo);
    app.tempo_input = "93".into();
    app.drafts.values.insert(DraftKey::Tempo, "93".into());
    for kind in Draft::ALL {
        assert!(kind.is_open(&fixture.app), "{kind:?} is not open");
    }
    assert!(commit_all(&mut fixture.app).all_committed());
    assert!(!unsaved(&fixture.app));
    for kind in Draft::ALL {
        assert!(landed(&fixture, *kind), "{kind:?}");
    }
}

#[test]
fn a_rejected_draft_is_reported_even_while_the_scheduler_failure_shows() {
    for kind in Draft::ALL.iter().copied().filter(|kind| rejects(*kind)) {
        for failure in [false, true] {
            let mut fixture = fixture();
            if failure {
                with_scheduler_failure(&mut fixture.app);
            }
            open(&mut fixture, kind, true);
            let outcome = commit_all(&mut fixture.app);
            assert!(
                !outcome.all_committed(),
                "{kind:?} (scheduler failure showing: {failure})"
            );
            assert!(!landed(&fixture, kind), "{kind:?}");
        }
    }
}

#[test]
fn a_rejected_field_is_named_with_its_error_and_is_no_longer_pending() {
    let mut fixture = fixture();
    open(&mut fixture, Draft::Field, true);
    let outcome = commit_all(&mut fixture.app);
    assert_eq!(outcome.rejected_field(), Some(DraftKey::Tempo));
    assert!(fixture.app.drafts.errors.contains_key(&DraftKey::Tempo));
    assert_eq!(fixture.app.pending_field, None);
}

#[test]
fn committing_one_kind_leaves_the_others_open() {
    let mut fixture = fixture();
    open(&mut fixture, Draft::Field, false);
    open(&mut fixture, Draft::DeviceRename, false);
    assert!(commit(&mut fixture.app, &[Draft::DeviceRename]).all_committed());
    assert!(!Draft::DeviceRename.is_open(&fixture.app));
    assert!(Draft::Field.is_open(&fixture.app));
}

#[test]
fn an_earlier_message_survives_a_commit_that_changes_nothing() {
    let mut fixture = fixture();
    // A pending field with nothing typed behind it commits nothing.
    let key = DraftKey::Track(fixture.track, crate::drafts::TrackField::Name);
    fixture.app.pending_field = Some(key);
    fixture.app.status.show_error("an earlier problem");
    let outcome = commit_all(&mut fixture.app);
    assert!(outcome.all_committed());
    assert_eq!(fixture.app.status.message(), Some("an earlier problem"));
    assert_eq!(fixture.app.status.severity(), Severity::Error);
}

// The four callers, each with every kind of draft.

#[test]
fn undo_drops_every_kind_of_draft() {
    for kind in Draft::ALL {
        let mut fixture = fixture();
        open(&mut fixture, *kind, false);
        let _ = crate::undo::update(&mut fixture.app, crate::undo::Step::Undo);
        assert!(!unsaved(&fixture.app), "{kind:?}");
        assert!(
            !landed(&fixture, *kind),
            "undo drops {kind:?}, never commits it"
        );
    }
}

#[test]
fn the_save_shortcut_commits_every_kind_of_draft() {
    for kind in Draft::ALL {
        let mut fixture = fixture();
        open(&mut fixture, *kind, false);
        let task = crate::update::update(
            &mut fixture.app,
            Message::KeyPressed {
                key: iced::keyboard::Key::Character("s".into()),
                modifiers: iced::keyboard::Modifiers::COMMAND,
                repeat: false,
                ignored: true,
            },
        );
        assert!(
            landed(&fixture, *kind),
            "{kind:?} was not in the set when saving"
        );
        assert!(task.units() > 0, "{kind:?}: and the save started");
    }
}

#[test]
fn the_save_shortcut_refuses_while_any_rejectable_kind_is_rejected() {
    for kind in Draft::ALL.iter().copied().filter(|kind| rejects(*kind)) {
        let mut fixture = fixture();
        with_scheduler_failure(&mut fixture.app);
        open(&mut fixture, kind, true);
        let task = crate::update::update(
            &mut fixture.app,
            Message::KeyPressed {
                key: iced::keyboard::Key::Character("s".into()),
                modifiers: iced::keyboard::Modifiers::COMMAND,
                repeat: false,
                ignored: true,
            },
        );
        assert_eq!(task.units(), 0, "{kind:?} was rejected, so nothing saves");
    }
}

#[test]
fn commit_field_commits_the_field_and_the_target_face_value() {
    for kind in [Draft::Field, Draft::TargetFace] {
        let mut fixture = fixture();
        open(&mut fixture, kind, false);
        let _ = crate::update::update(&mut fixture.app, Message::CommitField);
        assert!(!kind.is_open(&fixture.app), "{kind:?}");
        assert!(landed(&fixture, kind), "{kind:?}");
    }
}

#[test]
fn a_click_elsewhere_commits_the_field_the_device_name_and_the_target_face_value() {
    for kind in [Draft::Field, Draft::DeviceRename, Draft::TargetFace] {
        let mut fixture = fixture();
        open(&mut fixture, kind, false);
        let track = fixture.track;
        let _ = crate::update::update(
            &mut fixture.app,
            Message::Grid(crate::grid::Action::SelectTrack(track, false)),
        );
        assert!(!kind.is_open(&fixture.app), "{kind:?}");
        assert!(landed(&fixture, kind), "{kind:?}");
    }
}

#[test]
fn a_click_elsewhere_that_finds_a_rejected_field_shows_its_error() {
    let mut fixture = fixture();
    open(&mut fixture, Draft::Field, true);
    let track = fixture.track;
    let _ = crate::update::update(
        &mut fixture.app,
        Message::Grid(crate::grid::Action::SelectTrack(track, false)),
    );
    assert_eq!(fixture.app.status.severity(), Severity::Error);
    assert_eq!(fixture.app.pending_field, None, "the click drops the draft");
}

// The same path from the keyboard, through a real focused `TextField` (the capture hooks skip
// the widget, which is where Cmd-S once typed an `s`).

mod real_field {
    use super::*;
    use iced::advanced::widget::Tree;
    use iced::advanced::{Shell, layout};
    use iced::keyboard::{self, Key, Location, Modifiers};
    use iced::{Event, Point, Rectangle, Size, mouse};

    fn press(character: &str, modifiers: Modifiers) -> Event {
        let key = Key::Character(character.into());
        Event::Keyboard(keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            key,
            location: Location::Standard,
            modifiers,
            // What the OS reports on a Mac for Cmd-S: the character, whatever the modifier.
            text: Some(character.into()),
            repeat: false,
        })
    }

    /// The tempo field's widget with the app's current text, as the view builds it every frame.
    fn view(app: &App) -> iced::Element<'static, Message> {
        crate::control::text_field::TextField::new("", &app.tempo_input)
            .on_input(Message::TempoChanged)
            .on_submit(Message::TempoSubmitted)
            .width(150)
            .into()
    }

    /// One real widget, kept alive between events like the window's widget tree: each event goes
    /// to the field, the messages it publishes go to the app, and the field is rebuilt from the
    /// app's text.
    struct Window {
        tree: Option<Tree>,
    }

    impl Window {
        /// Sends the event to the focused-or-not field; returns whether the field took the key.
        fn send(&mut self, app: &mut App, event: &Event) -> bool {
            let renderer = crate::grid::test_renderer();
            let mut element = view(app);
            let tree = self.tree.get_or_insert_with(|| Tree::new(&element));
            tree.diff(&element);
            let size = Size::new(200.0, 40.0);
            let node = element.as_widget_mut().layout(
                tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, size),
            );
            let mut messages = Vec::new();
            let mut shell = Shell::new(&mut messages);
            element.as_widget_mut().update(
                tree,
                event,
                layout::Layout::new(&node),
                mouse::Cursor::Available(Point::new(10.0, 10.0)),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &Rectangle::with_size(size),
            );
            let captured = shell.is_event_captured();
            drop(shell);
            for message in messages {
                let _ = crate::update::update(app, message);
            }
            captured
        }

        fn click(&mut self, app: &mut App) {
            self.send(
                app,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            );
        }

        fn type_text(&mut self, app: &mut App, text: &str) {
            for character in text.chars() {
                self.send(app, &press(&character.to_string(), Modifiers::NONE));
            }
        }

        /// Cmd-S as the window delivers it: the field sees it first, and the app hears it as
        /// `ignored` only if the field left it alone.
        fn cmd_s(&mut self, app: &mut App) -> iced::Task<Message> {
            let modifiers = Modifiers::COMMAND;
            self.send(
                app,
                &Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)),
            );
            let captured = self.send(app, &press("s", modifiers));
            crate::update::update(
                app,
                Message::KeyPressed {
                    key: Key::Character("s".into()),
                    modifiers,
                    repeat: false,
                    ignored: !captured,
                },
            )
        }
    }

    fn fresh() -> (App, Window) {
        let mut app = fixture().app;
        app.tempo_input.clear();
        (app, Window { tree: None })
    }

    #[test]
    fn typing_in_the_tempo_field_then_cmd_s_commits_it_and_saves_without_typing_an_s() {
        let (mut app, mut window) = fresh();
        window.click(&mut app);
        window.type_text(&mut app, "93");
        assert_eq!(app.tempo_input, "93", "the real field took the typing");
        assert_eq!(app.pending_field, Some(DraftKey::Tempo));
        let task = window.cmd_s(&mut app);
        assert_eq!(
            app.project().tempo.bpm(),
            93.0,
            "the typed value is in the set"
        );
        assert_eq!(
            app.tempo_input, "93.00",
            "the field shows the set's tempo, with no s typed into it"
        );
        assert!(!unsaved(&app));
        assert!(task.units() > 0, "and the save started");
    }

    #[test]
    fn a_rejected_value_typed_in_the_real_field_stops_cmd_s() {
        let (mut app, mut window) = fresh();
        let before = app.project().tempo.bpm();
        window.click(&mut app);
        window.type_text(&mut app, "fast");
        assert_eq!(app.tempo_input, "fast");
        assert_eq!(window.cmd_s(&mut app).units(), 0, "nothing saves");
        assert_eq!(window.cmd_s(&mut app).units(), 0, "nor on the second press");
        assert_eq!(app.project().tempo.bpm(), before);
        assert_eq!(app.tempo_input, "fast", "the field keeps what was typed");
        assert_eq!(app.status.severity(), Severity::Error);
    }
}
