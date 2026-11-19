//! The clip editor's tab bar, Events | Envelopes | Overview, at the top left of the editor as
//! Live's Notes | Envelopes | MPE sit at the top of its clip view (thread decision 1). Built from
//! the control layer's view switch, so it looks like every other view switch.
use super::{Choice, EditorTab};
use crate::control::{Control, ViewTab};
use crate::control_focus::{ControlOwner, View};
use crate::drafts::{ControlId, Controls};
use crate::inspector::Message as Inspect;
use iced::Element;
use qtable_engine::model::TrackId;

/// The three tabs for the clip at `slot`, the shown one lit.
pub(crate) fn bar<'a>(
    controls: &Controls<'_>,
    (track, slot): (TrackId, usize),
    choice: &Choice,
) -> Element<'a, Inspect> {
    controls.view_tabs(EditorTab::ALL.map(|tab| {
        (
            Control::new(
                ControlId::action(View::Clip, tab.name(), ControlOwner::ClipSlot(track, slot)),
                tab.help(),
                ViewTab {
                    label: tab.label().into(),
                },
            )
            .lit(choice.tab == tab),
            Inspect::ChooseTab(track, slot, tab),
        )
    }))
}
