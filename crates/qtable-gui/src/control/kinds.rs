//! Kinds backed by iced widgets: toggle, button, number, text and dropdown.
//! Their colours come from `look`, which the canvas adapter reads too.
use super::{ControlKind, Interaction, Lit, Look, SizeRole, State, Theme};
use iced::widget::canvas::{Frame, Path};
use iced::{Color, Point, Rectangle, Size};
use std::borrow::Cow;

/// Live's latched rectangular toggle: dark when off, orange with dark ink when on.
/// Disabled stays subdued even if its saved value is on.
pub(crate) struct Toggle {
    pub label: String,
}

impl ControlKind for Toggle {
    fn look(&self, theme: &Theme, state: State, interaction: Interaction) -> Look {
        let on = state.lit != Lit::Off;
        let (fill, ink) = if !state.enabled {
            (theme.control_background, theme.text_disabled)
        } else if on && state.inactive {
            (theme.control_on_inactive, theme.control_on_foreground)
        } else if on {
            (theme.chosen_default, theme.control_on_foreground)
        } else if state.inactive {
            (theme.control_background, theme.text_disabled)
        } else {
            (theme.control_background, theme.control_foreground)
        };
        let frame = if state.enabled && interaction == Interaction::Hovered {
            theme.control_foreground
        } else {
            theme.control_contrast_frame
        };
        Look {
            fill,
            ink,
            frame,
            accent: theme.chosen_default,
        }
    }

    fn role(&self) -> SizeRole {
        SizeRole::Content
    }
}

/// A device's round on/off switch in its title strip (Live's Activator): the Toggle's colours on
/// a fixed circle (`DEVICE_ACTIVATOR_SIZE`), with no label. Generators and modulators share it.
/// It takes no hover look: the circle holds its idle colours under the pointer.
pub(crate) struct Activator;

impl ControlKind for Activator {
    fn look(&self, theme: &Theme, state: State, interaction: Interaction) -> Look {
        Toggle {
            label: String::new(),
        }
        .look(theme, state, interaction)
    }

    fn role(&self) -> SizeRole {
        SizeRole::Fixed {
            width: crate::style::detail::DEVICE_ACTIVATOR_SIZE,
            height: crate::style::detail::DEVICE_ACTIVATOR_SIZE,
        }
    }
}

/// One tab of a view switch, as Live's Detail View Notes | Envelopes | MPE:
/// the shown view lifted onto ViewControlOff
/// (`surface_highlight`), the others flat on the bar (`surface_background`), each set apart by
/// dark rules (`control_contrast_frame`). Lit means shown.
pub(crate) struct ViewTab {
    pub label: String,
}

impl ControlKind for ViewTab {
    fn look(&self, theme: &Theme, state: State, interaction: Interaction) -> Look {
        let shown = state.lit != Lit::Off;
        let fill = if shown {
            theme.surface_highlight
        } else if state.enabled && interaction != Interaction::Idle {
            theme.detail_view_background
        } else {
            theme.surface_background
        };
        Look {
            fill,
            ink: if state.enabled {
                theme.control_foreground
            } else {
                theme.text_disabled
            },
            frame: theme.control_contrast_frame,
            accent: theme.surface_highlight,
        }
    }

    fn role(&self) -> SizeRole {
        SizeRole::Fixed {
            width: crate::style::detail::VIEW_TAB_WIDTH,
            height: crate::style::detail::CONTROL_HEIGHT,
        }
    }
}

/// What a button shows.
pub(crate) enum Face {
    Text(String),
    Glyph(Glyph),
    /// Joined or broken chain with its Linked/Unlinked label.
    Link(bool),
}

/// Live's transport play triangle and stop square.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Glyph {
    Play,
    Stop,
    Up,
    Down,
    /// Live's Draw Mode pencil, its tip at the bottom-left corner.
    Pencil,
    Linked,
    Unlinked,
    /// The seven envelope shapes of the Insert Shape menu, drawn as thin lines.
    ShapeSine,
    ShapeTriangle,
    ShapeSawDown,
    ShapeSawUp,
    ShapeSquare,
    ShapeRampIn,
    ShapeRampOut,
}

/// A line the shape glyphs stroke, in the unit square (y grows downwards), and whether it is
/// dotted (the level a ramp links to).
type Line = (Vec<(f32, f32)>, bool);

impl Glyph {
    pub(crate) const SIZE: f32 = 12.0;

