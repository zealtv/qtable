//! One event clipboard and guarded targets shared by menus, keys and drags.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HitTarget {
    pub(crate) key: ClipKey,
    pub(crate) index: usize,
    pub(crate) revision: u64,
}
pub(crate) fn valid_target(app: &App, target: HitTarget) -> bool {
    app.chrome.detail_open
        && app.selection.inspector_tab == crate::inspector_panel::Tab::Clip
        && active(app, target.key)
        && target.revision == app.project_revision()
        && clip(app.project(), target.key.track, target.key.slot)
            .is_some_and(|c| target.index < c.events.len())
}
#[cfg(any(test, feature = "dev-hooks"))]
fn event(app: &App, target: HitTarget) -> Option<Event> {
    valid_target(app, target)
        .then(|| {
            super::super::lane_update::event(
                app.project(),
                target.key.track,
                target.key.slot,
                target.index,
            )
        })
        .flatten()
}
pub(super) fn duplicate_at(app: &mut App, target: HitTarget, position: BeatTime) {
    regions::move_hit(app, target, position, true);
}
#[cfg(test)]
mod tests;

/// Unattended captures select and edit through the same handlers, without desktop input.
#[cfg(any(test, feature = "dev-hooks"))]
pub(crate) fn capture(app: &mut App, menu: bool) {
    reconcile(app);
    let Some((track, slot, id)) = app.clip_view.editor.owner else {
        return;
    };
    let key = ClipKey {
        track,
        slot,
        id,
        epoch: app.clip_view.editor.epoch,
    };
    let target = HitTarget {
        key,
        index: 0,
        revision: app.project_revision(),
    };
    if !valid_target(app, target) {
        return;
    }
    app.focus = crate::focus::Focus::Detail;
    app.cursor = iced::Point::new(460.0, 540.0);
    let click = menu::Click {
        key,
        revision: target.revision,
        lane: lanes::Identity::Event(match event(app, target) {
            Some(event) => event.definition,
            None => return,
        }),
        under: menu::Under::Hit(target.index),
    };
    let Some(opened) = menu::open(app, click.clone()) else {
        return;
    };
    menu::command(
        app,
        opened,
        menu::ContentCommand::Key(crate::clip_shortcuts::Shortcut::Copy),
    );
    if menu {
        menu::open(app, click);
    } else if let Some(event) = event(app, target) {
        let Some(position) = event
            .position
            .ticks()
            .checked_add(app.clip_view.editor.grid.ticks())
            .map(BeatTime)
        else {
            return;
        };
        duplicate_at(app, target, position);
        reconcile(app);
    }
}
