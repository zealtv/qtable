//! Kinds drawn on a canvas: mixer Solo, and the grid's launch, stop,
//! Follow Actions and fold buttons. Painting moved here from `mixer_band` and `grid::draw`
//! unchanged; the grid passes playback colours as a `Look` where state alone cannot decide.
use super::{ControlKind, Interaction, Lit, Look, SizeRole, State, Theme};
use iced::widget::canvas::{Frame, Path, Stroke, Text as CanvasText};
use iced::{Color, Point, Rectangle, Size, alignment};

fn rect(frame: &mut Frame, area: Rectangle, fill: Color) {
    frame.fill_rectangle(area.position(), area.size(), fill);
}

/// Mixer Solo. `Lit::Partial` is Live's half-coloured group Solo (manual 18.3).
pub(crate) struct Solo;

impl ControlKind for Solo {
    fn activates_on_enter(&self) -> bool {
        true
    }
    fn look(&self, theme: &Theme, state: State, _: Interaction) -> Look {
        let (fill, ink, accent) = match state.lit {
            Lit::On => (
                theme.chosen_alternative,
                theme.control_on_foreground,
                theme.chosen_alternative,
            ),
            Lit::Partial => (
                theme.control_background,
                theme.control_foreground,
                theme.chosen_alternative,
            ),
            Lit::Off => (
                theme.control_background,
                theme.control_foreground,
                theme.control_background,
            ),
        };
        Look {
            fill,
            ink,
            frame: fill,
            accent,
        }
    }

    fn role(&self) -> SizeRole {
        SizeRole::Fixed {
            width: 22.0,
            height: 13.0,
        }
    }

    fn paint(&self, frame: &mut Frame, bounds: Rectangle, look: &Look) {
        rect(frame, bounds, look.fill);
        // The lower-right half, split on the diagonal as in Live; invisible unless partial.
        let half = Path::new(|p| {
            p.move_to(Point::new(bounds.x + bounds.width, bounds.y));
            p.line_to(Point::new(
                bounds.x + bounds.width,
                bounds.y + bounds.height,
            ));
            p.line_to(Point::new(bounds.x, bounds.y + bounds.height));
            p.close();
        });
        frame.fill(&half, look.accent);
        frame.fill_text(CanvasText {
            content: "S".into(),
            position: Point::new(bounds.center_x(), bounds.center_y()),
            color: look.ink,
            size: iced::Pixels(crate::style::detail::COMPACT_TEXT_SIZE as f32),
            align_x: iced::widget::text::Alignment::Center,
            align_y: alignment::Vertical::Center,
            ..CanvasText::default()
        });
    }
}

const GRID_BUTTON: SizeRole = SizeRole::Fixed {
    width: 20.0,
    height: 20.0,
};

/// A button square holding a small centred stop square.
pub(crate) struct Stop;

impl ControlKind for Stop {
    fn look(&self, theme: &Theme, _: State, _: Interaction) -> Look {
        Look::glyph(theme.control_background, theme.text_disabled)
    }

    fn role(&self) -> SizeRole {
        GRID_BUTTON
    }

    fn paint(&self, frame: &mut Frame, bounds: Rectangle, look: &Look) {
        rect(frame, bounds, look.fill);
        let s = 7.0;
        rect(
            frame,
            Rectangle::new(
                Point::new(
                    bounds.x + (bounds.width - s) / 2.0,
                    bounds.y + (bounds.height - s) / 2.0,
                ),
                Size::new(s, s),
            ),
            look.ink,
        );
    }
}

/// A clip or scene launch triangle; with `follow`, Live 12's idle follow-action arrow with
/// two vertical stripes before the tip (a 2026-10-11 screenshot of Live 12). Draws the glyph only: the grid owns the
/// launch area behind it.
pub(crate) struct Launch {
    pub follow: bool,
}

impl ControlKind for Launch {
    fn look(&self, theme: &Theme, _: State, _: Interaction) -> Look {
        Look::glyph(Color::TRANSPARENT, theme.text_disabled)
    }

    fn role(&self) -> SizeRole {
        GRID_BUTTON
    }

    fn paint(&self, frame: &mut Frame, area: Rectangle, look: &Look) {
        let size = 9.0;
        if !self.follow {
            let x = area.x + (area.width - size * 0.85) / 2.0;
            let y = area.y + (area.height - size) / 2.0;
            let path = Path::new(|b| {
                b.move_to(Point::new(x, y));
                b.line_to(Point::new(x + size * 0.85, y + size / 2.0));
                b.line_to(Point::new(x, y + size));
                b.close();
            });
            frame.fill(&path, look.ink);
            return;
        }
        let x = area.x + (area.width - size) / 2.0;
        let y = area.y + (area.height - size) / 2.0;
        // Slice the arrowhead vertically, retaining its taper across both stripes.
        let path = Path::new(|b| {
            for (left, right) in [(0.0, 1.5), (3.0, 4.5), (6.0, size)] {
                let top_left = size * left / (2.0 * size);
                let top_right = size * right / (2.0 * size);
                b.move_to(Point::new(x + left, y + top_left));
                b.line_to(Point::new(x + right, y + top_right));
                b.line_to(Point::new(x + right, y + size - top_right));
                b.line_to(Point::new(x + left, y + size - top_left));
                b.close();
            }
        });
        frame.fill(&path, look.ink);
    }
}

