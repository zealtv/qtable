//! Pointer-only Detail View reveal policy; selection itself never expands the panel.
use super::Action;
use crate::inspector_panel::Tab;

/// Decided 2026-11-06: double-clicking a scene or Main's header reveals its Detail
/// View tab. Single clicks only select, including on existing clips; their
/// double-click continues to open Clip View.
fn opens_detail(tab: Tab, double_click: bool) -> bool {
    match tab {
        Tab::Track | Tab::Output => double_click,
        Tab::Clip => double_click,
        Tab::Scene => double_click,
        Tab::Main => double_click,
    }
}

pub(super) fn click(action: Action, tab: Tab, double_click: bool) -> Action {
    if !opens_detail(tab, double_click) {
        return action;
    }
    match action {
        Action::Select(track, slot) => Action::OpenClip(track, slot),
        action => Action::OpenDetail(Box::new(action)),
    }
}
