//! Measured text: how wide a string draws, and the longest piece of it that fits. One home for
//! the paragraph plumbing that the grid's clip names, the Events header strip and the path
//! field each used to carry, so a label clips by what it measures, never by a character count.
use iced::advanced::text::{self, Paragraph as _};
use iced::{Font, Pixels, Size, alignment};

type Para = <iced::Renderer as text::Renderer>::Paragraph;

/// What a string is drawn in: the three things that decide its width.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Face {
    pub size: Pixels,
    pub font: Font,
    pub shaping: text::Shaping,
}

impl Face {
    /// Canvas text in the default font: the grid's labels and the lane headers.
    pub(crate) fn canvas(size: f32) -> Self {
        Self {
            size: Pixels(size),
            font: Font::default(),
            shaping: text::Shaping::default(),
        }
    }

    /// A field's value typography (`style::VALUE_FONT`), with the path field's basic shaping.
    pub(crate) fn value(size: Pixels) -> Self {
        Self {
            size,
            font: crate::style::VALUE_FONT,
            shaping: text::Shaping::Basic,
        }
    }
}

/// The ellipsis a clipped label ends in.
pub(crate) const ELLIPSIS: char = '…';

/// Rendered width of `content` on one line.
pub(crate) fn width(content: &str, face: Face) -> f32 {
    Para::with_text(text::Text {
        content,
        bounds: Size::INFINITE,
        size: face.size,
        line_height: text::LineHeight::default(),
        font: face.font,
        align_x: text::Alignment::Default,
        align_y: alignment::Vertical::Center,
        shaping: face.shaping,
        wrapping: text::Wrapping::None,
    })
    .min_bounds()
    .width
}

/// The longest prefix of `content` whose measured width fits in `room` (at least one
/// character, so a very narrow label still shows where it begins).
pub(crate) fn fit(content: &str, room: f32, face: Face) -> String {
    if width(content, face) <= room {
        return content.to_owned();
    }
    longest_prefix(content, |piece| width(piece, face) <= room).to_owned()
}

/// `content` whole if it fits `room`; otherwise its longest prefix that fits together with a
/// closing `…` (at least one character).
pub(crate) fn fit_ellipsis(content: &str, room: f32, face: Face) -> String {
    if width(content, face) <= room {
        return content.to_owned();
    }
    let prefix = longest_prefix(content, |piece| {
        let mut shown = String::with_capacity(piece.len() + ELLIPSIS.len_utf8());
        shown.push_str(piece);
        shown.push(ELLIPSIS);
        width(&shown, face) <= room
    });
    format!("{prefix}{ELLIPSIS}")
}

/// The longest prefix of `content` for which `fits` holds, found by bisection (widths grow
/// with length). Never shorter than one character.
fn longest_prefix(content: &str, fits: impl Fn(&str) -> bool) -> &str {
    let ends: Vec<usize> = content
        .char_indices()
        .map(|(i, c)| i + c.len_utf8())
        .collect();
    let (mut lo, mut hi) = (1, ends.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if fits(&content[..ends[mid - 1]]) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    &content[..ends.get(lo - 1).copied().unwrap_or(0)]
}

#[cfg(test)]
mod tests;