/// Main's global Follow Actions button: Live's striped launch arrow in a framed, latched
/// on/off button. Live 12.1's enabled toggle was sampled 2026-10-12 as ChosenDefault.
pub(crate) struct FollowToggle;

impl ControlKind for FollowToggle {
    fn activates_on_enter(&self) -> bool {
        true
    }
    fn look(&self, theme: &Theme, state: State, _: Interaction) -> Look {
        let (fill, ink) = if state.lit == Lit::On {
            (theme.chosen_default, theme.control_on_foreground)
        } else {
            (theme.control_background, theme.control_foreground)
        };
        Look {
            fill,
            ink,
            frame: theme.control_on_foreground,
            accent: theme.chosen_default,
        }
    }

    fn role(&self) -> SizeRole {
        GRID_BUTTON
    }

    fn paint(&self, frame: &mut Frame, area: Rectangle, look: &Look) {
        let button = Rectangle::new(
            Point::new(area.x + 1.0, area.y + 2.0),
            Size::new(area.width - 2.0, area.height - 4.0),
        );
        rect(frame, button, look.frame);
        rect(
            frame,
            Rectangle::new(
                Point::new(button.x + 1.0, button.y + 1.0),
                Size::new(button.width - 2.0, button.height - 2.0),
            ),
            look.fill,
        );
        let (cx, cy) = (area.center_x(), area.center_y());
        let arrow = Path::new(|b| {
            b.move_to(Point::new(cx - 5.0, cy - 4.0));
            b.line_to(Point::new(cx + 5.0, cy));
            b.line_to(Point::new(cx - 5.0, cy + 4.0));
            b.close();
        });
        frame.fill(&arrow, look.ink);
        for x in [cx - 2.0, cx + 1.0] {
            rect(
                frame,
                Rectangle::new(Point::new(x, cy - 4.0), Size::new(1.0, 8.0)),
                look.fill,
            );
        }
    }
}

/// Live 12's group fold button: three lines in a circle, stepped like a tree while the
/// group is open and level once folded. Its ink contrasts with the group colour, so the grid
/// passes it as a `Look`.
/// Live's mixer icon in the status bar: four level bars, orange while the strip switch is on
/// and grey while it is off. Clicking it shows or hides every checked strip row.
pub(crate) struct StripIcon;

impl StripIcon {
    /// Bar heights, left to right, in logical pixels.
    const BARS: [f32; 4] = [8.0, 14.0, 10.0, 12.0];
    const BAR_W: f32 = 3.0;
    const BAR_GAP: f32 = 2.0;
}

impl ControlKind for StripIcon {
    fn activates_on_enter(&self) -> bool {
        true
    }
    fn look(&self, theme: &Theme, state: State, _: Interaction) -> Look {
        let ink = if state.lit == Lit::On {
            theme.chosen_default
        } else {
            theme.control_foreground
        };
        Look::glyph(Color::TRANSPARENT, ink)
    }

    fn role(&self) -> SizeRole {
        SizeRole::Fixed {
            width: 22.0,
            height: 20.0,
        }
    }

    fn paint(&self, frame: &mut Frame, area: Rectangle, look: &Look) {
        let pitch = Self::BAR_W + Self::BAR_GAP;
        let width = Self::BARS.len() as f32 * pitch - Self::BAR_GAP;
        let left = area.center_x() - width / 2.0;
        let bottom = area.center_y() + 7.0;
        for (i, height) in Self::BARS.into_iter().enumerate() {
            let at = Point::new(left + i as f32 * pitch, bottom - height);
            rect(
                frame,
                Rectangle::new(at, Size::new(Self::BAR_W, height)),
                look.ink,
            );
        }
    }
}

pub(crate) struct Fold {
    pub folded: bool,
}

impl ControlKind for Fold {
    fn look(&self, theme: &Theme, _: State, _: Interaction) -> Look {
        Look::glyph(Color::TRANSPARENT, theme.control_foreground)
    }

    fn role(&self) -> SizeRole {
        GRID_BUTTON
    }

    fn paint(&self, frame: &mut Frame, area: Rectangle, look: &Look) {
        let centre = area.center();
        let stroke = Stroke::default()
            .with_color(look.ink)
            .with_width(crate::style::detail::FOLD_STROKE);
        frame.stroke(
            &Path::circle(centre, crate::style::detail::FOLD_RADIUS),
            stroke,
        );
        let path = Path::new(|b| {
            for (i, dy) in [-2.5_f32, 0.0, 2.5].into_iter().enumerate() {
                let indent = if self.folded { 0.0 } else { i as f32 };
                b.move_to(Point::new(centre.x - 3.0 + indent, centre.y + dy));
                b.line_to(Point::new(centre.x + 3.0, centre.y + dy));
            }
        });
        frame.stroke(&path, stroke);
    }
}