    /// The lines of a shape glyph; `None` for every other glyph.
    fn shape_lines(self) -> Option<Vec<Line>> {
        let solid = |points: &[(f32, f32)]| (points.to_vec(), false);
        let dotted = |points: &[(f32, f32)]| (points.to_vec(), true);
        // A curve leaving or reaching its level the way a ramp does: fast at one end.
        let ramp = |rising: bool| -> Vec<(f32, f32)> {
            (0..=8)
                .map(|i| {
                    let t = i as f32 / 8.0;
                    let eased = 1.0 - (1.0 - t).powi(2);
                    (
                        0.62 * t,
                        0.9 - 0.8 * if rising { eased } else { 1.0 - t * t },
                    )
                })
                .collect()
        };
        Some(match self {
            Self::ShapeSine => vec![solid(
                &(0..=16)
                    .map(|i| {
                        let t = i as f32 / 16.0;
                        (t, 0.5 - 0.4 * (t * std::f32::consts::TAU).sin())
                    })
                    .collect::<Vec<_>>(),
            )],
            Self::ShapeTriangle => vec![solid(&[(0.0, 0.9), (0.5, 0.1), (1.0, 0.9)])],
            Self::ShapeSawDown => vec![solid(&[(0.0, 0.1), (1.0, 0.9)])],
            Self::ShapeSawUp => vec![solid(&[(0.0, 0.9), (1.0, 0.1)])],
            Self::ShapeSquare => vec![solid(&[(0.0, 0.1), (0.5, 0.1), (0.5, 0.9), (1.0, 0.9)])],
            Self::ShapeRampIn => {
                vec![solid(&ramp(true)), dotted(&[(0.62, 0.1), (1.0, 0.1)])]
            }
            Self::ShapeRampOut => {
                // The mirror of Ramp In: a dotted level on the left, then the fall.
                let fall: Vec<_> = ramp(false)
                    .into_iter()
                    .map(|(x, y)| (1.0 - x, y))
                    .rev()
                    .collect();
                vec![dotted(&[(0.0, 0.1), (0.38, 0.1)]), solid(&fall)]
            }
            _ => return None,
        })
    }

    /// The Draw Mode cursor: a pencil whose tip sits on the pointer.
    pub(crate) fn paint_cursor(self, frame: &mut Frame, tip: Point) {
        let size = 16.0;
        let bounds = Rectangle::new(Point::new(tip.x, tip.y - size), Size::new(size, size));
        self.paint(frame, bounds, crate::style::TEXT);
    }

