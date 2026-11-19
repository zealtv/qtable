//! The help protocol: what a control says about itself, whatever panel renders it.
//!
//! Like Live's Info View, qtable describes controls in one place and shows them in another:
//! a control carries an `Identity` (its help id, and what it drives) and its wrapper
//! publishes a `Hover` when the pointer rests on it. The control layer owns that protocol and
//! names no panel. The app picks the help id type (`HelpId`), supplies the registry that gives
//! each id its text, and the Info View only renders what the hovers report.
mod area;

pub(crate) use area::{region, region_declaring, widget, widget_about, widget_with};
use iced::Element;
use qtable_engine::model::ControlRef;
use std::fmt::Debug;

/// An app's help ids. A `Subject` stands for one thing in the set that a hover can also name
/// (a path field's track, a Target argument), so the panel can say more than the entry's text.
pub(crate) trait HelpId: Copy + Eq + Debug + 'static {
    type Subject: Clone + PartialEq + Debug + 'static;
}

/// What a control is, for help and for mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Identity<H> {
    /// Its help entry; required, so no control ships without help. Many controls share one.
    pub help: H,
    /// What it drives, by stable ids. `Some` marks the control as mappable (Map Mode lights
    /// it); `None` for a control that drives nothing a controller could.
    pub drives: Option<ControlRef>,
}

impl<H> Identity<H> {
    pub(crate) fn new(help: H) -> Self {
        Self { help, drives: None }
    }
}

/// What a control declares to widget operations: the control it drives. Map Mode asks for
/// these to light the mappable controls and to learn which one was clicked.
pub(crate) struct Declared(pub ControlRef);

/// What a canvas declares: each mappable part's rectangle, relative to the canvas's own origin,
/// and what it drives. A canvas cell has no `Control` of its own, so the canvas lists them from
/// the geometry its hit test already uses.
pub(crate) struct DeclaredRegions(pub Vec<(iced::Rectangle, ControlRef)>);

/// Each wrapper has its own source, even when several controls use the same help entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Source(u64);

impl Source {
    pub(crate) fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Hover<H: HelpId> {
    Enter(Source, H, Option<H::Subject>),
    Leave(Source),
    Clear,
}

/// Import this trait to register any finished control with `.info(id, Message::Info)`.
pub(crate) trait Help<'a, M: 'a>: Into<Element<'a, M>> + Sized {
    fn info<H: HelpId>(self, id: H, on_hover: impl Fn(Hover<H>) -> M + 'a) -> Element<'a, M> {
        widget(self, id, on_hover)
    }

    /// Like `info`, for a control standing for a track, a Volume Path or a lane.
    fn info_about<H: HelpId>(
        self,
        id: H,
        subject: H::Subject,
        on_hover: impl Fn(Hover<H>) -> M + 'a,
    ) -> Element<'a, M> {
        widget_about(self, id, subject, on_hover)
    }
}

impl<'a, M: 'a, W: Into<Element<'a, M>>> Help<'a, M> for W {}
