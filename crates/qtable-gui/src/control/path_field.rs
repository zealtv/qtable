//! The path field: an editable OSC path set after the dim part it inherits, so the line
//! reads as the address that gets sent. Three small parts sit in one frame: the inherited prefix (`PrefixText`),
//! the borderless input, and an optional Absolute toggle. `PathFrame` draws the frame.
use super::widgets::Looks;
use super::{ControlKind, Interaction, Look, SizeRole, State, Theme, kinds};
use crate::style::VALUE_FONT;
use iced::advanced::text::{self, Paragraph as _};
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, overlay, renderer};
use iced::{
    Border, Color, Element, Event, Length, Padding, Pixels, Rectangle, Renderer, Size, mouse,
};

/// Minimum editable width, including padding and room for the cursor.
pub(crate) const CURSOR_FLOOR: f32 = 12.0;

/// A path field: names a piece of an OSC address and shows what it inherits.
pub(crate) struct PathField<S> {
    /// Shown when the field is empty: "no path", "/volume".
    pub placeholder: &'static str,
    /// The inherited part; empty when absolute or when nothing is inherited.
    pub prefix: Prefix,
    /// `None`: this field offers no Absolute toggle.
    pub absolute: Option<bool>,
    /// What the field reports while the pointer rests on it; the panel chooses the type.
    pub subject: S,
}

impl<S> PathField<S> {
    pub(crate) fn input_placeholder(&self) -> &'static str {
        if self.prefix.text().is_empty() {
            self.placeholder
        } else {
            ""
        }
    }
}

impl<S> ControlKind for PathField<S> {
    fn look(&self, theme: &Theme, state: State, interaction: Interaction) -> Look {
        kinds::field_look(theme, state, interaction)
    }

    fn role(&self) -> SizeRole {
        SizeRole::Fill
    }
}

/// Presentation ownership, independent of the model that supplies a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ownership {
    Inherited,
    Parent,
    Own,
}

impl Ownership {
    pub(crate) fn ink(self, theme: &Theme, enabled: bool) -> Color {
        if !enabled {
            return theme.text_disabled;
        }
        match self {
            Self::Inherited => theme.text_disabled,
            Self::Parent => theme.path_parent,
            Self::Own => theme.control_foreground,
        }
    }
}

/// A labelled piece; the label is used by the Info View, never interpreted by the field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Segment {
    pub label: &'static str,
    pub path: String,
    pub ownership: Ownership,
}

impl Segment {
    pub(crate) fn new(label: &'static str, path: impl Into<String>, ownership: Ownership) -> Self {
        Self {
            label,
            path: path.into(),
            ownership,
        }
    }
}

/// The first destination's ordered prefix pieces, and the count of other distinct paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Prefix {
    pub segments: Vec<Segment>,
    pub more: usize,
}

impl Prefix {
    pub(crate) fn text(&self) -> String {
        self.segments.iter().map(|s| s.path.as_str()).collect()
    }
}

/// The border and fill the frame draws: the pressed (orange) look while the input has
/// focus, otherwise idle. A rejected draft is already in both looks, so it wins over focus.
pub(crate) fn frame_look(looks: &Looks, focused: bool) -> Look {
    if focused { looks.pressed } else { looks.idle }
}

/// Drop whole leading segments for an ellipsis until `full` fits `budget`:
/// `/bop/drums/kick` → `.../drums/kick` → `.../kick` → `...`.
pub(crate) fn shorten(full: &str, budget: f32, width: impl Fn(&str) -> f32) -> String {
    if full.is_empty() || width(full) <= budget {
        return full.to_owned();
    }
    for (start, _) in full.match_indices('/').skip(1) {
        let shown = format!("...{}", &full[start..]);
        if width(&shown) <= budget {
            return shown;
        }
    }
    "...".to_owned()
}

type Para = <Renderer as text::Renderer>::Paragraph;

fn paragraph(content: &str, size: Pixels, bounds: Size) -> Para {
    Para::with_text(text::Text {
        content,
        bounds,
        size,
        line_height: text::LineHeight::default(),
        font: VALUE_FONT,
        align_x: text::Alignment::Default,
        align_y: iced::alignment::Vertical::Center,
        shaping: text::Shaping::Basic,
        wrapping: text::Wrapping::None,
    })
}

/// Text width in the value font (`control::measure`, the one place text is measured).
pub(crate) fn measure(content: &str, size: Pixels) -> f32 {
    super::measure::width(content, super::measure::Face::value(size))
}

/// The dim inherited part. It shortens itself from the left to fit the space left by the typed text and controls.
/// The editable part always keeps room for its cursor. It takes no cursor, selection or copy.
pub(crate) struct PrefixText {
    segments: Vec<Segment>,
    reserved: f32,
    size: Pixels,
    theme: Theme,
    enabled: bool,
    shown: Vec<(Para, Color)>,
}

impl PrefixText {
    pub(crate) fn new(
        segments: Vec<Segment>,
        reserved: f32,
        size: u32,
        theme: Theme,
        enabled: bool,
    ) -> Self {
        Self {
            segments,
            reserved,
            size: Pixels(size as f32),
            theme,
            enabled,
            shown: Vec::new(),
        }
    }

