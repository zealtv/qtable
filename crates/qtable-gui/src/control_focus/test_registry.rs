//! Inspect the registered focus controls in the actual widget tree.
use super::*;

pub(crate) fn controls<M, Key: ControlKey>(element: &mut Element<'_, M>) -> Vec<ControlId<Key>> {
    rectangles(element).into_iter().map(|(id, _)| id).collect()
}

/// Each registered control with whether it is enabled: a dimmed control is present but disabled.
pub(crate) fn states_in<M, Key: ControlKey>(
    element: &mut Element<'_, M>,
    size: Size,
) -> Vec<(ControlId<Key>, bool)> {
    struct Registry<Key>(Vec<(ControlId<Key>, bool)>);
    impl<Key: ControlKey> Operation for Registry<Key> {
        fn custom(&mut self, _: Option<&widget::Id>, _: Rectangle, state: &mut dyn Any) {
            if let Some(marker) = state.downcast_ref::<Marker<Key>>() {
                self.0.push((marker.0, marker.1));
            }
        }
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
    }
    let renderer = crate::grid::test_renderer();
    let mut tree = Tree::new(&*element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, size),
    );
    let mut registry = Registry(Vec::new());
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut registry);
    registry.0
}

pub(crate) fn rectangles<M, Key: ControlKey>(
    element: &mut Element<'_, M>,
) -> Vec<(ControlId<Key>, Rectangle)> {
    rectangles_in(element, Size::new(600.0, 900.0))
}

pub(crate) fn rectangles_in<M, Key: ControlKey>(
    element: &mut Element<'_, M>,
    size: Size,
) -> Vec<(ControlId<Key>, Rectangle)> {
    struct Registry<Key>(Vec<(ControlId<Key>, Rectangle)>);
    impl<Key: ControlKey> Operation for Registry<Key> {
        fn custom(&mut self, _: Option<&widget::Id>, bounds: Rectangle, state: &mut dyn Any) {
            if let Some(marker) = state.downcast_ref::<Marker<Key>>() {
                self.0.push((marker.0, bounds));
            }
        }
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
    }
    let renderer = crate::grid::test_renderer();
    let mut tree = Tree::new(&*element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, size),
    );
    let mut registry = Registry(Vec::new());
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut registry);
    registry.0
}

/// What the controls of `element` declare they drive, in tree order.
pub(crate) fn declared<M>(
    element: &mut Element<'_, M>,
    size: Size,
) -> Vec<qtable_engine::model::ControlRef> {
    struct Declarations(Vec<qtable_engine::model::ControlRef>);
    impl Operation for Declarations {
        fn custom(&mut self, _: Option<&widget::Id>, _: Rectangle, state: &mut dyn Any) {
            if let Some(declared) = state.downcast_ref::<crate::control::help::Declared>() {
                self.0.push(declared.0.clone());
            }
        }
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
            operate(self);
        }
    }
    let renderer = crate::grid::test_renderer();
    let mut tree = Tree::new(&*element);
    let node = element.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, size),
    );
    let mut found = Declarations(Vec::new());
    element
        .as_widget_mut()
        .operate(&mut tree, Layout::new(&node), &renderer, &mut found);
    found.0
}

/// A stand-in field key, so control-layer tests need no app key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TestKey(pub u64);

/// A stand-in set of help ids, so control-layer tests need no app registry. Its subject is
/// a track id, which is all a path field test needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TestHelp {
    ClipLength,
    OutputHost,
    Path,
    Position,
    TargetFieldControl,
    TargetNumber,
    TargetText,
    TargetToggle,
    TargetValue,
    TrackSolo,
    Volume,
    VolumePath,
}

impl crate::control::help::HelpId for TestHelp {
    type Subject = qtable_engine::model::TrackId;
}

impl ControlKey for TestKey {
    type Help = TestHelp;

    fn input_id(self) -> iced::widget::Id {
        iced::widget::Id::from(format!("test:{self:?}"))
    }
}

/// A stand-in message, so control-layer tests need no app message.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TestMessage {
    Info(crate::control::help::Hover<TestHelp>),
    Focused(ControlId<TestKey>),
    Tick,
    Commit,
    Cancel,
}

impl crate::control::widgets::ControlMessage<TestKey> for TestMessage {
    fn info(hover: crate::control::help::Hover<TestHelp>) -> Self {
        Self::Info(hover)
    }
    fn commit_number() -> Self {
        Self::Commit
    }
    fn field_entered() -> Self {
        Self::Tick
    }
    fn focused(id: ControlId<TestKey>) -> Self {
        Self::Focused(id)
    }
}
