//! A thin white outline follows the last control clicked in this window, like Live's focus border.
//! GUI session state only: it never dispatches project edits or OSC.
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer, widget};
use iced::{
    Border, Color, Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector, mouse,
};
use qtable_engine::model::{SceneId, TrackId};
use std::any::Any;
use std::fmt::Debug;
use std::hash::Hash;
use widget::{Operation, Tree, tree};

mod region;
pub(crate) use region::region;

/// Where a control lives, so the same model field shown twice stays two focus targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum View {
    ControlBar,
    StatusBar,
    Mixer,
    Main,
    Track,
    Output,
    Grid,
    Clip,
    Scene,
    /// The mapping list of Map Mode.
    Mappings,
    /// The Settings pane.
    Settings,
}

/// Whose control it is, after the Live Object Model's Song, Track, Scene and ClipSlot.
/// Groups and outputs are tracks reached through `TrackId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ControlOwner {
    Song,
    Device(qtable_engine::model::DeviceId),
    Track(TrackId),
    Scene(SceneId),
    /// A slot by its track (or group) and row; a clip's fields belong to its slot.
    ClipSlot(TrackId, usize),
}

/// The model field a text control edits, as the app names it. The control layer only stores,
/// compares and hands the key back; the key is its own owner, and it names its text input.
pub(crate) trait ControlKey: Copy + Eq + Hash + Debug + Send + 'static {
    /// The app's help ids, which every control built over this key carries (`control::help`).
    type Help: crate::control::help::HelpId;

    fn input_id(self) -> iced::widget::Id;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Target<Key> {
    Field(Key),
    FieldAction(Key, &'static str),
    Action(ControlOwner, &'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ControlId<Key> {
    view: View,
    target: Target<Key>,
}

impl<Key: ControlKey> ControlId<Key> {
    /// A text field, by the model field it edits and the view that shows it.
    pub(crate) fn field(view: View, key: Key) -> Self {
        Self {
            view,
            target: Target::Field(key),
        }
    }

    /// A picker or button attached to a field, distinct from its value input.
    pub(crate) fn field_action(view: View, key: Key, name: &'static str) -> Self {
        Self {
            view,
            target: Target::FieldAction(key, name),
        }
    }

    /// The view the control sits in.
    pub(crate) fn view(self) -> View {
        self.view
    }

    /// The model field a text control edits, for its iced widget id.
    pub(crate) fn field_key(self) -> Option<Key> {
        match self.target {
            Target::Field(key) => Some(key),
            Target::Action(..) | Target::FieldAction(..) => None,
        }
    }

    /// A button or picker, by a stable action name and its owner.
    pub(crate) fn action(view: View, name: &'static str, owner: ControlOwner) -> Self {
        Self {
            view,
            target: Target::Action(owner, name),
        }
    }
}

/// The last clicked control of one window.
#[derive(Debug)]
pub(crate) struct ControlFocus<Key> {
    current: Option<ControlId<Key>>,
}

impl<Key> Default for ControlFocus<Key> {
    fn default() -> Self {
        Self { current: None }
    }
}

impl<Key: ControlKey> ControlFocus<Key> {
    pub(crate) fn claim(&mut self, id: ControlId<Key>) {
        self.current = Some(id);
    }

    /// Keyboard navigation moves widget focus itself, so the latched click outline would double up.
    pub(crate) fn clear(&mut self) {
        self.current = None;
    }

    pub(crate) fn has(&self, id: ControlId<Key>) -> bool {
        self.current == Some(id)
    }

    /// The track owning the focused action control of this name and view, if that is the focus.
    pub(crate) fn track_action(&self, view: View, name: &'static str) -> Option<TrackId> {
        match self.current? {
            ControlId {
                view: v,
                target: Target::Action(ControlOwner::Track(track), n),
            } if v == view && n == name => Some(track),
            _ => None,
        }
    }

    /// The focused control, for canvases that draw their own outline.
    pub(crate) fn current(&self) -> Option<ControlId<Key>> {
        self.current
    }

    /// Wrap a finished control. A disabled control never claims focus, and a validation
    /// border (`invalid`) stays the stronger signal, so the outline is not drawn over it.
    pub(crate) fn wrap<'a, M: 'a>(
        &self,
        id: ControlId<Key>,
        enabled: bool,
        invalid: bool,
        content: impl Into<Element<'a, M>>,
        on_claim: impl Fn(ControlId<Key>) -> M + 'a,
    ) -> Element<'a, M> {
        self.wrap_as(id, enabled, invalid, Outline::Box, content, on_claim)
    }

    /// `wrap` with a chosen outline: Live draws a Slider's and Dial's focus as corner brackets.
    pub(crate) fn wrap_as<'a, M: 'a>(
        &self,
        id: ControlId<Key>,
        enabled: bool,
        invalid: bool,
        shape: Outline,
        content: impl Into<Element<'a, M>>,
        on_claim: impl Fn(ControlId<Key>) -> M + 'a,
    ) -> Element<'a, M> {
        Element::new(self.slot(id, enabled, invalid, shape, content, on_claim))
    }

    /// A selected toggle uses the same action for Enter as for a pointer click.
    pub(crate) fn wrap_enter<'a, M: Clone + 'a>(
        &self,
        id: ControlId<Key>,
        enabled: bool,
        invalid: bool,
        content: impl Into<Element<'a, M>>,
        on_claim: impl Fn(ControlId<Key>) -> M + 'a,
        on_enter: M,
    ) -> Element<'a, M> {
        let mut slot = self.slot(id, enabled, invalid, Outline::Box, content, on_claim);
        slot.on_enter = Some(Box::new(move || on_enter.clone()));
        Element::new(slot)
    }

    fn slot<'a, M: 'a>(
        &self,
        id: ControlId<Key>,
        enabled: bool,
        invalid: bool,
        shape: Outline,
        content: impl Into<Element<'a, M>>,
        on_claim: impl Fn(ControlId<Key>) -> M + 'a,
    ) -> Focusable<'a, M, Key> {
        Focusable {
            content: content.into(),
            id,
            enabled,
            shape,
            selected: self.has(id),
            outlined: self.has(id) && !invalid,
            on_claim: Box::new(on_claim),
            on_enter: None,
        }
    }
}