    pub(crate) fn paint(self, frame: &mut Frame, bounds: Rectangle, ink: Color) {
        if let Some(lines) = self.shape_lines() {
            let at = |(x, y): (f32, f32)| {
                Point::new(bounds.x + x * bounds.width, bounds.y + y * bounds.height)
            };
            for (points, dotted) in lines {
                let path = Path::new(|p| {
                    if dotted {
                        for pair in points.windows(2) {
                            let (from, to) = (at(pair[0]), at(pair[1]));
                            let length = from.distance(to);
                            let dashes = (length / 2.0).floor().max(1.0) as usize;
                            for i in (0..dashes).step_by(2) {
                                let (a, b) = (
                                    i as f32 / dashes as f32,
                                    ((i + 1) as f32 / dashes as f32).min(1.0),
                                );
                                p.move_to(Point::new(
                                    from.x + (to.x - from.x) * a,
                                    from.y + (to.y - from.y) * a,
                                ));
                                p.line_to(Point::new(
                                    from.x + (to.x - from.x) * b,
                                    from.y + (to.y - from.y) * b,
                                ));
                            }
                        }
                    } else if let Some((first, rest)) = points.split_first() {
                        p.move_to(at(*first));
                        for point in rest {
                            p.line_to(at(*point));
                        }
                    }
                });
                frame.stroke(
                    &path,
                    iced::widget::canvas::Stroke::default()
                        .with_color(ink)
                        .with_width(1.0),
                );
            }
            return;
        }
        if matches!(self, Self::Linked | Self::Unlinked) {
            let at = |x: f32, y: f32| {
                Point::new(bounds.x + x * bounds.width, bounds.y + y * bounds.height)
            };
            let path = Path::new(|p| {
                if self == Self::Linked {
                    for (left, right) in [(0.08, 0.55), (0.45, 0.92)] {
                        p.move_to(at(left, 0.3));
                        p.line_to(at(right, 0.3));
                        p.line_to(at(right, 0.7));
                        p.line_to(at(left, 0.7));
                        p.close();
                    }
                } else {
                    p.move_to(at(0.4, 0.2));
                    p.line_to(at(0.1, 0.2));
                    p.line_to(at(0.1, 0.6));
                    p.line_to(at(0.4, 0.6));
                    p.move_to(at(0.6, 0.4));
                    p.line_to(at(0.9, 0.4));
                    p.line_to(at(0.9, 0.8));
                    p.line_to(at(0.6, 0.8));
                }
                p.move_to(at(0.0, 0.5));
                p.line_to(at(0.3, 0.5));
                p.move_to(at(0.7, 0.5));
                p.line_to(at(1.0, 0.5));
            });
            frame.stroke(
                &path,
                iced::widget::canvas::Stroke::default()
                    .with_color(ink)
                    .with_width(1.0),
            );
            return;
        }
        let path = match self {
            Self::Play => Path::new(|p| {
                p.move_to(Point::new(bounds.x + 1.0, bounds.y));
                p.line_to(Point::new(
                    bounds.x + bounds.width,
                    bounds.y + bounds.height / 2.0,
                ));
                p.line_to(Point::new(bounds.x + 1.0, bounds.y + bounds.height));
                p.close();
            }),
            Self::Up | Self::Down => Path::new(|p| {
                let at = |x: f32, y: f32| {
                    Point::new(
                        bounds.x + x * bounds.width,
                        bounds.y + if self == Self::Up { y } else { 1.0 - y } * bounds.height,
                    )
                };
                p.move_to(at(0.5, 0.0));
                for (x, y) in [
                    (1.0, 0.5),
                    (0.65, 0.5),
                    (0.65, 1.0),
                    (0.35, 1.0),
                    (0.35, 0.5),
                    (0.0, 0.5),
                ] {
                    p.line_to(at(x, y));
                }
                p.close();
            }),
            Self::Stop => Path::rectangle(
                Point::new(bounds.x + 1.0, bounds.y + 1.0),
                Size::new(bounds.width - 2.0, bounds.height - 2.0),
            ),
            Self::Linked | Self::Unlinked => unreachable!(),
            Self::ShapeSine
            | Self::ShapeTriangle
            | Self::ShapeSawDown
            | Self::ShapeSawUp
            | Self::ShapeSquare
            | Self::ShapeRampIn
            | Self::ShapeRampOut => unreachable!(),
            Self::Pencil => {
                // A diagonal body and a cone tip, in the unit square of `bounds`.
                let at = |x: f32, y: f32| {
                    Point::new(bounds.x + x * bounds.width, bounds.y + y * bounds.height)
                };
                let (ux, uy) = (
                    std::f32::consts::FRAC_1_SQRT_2,
                    -std::f32::consts::FRAC_1_SQRT_2,
                );
                let (nx, ny) = (-uy, ux);
                let (tip, cone, end, half) = ((0.02, 0.98), 0.3, 1.3, 0.14);
                let along = |d: f32, side: f32| {
                    at(
                        tip.0 + ux * d + nx * half * side,
                        tip.1 + uy * d + ny * half * side,
                    )
                };
                Path::new(|p| {
                    p.move_to(at(tip.0, tip.1));
                    p.line_to(along(cone, -1.0));
                    p.line_to(along(end, -1.0));
                    p.line_to(along(end, 1.0));
                    p.line_to(along(cone, 1.0));
                    p.close();
                })
            }
        };
        frame.fill(&path, ink);
    }
}

/// A flat action button, as in Live's top bar. Lit means playing (ChosenPlay), so only the
/// transport lights one; pressing an ordinary button never leaves it looking on.
pub(crate) struct Button {
    pub face: Face,
    /// A latching switch: lit in the default chosen colour, as a toggle, not in play green.
    pub latched: bool,
}

impl ControlKind for Button {
    fn look(&self, theme: &Theme, state: State, interaction: Interaction) -> Look {
        let lit = state.lit != Lit::Off;
        let lit_fill = if matches!(self.face, Face::Link(false)) {
            theme.chosen_alternative
        } else if self.latched {
            theme.chosen_default
        } else {
            theme.chosen_play
        };
        let (fill, ink) = match (state.enabled, lit, interaction) {
            (false, ..) => (theme.control_background, theme.text_disabled),
            (true, true, Interaction::Hovered) => (
                Color {
                    a: 0.85,
                    ..lit_fill
                },
                theme.control_on_foreground,
            ),
            (true, true, _) => (lit_fill, theme.control_on_foreground),
            (true, false, Interaction::Hovered) => {
                (theme.surface_highlight, theme.control_foreground)
            }
            (true, false, Interaction::Pressed) => {
                (theme.chosen_default, theme.control_on_foreground)
            }
            (true, false, Interaction::Idle) => {
                (theme.control_background, theme.control_foreground)
            }
        };
        Look {
            fill,
            ink,
            frame: fill,
            accent: lit_fill,
        }
    }

    fn role(&self) -> SizeRole {
        SizeRole::Content
    }

    fn paint(&self, frame: &mut Frame, bounds: Rectangle, look: &Look) {
        frame.fill_rectangle(bounds.position(), bounds.size(), look.fill);
        if let Face::Glyph(glyph) = self.face {
            let size = Glyph::SIZE.min(bounds.width).min(bounds.height);
            let at = Rectangle::new(
                Point::new(
                    bounds.center_x() - size / 2.0,
                    bounds.center_y() - size / 2.0,
                ),
                Size::new(size, size),
            );
            glyph.paint(frame, at, look.ink);
        }
    }
}