    fn shown_spans(&self, budget: f32) -> Vec<(String, Ownership)> {
        let full: String = self.segments.iter().map(|s| s.path.as_str()).collect();
        let shown = shorten(&full, budget, |s| measure(s, self.size));
        let omitted = if shown == full {
            0
        } else {
            full.len() - (shown.len() - 3)
        };
        let mut spans = Vec::new();
        if omitted > 0 {
            spans.push(("...".to_owned(), Ownership::Inherited));
        }
        let mut offset = 0;
        for segment in &self.segments {
            let start = omitted.saturating_sub(offset).min(segment.path.len());
            if start < segment.path.len() {
                spans.push((segment.path[start..].to_owned(), segment.ownership));
            }
            offset += segment.path.len();
        }
        spans
    }
}

impl<M> Widget<M, iced::Theme, Renderer> for PrefixText {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(&mut self, _: &mut Tree, _: &Renderer, limits: &layout::Limits) -> layout::Node {
        let budget = (limits.max().width - self.reserved).max(0.0);
        self.shown = self
            .shown_spans(budget)
            .into_iter()
            .map(|(text, ownership)| {
                (
                    paragraph(&text, self.size, Size::INFINITE),
                    ownership.ink(&self.theme, self.enabled),
                )
            })
            .collect();
        let width: f32 = self.shown.iter().map(|(p, _)| p.min_bounds().width).sum();
        let height = self
            .shown
            .iter()
            .map(|(p, _)| p.min_bounds().height)
            .fold(0.0, f32::max);
        layout::Node::new(Size::new(width.ceil(), height.ceil()))
    }

    fn draw(
        &self,
        _: &Tree,
        renderer: &mut Renderer,
        _: &iced::Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::text::Renderer as _;
        let bounds = layout.bounds();
        let mut x = bounds.x;
        for (paragraph, colour) in &self.shown {
            let anchor = Rectangle { x, ..bounds }.anchor(
                paragraph.min_bounds(),
                paragraph.align_x(),
                iced::alignment::Vertical::Center,
            );
            renderer.fill_paragraph(paragraph, anchor, *colour, *viewport);
            x += paragraph.min_bounds().width;
        }
    }
}

impl<'a, M: 'a> From<PrefixText> for Element<'a, M> {
    fn from(widget: PrefixText) -> Self {
        Element::new(widget)
    }
}

/// The frame around the row of parts: the field's fill and 1 px border, orange while the
/// input inside has focus and red while its draft is rejected. A press on the dim parts or
/// the padding asks for focus (`on_focus`) instead of dropping it.
pub(crate) struct PathFrame<'a, M> {
    content: Element<'a, M>,
    looks: Looks,
    width: Length,
    on_focus: Option<M>,
    absolute: bool,
}

impl<'a, M> PathFrame<'a, M> {
    pub(crate) fn new(
        content: impl Into<Element<'a, M>>,
        looks: Looks,
        width: Length,
        on_focus: Option<M>,
        absolute: bool,
    ) -> Self {
        Self {
            content: content.into(),
            looks,
            width,
            on_focus,
            absolute,
        }
    }
}

// Controls::path builds the row as [leading padding, prefix, text_input, +N?,
// divider?, Absolute?]. Keep this index aligned with that order: draw downcasts
// this child's state to text_input::State, and update uses its bounds for presses.
const INPUT_PART: usize = 2;

const BORDER: f32 = 1.0;

impl<M: Clone> Widget<M, iced::Theme, Renderer> for PathFrame<'_, M> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::padded(
            limits,
            self.width,
            Length::Shrink,
            Padding::new(BORDER),
            |limits| {
                self.content
                    .as_widget_mut()
                    .layout(&mut tree.children[0], renderer, limits)
            },
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            layout.children().next().expect("frame content"),
            renderer,
            operation,
        );
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
        let inner = layout.children().next().expect("frame content");
        // Swallow inherited-part presses before the input can treat them as blur.
        let parts: Vec<_> = inner.children().collect();
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
            && let Some(on_focus) = &self.on_focus
            && let Some(p) = cursor
                .position_over(layout.bounds())
                .filter(|p| viewport.contains(*p))
            && !parts[INPUT_PART].bounds().contains(p)
            && !(self.absolute && parts.last().expect("toggle").bounds().contains(p))
        {
            shell.publish(on_focus.clone());
            shell.capture_event();
            return;
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            inner,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
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
            layout.children().next().expect("frame content"),
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        // Read the input state at draw time: focus operations can run without an update.
        let focused = tree.children[0].children[INPUT_PART]
            .state
            .downcast_ref::<iced::widget::text_input::State<Para>>()
            .is_focused();
        let look = frame_look(&self.looks, focused);
        renderer.fill_quad(
            renderer::Quad {
                bounds: layout.bounds(),
                border: Border {
                    color: look.frame,
                    width: BORDER,
                    radius: crate::control::LIVE_DARK.corner_control.into(),
                },
                ..Default::default()
            },
            look.fill,
        );
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout.children().next().expect("frame content"),
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: iced::Vector,
    ) -> Option<overlay::Element<'b, M, iced::Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout.children().next().expect("frame content"),
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, M: Clone + 'a> From<PathFrame<'a, M>> for Element<'a, M> {
    fn from(frame: PathFrame<'a, M>) -> Self {
        Element::new(frame)
    }
}

#[cfg(test)]
mod tests;