/// How a focused control is outlined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outline {
    /// The widget paints its own selected state (flat Chrome numeric faces).
    None,
    /// A 1 px stroke all round, following the control's corners.
    Box,
    /// A 1 px stroke all round, as a circle (a round switch such as the device Activator).
    Round,
    /// An L at each corner, as Live draws a Slider's or Dial's focus.
    Corners,
}

struct Focusable<'a, M, Key> {
    content: Element<'a, M>,
    id: ControlId<Key>,
    enabled: bool,
    shape: Outline,
    outlined: bool,
    selected: bool,
    on_enter: Option<Box<dyn Fn() -> M + 'a>>,
    on_claim: Box<dyn Fn(ControlId<Key>) -> M + 'a>,
}

/// The eight 1 px bars of four corner brackets with arms `arm` long, inside `bounds`.
pub(crate) fn corner_brackets(bounds: Rectangle, arm: f32) -> [Rectangle; 8] {
    let arm = arm.min(bounds.width / 2.0).min(bounds.height / 2.0);
    let (right, bottom) = (bounds.x + bounds.width, bounds.y + bounds.height);
    let bar = |x, y, width, height| {
        Rectangle::new(iced::Point::new(x, y), iced::Size::new(width, height))
    };
    [
        bar(bounds.x, bounds.y, arm, 1.0),
        bar(bounds.x, bounds.y, 1.0, arm),
        bar(right - arm, bounds.y, arm, 1.0),
        bar(right - 1.0, bounds.y, 1.0, arm),
        bar(bounds.x, bottom - 1.0, arm, 1.0),
        bar(bounds.x, bottom - arm, 1.0, arm),
        bar(right - arm, bottom - 1.0, arm, 1.0),
        bar(right - 1.0, bottom - arm, 1.0, arm),
    ]
}

/// Lets an outer wrapper find inner ones, so the deepest control keeps the click.
/// A focusable control's id, and whether it is enabled (tests read it to tell dimmed from live).
struct Marker<Key>(
    ControlId<Key>,
    #[cfg_attr(not(test), allow(dead_code))] bool,
);

/// Locate the rendered control, after layout, so keyboard-opened popovers can anchor to it.
pub(crate) fn bounds<Key: ControlKey>(id: ControlId<Key>) -> iced::Task<Option<Rectangle>> {
    iced::advanced::widget::operate(ControlBounds::new(id))
}

struct ControlBounds<Key> {
    id: ControlId<Key>,
    found: Option<Rectangle>,
    depth: usize,
    scrolls: Vec<(usize, Vector, Option<Rectangle>)>,
}

impl<Key> ControlBounds<Key> {
    fn new(id: ControlId<Key>) -> Self {
        Self {
            id,
            found: None,
            depth: 0,
            scrolls: Vec::new(),
        }
    }
}