/// Field colours shared by numbers and text: a recessed display box whose border turns
/// orange while editing and alert-red while its draft is rejected.
pub(super) fn field_look(theme: &Theme, state: State, interaction: Interaction) -> Look {
    let frame = if state.invalid {
        theme.chosen_alert
    } else if interaction == Interaction::Pressed {
        theme.chosen_default
    } else {
        theme.surface_highlight
    };
    Look {
        fill: theme.display_background,
        ink: if state.enabled {
            theme.control_foreground
        } else {
            theme.text_disabled
        },
        frame,
        accent: theme.chosen_default,
    }
}

/// A right-aligned numeric field. The caller passes the draft, or the model value formatted
/// by its `ValueFormat`, as its text. Musical fields use `TimeField`.
pub(crate) struct Number {
    pub placeholder: &'static str,
    pub surface: NumberSurface,
    pub adjust_text: Option<super::adjustment::TextAdjustment>,
    pub(super) time_field: Option<super::TimeField>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum NumberSurface {
    #[default]
    Device,
    Chrome,
}

impl Number {
    pub(crate) fn new(placeholder: &'static str) -> Self {
        Self {
            placeholder,
            surface: NumberSurface::Device,
            adjust_text: None,
            time_field: None,
        }
    }

    pub(crate) fn adjust_text(
        mut self,
        adjust: impl Fn(&str, bool, bool) -> Option<String> + 'static,
    ) -> Self {
        self.adjust_text = Some(std::rc::Rc::new(adjust));
        self
    }
    pub(crate) fn chrome(mut self) -> Self {
        self.surface = NumberSurface::Chrome;
        self
    }
}

impl ControlKind for Number {
    fn look(&self, theme: &Theme, state: State, interaction: Interaction) -> Look {
        if let Some(field) = self.time_field {
            return field.look(theme, state, interaction);
        }
        if self.surface == NumberSurface::Device {
            return field_look(theme, state, interaction);
        }
        let fill = match interaction {
            Interaction::Idle => theme.transport_off_background,
            Interaction::Hovered => theme.transport_selection_background,
            Interaction::Pressed => theme.control_selection_frame,
        };
        Look {
            fill,
            frame: if state.invalid {
                theme.chosen_alert
            } else {
                fill
            },
            ink: if state.enabled {
                theme.control_foreground
            } else {
                theme.text_disabled
            },
            accent: theme.chosen_default,
        }
    }

    fn role(&self) -> SizeRole {
        SizeRole::Float
    }
}

/// A text field: names, hosts and OSC paths.
pub(crate) struct Text {
    pub placeholder: Cow<'static, str>,
}

impl ControlKind for Text {
    fn look(&self, theme: &Theme, state: State, interaction: Interaction) -> Look {
        field_look(theme, state, interaction)
    }

    fn role(&self) -> SizeRole {
        SizeRole::Name
    }
}

/// A display box that shows a value and never takes focus or input: a clip's Trigger, a
/// track's throughput counters, an output strip's host. It has the recessed face of a field
/// inside the darkest frame, since nothing about it edits. A compact role (`SizeRole::Strip`)
/// gives it the strip's 10 px text and tighter padding.
pub(crate) struct Readout {
    pub align: iced::alignment::Horizontal,
}

impl Readout {
    /// Numbers and short values read from the right, as a Number box does.
    pub(crate) fn right() -> Self {
        Self {
            align: iced::alignment::Horizontal::Right,
        }
    }

    /// Text such as a host reads from the left, as a Text field does.
    pub(crate) fn left() -> Self {
        Self {
            align: iced::alignment::Horizontal::Left,
        }
    }
}

impl ControlKind for Readout {
    fn look(&self, theme: &Theme, state: State, _: Interaction) -> Look {
        Look {
            fill: theme.display_background,
            ink: if state.enabled {
                theme.control_foreground
            } else {
                theme.text_disabled
            },
            frame: theme.control_contrast_frame,
            accent: theme.chosen_default,
        }
    }

    fn role(&self) -> SizeRole {
        SizeRole::Fill
    }
}

/// A boxed ▼ picker; its frame turns orange while the menu is open.
pub(crate) struct Dropdown;

impl ControlKind for Dropdown {
    fn look(&self, theme: &Theme, state: State, interaction: Interaction) -> Look {
        Look {
            fill: theme.display_background,
            ink: if state.enabled {
                theme.control_foreground
            } else {
                theme.text_disabled
            },
            frame: match interaction {
                Interaction::Pressed => theme.chosen_default,
                Interaction::Hovered => theme.surface_highlight,
                Interaction::Idle => theme.control_contrast_frame,
            },
            accent: theme.selection_background,
        }
    }

    fn role(&self) -> SizeRole {
        SizeRole::Content
    }
}
