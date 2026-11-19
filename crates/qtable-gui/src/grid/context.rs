//! Right-click on a clip slot, track title or group track title, which opens its context menu.
use super::{Action, Grid, Hit, column_at, hit_test_layout};
use iced::Point;

impl Grid<'_> {
    /// The menu action for a right-click at `p`. Group slots have no menu of their own.
    pub(super) fn context_action(&self, p: Point) -> Option<Action> {
        if self.status_row {
            return None;
        }
        if p.y < self.layout.header_h {
            return self.header_menu_action(p);
        }
        match hit_test_layout(p, &self.layout.columns, self.scenes.len())? {
            Hit::ClipLaunch(t, s) | Hit::ClipName(t, s) => {
                Some(Action::ContextMenu(self.tracks.get(t)?.id, s))
            }
            _ => None,
        }
    }

    /// A group track's title opens the group's menu; a track title opens the track's.
    fn header_menu_action(&self, p: Point) -> Option<Action> {
        let (_, col) = column_at(&self.layout.columns, &self.layout.xs, p.x)?;
        Some(super::header::menu(col))
    }
}
