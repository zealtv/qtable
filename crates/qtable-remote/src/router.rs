//! Explicit consumers see the ingress envelope before any gesture decoding or reduction.
//! A recorder can subscribe here later without inheriting a controller's coalescing.
use crate::{Drain, Envelope, Ingress};
use qtable_engine::{
    engine::{Command, Receipt, Snapshot},
    model::{Project, Source, TrackId},
};

/// Ordered client work with its original discrete receipt. Value batches have no
/// receipt: the scheduler stamps their enqueue time, not an arbitrary constituent CC.
///
/// `continuous` names the controller source of a move that is one stretch of an ongoing
/// action (a CC or pitch bend moving a number). A host gives every command from one such
/// source the same undo gesture token for as long as the set stays loaded, so the moves join
/// one history step. Notes, keys and switch presses leave it
/// empty, and each makes a step of its own.
#[derive(Clone, Debug)]
pub struct Received<T> {
    pub value: T,
    pub receipt: Option<Receipt>,
    pub continuous: Option<Source>,
}

impl<T> Received<T> {
    pub fn unstamped(value: T) -> Self {
        Self {
            value,
            receipt: None,
            continuous: None,
        }
    }
    pub fn from_event(value: T, event: &Envelope) -> Self {
        Self {
            value,
            receipt: Some(Receipt {
                received_at: event.received_at,
                sequence: Some(event.sequence),
            }),
            continuous: None,
        }
    }
    /// The same work, marked as a continuous move of `source`.
    pub fn continuous(mut self, source: Option<Source>) -> Self {
        self.continuous = source;
        self
    }
}

/// Ordered work for the host. View actions name no GUI toolkit or saved playback state.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum Action {
    Command(Command),
    View(ViewAction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewAction {
    SelectTrack(TrackId),
    LaunchScene,
    MoveScene(i32),
    SelectScene(usize),
    /// Advance one scene, wrapping from the last to the first.
    NextScene,
}

impl ViewAction {
    /// The selected scene row after this action, given the row selected before it (`None`: no
    /// scene) and the scene count. Hosts apply a scene move this way, and a consumer that
    /// tracks the selection across a drain uses it to follow along. The selection stays within
    /// the scenes; moving from no selection selects the first scene.
    pub fn scene_after(self, selected: Option<usize>, count: usize) -> Option<usize> {
        let last = count.checked_sub(1)?;
        match self {
            Self::MoveScene(steps) => Some(
                selected
                    .map_or(0, |row| row.saturating_add_signed(steps as isize))
                    .min(last),
            ),
            Self::SelectScene(row) => Some(row.min(last)),
            Self::NextScene => Some(selected.map_or(0, |row| (row + 1) % count)),
            // Launching selects the scene below, and does nothing with no scene selected.
            Self::LaunchScene => selected.map(|row| (row + 1).min(last)),
            Self::SelectTrack(_) => selected,
        }
    }
}

pub trait Consumer: Send {
    /// Project replacement discards source history and takeover state.
    fn reset(&mut self) {}

    /// The host's selected scene row (`None`: no scene selected), told before each ordered
    /// drain. Only a consumer that needs the selection reads it.
    fn selected_scene(&mut self, _row: Option<usize>) {}

    /// Hosts with a View use this ordered stream. Command-only consumers need no changes.
    fn receive_actions(
        &mut self,
        events: &[Envelope],
        project: &Project,
        snapshot: &Snapshot,
        actions: &mut Vec<Received<Action>>,
    ) {
        let mut commands = Vec::new();
        self.receive(events, project, snapshot, &mut commands);
        actions.extend(commands.into_iter().map(|command| Received {
            value: Action::Command(command.value),
            receipt: command.receipt,
            continuous: command.continuous,
        }));
    }

    fn receive(
        &mut self,
        events: &[Envelope],
        project: &Project,
        snapshot: &Snapshot,
        commands: &mut Vec<Received<Command>>,
    );
}

#[derive(Default)]
pub struct Router {
    consumers: Vec<Box<dyn Consumer>>,
}

impl Router {
    pub fn reset(&mut self) {
        for consumer in &mut self.consumers {
            consumer.reset();
        }
    }

    pub fn subscribe(&mut self, consumer: impl Consumer + 'static) {
        self.consumers.push(Box::new(consumer));
    }

    /// No implicit consumers. With no mappings installed this returns no commands.
    pub fn drain(
        &mut self,
        ingress: &Ingress,
        project: &Project,
        snapshot: &Snapshot,
    ) -> (Drain, Vec<Received<Command>>) {
        let (drain, actions) = self.drain_actions(ingress, project, snapshot, None);
        let commands = actions
            .into_iter()
            .filter_map(|action| match action.value {
                Action::Command(command) => Some(Received {
                    value: command,
                    receipt: action.receipt,
                    continuous: action.continuous,
                }),
                Action::View(_) => None,
            })
            .collect();
        (drain, commands)
    }

    /// Drain with host View work interleaved with engine commands. A command-only/headless
    /// host may use `drain`; it deliberately ignores View actions.
    pub fn drain_actions(
        &mut self,
        ingress: &Ingress,
        project: &Project,
        snapshot: &Snapshot,
        selected_scene: Option<usize>,
    ) -> (Drain, Vec<Received<Action>>) {
        let drain = ingress.drain();
        let mut actions = Vec::new();
        for consumer in &mut self.consumers {
            consumer.selected_scene(selected_scene);
            consumer.receive_actions(&drain.events, project, snapshot, &mut actions);
        }
        (drain, actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Endpoint, Input};
    use std::sync::{Arc, Mutex};
    struct Observer(Arc<Mutex<Vec<u64>>>);
    impl Consumer for Observer {
        fn receive(
            &mut self,
            events: &[Envelope],
            _: &Project,
            _: &Snapshot,
            _: &mut Vec<Received<Command>>,
        ) {
            self.0
                .lock()
                .unwrap()
                .extend(events.iter().map(|event| event.sequence));
        }
    }
    #[test]
    fn explicit_consumers_receive_every_envelope_before_reduction() {
        let ingress = Ingress::default();
        for _ in 0..10 {
            ingress.push_midi(&Endpoint::Keyboard, &[0xb0, 1, 64]);
        }
        ingress.push(
            Endpoint::Keyboard,
            Input::Key {
                key: "a".into(),
                pressed: true,
                repeat: false,
            },
        );
        let mut router = Router::default();
        let first = Arc::new(Mutex::new(Vec::new()));
        let second = Arc::new(Mutex::new(Vec::new()));
        router.subscribe(Observer(first.clone()));
        router.subscribe(Observer(second.clone()));
        let engine = qtable_engine::engine::Engine::new(Project::default()).unwrap();
        let (drain, commands) = router.drain(&ingress, engine.project(), &engine.snapshot());
        assert!(commands.is_empty());
        assert_eq!(drain.events.len(), 11);
        assert_eq!(*first.lock().unwrap(), (1..=11).collect::<Vec<_>>());
        assert_eq!(*first.lock().unwrap(), *second.lock().unwrap());
    }
}