impl<Key: ControlKey> Operation<Option<Rectangle>> for ControlBounds<Key> {
    fn custom(&mut self, _: Option<&widget::Id>, bounds: Rectangle, state: &mut dyn Any) {
        if state
            .downcast_ref::<Marker<Key>>()
            .is_some_and(|marker| marker.0 == self.id)
        {
            self.found = match self.scrolls.last() {
                Some((_, translation, clip)) => {
                    clip.and_then(|clip| (bounds - *translation).intersection(&clip))
                }
                None => Some(bounds),
            }
            .filter(|bounds| bounds.width > 0.0 && bounds.height > 0.0);
        }
    }
    fn scrollable(
        &mut self,
        _: Option<&widget::Id>,
        bounds: Rectangle,
        _: Rectangle,
        translation: Vector,
        _: &mut dyn widget::operation::Scrollable,
    ) {
        let (parent_translation, parent_clip) = self
            .scrolls
            .last()
            .map(|(_, translation, clip)| (*translation, *clip))
            .unwrap_or((Vector::ZERO, Some(bounds)));
        let clip = parent_clip.and_then(|clip| (bounds - parent_translation).intersection(&clip));
        self.scrolls
            .push((self.depth, parent_translation + translation, clip));
    }
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Option<Rectangle>>)) {
        self.depth += 1;
        operate(self);
        self.depth -= 1;
        self.scrolls.retain(|(depth, _, _)| *depth < self.depth);
    }
    fn finish(&self) -> widget::operation::Outcome<Option<Rectangle>> {
        widget::operation::Outcome::Some(self.found)
    }
}

struct Probe<Key> {
    inner_at: Option<iced::Point>,
    hit: bool,
    key: std::marker::PhantomData<Key>,
}

impl<Key: ControlKey> Operation for Probe<Key> {
    fn custom(&mut self, _: Option<&widget::Id>, bounds: Rectangle, state: &mut dyn Any) {
        if let Some(point) = self.inner_at
            && state.is::<Marker<Key>>()
            && bounds.contains(point)
        {
            self.hit = true;
        }
    }
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
}

impl<M, Key: ControlKey> Widget<M, Theme, Renderer> for Focusable<'_, M, Key> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Marker<Key>>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(Marker(self.id, self.enabled))
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.custom(None, layout.bounds(), &mut Marker(self.id, self.enabled));
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let was_captured = shell.is_event_captured();
        if !was_captured
            && self.enabled
            && self.selected
            && let Some(activate) = &self.on_enter
            && let Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
                modifiers,
                repeat,
                ..
            }) = event
            && *modifiers == iced::keyboard::Modifiers::NONE
        {
            if !repeat {
                shell.publish(activate());
            }
            shell.capture_event();
            return;
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if !self.enabled
            || was_captured
            || !shell.is_event_captured()
            || !matches!(
                event,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            )
        {
            return;
        }
        // Clipped controls must not claim a click that lands outside their visible area.
        let Some(point) = cursor
            .position()
            .filter(|p| viewport.contains(*p) && layout.bounds().contains(*p))
        else {
            return;
        };
        let mut probe = Probe::<Key> {
            inner_at: Some(point),
            hit: false,
            key: std::marker::PhantomData,
        };
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, &mut probe);
        if !probe.hit {
            shell.publish((self.on_claim)(self.id));
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        if self.outlined && self.enabled {
            let theme = crate::control::LIVE_DARK;
            let bounds = layout.bounds();
            // Render after the child's canvas geometry. Renderers batch quads before canvas
            // primitives within a layer, even when fill_quad is called after the child draws.
            renderer.with_layer(*viewport, |renderer| match self.shape {
                Outline::None => {}
                Outline::Box => renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        border: Border {
                            color: theme.focus_stroke,
                            width: 1.0,
                            radius: theme.corner_control.into(),
                        },
                        ..Default::default()
                    },
                    Color::TRANSPARENT,
                ),
                Outline::Round => renderer.fill_quad(
                    renderer::Quad {
                        bounds,
                        border: Border {
                            color: theme.focus_stroke,
                            width: 1.0,
                            radius: (bounds.width.min(bounds.height) / 2.0).into(),
                        },
                        ..Default::default()
                    },
                    Color::TRANSPARENT,
                ),
                Outline::Corners => {
                    for bracket in corner_brackets(bounds, theme.focus_bracket_arm) {
                        renderer.fill_quad(
                            renderer::Quad {
                                bounds: bracket,
                                ..Default::default()
                            },
                            theme.focus_stroke,
                        );
                    }
                }
            });
        }
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod test_registry;
