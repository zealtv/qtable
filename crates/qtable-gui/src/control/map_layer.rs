//! Map Mode's layer over the window: it lays Live's blue over every control that declares what
//! it drives, keeps those controls from operating, and reports which one was clicked.
//!
//! The layer finds mappable controls the way every other client does, through the `Declared`
//! and `DeclaredRegions` markers that widget operations see (`control::help`); nothing is listed
//! by hand. While inactive it is a transparent pass-through, so turning Map Mode on or off
//! leaves the widget tree below it, with its scroll offsets and focus, untouched.
use super::Theme;
use super::help::{Declared, DeclaredRegions};
use iced::advanced::text::{self, Renderer as _};
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Renderer as _, Shell, Widget, layout, overlay, renderer};
use iced::{
    Border, Color, Element, Event, Length, Pixels, Point, Rectangle, Renderer, Size, Vector, mouse,
};
use qtable_engine::model::ControlRef;
use std::collections::HashMap;

/// How the layer draws one control: the source it is mapped to, if any, and whether it is the
/// control waiting for a controller message.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Mark {
    pub label: Option<String>,
    pub selected: bool,
}

/// `content` under Map Mode's layer. Inactive, it only passes everything through.
pub(crate) fn map_layer<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    active: bool,
    theme: Theme,
    ink: Color,
    marks: HashMap<ControlRef, Mark>,
    on_pick: impl Fn(ControlRef) -> M + 'a,
) -> Element<'a, M> {
    Element::new(Layer {
        content: content.into(),
        active,
        theme,
        ink,
        marks,
        on_pick: Box::new(on_pick),
    })
}

/// Every mappable region of `operate`'s tree that shows on screen, in drawing order. A region
/// scrolled out of its scrollable's viewport is cut to what remains, or dropped.
pub(crate) fn collect(operate: impl FnOnce(&mut dyn Operation)) -> Vec<(Rectangle, ControlRef)> {
    let mut found = Vec::new();
    let mut collector = Collector {
        out: &mut found,
        frames: Vec::new(),
        pending: None,
    };
    operate(&mut collector);
    found
}

struct Frame {
    clip: Rectangle,
    shift: Vector,
}

struct Collector<'r> {
    out: &'r mut Vec<(Rectangle, ControlRef)>,
    frames: Vec<Frame>,
    pending: Option<Frame>,
}

impl Collector<'_> {
    fn shift(&self) -> Vector {
        self.frames.last().map_or(Vector::ZERO, |frame| frame.shift)
    }

    fn add(&mut self, rect: Rectangle, control: ControlRef) {
        let mut visible = rect + self.shift();
        for frame in &self.frames {
            let Some(inside) = visible.intersection(&frame.clip) else {
                return;
            };
            visible = inside;
        }
        if visible.width > 0.0 && visible.height > 0.0 {
            self.out.push((visible, control));
        }
    }
}

impl Operation for Collector<'_> {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        let frame = self.pending.take();
        let scrolled = frame.is_some();
        if let Some(frame) = frame {
            self.frames.push(frame);
        }
        operate(self);
        if scrolled {
            self.frames.pop();
        }
    }

    fn scrollable(
        &mut self,
        _: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        _: Rectangle,
        translation: Vector,
        _: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        let shift = self.shift();
        self.pending = Some(Frame {
            clip: bounds + shift,
            shift: shift - translation,
        });
    }

    fn custom(
        &mut self,
        _: Option<&iced::advanced::widget::Id>,
        bounds: Rectangle,
        state: &mut dyn std::any::Any,
    ) {
        if let Some(declared) = state.downcast_ref::<Declared>() {
            self.add(bounds, declared.0.clone());
        } else if let Some(regions) = state.downcast_ref::<DeclaredRegions>() {
            for (rect, control) in &regions.0 {
                self.add(*rect, control.clone());
            }
        }
    }
}

struct Layer<'a, M> {
    content: Element<'a, M>,
    active: bool,
    theme: Theme,
    ink: Color,
    marks: HashMap<ControlRef, Mark>,
    on_pick: Box<dyn Fn(ControlRef) -> M + 'a>,
}

#[derive(Default)]
struct State {
    regions: Vec<(Rectangle, ControlRef)>,
}

impl<'a, M> Layer<'a, M> {
    fn refresh(&mut self, tree: &mut Tree, layout: Layout<'_>, renderer: &Renderer) {
        let regions = collect(|operation| {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                operation,
            );
        });
        tree.state.downcast_mut::<State>().regions = regions;
    }
}

impl<M> Widget<M, iced::Theme, Renderer> for Layer<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
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
        if self.active {
            let redraw = matches!(
                event,
                Event::Window(iced::window::Event::RedrawRequested(_))
                    | Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            );
            if redraw {
                self.refresh(tree, layout, renderer);
            }
            if matches!(
                event,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            ) && let Some(position) = cursor.position()
                && viewport.contains(position)
            {
                // The last region is the one drawn on top.
                let hit = tree
                    .state
                    .downcast_ref::<State>()
                    .regions
                    .iter()
                    .rev()
                    .find(|(rect, _)| rect.contains(position))
                    .map(|(_, control)| control.clone());
                if let Some(control) = hit {
                    shell.publish((self.on_pick)(control));
                    shell.capture_event();
                    return;
                }
            }
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
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.active
            && let Some(position) = cursor.position()
            && tree
                .state
                .downcast_ref::<State>()
                .regions
                .iter()
                .any(|(rect, _)| rect.contains(position))
        {
            return mouse::Interaction::Pointer;
        }
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
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        if !self.active {
            return;
        }
        let regions = &tree.state.downcast_ref::<State>().regions;
        renderer.with_layer(*viewport, |renderer| {
            for (rect, control) in regions {
                let mark = self.marks.get(control);
                paint(renderer, &self.theme, self.ink, *rect, mark);
            }
        });
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, iced::Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// Alpha of the blue over a control nothing maps yet, and over one that is mapped.
const MAPPABLE_ALPHA: f32 = 0.45;
const MAPPED_ALPHA: f32 = 0.85;
const LABEL_SIZE: f32 = 9.0;
const SELECTED_FRAME: f32 = 2.0;

fn paint(renderer: &mut Renderer, theme: &Theme, ink: Color, rect: Rectangle, mark: Option<&Mark>) {
    let mapped = mark.is_some_and(|mark| mark.label.is_some());
    let selected = mark.is_some_and(|mark| mark.selected);
    let blue = Color {
        a: if mapped { MAPPED_ALPHA } else { MAPPABLE_ALPHA },
        ..ink
    };
    renderer.fill_quad(
        renderer::Quad {
            bounds: rect,
            border: Border {
                color: theme.control_selection_frame,
                width: if selected { SELECTED_FRAME } else { 0.0 },
                radius: theme.corner_control.into(),
            },
            ..Default::default()
        },
        blue,
    );
    if let Some(label) = mark.and_then(|mark| mark.label.as_ref()) {
        renderer.fill_text(
            text::Text {
                content: label.clone(),
                bounds: rect.size(),
                size: Pixels(LABEL_SIZE),
                line_height: text::LineHeight::default(),
                font: iced::Font::default(),
                align_x: text::Alignment::Center,
                align_y: iced::alignment::Vertical::Center,
                shaping: text::Shaping::Basic,
                wrapping: text::Wrapping::None,
            },
            Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0),
            Color::WHITE,
            rect,
        );
    }
}

#[cfg(test)]
mod tests;
